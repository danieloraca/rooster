use rooster_core::{
    CancellationToken, ConfigStore,
    artifacts::{self, CodexSettings, Inventory, InventoryOptions},
    changes::{ChangeStore, DraftTarget, FaultPoint, FileKind, Request, Status},
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
        let t = tempfile::tempdir().unwrap();
        let base = t.path().canonicalize().unwrap();
        let repo = base.join("Project");
        fs::create_dir(&repo).unwrap();
        git(
            &repo,
            &["init", "--quiet", "--template=", "--initial-branch=main"],
        );
        fs::write(repo.join("AGENTS.md"), "before\r\nunchanged\n").unwrap();
        let config = ConfigStore::new(base.join("config.json")).unwrap();
        config.add("Test", &repo).unwrap();
        let mut c: serde_json::Value =
            serde_json::from_slice(&fs::read(config.path()).unwrap()).unwrap();
        c["codex"] = serde_json::to_value(CodexSettings {
            include_default_roots: false,
            ..Default::default()
        })
        .unwrap();
        fs::write(config.path(), serde_json::to_vec(&c).unwrap()).unwrap();
        Self {
            _temp: t,
            base,
            repo,
            config,
        }
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
    fn target(&self, inv: &Inventory) -> DraftTarget {
        DraftTarget::Edit {
            artifact_id: inv
                .artifacts
                .iter()
                .find(|a| a.path.as_path().ends_with("AGENTS.md"))
                .unwrap()
                .id
                .clone(),
        }
    }
}
fn git(p: &Path, args: &[&str]) {
    assert!(
        Command::new("git")
            .arg("-C")
            .arg(p)
            .args(args)
            .status()
            .unwrap()
            .success()
    )
}
#[test]
fn saved_draft_survives_restart_detects_external_edit_and_rebases_only_compared_revision() {
    let f = Fixture::new();
    let inv = f.inv();
    let d = f.store().open_draft(&inv, f.target(&inv), None).unwrap();
    let d = f
        .store()
        .save_draft(&d.id, d.revision, "my draft\r\nunchanged\n".into())
        .unwrap();
    assert_eq!(f.store().draft(&d.id).unwrap().text, d.text);
    assert_eq!(
        f.store().open_draft(&inv, f.target(&inv), None).unwrap().id,
        d.id
    );
    fs::write(f.repo.join("AGENTS.md"), "external\r\nunchanged\n").unwrap();
    let fresh = f.inv();
    assert!(f.store().prepare_draft(&fresh, &d.id, d.revision).is_err());
    let compared = f.store().compare_draft(&fresh, &d.id).unwrap();
    assert!(compared.conflict.is_some() && compared.can_rebase);
    assert_eq!(
        compared.current_text.as_deref(),
        Some("external\r\nunchanged\n")
    );
    assert!(
        f.store()
            .rebase_draft(&fresh, &d.id, d.revision, "wrong-token")
            .is_err()
    );
    let rebased = f
        .store()
        .rebase_draft(
            &fresh,
            &d.id,
            d.revision,
            compared.current_token.as_deref().unwrap(),
        )
        .unwrap();
    assert_eq!(rebased.text, d.text);
    let p = f
        .store()
        .prepare_draft(&fresh, &d.id, rebased.revision)
        .unwrap();
    assert_eq!(
        fs::read_to_string(f.repo.join("AGENTS.md")).unwrap(),
        "external\r\nunchanged\n"
    );
    assert_eq!(f.store().apply(&p.id).unwrap().status, Status::Completed);
    assert_eq!(
        fs::read_to_string(f.repo.join("AGENTS.md")).unwrap(),
        d.text
    );
    assert!(f.store().discard_draft(&d.id, d.revision).is_err());
    f.store().discard_draft(&d.id, rebased.revision).unwrap();
    assert!(f.store().drafts().unwrap().is_empty());
    assert_eq!(f.store().restore(&p.id).unwrap().status, Status::Restored);
    assert_eq!(
        fs::read_to_string(f.repo.join("AGENTS.md")).unwrap(),
        "external\r\nunchanged\n"
    );
}
#[test]
fn draft_versions_native_validation_and_checkout_changes_preserve_text() {
    let f = Fixture::new();
    let inv = f.inv();
    let d = f.store().open_draft(&inv, f.target(&inv), None).unwrap();
    let newer = f
        .store()
        .save_draft(&d.id, d.revision, "new draft".into())
        .unwrap();
    assert!(
        f.store()
            .save_draft(&d.id, d.revision, "stale writer".into())
            .is_err()
    );
    git(&f.repo, &["symbolic-ref", "HEAD", "refs/heads/other"]);
    let fresh = f.inv();
    let comparison = f.store().compare_draft(&fresh, &d.id).unwrap();
    assert!(!comparison.can_rebase && comparison.conflict.is_some());
    assert!(
        f.store()
            .prepare_draft(&fresh, &d.id, newer.revision)
            .is_err()
    );
    assert_eq!(f.store().draft(&d.id).unwrap().text, "new draft");
    let created = f
        .store()
        .open_draft(
            &fresh,
            DraftTarget::Create {
                owner_id: fresh.repositories.checkouts[0].id.clone(),
                path: PathBuf::from(".codex/agents/example.toml").into(),
                kind: FileKind::Agent,
            },
            Some("not valid toml = [".into()),
        )
        .unwrap();
    assert!(
        f.store()
            .prepare_draft(&fresh, &created.id, created.revision)
            .is_err()
    );
    assert_eq!(f.store().draft(&created.id).unwrap().text, created.text);
    assert!(!f.repo.join(".codex/agents/example.toml").exists());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(f.store().path().join(&d.id).join("draft.json"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
}
#[test]
fn new_file_draft_can_be_prepared_applied_and_restored() {
    let f = Fixture::new();
    let inv = f.inv();
    let d = f
        .store()
        .open_draft(
            &inv,
            DraftTarget::Create {
                owner_id: inv.repositories.checkouts[0].id.clone(),
                path: PathBuf::from("docs/new.md").into(),
                kind: FileKind::Markdown,
            },
            Some("# New".into()),
        )
        .unwrap();
    let p = f.store().prepare_draft(&inv, &d.id, d.revision).unwrap();
    assert!(!f.repo.join("docs").exists());
    assert_eq!(f.store().apply(&p.id).unwrap().status, Status::Completed);
    assert_eq!(
        fs::read_to_string(f.repo.join("docs/new.md")).unwrap(),
        "# New"
    );
    assert_eq!(f.store().restore(&p.id).unwrap().status, Status::Restored);
    assert!(!f.repo.join("docs").exists());
}
#[test]
fn explicit_cleanup_excludes_prepared_and_unresolved_operations() {
    let f = Fixture::new();
    let inv = f.inv();
    let id = match f.target(&inv) {
        DraftTarget::Edit { artifact_id } => artifact_id,
        _ => unreachable!(),
    };
    let request = |text: &str| Request::Replace {
        artifact_id: id.clone(),
        text: text.into(),
    };
    let p = f.store().prepare(&inv, request("after")).unwrap();
    assert!(f.store().cleanup(0, vec![p.id.clone()]).is_err());
    assert_eq!(f.store().apply(&p.id).unwrap().status, Status::Completed);
    assert!(f.store().cleanup_preview(30).unwrap().candidates.is_empty());
    let fresh = f.inv();
    let q = f.store().prepare(&fresh, request("later")).unwrap();
    let faulty = f.store().with_fault_injector(|point| {
        if matches!(point, FaultPoint::AfterChange(_)) {
            Err(std::io::Error::other("interrupted"))
        } else {
            Ok(())
        }
    });
    assert_eq!(
        faulty.apply(&q.id).unwrap().status,
        Status::RecoveryRequired
    );
    assert!(f.store().cleanup(0, vec![q.id.clone()]).is_err());
    let result = f.store().cleanup(0, vec![p.id.clone()]).unwrap();
    assert_eq!(result.removed, vec![p.id]);
    assert!(f.store().preview(&q.id).is_ok());
    assert_eq!(
        fs::read_to_string(f.repo.join("AGENTS.md")).unwrap(),
        "later"
    );
}
#[test]
fn git_status_reports_staged_rename_and_untracked_paths_without_changing_git() {
    let f = Fixture::new();
    git(&f.repo, &["add", "AGENTS.md"]);
    git(
        &f.repo,
        &[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "-c",
            "core.hooksPath=/dev/null",
            "commit",
            "--quiet",
            "-m",
            "fixture",
        ],
    );
    git(&f.repo, &["mv", "AGENTS.md", "MOVED.md"]);
    fs::write(f.repo.join("new ü.md"), "new").unwrap();
    let inv = f.inv();
    let index = fs::read(f.repo.join(".git/index")).unwrap();
    let head = fs::read(f.repo.join(".git/HEAD")).unwrap();
    let changes = ChangeStore::git_changes(&inv, &inv.repositories.checkouts[0].id).unwrap();
    assert!(changes.entries.iter().any(|e| e.index_status == "R"
        && e.original_path.as_ref().unwrap().as_path() == Path::new("AGENTS.md")));
    assert!(
        changes
            .entries
            .iter()
            .any(|e| e.path.as_path() == Path::new("new ü.md") && e.worktree_status == "?")
    );
    assert_eq!(fs::read(f.repo.join(".git/index")).unwrap(), index);
    assert_eq!(fs::read(f.repo.join(".git/HEAD")).unwrap(), head);
}
