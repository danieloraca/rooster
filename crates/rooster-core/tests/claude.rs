use rooster_core::{
    CancellationToken, ConfigStore, ScanStatus,
    artifacts::{
        self, Artifact, ArtifactKind as K, ClaudeSettings, CodexSettings, Inventory,
        InventoryOptions, Provenance as P, ReferenceStatus as R, Validation as V,
    },
    changes::{ChangeStore, DraftTarget, FileKind, Request, Status},
    providers::{ArtifactProvider, Provider, claude::ClaudeProvider},
};
use serde_json::json;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};
fn write(p: impl AsRef<Path>, bytes: impl AsRef<[u8]>) {
    fs::create_dir_all(p.as_ref().parent().unwrap()).unwrap();
    fs::write(p, bytes).unwrap();
}
fn agent(name: &str) -> String {
    format!(
        "---\nname: {name}\ndescription: Review changes.\nfuture_field: {{keep: true}}\n---\n\nOriginal body.\n"
    )
}
struct Fixture {
    _t: tempfile::TempDir,
    base: PathBuf,
    a: PathBuf,
    b: PathBuf,
    home: PathBuf,
    config: ConfigStore,
}
impl Fixture {
    fn new() -> Self {
        let t = tempfile::tempdir().unwrap();
        let base = t.path().canonicalize().unwrap();
        let a = base.join("projects/API ü");
        let b = base.join("projects/Web");
        for p in [&a, &b] {
            fs::create_dir_all(p).unwrap();
            assert!(
                Command::new("git")
                    .args(["init", "--quiet", "--template=", "--initial-branch=main"])
                    .arg(p)
                    .status()
                    .unwrap()
                    .success()
            );
        }
        let home = a.join("personal-root");
        fs::create_dir_all(&home).unwrap();
        let config = ConfigStore::new(base.join("config.json")).unwrap();
        config.add("Projects", base.join("projects")).unwrap();
        let mut c = config.load().unwrap();
        c.codex = CodexSettings {
            include_default_roots: false,
            ..Default::default()
        };
        c.claude = ClaudeSettings {
            include_default_roots: false,
            home: Some(home.clone().into()),
            managed: Some(base.join("managed").into()),
            ..Default::default()
        };
        fs::create_dir(base.join("managed")).unwrap();
        write(config.path(), serde_json::to_vec(&c).unwrap());
        Self {
            _t: t,
            base,
            a,
            b,
            home,
            config,
        }
    }
    fn inv(&self, p: Provider, context: &Path) -> Inventory {
        let c = self.config.load().unwrap();
        artifacts::inventory(
            &c.roots(None).unwrap(),
            &InventoryOptions {
                provider: p,
                claude: c.claude,
                codex: c.codex,
                all_markdown: true,
                context: Some(context.into()),
                ..Default::default()
            },
            &CancellationToken::default(),
            |_| {},
        )
    }
    fn scan(&self) -> Inventory {
        self.inv(Provider::Claude, &self.a)
    }
    fn store(&self) -> ChangeStore {
        ChangeStore::new(self.config.clone(), self.base.join("recovery")).unwrap()
    }
    fn owner(&self, i: &Inventory) -> String {
        i.repositories
            .checkouts
            .iter()
            .find(|c| c.path.as_path() == self.a)
            .unwrap()
            .id
            .clone()
    }
}
fn find(i: &Inventory, p: impl AsRef<Path>) -> &Artifact {
    i.artifacts
        .iter()
        .find(|a| a.path.as_path() == p.as_ref())
        .unwrap_or_else(|| {
            panic!(
                "missing {:?}: {:?}",
                p.as_ref(),
                i.artifacts.iter().map(|a| &a.path).collect::<Vec<_>>()
            )
        })
}
#[test]
fn native_roots_provenance_packages_and_provider_isolation() {
    let f = Fixture::new();
    write(f.a.join("CLAUDE.md"), "API");
    write(f.a.join(".claude/CLAUDE.md"), "Combined");
    write(f.a.join("CLAUDE.local.md"), "Local");
    write(f.b.join("CLAUDE.md"), "Web");
    write(
        f.a.join("App/.claude/rules/api.md"),
        "---\npaths: ['src/**']\n---\nRule",
    );
    write(
        f.a.join("App/.claude/skills/review/SKILL.md"),
        "Optional frontmatter.",
    );
    write(
        f.a.join("App/.claude/skills/review/agents/openai.yaml"),
        "not Claude metadata",
    );
    write(f.home.join("skills/personal/SKILL.md"), "Personal");
    write(f.home.join("skills/synced/cloud/SKILL.md"), "Synced");
    write(f.home.join("skills/.trash/old/SKILL.md"), "Ignore");
    write(
        f.home.join("plugins/cache/x/1/skills/plugin/SKILL.md"),
        "Installed",
    );
    write(f.home.join(".credentials.json"), "Never inventory");
    write(f.base.join("managed/CLAUDE.md"), "Managed");
    write(
        f.a.join(".codex/agents/reviewer.toml"),
        "name='reviewer'\ndescription='review'\ndeveloper_instructions='inspect'",
    );
    write(f.a.join("AGENTS.md"), "Codex");
    write(f.home.join("agents/reviewer.md"), agent("reviewer"));
    let i = f.inv(Provider::Claude, &f.a.join("App"));
    assert_eq!(i.status, ScanStatus::Complete);
    assert_eq!(i.provider, "claude");
    assert_eq!(find(&i, f.a.join("App/.claude/rules/api.md")).kind, K::Rule);
    assert_eq!(
        find(&i, f.a.join("App/.claude/skills/review/agents/openai.yaml")).kind,
        K::SupportingFile
    );
    let personal = find(&i, f.home.join("skills/personal/SKILL.md"));
    assert_eq!(personal.provenance, P::Personal);
    assert!(personal.owner.checkout_id.is_some());
    assert!(personal.scope_checkout_id.is_none());
    for (p, provenance) in [
        (f.home.join("skills/synced/cloud/SKILL.md"), P::Synced),
        (
            f.home.join("plugins/cache/x/1/skills/plugin/SKILL.md"),
            P::InstalledPlugin,
        ),
        (f.base.join("managed/CLAUDE.md"), P::Managed),
    ] {
        let a = find(&i, p);
        assert_eq!(a.provenance, provenance);
        assert!(a.read_only_reason.is_some());
    }
    assert!(
        i.artifacts
            .iter()
            .all(|a| !a.path.to_string().contains(".trash")
                && !a.path.to_string().contains(".credentials")
                && !a.path.to_string().contains(".codex")
                && !a.path.as_path().ends_with("AGENTS.md"))
    );
    let scope = i.scope.as_ref().unwrap();
    assert_eq!(scope.instruction_byte_limit, None);
    assert!(
        !scope
            .instructions
            .iter()
            .any(|d| d.artifact_id == find(&i, f.b.join("CLAUDE.md")).id)
    );
    assert!(scope.instructions.iter().any(|d| d.artifact_id
        == find(&i, f.a.join("App/.claude/rules/api.md")).id
        && d.bytes_included == 0
        && d.reason.contains("Conditional")));
    let codex = f.inv(Provider::Codex, &f.a);
    assert!(
        codex
            .artifacts
            .iter()
            .all(|a| !a.path.as_path().starts_with(&f.home))
    );
    assert!(
        f.store()
            .prepare(
                &codex,
                Request::Create {
                    owner_id: f.owner(&codex),
                    path: PathBuf::from("personal-root/agents/bypass.md").into(),
                    kind: FileKind::Markdown,
                    text: "Wrong native validation".into()
                }
            )
            .is_err()
    );
    assert_eq!(
        find(&codex, f.a.join(".codex/agents/reviewer.toml")).kind,
        K::Agent
    );
    assert!(codex.artifacts.iter().all(
        |a| !a.path.to_string().contains(".claude") && !a.path.as_path().ends_with("CLAUDE.md")
    ));
}
#[test]
fn structural_validation_optional_skill_headers_required_agents_and_unknown_keys() {
    let p = ClaudeProvider;
    assert_eq!(p.parse(K::Skill, b"# Plain skill\n").validation, V::Valid);
    for bytes in [
        b"---\nname: broken\n".as_slice(),
        b"---\nname: a\n---\n",
        b"---\nname: -a\ndescription: test\n---\n",
        b"---\nname: bad:name\ndescription: test\n---\n",
    ] {
        assert_eq!(p.parse(K::Agent, bytes).validation, V::Malformed);
    }
    let a = p.parse(K::Agent, agent("reviewer").as_bytes());
    assert_eq!(a.validation, V::Unsupported);
    assert_eq!(a.metadata["future_field"], json!({"keep":true}));
    assert_eq!(
        p.parse(K::Rule, b"---\npaths: {invalid: true}\n---\n")
            .validation,
        V::Malformed
    );
    assert_eq!(
        p.parse(
            K::Skill,
            b"---\ndisable-model-invocation: YES\nuser-invocable: 0\n---\n"
        )
        .validation,
        V::Valid
    );
    assert_eq!(
        p.parse(K::ProviderConfig, b"{\"permissions\":{},}")
            .validation,
        V::Malformed
    );
    for kind in [
        K::Instruction,
        K::Rule,
        K::Skill,
        K::Agent,
        K::LegacyCommand,
    ] {
        let t = p.template(kind, "example", "Purpose.").unwrap();
        assert_eq!(p.parse(kind, t.content.as_bytes()).validation, V::Valid);
        assert_eq!(t.provider, "claude");
    }
}
#[test]
fn imports_are_literal_bounded_and_ignore_code_without_executing_dynamic_content() {
    let f = Fixture::new();
    write(f.a.join("docs/a%20#?.md"), "Reference\n@second.md");
    write(f.a.join("docs/second.md"), "Second\n@a%20#?.md");
    write(f.base.join("outside.md"), "Outside");
    let text = format!(
        "@docs/a%20#?.md\n@missing.md\n@{}\n`@inline.md`\n```md\n@fenced.md\n```\n!`touch NEVER`\n@${{DYNAMIC}}/file.md\n",
        f.base.join("outside.md").display()
    );
    write(f.a.join("CLAUDE.md"), &text);
    let i = f.scan();
    let a = find(&i, f.a.join("CLAUDE.md"));
    assert_eq!(a.references.len(), 3);
    assert_eq!(a.references[0].status, R::Resolved);
    assert_eq!(a.references[1].status, R::Missing);
    assert_eq!(a.references[2].status, R::OutsideRoots);
    assert_eq!(find(&i, f.a.join("docs/a%20#?.md")).kind, K::Reference);
    assert_eq!(
        find(&i, f.a.join("docs/second.md")).references[0].status,
        R::Resolved
    );
    assert!(!f.a.join("NEVER").exists());
    assert_eq!(fs::read_to_string(f.a.join("CLAUDE.md")).unwrap(), text);
    assert_eq!(i.scope.as_ref().unwrap().instructions.len(), 1);
}
#[test]
fn native_agent_edit_preserves_bom_crlf_unknown_metadata_and_restores() {
    let f = Fixture::new();
    let path = f.a.join(".claude/agents/reviewer.md");
    let original = format!("\u{feff}{}", agent("reviewer").replace('\n', "\r\n"));
    write(&path, &original);
    let i = f.scan();
    let a = find(&i, &path);
    let p = f
        .store()
        .prepare(
            &i,
            Request::Replace {
                artifact_id: a.id.clone(),
                text: agent("reviewer").replace("Original body.", "Edited body."),
            },
        )
        .unwrap();
    assert_eq!(fs::read_to_string(&path).unwrap(), original);
    assert_eq!(f.store().apply(&p.id).unwrap().status, Status::Completed);
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        original.replace("Original body.", "Edited body.")
    );
    assert_eq!(f.store().restore(&p.id).unwrap().status, Status::Restored);
    assert_eq!(fs::read_to_string(&path).unwrap(), original);
    let i = f.scan();
    assert!(
        f.store()
            .prepare(
                &i,
                Request::Replace {
                    artifact_id: find(&i, &path).id.clone(),
                    text: "name = 'wrong format'".into()
                }
            )
            .is_err()
    );
}
#[test]
fn synced_package_copy_is_independent_and_reserved_or_foreign_destinations_are_rejected() {
    let f = Fixture::new();
    let path = f.home.join("skills/synced/cloud/SKILL.md");
    write(&path, "# Cloud skill\n");
    write(path.parent().unwrap().join("assets/blob.bin"), [0, 1, 255]);
    fs::create_dir(path.parent().unwrap().join("empty")).unwrap();
    let i = f.scan();
    let a = find(&i, &path);
    assert!(
        f.store()
            .prepare(
                &i,
                Request::Replace {
                    artifact_id: a.id.clone(),
                    text: "edit".into()
                }
            )
            .is_err()
    );
    for dest in [
        ".claude/skills/synced/copy",
        ".claude/skills/SYNCED/copy",
        ".agents/skills/copy",
    ] {
        assert!(
            f.store()
                .prepare(
                    &i,
                    Request::Duplicate {
                        artifact_id: a.id.clone(),
                        owner_id: f.owner(&i),
                        path: PathBuf::from(dest).into()
                    }
                )
                .is_err(),
            "{dest}"
        );
    }
    let dest = f.a.join(".claude/skills/independent");
    let p = f
        .store()
        .prepare(
            &i,
            Request::Duplicate {
                artifact_id: a.id.clone(),
                owner_id: f.owner(&i),
                path: PathBuf::from(".claude/skills/independent").into(),
            },
        )
        .unwrap();
    assert_eq!(f.store().apply(&p.id).unwrap().status, Status::Completed);
    assert_eq!(
        fs::read(dest.join("assets/blob.bin")).unwrap(),
        vec![0, 1, 255]
    );
    assert!(dest.join("empty").is_dir());
    write(&path, "Synced external update");
    assert_eq!(
        fs::read_to_string(dest.join("SKILL.md")).unwrap(),
        "# Cloud skill\n"
    );
    let i = f.scan();
    assert!(find(&i, dest.join("SKILL.md")).read_only_reason.is_none());
    assert_eq!(f.store().restore(&p.id).unwrap().status, Status::Restored);
    assert!(!dest.exists());
    assert_eq!(fs::read_to_string(path).unwrap(), "Synced external update");
}
#[test]
fn same_name_precedence_uses_native_identity_and_keeps_nested_candidates() {
    let f = Fixture::new();
    for p in [
        f.a.join(".claude/skills/deploy/SKILL.md"),
        f.home.join("skills/deploy/SKILL.md"),
        f.a.join("App/.claude/skills/deploy/SKILL.md"),
    ] {
        write(p, "---\nname: Display name\n---\nBody");
    }
    for p in [
        f.a.join(".claude/skills/local/SKILL.md"),
        f.a.join("App/.claude/skills/local/SKILL.md"),
    ] {
        write(p, "Body");
    }
    write(f.home.join("commands/local.md"), "Legacy");
    write(f.a.join(".claude/agents/project.md"), agent("reviewer"));
    write(f.home.join("agents/personal.md"), agent("reviewer"));
    let i = f.inv(Provider::Claude, &f.a.join("App"));
    let scope = i.scope.as_ref().unwrap();
    let state = |p: PathBuf| {
        scope
            .skills
            .iter()
            .chain(&scope.agents)
            .find(|s| s.artifact_id == find(&i, &p).id)
            .unwrap()
            .state
            .as_str()
    };
    assert_eq!(
        state(f.a.join(".claude/skills/deploy/SKILL.md")),
        "shadowed_candidate"
    );
    assert_eq!(state(f.home.join("skills/deploy/SKILL.md")), "candidate");
    assert_eq!(
        state(f.a.join(".claude/skills/local/SKILL.md")),
        "candidate"
    );
    assert_eq!(
        state(f.a.join("App/.claude/skills/local/SKILL.md")),
        "candidate"
    );
    assert_eq!(
        state(f.home.join("commands/local.md")),
        "shadowed_candidate"
    );
    assert_eq!(state(f.a.join(".claude/agents/project.md")), "candidate");
    assert_eq!(
        state(f.home.join("agents/personal.md")),
        "shadowed_candidate"
    );
    assert!(
        i.diagnostics
            .iter()
            .any(|d| d.code == "claude_name_overlap")
    );
    assert!(!i.diagnostics.iter().any(|d| d.code == "duplicate_name"));
}
#[test]
fn drafts_keep_provider_and_old_codex_drafts_default_without_conversion() {
    let f = Fixture::new();
    let path = f.a.join(".claude/agents/reviewer.md");
    write(&path, agent("reviewer"));
    write(f.a.join("AGENTS.md"), "Codex");
    let i = f.scan();
    let d = f
        .store()
        .open_draft(
            &i,
            DraftTarget::Edit {
                artifact_id: find(&i, path).id.clone(),
            },
            None,
        )
        .unwrap();
    assert_eq!(d.provider, Provider::Claude);
    let d = f
        .store()
        .save_draft(
            &d.id,
            d.revision,
            agent("reviewer").replace("Original", "Revised"),
        )
        .unwrap();
    let ci = f.inv(Provider::Codex, &f.a);
    assert!(f.store().prepare_draft(&ci, &d.id, d.revision).is_err());
    assert!(f.store().prepare_draft(&i, &d.id, d.revision).is_ok());
    let cd = f
        .store()
        .open_draft(
            &ci,
            DraftTarget::Edit {
                artifact_id: find(&ci, f.a.join("AGENTS.md")).id.clone(),
            },
            None,
        )
        .unwrap();
    let stored = f.store().path().join(&cd.id).join("draft.json");
    let mut v: serde_json::Value = serde_json::from_slice(&fs::read(&stored).unwrap()).unwrap();
    v["draft"].as_object_mut().unwrap().remove("provider");
    v["baseline"].as_object_mut().unwrap().remove("provider");
    v["baseline"]["config"]
        .as_object_mut()
        .unwrap()
        .remove("claude");
    write(&stored, serde_json::to_vec(&v).unwrap());
    assert_eq!(f.store().draft(&cd.id).unwrap().provider, Provider::Codex);
}
#[test]
fn native_creation_validates_kind_and_retains_unknown_fields() {
    let f = Fixture::new();
    let i = f.scan();
    for (path, kind, text) in [
        (
            ".claude/rules/api.md",
            FileKind::Rule,
            "---\npaths: ['src/**']\nfuture: keep\n---\nRule",
        ),
        (
            ".claude/commands/task.md",
            FileKind::LegacyCommand,
            "# Legacy task",
        ),
        ("CLAUDE.md", FileKind::Instruction, "Project"),
    ] {
        let p = f
            .store()
            .prepare(
                &f.scan(),
                Request::Create {
                    owner_id: f.owner(&i),
                    path: PathBuf::from(path).into(),
                    kind,
                    text: text.into(),
                },
            )
            .unwrap();
        assert_eq!(f.store().apply(&p.id).unwrap().status, Status::Completed);
        assert_eq!(fs::read_to_string(f.a.join(path)).unwrap(), text);
    }
    for path in [
        ".claude/agents/invalid.md",
        ".codex/agents/wrong.md",
        ".claude/skills/synced/new.md",
    ] {
        assert!(
            f.store()
                .prepare(
                    &f.scan(),
                    Request::Create {
                        owner_id: f.owner(&i),
                        path: PathBuf::from(path).into(),
                        kind: FileKind::Markdown,
                        text: "Not an agent".into()
                    }
                )
                .is_err()
        );
    }
}

#[test]
fn nested_agents_support_create_duplicate_and_rename_without_losing_native_validation() {
    let f = Fixture::new();
    let path = ".claude/agents/review/security.md";
    let i = f.scan();
    let p = f
        .store()
        .prepare(
            &i,
            Request::Create {
                owner_id: f.owner(&i),
                path: PathBuf::from(path).into(),
                kind: FileKind::Agent,
                text: agent("security"),
            },
        )
        .unwrap();
    assert_eq!(f.store().apply(&p.id).unwrap().status, Status::Completed);
    let i = f.scan();
    let source = find(&i, f.a.join(path));
    assert_eq!(source.kind, K::Agent);
    let copy = ".claude/agents/review/copied.md";
    let p = f
        .store()
        .prepare(
            &i,
            Request::Duplicate {
                artifact_id: source.id.clone(),
                owner_id: f.owner(&i),
                path: PathBuf::from(copy).into(),
            },
        )
        .unwrap();
    assert_eq!(f.store().apply(&p.id).unwrap().status, Status::Completed);
    let i = f.scan();
    let p = f
        .store()
        .prepare(
            &i,
            Request::Rename {
                artifact_id: find(&i, f.a.join(copy)).id.clone(),
                path: PathBuf::from(".claude/agents/review/deep/renamed.md").into(),
                repair_links: vec![],
            },
        )
        .unwrap();
    assert_eq!(f.store().apply(&p.id).unwrap().status, Status::Completed);
    assert_eq!(
        fs::read_to_string(f.a.join(".claude/agents/review/deep/renamed.md")).unwrap(),
        agent("security")
    );
    for (path, kind, text) in [
        (
            ".claude/agents/review/invalid.md",
            FileKind::Agent,
            "No frontmatter",
        ),
        (
            ".claude/agents/review/wrong.md",
            FileKind::Markdown,
            "Bypass",
        ),
        (
            "docs/agents/review/wrong.md",
            FileKind::Agent,
            "---\nname: wrong\ndescription: Wrong location\n---\nBody",
        ),
    ] {
        let i = f.scan();
        assert!(
            f.store()
                .prepare(
                    &i,
                    Request::Create {
                        owner_id: f.owner(&i),
                        path: PathBuf::from(path).into(),
                        kind,
                        text: text.into()
                    }
                )
                .is_err()
        );
    }
}

#[test]
fn foreign_configured_roots_cannot_be_edited_or_created_through_another_provider() {
    let f = Fixture::new();
    let mut config: serde_json::Value = serde_json::to_value(f.config.load().unwrap()).unwrap();
    for (provider, field, root) in [
        ("codex", "user_skills", "custom-user"),
        ("codex", "admin_skills", "custom-admin"),
        ("claude", "managed", "custom-managed"),
    ] {
        config[provider][field] = json!(f.a.join(root));
    }
    config["codex"]["extra_roots"] =
        json!([{"path":f.a.join("custom-installed"),"provenance":"installed_plugin"}]);
    config["claude"]["extra_roots"] =
        json!([{"path":f.a.join("custom-synced"),"provenance":"synced"}]);
    write(f.config.path(), serde_json::to_vec(&config).unwrap());
    let skill = "---\nname: example\ndescription: Example skill.\n---\nOriginal\n";
    for root in [
        "custom-user",
        "custom-admin",
        "custom-managed",
        "custom-installed",
        "custom-synced",
    ] {
        write(f.a.join(root).join("example/SKILL.md"), skill);
    }
    let links = [
        "custom-user",
        "custom-admin",
        "custom-managed",
        "custom-installed",
        "custom-synced",
    ]
    .iter()
    .map(|root| format!("[reference]({root}/example/SKILL.md)\n"))
    .collect::<String>();
    write(f.a.join("AGENTS.md"), &links);
    write(f.a.join("CLAUDE.md"), &links);
    for (provider, roots) in [
        (
            Provider::Claude,
            vec!["custom-user", "custom-admin", "custom-installed"],
        ),
        (Provider::Codex, vec!["custom-managed", "custom-synced"]),
    ] {
        let i = f.inv(provider, &f.a);
        for root in roots {
            let source = f.a.join(root).join("example/SKILL.md");
            if let Some(a) = i.artifacts.iter().find(|a| a.path.as_path() == source) {
                assert!(
                    a.read_only_reason.is_some(),
                    "{} became editable",
                    source.display()
                );
                assert!(
                    f.store()
                        .prepare(
                            &i,
                            Request::Replace {
                                artifact_id: a.id.clone(),
                                text: "Bypass".into()
                            }
                        )
                        .is_err()
                );
            }
            assert!(
                f.store()
                    .prepare(
                        &i,
                        Request::Create {
                            owner_id: f.owner(&i),
                            path: PathBuf::from(root).join("new.md").into(),
                            kind: FileKind::Markdown,
                            text: "Bypass".into(),
                        }
                    )
                    .is_err(),
                "{} permitted new foreign content",
                root
            );
            let original = if provider == Provider::Codex {
                "AGENTS.md"
            } else {
                "CLAUDE.md"
            };
            assert!(
                f.store()
                    .prepare(
                        &i,
                        Request::Duplicate {
                            artifact_id: find(&i, f.a.join(original)).id.clone(),
                            owner_id: f.owner(&i),
                            path: PathBuf::from(root).join(original).into(),
                        }
                    )
                    .is_err()
            );
            assert_eq!(fs::read_to_string(source).unwrap(), skill);
        }
    }
}

#[test]
fn whole_packages_cannot_mutate_nested_foreign_sources_or_use_stale_foreign_settings() {
    let f = Fixture::new();
    let outer = f.a.join(".claude/skills/outer");
    let foreign = outer.join("installed");
    write(outer.join("SKILL.md"), "Outer Claude skill");
    write(
        foreign.join("review/SKILL.md"),
        "---\nname: review\ndescription: Installed Codex skill.\n---\nOriginal\n",
    );
    let stale = f.scan();
    let mut c = f.config.load().unwrap();
    c.codex.extra_roots.push(artifacts::AdditionalRoot {
        path: foreign.into(),
        provenance: P::InstalledPlugin,
    });
    write(f.config.path(), serde_json::to_vec(&c).unwrap());
    assert!(
        f.store()
            .prepare(
                &stale,
                Request::Delete {
                    artifact_id: find(&stale, outer.join("SKILL.md")).id.clone()
                }
            )
            .is_err()
    );
    let i = f.scan();
    let id = find(&i, outer.join("SKILL.md")).id.clone();
    for request in [
        Request::Delete {
            artifact_id: id.clone(),
        },
        Request::Rename {
            artifact_id: id.clone(),
            path: PathBuf::from(".claude/skills/moved").into(),
            repair_links: vec![],
        },
    ] {
        assert!(f.store().prepare(&i, request).is_err());
    }
    assert!(outer.join("installed/review/SKILL.md").is_file());
}
