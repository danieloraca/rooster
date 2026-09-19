use rooster_core::{
    CancellationToken, ConfigStore,
    artifacts::{self, ArtifactKind, CodexSettings, Inventory, InventoryOptions},
    changes::{ChangeStore, FaultPoint, FileKind, NewFile, Request, Status, TextEdit},
};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};
struct Fixture {
    _temp: tempfile::TempDir,
    base: PathBuf,
    repo: PathBuf,
    config: ConfigStore,
}
impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let base = temp.path().canonicalize().unwrap();
        let repo = base.join("Project ü");
        fs::create_dir(&repo).unwrap();
        git(
            &repo,
            &["init", "--quiet", "--template=", "--initial-branch=main"],
        );
        let config = ConfigStore::new(base.join("config.json")).unwrap();
        config.add("Example", &repo).unwrap();
        let mut json: serde_json::Value =
            serde_json::from_slice(&fs::read(config.path()).unwrap()).unwrap();
        json["codex"] = serde_json::to_value(CodexSettings {
            include_default_roots: false,
            ..Default::default()
        })
        .unwrap();
        fs::write(config.path(), serde_json::to_vec(&json).unwrap()).unwrap();
        Self {
            _temp: temp,
            base,
            repo,
            config,
        }
    }
    fn put(&self, path: &str, bytes: impl AsRef<[u8]>) {
        let p = self.repo.join(path);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, bytes).unwrap();
    }
    fn inv(&self) -> Inventory {
        let c = self.config.load().unwrap();
        artifacts::inventory(
            &c.roots(None).unwrap(),
            &InventoryOptions {
                codex: c.codex,
                all_markdown: true,
                ..Default::default()
            },
            &CancellationToken::default(),
            |_| {},
        )
    }
    fn store(&self) -> ChangeStore {
        ChangeStore::new(self.config.clone(), self.base.join("recovery")).unwrap()
    }
    fn package(&self) {
        self.put("App/.agents/skills/example/SKILL.md",b"---\nname: example\ndescription: Example skill.\n---\n# Example\n[Guide](references/guide.md)\n");
        self.put(
            "App/.agents/skills/example/references/guide.md",
            b"# Guide\nKeep bytes.\n",
        );
        self.put("App/.agents/skills/example/assets/raw.bin", [0, 1, 255]);
        self.put(
            "App/.agents/skills/example/scripts/run.sh",
            b"#!/bin/sh\nexit 0\n",
        );
        fs::create_dir(self.repo.join("App/.agents/skills/example/empty")).unwrap();
    }
}
fn git(repo: &Path, args: &[&str]) {
    assert!(
        Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(args)
            .status()
            .unwrap()
            .success()
    );
}
fn id(inv: &Inventory, suffix: &str) -> String {
    inv.artifacts
        .iter()
        .find(|a| a.path.as_path().ends_with(suffix))
        .unwrap()
        .id
        .clone()
}
fn replace(inv: &Inventory, suffix: &str, text: &str) -> Request {
    Request::Replace {
        artifact_id: id(inv, suffix),
        text: text.into(),
    }
}
#[test]
fn edit_preserves_unknown_metadata_crlf_bom_permissions_and_restores() {
    let f = Fixture::new();
    let original = "\u{feff}---\r\nname: example\r\ndescription: Original.\r\ncustom: keep\r\n---\r\nBody.\r\n";
    f.put(".agents/skills/example/SKILL.md", original);
    let path = f.repo.join(".agents/skills/example/SKILL.md");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
    }
    let inv = f.inv();
    let store = f.store();
    let preview = store
        .prepare(
            &inv,
            replace(
                &inv,
                "SKILL.md",
                "---\nname: example\ndescription: Changed.\ncustom: keep\n---\nBody.\n",
            ),
        )
        .unwrap();
    assert!(!preview.warnings.is_empty());
    assert_eq!(fs::read(&path).unwrap(), original.as_bytes());
    assert_eq!(store.apply(&preview.id).unwrap().status, Status::Completed);
    let changed = fs::read_to_string(&path).unwrap();
    assert!(changed.starts_with('\u{feff}'));
    assert!(changed.contains("custom: keep\r\n"));
    assert!(!changed.replace("\r\n", "").contains('\n'));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o640
        );
    }
    assert_eq!(store.restore(&preview.id).unwrap().status, Status::Restored);
    assert_eq!(fs::read_to_string(path).unwrap(), original);
}
#[test]
fn stale_source_identity_branch_and_destination_are_rejected() {
    let f = Fixture::new();
    f.put("AGENTS.md", "before");
    let inv = f.inv();
    let store = f.store();
    let p = store
        .prepare(&inv, replace(&inv, "AGENTS.md", "after"))
        .unwrap();
    f.put("AGENTS.md", "external");
    assert!(store.apply(&p.id).is_err());
    assert!(
        store
            .prepare(&inv, replace(&inv, "AGENTS.md", "draft"))
            .is_err()
    );
    let inv = f.inv();
    let p = store
        .prepare(&inv, replace(&inv, "AGENTS.md", "after"))
        .unwrap();
    git(&f.repo, &["symbolic-ref", "HEAD", "refs/heads/other"]);
    assert!(
        store
            .apply(&p.id)
            .unwrap_err()
            .to_string()
            .contains("Checkout")
    );
    let inv = f.inv();
    let owner = inv.repositories.checkouts[0].id.clone();
    assert!(
        store
            .prepare(
                &inv,
                Request::Create {
                    owner_id: owner.clone(),
                    path: PathBuf::from("AGENTS.md").into(),
                    kind: FileKind::Markdown,
                    text: "collision".into()
                }
            )
            .is_err()
    );
    let p = store
        .prepare(
            &inv,
            Request::Create {
                owner_id: owner,
                path: PathBuf::from("new.md").into(),
                kind: FileKind::Markdown,
                text: "new".into(),
            },
        )
        .unwrap();
    f.put("new.md", "external");
    assert!(store.apply(&p.id).is_err());
    assert_eq!(
        fs::read_to_string(f.repo.join("new.md")).unwrap(),
        "external"
    );
}
#[test]
fn package_duplicate_delete_and_restore_include_binary_scripts_and_empty_dirs() {
    let f = Fixture::new();
    f.package();
    let inv = f.inv();
    let store = f.store();
    let p = store
        .prepare(
            &inv,
            Request::Duplicate {
                artifact_id: id(&inv, "example/SKILL.md"),
                owner_id: inv.repositories.checkouts[0].id.clone(),
                path: PathBuf::from("App/.agents/skills/copy").into(),
            },
        )
        .unwrap();
    assert!(
        p.files
            .iter()
            .any(|x| x.path.as_path().ends_with("assets/raw.bin"))
    );
    assert_eq!(store.apply(&p.id).unwrap().status, Status::Completed);
    assert_eq!(
        fs::read(f.repo.join("App/.agents/skills/copy/assets/raw.bin")).unwrap(),
        [0, 1, 255]
    );
    assert!(f.repo.join("App/.agents/skills/copy/empty").is_dir());
    let inv = f.inv();
    let del = store
        .prepare(
            &inv,
            Request::Delete {
                artifact_id: id(&inv, "copy/SKILL.md"),
            },
        )
        .unwrap();
    assert_eq!(store.apply(&del.id).unwrap().status, Status::Completed);
    assert!(!f.repo.join("App/.agents/skills/copy").exists());
    assert_eq!(store.restore(&del.id).unwrap().status, Status::Restored);
    assert!(f.repo.join("App/.agents/skills/copy/empty").is_dir());
    assert_eq!(
        fs::read(f.repo.join("App/.agents/skills/copy/assets/raw.bin")).unwrap(),
        [0, 1, 255]
    );
}
#[test]
fn interrupted_multifile_apply_survives_restart_and_can_finish_or_restore() {
    let f = Fixture::new();
    f.package();
    let inv = f.inv();
    let store = f.store();
    let p = store
        .prepare(
            &inv,
            Request::Rename {
                artifact_id: id(&inv, "example/SKILL.md"),
                path: PathBuf::from("App/.agents/skills/renamed").into(),
                repair_links: vec![],
            },
        )
        .unwrap();
    let broken = f.store().with_fault_injector(|point| {
        if matches!(point, FaultPoint::AfterChange(_)) {
            Err(std::io::Error::other(
                "simulated process interruption after write",
            ))
        } else {
            Ok(())
        }
    });
    assert_eq!(
        broken.apply(&p.id).unwrap().status,
        Status::RecoveryRequired
    );
    drop(broken);
    assert_eq!(f.store().apply(&p.id).unwrap().status, Status::Completed);
    assert!(!f.repo.join("App/.agents/skills/example").exists());
    assert_eq!(f.store().restore(&p.id).unwrap().status, Status::Restored);
    assert!(f.repo.join("App/.agents/skills/example/SKILL.md").exists());
    assert!(!f.repo.join("App/.agents/skills/renamed").exists());
    let inv = f.inv();
    let p = store
        .prepare(
            &inv,
            Request::Delete {
                artifact_id: id(&inv, "example/SKILL.md"),
            },
        )
        .unwrap();
    let broken = f.store().with_fault_injector(|p| {
        if matches!(p, FaultPoint::AfterChange(_)) {
            Err(std::io::Error::other("stop"))
        } else {
            Ok(())
        }
    });
    assert_eq!(
        broken.apply(&p.id).unwrap().status,
        Status::RecoveryRequired
    );
    assert_eq!(f.store().restore(&p.id).unwrap().status, Status::Restored);
    assert!(f.repo.join("App/.agents/skills/example/SKILL.md").exists());
}
#[test]
fn backup_and_write_failures_never_claim_success_or_lose_originals() {
    let f = Fixture::new();
    f.put("AGENTS.md", "original");
    let inv = f.inv();
    let broken = f.store().with_fault_injector(|p| {
        if p == FaultPoint::Backup {
            Err(std::io::Error::other("disk full"))
        } else {
            Ok(())
        }
    });
    assert!(
        broken
            .prepare(&inv, replace(&inv, "AGENTS.md", "draft"))
            .is_err()
    );
    assert_eq!(
        fs::read_to_string(f.repo.join("AGENTS.md")).unwrap(),
        "original"
    );
    let store = f.store();
    let p = store
        .prepare(&inv, replace(&inv, "AGENTS.md", "draft"))
        .unwrap();
    let broken = f.store().with_fault_injector(|p| {
        if matches!(p, FaultPoint::BeforeChange(_)) {
            Err(std::io::Error::other("write failed"))
        } else {
            Ok(())
        }
    });
    assert_eq!(
        broken.apply(&p.id).unwrap().status,
        Status::RecoveryRequired
    );
    assert_eq!(
        fs::read_to_string(f.repo.join("AGENTS.md")).unwrap(),
        "original"
    );
    assert!(
        store.preview(&p.id).unwrap().files[0]
            .after_text
            .as_ref()
            .unwrap()
            .contains("draft")
    );
    assert_eq!(store.restore(&p.id).unwrap().status, Status::Restored);
}
#[test]
fn restore_rejects_external_edits_and_delete_destination_collision() {
    let f = Fixture::new();
    f.put("AGENTS.md", "original");
    let inv = f.inv();
    let store = f.store();
    let p = store
        .prepare(&inv, replace(&inv, "AGENTS.md", "changed"))
        .unwrap();
    store.apply(&p.id).unwrap();
    f.put("AGENTS.md", "newer");
    assert!(store.restore(&p.id).is_err());
    assert_eq!(
        fs::read_to_string(f.repo.join("AGENTS.md")).unwrap(),
        "newer"
    );
    let inv = f.inv();
    let p = store
        .prepare(
            &inv,
            Request::Delete {
                artifact_id: id(&inv, "AGENTS.md"),
            },
        )
        .unwrap();
    store.apply(&p.id).unwrap();
    f.put("AGENTS.md", "collision");
    assert!(store.restore(&p.id).is_err());
    assert_eq!(
        fs::read_to_string(f.repo.join("AGENTS.md")).unwrap(),
        "collision"
    );
}
#[test]
fn package_membership_changed_after_preview_is_rejected() {
    let f = Fixture::new();
    f.package();
    let inv = f.inv();
    let store = f.store();
    let p = store
        .prepare(
            &inv,
            Request::Delete {
                artifact_id: id(&inv, "example/SKILL.md"),
            },
        )
        .unwrap();
    f.put("App/.agents/skills/example/new.txt", "unreviewed");
    assert!(store.apply(&p.id).is_err());
    assert!(f.repo.join("App/.agents/skills/example/new.txt").exists());
}
#[test]
fn explicit_inline_link_repair_preserves_other_bytes_and_reports_unselected_links() {
    let f = Fixture::new();
    f.put("guide.md", "# Guide\n");
    f.put(
        "README.md",
        "Untouched.\n[Guide](guide.md#section \"Title\")\n`[example](guide.md)`\n",
    );
    f.put("other.md", "[Guide][ref]\n\n[ref]: guide.md\n");
    let inv = f.inv();
    let store = f.store();
    let p = store
        .prepare(
            &inv,
            Request::Rename {
                artifact_id: id(&inv, "guide.md"),
                path: PathBuf::from("new guide.md").into(),
                repair_links: vec![id(&inv, "README.md")],
            },
        )
        .unwrap();
    assert!(!p.warnings.is_empty());
    assert_eq!(store.apply(&p.id).unwrap().status, Status::Completed);
    let text = fs::read_to_string(f.repo.join("README.md")).unwrap();
    assert!(text.contains("Untouched.\n"));
    assert!(text.contains("new%20guide%2Emd#section \"Title\""));
    assert!(text.contains("`[example](guide.md)`"));
    assert_eq!(store.restore(&p.id).unwrap().status, Status::Restored);
}
#[test]
fn create_package_and_text_patch_use_valid_native_metadata() {
    let f = Fixture::new();
    let inv = f.inv();
    let store = f.store();
    let p = store
        .prepare(
            &inv,
            Request::CreatePackage {
                owner_id: inv.repositories.checkouts[0].id.clone(),
                path: PathBuf::from(".agents/skills/new").into(),
                files: vec![
                    NewFile {
                        path: PathBuf::from("SKILL.md").into(),
                        bytes: b"---\nname: new\ndescription: New skill.\n---\nBody\n".to_vec(),
                    },
                    NewFile {
                        path: PathBuf::from("agents/openai.yaml").into(),
                        bytes: b"policy:\n  allow_implicit_invocation: false\n".to_vec(),
                    },
                ],
            },
        )
        .unwrap();
    assert_eq!(store.apply(&p.id).unwrap().status, Status::Completed);
    let inv = f.inv();
    assert!(inv.artifacts.iter().any(|a| a.kind == ArtifactKind::Skill));
    let a = inv
        .artifacts
        .iter()
        .find(|a| a.kind == ArtifactKind::Skill)
        .unwrap();
    let text = inv
        .inspect(&a.id)
        .unwrap()
        .snapshot
        .unwrap()
        .text
        .as_ref()
        .unwrap();
    let start = text.find("Body").unwrap();
    let p = store
        .prepare(
            &inv,
            Request::Edit {
                artifact_id: a.id.clone(),
                edits: vec![TextEdit {
                    start,
                    end: start + 4,
                    replacement: "Updated".into(),
                }],
            },
        )
        .unwrap();
    store.apply(&p.id).unwrap();
    assert!(
        fs::read_to_string(a.path.as_path())
            .unwrap()
            .ends_with("Updated\n")
    );
    assert!(
        store
            .prepare(
                &f.inv(),
                Request::Replace {
                    artifact_id: a.id.clone(),
                    text: "invalid skill".into()
                }
            )
            .is_err()
    );
}
#[test]
fn traversal_nested_checkout_symlinks_hardlinks_and_linked_package_are_rejected() {
    let f = Fixture::new();
    f.put("AGENTS.md", "before");
    let inv = f.inv();
    let store = f.store();
    let owner = inv.repositories.checkouts[0].id.clone();
    for p in [
        "../escape.md",
        ".git/config.md",
        ".GIT/config.md",
        "/tmp/escape.md",
    ] {
        assert!(
            store
                .prepare(
                    &inv,
                    Request::Create {
                        owner_id: owner.clone(),
                        path: PathBuf::from(p).into(),
                        kind: FileKind::Markdown,
                        text: "bad".into()
                    }
                )
                .is_err()
        );
    }
    let nested = f.repo.join("nested");
    fs::create_dir(&nested).unwrap();
    git(&nested, &["init", "--quiet", "--template="]);
    assert!(
        store
            .prepare(
                &inv,
                Request::Create {
                    owner_id: owner.clone(),
                    path: PathBuf::from("nested/file.md").into(),
                    kind: FileKind::Markdown,
                    text: "bad".into()
                }
            )
            .is_err()
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        symlink(&f.base, f.repo.join("link")).unwrap();
        assert!(
            store
                .prepare(
                    &inv,
                    Request::Create {
                        owner_id: owner,
                        path: PathBuf::from("link/escape.md").into(),
                        kind: FileKind::Markdown,
                        text: "bad".into()
                    }
                )
                .is_err()
        );
        fs::hard_link(f.repo.join("AGENTS.md"), f.base.join("hard.md")).unwrap();
        assert!(
            store
                .prepare(&inv, replace(&inv, "AGENTS.md", "bad"))
                .is_err()
        );
        f.package();
        symlink(
            f.base.join("hard.md"),
            f.repo.join("App/.agents/skills/example/linked.md"),
        )
        .unwrap();
        let inv = f.inv();
        assert!(
            store
                .prepare(
                    &inv,
                    Request::Delete {
                        artifact_id: id(&inv, "example/SKILL.md")
                    }
                )
                .is_err()
        );
    }
}

#[test]
fn installed_sources_are_read_only_but_can_be_copied_to_an_authoring_owner() {
    let f = Fixture::new();
    let installed = f.base.join("installed/example");
    fs::create_dir_all(&installed).unwrap();
    fs::write(
        installed.join("SKILL.md"),
        "---\nname: installed\ndescription: Installed fixture.\n---\nBody\n",
    )
    .unwrap();
    let mut json: serde_json::Value =
        serde_json::from_slice(&fs::read(f.config.path()).unwrap()).unwrap();
    json["codex"]["extra_roots"] =
        serde_json::json!([{"path":f.base.join("installed"),"provenance":"installed_plugin"}]);
    fs::write(f.config.path(), serde_json::to_vec(&json).unwrap()).unwrap();
    let inv = f.inv();
    let store = f.store();
    let source = id(&inv, "example/SKILL.md");
    assert!(
        store
            .prepare(
                &inv,
                Request::Delete {
                    artifact_id: source.clone()
                }
            )
            .is_err()
    );
    let p = store
        .prepare(
            &inv,
            Request::Duplicate {
                artifact_id: source,
                owner_id: inv.repositories.checkouts[0].id.clone(),
                path: PathBuf::from(".agents/skills/independent").into(),
            },
        )
        .unwrap();
    assert_eq!(store.apply(&p.id).unwrap().status, Status::Completed);
    assert_eq!(
        fs::read(installed.join("SKILL.md")).unwrap(),
        fs::read(f.repo.join(".agents/skills/independent/SKILL.md")).unwrap()
    );
}
#[test]
fn source_file_and_destination_parent_replacement_are_conflicts_even_with_equal_bytes() {
    let f = Fixture::new();
    f.put("AGENTS.md", "original");
    let inv = f.inv();
    let store = f.store();
    let p = store
        .prepare(&inv, replace(&inv, "AGENTS.md", "draft"))
        .unwrap();
    fs::rename(f.repo.join("AGENTS.md"), f.base.join("old.md")).unwrap();
    f.put("AGENTS.md", "original");
    assert!(store.apply(&p.id).is_err());
    f.put("docs/old.md", "existing");
    let inv = f.inv();
    let p = store
        .prepare(
            &inv,
            Request::Create {
                owner_id: inv.repositories.checkouts[0].id.clone(),
                path: PathBuf::from("docs/new.md").into(),
                kind: FileKind::Markdown,
                text: "new".into(),
            },
        )
        .unwrap();
    fs::rename(f.repo.join("docs"), f.repo.join("old-docs")).unwrap();
    fs::create_dir(f.repo.join("docs")).unwrap();
    assert!(store.apply(&p.id).is_err());
    assert!(!f.repo.join("docs/new.md").exists());
}
#[test]
fn agent_metadata_edit_and_mixed_newline_patch_preserve_unedited_bytes() {
    let f = Fixture::new();
    let original = "# keep comment\nname = \"reviewer\"\ndescription = \"Review\"\ndeveloper_instructions = \"Inspect\"\ncustom = { untouched = true }\n";
    f.put(".codex/agents/reviewer.toml", original);
    f.put("mixed.md", "First\r\nSecond\nThird\r\n");
    let inv = f.inv();
    let store = f.store();
    let p = store
        .prepare(
            &inv,
            replace(
                &inv,
                "reviewer.toml",
                &original.replace("Inspect", "Inspect carefully"),
            ),
        )
        .unwrap();
    store.apply(&p.id).unwrap();
    assert!(
        fs::read_to_string(f.repo.join(".codex/agents/reviewer.toml"))
            .unwrap()
            .ends_with("custom = { untouched = true }\n")
    );
    let inv = f.inv();
    let p = store
        .prepare(
            &inv,
            Request::Edit {
                artifact_id: id(&inv, "mixed.md"),
                edits: vec![TextEdit {
                    start: 7,
                    end: 13,
                    replacement: "Edited".into(),
                }],
            },
        )
        .unwrap();
    store.apply(&p.id).unwrap();
    assert_eq!(
        fs::read(f.repo.join("mixed.md")).unwrap(),
        b"First\r\nEdited\nThird\r\n"
    );
}
#[test]
fn recovery_backup_corruption_is_detected_before_writing_and_data_cannot_live_in_repo() {
    let f = Fixture::new();
    f.put("AGENTS.md", "original");
    let inv = f.inv();
    let store = f.store();
    let p = store
        .prepare(&inv, replace(&inv, "AGENTS.md", "draft"))
        .unwrap();
    let h = p.files[0].before_sha256.as_ref().unwrap();
    fs::write(store.path().join(&p.id).join("blobs").join(h), "corrupt").unwrap();
    assert!(store.apply(&p.id).is_err());
    assert_eq!(
        fs::read_to_string(f.repo.join("AGENTS.md")).unwrap(),
        "original"
    );
    let bad = ChangeStore::new(f.config.clone(), f.repo.join("recovery")).unwrap();
    assert!(
        bad.prepare(&inv, replace(&inv, "AGENTS.md", "draft"))
            .is_err()
    );
    assert!(!f.repo.join("recovery").exists());
}

#[test]
fn duplicate_rechecks_source_and_reserved_destinations_cannot_bypass_validation() {
    let f = Fixture::new();
    f.put("AGENTS.md", "original");
    let inv = f.inv();
    let store = f.store();
    let owner = inv.repositories.checkouts[0].id.clone();
    let p = store.prepare(
        &inv,
        Request::Duplicate {
            artifact_id: id(&inv, "AGENTS.md"),
            owner_id: owner.clone(),
            path: PathBuf::from("copied.md").into(),
        },
    );
    assert!(
        p.is_err(),
        "instruction rename must retain a native instruction destination"
    );
    f.put("notes.md", "original");
    let inv = f.inv();
    let p = store
        .prepare(
            &inv,
            Request::Duplicate {
                artifact_id: id(&inv, "notes.md"),
                owner_id: owner.clone(),
                path: PathBuf::from("copy.md").into(),
            },
        )
        .unwrap();
    f.put("notes.md", "changed");
    assert!(store.apply(&p.id).is_err());
    for path in [
        ".agents/skills/new/SKILL.md",
        ".system/notes.md",
        ".codex/skills/new/readme.md",
    ] {
        assert!(
            store
                .prepare(
                    &f.inv(),
                    Request::Create {
                        owner_id: owner.clone(),
                        path: PathBuf::from(path).into(),
                        kind: FileKind::Markdown,
                        text: "invalid".into()
                    }
                )
                .is_err()
        );
    }
}
#[test]
fn lost_journal_update_and_interrupted_restore_are_recoverable() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let f = Fixture::new();
    f.put("AGENTS.md", "original");
    let inv = f.inv();
    let store = f.store();
    let p = store
        .prepare(&inv, replace(&inv, "AGENTS.md", "draft"))
        .unwrap();
    let saves = AtomicUsize::new(0);
    let broken = f.store().with_fault_injector(move |point| {
        if point == FaultPoint::Journal && saves.fetch_add(1, Ordering::SeqCst) >= 1 {
            Err(std::io::Error::other("journal volume failure"))
        } else {
            Ok(())
        }
    });
    assert_eq!(
        broken.apply(&p.id).unwrap().status,
        Status::RecoveryRequired
    );
    assert_eq!(
        fs::read_to_string(f.repo.join("AGENTS.md")).unwrap(),
        "draft"
    );
    assert_eq!(store.history().unwrap()[0].status, Status::Applying);
    let broken = f.store().with_fault_injector(|p| {
        if matches!(p, FaultPoint::AfterChange(_)) {
            Err(std::io::Error::other("restore interrupted"))
        } else {
            Ok(())
        }
    });
    assert_eq!(broken.restore(&p.id).unwrap().status, Status::Restoring);
    assert_eq!(f.store().restore(&p.id).unwrap().status, Status::Restored);
    assert_eq!(
        fs::read_to_string(f.repo.join("AGENTS.md")).unwrap(),
        "original"
    );
}

#[test]
fn malformed_skill_cannot_be_duplicated_as_a_new_definition() {
    let f = Fixture::new();
    f.put(
        ".agents/skills/broken/SKILL.md",
        "---\nname: broken\n---\nMissing description\n",
    );
    let inv = f.inv();
    let store = f.store();
    assert!(
        store
            .prepare(
                &inv,
                Request::Duplicate {
                    artifact_id: id(&inv, "broken/SKILL.md"),
                    owner_id: inv.repositories.checkouts[0].id.clone(),
                    path: PathBuf::from(".agents/skills/copy").into()
                }
            )
            .is_err()
    );
    assert!(!f.repo.join(".agents/skills/copy").exists());
}

#[test]
fn reviewed_nested_package_destinations_are_rejected_but_siblings_remain_skills() {
    let f = Fixture::new();
    f.package();
    let inv = f.inv();
    let owner = inv.repositories.checkouts[0].id.clone();
    let skill = id(&inv, "example/SKILL.md");
    let nested = "App/.agents/skills/example/child";
    for request in [
        Request::Duplicate {
            artifact_id: skill.clone(),
            owner_id: owner.clone(),
            path: PathBuf::from(nested).into(),
        },
        Request::Rename {
            artifact_id: skill.clone(),
            path: PathBuf::from(nested).into(),
            repair_links: vec![],
        },
        Request::CreatePackage {
            owner_id: owner.clone(),
            path: PathBuf::from(nested).into(),
            files: vec![NewFile {
                path: PathBuf::from("SKILL.md").into(),
                bytes: b"---\nname: child\ndescription: Child\n---\n".to_vec(),
            }],
        },
    ] {
        assert!(f.store().prepare(&inv, request).is_err());
        assert!(!f.repo.join(nested).exists());
    }
    let p = f
        .store()
        .prepare(
            &inv,
            Request::Duplicate {
                artifact_id: skill,
                owner_id: owner,
                path: PathBuf::from("App/.agents/skills/sibling").into(),
            },
        )
        .unwrap();
    assert_eq!(f.store().apply(&p.id).unwrap().status, Status::Completed);
    let current = f.inv();
    assert_eq!(
        current
            .artifacts
            .iter()
            .find(|a| a.path.as_path().ends_with("sibling/SKILL.md"))
            .unwrap()
            .kind,
        ArtifactKind::Skill
    );
}

#[test]
fn reviewed_link_title_is_preserved_and_nested_labels_are_diagnostic() {
    let f = Fixture::new();
    f.put("guide.md", "Guide");
    f.put(
        "README.md",
        "[Guide](guide.md \"example ](guide.md\")\n[![badge](badge.png)](guide.md \"keep\")\n",
    );
    f.put("badge.png", [0, 1]);
    let inv = f.inv();
    let p = f
        .store()
        .prepare(
            &inv,
            Request::Rename {
                artifact_id: id(&inv, "guide.md"),
                path: PathBuf::from("renamed.md").into(),
                repair_links: vec![id(&inv, "README.md")],
            },
        )
        .unwrap();
    assert!(p.warnings.iter().any(|w| w.contains("complex link syntax")));
    assert_eq!(f.store().apply(&p.id).unwrap().status, Status::Completed);
    let text = fs::read_to_string(f.repo.join("README.md")).unwrap();
    assert!(text.starts_with("[Guide](renamed%2Emd \"example ](guide.md\")\n"));
    assert!(text.contains("](guide.md \"keep\")"));
    assert!(text.contains("![badge](badge%2Epng)"));
}
