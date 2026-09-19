use rooster_core::{
    CancellationToken, NativePath, Root, ScanStatus,
    artifacts::{
        AdditionalRoot, ArtifactKind, CodexSettings, Inventory, InventoryOptions, Provenance,
        ReferenceStatus, Validation, inventory,
    },
    providers::{ArtifactProvider, codex::CodexProvider},
};
use serde_json::json;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::Command,
};
use tempfile::{TempDir, tempdir};

fn write(path: impl AsRef<Path>, content: impl AsRef<[u8]>) {
    fs::create_dir_all(path.as_ref().parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
}

fn repo(path: &Path) {
    fs::create_dir_all(path).unwrap();
    let mut command = Command::new("git");
    for (key, _) in std::env::vars_os() {
        if key.to_string_lossy().starts_with("GIT_") {
            command.env_remove(key);
        }
    }
    let output = command
        .args(["init", "--quiet", "--template=", "--initial-branch=main"])
        .arg(path)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn root(path: &Path) -> Root {
    Root {
        id: "test-root".into(),
        path: path.to_path_buf().into(),
    }
}

fn skill(path: &Path, name: &str, body: &str) {
    write(
        path.join("SKILL.md"),
        format!("---\nname: {name}\ndescription: Fixture workflow.\n---\n\n{body}\n"),
    );
}

fn options() -> InventoryOptions {
    InventoryOptions {
        codex: CodexSettings {
            include_default_roots: false,
            ..CodexSettings::default()
        },
        ..InventoryOptions::default()
    }
}

fn scan(path: &Path, options: &InventoryOptions) -> Inventory {
    inventory(
        &[root(path)],
        options,
        &CancellationToken::default(),
        |_| {},
    )
}

fn find<'a>(result: &'a Inventory, path: &Path) -> &'a rooster_core::artifacts::Artifact {
    result
        .artifacts
        .iter()
        .find(|a| a.path.as_path() == path)
        .unwrap_or_else(|| {
            panic!(
                "missing {path:?}: {:?}",
                result.artifacts.iter().map(|a| &a.path).collect::<Vec<_>>()
            )
        })
}

fn snapshot(path: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut files = BTreeMap::new();
    let mut pending = vec![path.to_path_buf()];
    while let Some(path) = pending.pop() {
        for entry in fs::read_dir(path).unwrap() {
            let entry = entry.unwrap();
            let kind = entry.file_type().unwrap();
            if kind.is_dir() {
                pending.push(entry.path());
            } else if kind.is_file() {
                files.insert(entry.path(), fs::read(entry.path()).unwrap());
            }
        }
    }
    files
}

struct Fixture {
    _temp: TempDir,
    base: PathBuf,
    projects: PathBuf,
    api: PathBuf,
    web: PathBuf,
    home: PathBuf,
    personal: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let temp = tempdir().unwrap();
        let base = temp.path().canonicalize().unwrap();
        let projects = base.join("repos — 東京");
        let api = projects.join("API");
        let web = projects.join("Web");
        let home = base.join("independent codex home");
        let personal = base.join("independent skills");
        repo(&api);
        repo(&web);
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(&personal).unwrap();
        Self {
            _temp: temp,
            base,
            projects,
            api,
            web,
            home,
            personal,
        }
    }
    fn options(&self) -> InventoryOptions {
        InventoryOptions {
            codex: CodexSettings {
                include_default_roots: false,
                home: Some(self.home.clone().into()),
                user_skills: Some(self.personal.clone().into()),
                ..CodexSettings::default()
            },
            ..InventoryOptions::default()
        }
    }
}

#[test]
fn nested_app_inventory_groups_packages_preserves_bytes_and_isolates_scope() {
    let f = Fixture::new();
    write(f.home.join("AGENTS.md"), "Personal defaults.");
    write(f.home.join("AGENTS.override.md"), "");
    write(
        f.home.join("config.toml"),
        "project_doc_fallback_filenames = ['TEAM.md']\n",
    );
    write(f.home.join("auth.json"), "{secret: 'must not be indexed'}");
    write(f.api.join("AGENTS.md"), "API conventions.");
    write(f.api.join("App/AGENTS.md"), "Shadowed app conventions.");
    write(
        f.api.join("App/AGENTS.override.md"),
        "Selected app conventions.",
    );
    write(f.api.join("App/service/TEAM.md"), "Service fallback.");
    write(f.api.join("README.md"), "Ordinary docs.");
    write(f.api.join("ignored.md"), "Ignored ordinary docs.");
    write(f.api.join(".gitignore"), "ignored.md\n.agents/\n");
    write(f.web.join("AGENTS.md"), "Separate Web instructions.");
    let package = f.api.join("App/.agents/skills/review");
    let source = b"\xef\xbb\xbf---\r\nname: review\r\ndescription: Review API code.\r\nfuture_field: {keep: true}\r\n---\r\n\r\n[Commands](references/commands.md)\r\n[Missing](references/missing.md)\r\n[Remote](https://example.invalid/no-fetch)\r\n";
    write(package.join("SKILL.md"), source);
    write(
        package.join("references/commands.md"),
        "[Back](../SKILL.md)\n\n~~~md\n[Not a link](not-real.md)\n~~~\n",
    );
    write(
        package.join("agents/openai.yaml"),
        "interface:\n  display_name: Review\n  icon_small: ./assets/icon.svg\npolicy:\n  allow_implicit_invocation: false\n",
    );
    write(package.join("assets/icon.svg"), "<svg/>");
    write(
        package.join("scripts/run.sh"),
        "#!/bin/sh\ntouch NEVER_EXECUTE\n",
    );
    skill(
        &f.api.join(".agents/skills/also-review"),
        "review",
        "Root review.",
    );
    skill(
        &f.web.join(".agents/skills/review"),
        "review",
        "Web review.",
    );
    skill(
        &f.personal.join("personal"),
        "personal-helper",
        "Personal workflow.",
    );
    write(
        f.api.join(".codex/agents/reviewer.toml"),
        "name = 'reviewer'\ndescription = 'Review changes.'\ndeveloper_instructions = 'PRIVATE_PROMPT_BODY'\nfuture_setting = { value = 17 }\n",
    );
    write(
        f.api.join(".codex/agents/broken.toml"),
        "name = 'broken'\ndescription = 'Missing prompt.'\n",
    );
    write(
        f.api.join(".codex/config.toml"),
        "[agents.legacy]\ndescription = 'Legacy role.'\nconfig_file = 'roles/legacy.toml'\n",
    );
    write(
        f.api.join(".codex/roles/legacy.toml"),
        "model = 'provider-selected-model'\n",
    );
    let nested = f.api.join("App/nested");
    repo(&nested);
    write(nested.join("AGENTS.md"), "Nested checkout ownership.");
    let before = snapshot(&f.base);
    let mut opts = f.options();
    opts.context = Some(f.api.join("App/service"));
    let result = scan(&f.projects, &opts);
    assert_eq!(
        result.status,
        ScanStatus::Complete,
        "{:?}",
        result.diagnostics
    );
    assert_eq!(result.packages.len(), 4);
    let s = find(&result, &package.join("SKILL.md"));
    assert_eq!(s.kind, ArtifactKind::Skill);
    assert_eq!(s.validation, Validation::Unsupported);
    assert_eq!(s.unknown_fields, ["future_field"]);
    let view = result.inspect(&s.id).unwrap();
    assert_eq!(view.snapshot.unwrap().bytes, source);
    assert_eq!(view.snapshot.unwrap().info.newline, "crlf");
    assert!(view.snapshot.unwrap().info.has_bom);
    assert_eq!(view.package.unwrap().member_ids.len(), 5);
    assert_eq!(view.metadata["future_field"]["keep"], true);
    assert_eq!(
        find(&result, &package.join("agents/openai.yaml")).kind,
        ArtifactKind::SkillMetadata
    );
    assert_eq!(
        find(&result, &f.api.join(".codex/agents/broken.toml")).validation,
        Validation::Malformed
    );
    assert_eq!(
        find(&result, &f.api.join(".codex/roles/legacy.toml")).kind,
        ArtifactKind::LegacyAgent
    );
    assert_eq!(
        find(&result, &f.api.join(".codex/roles/legacy.toml"))
            .name
            .as_deref(),
        Some("legacy")
    );
    assert_eq!(
        find(&result, &f.api.join(".agents/skills/also-review/SKILL.md")).ignored,
        Some(true)
    );
    assert!(
        result
            .diagnostics
            .iter()
            .any(|d| d.code == "missing_reference" && d.artifact_id.as_deref() == Some(&s.id))
    );
    assert!(
        result
            .diagnostics
            .iter()
            .any(|d| d.code == "duplicate_identity")
    );
    assert!(
        !result
            .diagnostics
            .iter()
            .any(|d| d.message.contains("not-real"))
    );
    assert!(
        s.references
            .iter()
            .any(|r| r.status == ReferenceStatus::Remote)
    );
    assert!(s.references.iter().any(|r| r.target_id.is_some()));
    assert!(
        !result
            .artifacts
            .iter()
            .any(|a| a.path.as_path() == f.api.join("README.md")
                || a.path.as_path() == f.home.join("auth.json"))
    );
    let nested_instruction = find(&result, &nested.join("AGENTS.md"));
    assert_ne!(
        nested_instruction.owner.id,
        find(&result, &f.api.join("AGENTS.md")).owner.id
    );
    let scope = result.scope.as_ref().unwrap();
    let selected: Vec<_> = scope
        .instructions
        .iter()
        .filter(|i| i.bytes_included > 0)
        .map(|i| {
            result
                .inspect(&i.artifact_id)
                .unwrap()
                .artifact
                .path
                .as_path()
                .to_path_buf()
        })
        .collect();
    assert_eq!(
        selected,
        [
            f.home.join("AGENTS.md"),
            f.api.join("AGENTS.md"),
            f.api.join("App/AGENTS.override.md"),
            f.api.join("App/service/TEAM.md")
        ]
    );
    let availability = scope.skills.iter().find(|a| a.artifact_id == s.id).unwrap();
    assert_eq!(availability.state, "candidate");
    assert_eq!(availability.implicit_invocation, Some(false));
    let web_skill = find(&result, &f.web.join(".agents/skills/review/SKILL.md"));
    assert_eq!(
        scope
            .skills
            .iter()
            .find(|a| a.artifact_id == web_skill.id)
            .unwrap()
            .state,
        "out_of_scope"
    );
    let json = serde_json::to_string(&result).unwrap();
    assert!(!json.contains("PRIVATE_PROMPT_BODY"));
    assert_eq!(snapshot(&f.base), before);
    opts.all_markdown = true;
    let all = scan(&f.projects, &opts);
    assert_eq!(
        find(&all, &f.api.join("README.md")).kind,
        ArtifactKind::Markdown
    );
    assert!(
        !all.artifacts
            .iter()
            .any(|a| a.path.as_path() == f.api.join("ignored.md"))
    );
}

#[test]
fn installed_system_and_compatibility_sources_are_read_only_and_not_assumed_active() {
    let f = Fixture::new();
    let system = f.home.join("skills/.system/builtin");
    let compatibility = f.base.join("compatibility");
    let old = f.home.join("plugins/cache/package/v1");
    let new = f.home.join("plugins/cache/package/v2");
    skill(&system, "builtin", "System skill.");
    skill(
        &compatibility.join("legacy"),
        "legacy-skill",
        "Compatibility source.",
    );
    for plugin in [&old, &new] {
        skill(
            &plugin.join("skills/tool"),
            "plugin-tool",
            "Installed workflow.",
        );
        write(plugin.join(".codex-plugin/plugin.json"), json!({"name":"fixture-plugin","version":plugin.file_name().unwrap().to_str().unwrap()}).to_string());
    }
    let mut opts = f.options();
    opts.codex.extra_roots.push(AdditionalRoot {
        path: compatibility.clone().into(),
        provenance: Provenance::Compatibility,
    });
    opts.context = Some(f.api.clone());
    let result = scan(&f.projects, &opts);
    assert_eq!(
        result.status,
        ScanStatus::Complete,
        "{:?}",
        result.diagnostics
    );
    let system = find(&result, &system.join("SKILL.md"));
    assert_eq!(system.provenance, Provenance::System);
    assert!(system.read_only_reason.is_some());
    assert_eq!(
        find(&result, &compatibility.join("legacy/SKILL.md")).provenance,
        Provenance::Compatibility
    );
    let installed: Vec<_> = result
        .artifacts
        .iter()
        .filter(|a| a.kind == ArtifactKind::Skill && a.provenance == Provenance::InstalledPlugin)
        .collect();
    assert_eq!(installed.len(), 2);
    assert!(installed.iter().all(|a| a.read_only_reason.is_some()));
    for item in installed {
        assert_eq!(
            result
                .scope
                .as_ref()
                .unwrap()
                .skills
                .iter()
                .find(|a| a.artifact_id == item.id)
                .unwrap()
                .state,
            "unknown"
        );
    }
    assert_eq!(
        result
            .artifacts
            .iter()
            .filter(|a| a.kind == ArtifactKind::PluginManifest)
            .count(),
        2
    );
}

#[test]
fn context_budget_and_disabled_skill_settings_are_estimates() {
    let f = Fixture::new();
    write(f.home.join("AGENTS.md"), "Global");
    write(
        f.api.join("AGENTS.md"),
        "Repository instructions are longer than the budget.",
    );
    let package = f.api.join(".agents/skills/disabled");
    skill(&package, "disabled", "No implicit activation.");
    write(
        f.home.join("config.toml"),
        format!(
            "project_doc_max_bytes = 12\n[[skills.config]]\npath = {}\nenabled = false\n",
            serde_json::to_string(&package.to_string_lossy()).unwrap()
        ),
    );
    let mut opts = f.options();
    opts.context = Some(f.api.clone());
    let result = scan(&f.projects, &opts);
    let scope = result.scope.as_ref().unwrap();
    assert_eq!(scope.instruction_byte_limit, Some(12));
    assert_eq!(
        scope
            .instructions
            .iter()
            .map(|i| i.bytes_included)
            .collect::<Vec<_>>(),
        [6, 4]
    );
    assert!(scope.instructions[1].reason.contains("truncated"));
    assert_eq!(scope.skills[0].state, "disabled_by_config");
    assert!(scope.unknowns.iter().any(|u| u.contains("trust")));
    assert!(scope.unknowns.iter().any(|u| u.contains("running")));
}

#[cfg(unix)]
#[test]
fn registered_skill_links_work_but_external_links_cycles_and_excluded_content_do_not_leak() {
    use std::os::unix::fs::symlink;
    let f = Fixture::new();
    let target = f.projects.join("shared source");
    skill(&target, "linked", "[Shared](references/shared.md)");
    write(target.join("references/shared.md"), "Registered source.");
    let external = f.base.join("outside");
    skill(&external, "outside-secret", "Must not be read.");
    fs::create_dir_all(f.api.join(".agents/skills")).unwrap();
    symlink(&target, f.api.join(".agents/skills/linked")).unwrap();
    symlink(&external, f.api.join(".agents/skills/external")).unwrap();
    symlink(
        f.api.join(".agents/skills"),
        f.api.join(".agents/skills/cycle"),
    )
    .unwrap();
    write(
        f.api.join("AGENTS.md"),
        format!(
            "[Secret]({})\n[Excluded](node_modules/dependency/AGENTS.md)\n[Cross repo](../Web/reference.md)\n",
            external.join("SKILL.md").display()
        ),
    );
    write(
        f.api.join("node_modules/dependency/AGENTS.md"),
        "Excluded body.",
    );
    write(f.web.join("reference.md"), "Other repository reference.");
    let result = scan(&f.projects, &f.options());
    assert_eq!(
        result.status,
        ScanStatus::Complete,
        "{:?}",
        result.diagnostics
    );
    let linked = find(&result, &f.api.join(".agents/skills/linked/SKILL.md"));
    assert_eq!(linked.physical_path.as_path(), target.join("SKILL.md"));
    assert!(linked.read_only_reason.is_some());
    assert!(result.linked_sources.iter().any(|link| link.inspected));
    assert!(result.linked_sources.iter().any(|link| !link.inspected));
    assert!(
        !result
            .artifacts
            .iter()
            .any(|a| a.name.as_deref() == Some("outside-secret"))
    );
    assert!(
        !result
            .artifacts
            .iter()
            .any(|a| a.path.as_path().starts_with(f.api.join("node_modules")))
    );
    let guidance = find(&result, &f.api.join("AGENTS.md"));
    assert!(
        guidance
            .references
            .iter()
            .any(|r| r.status == ReferenceStatus::OutsideRoots)
    );
    assert!(
        guidance
            .references
            .iter()
            .any(|r| r.status == ReferenceStatus::Excluded)
    );
    let cross = find(&result, &f.web.join("reference.md"));
    assert_ne!(cross.owner.id, guidance.owner.id);
}

#[test]
fn templates_metadata_failures_and_unknown_features_keep_distinct_outcomes() {
    for kind in [
        ArtifactKind::Instruction,
        ArtifactKind::Skill,
        ArtifactKind::Agent,
    ] {
        let template = CodexProvider
            .template(kind, "fixture", "When testing a local setup.")
            .unwrap();
        let parsed = CodexProvider.parse(kind, template.content.as_bytes());
        assert_eq!(parsed.validation, Validation::Valid, "{:?}", parsed.issues);
    }
    assert!(
        CodexProvider
            .template(ArtifactKind::Agent, "../bad", "test")
            .is_err()
    );
    let invalid = CodexProvider.parse(
        ArtifactKind::Skill,
        b"---\nname: [broken\ndescription: bad\n---\n",
    );
    assert_eq!(invalid.validation, Validation::Malformed);
    let unknown = CodexProvider.parse(
        ArtifactKind::Skill,
        b"---\nname: valid\ndescription: text\nfuture: true\n---\n",
    );
    assert_eq!(unknown.validation, Validation::Unsupported);
    assert_eq!(unknown.metadata["future"], true);
    let include = CodexProvider.parse(
        ArtifactKind::Skill,
        b"---\nname: valid\ndescription: !include /private/secret\n---\n",
    );
    assert_eq!(
        include.validation,
        Validation::Unsupported,
        "{:?}",
        include.issues
    );
    let invalid_policy = CodexProvider.parse(
        ArtifactKind::SkillMetadata,
        b"policy:\n  allow_implicit_invocation: sometimes\n",
    );
    assert_eq!(invalid_policy.validation, Validation::Malformed);
    let unknown_agent = CodexProvider.parse(ArtifactKind::Agent, b"name = 'metadata-name'\ndescription = 'd'\ndeveloper_instructions = 'p'\nfuture = { retained = 9 }\n");
    assert_eq!(unknown_agent.name.as_deref(), Some("metadata-name"));
    assert_eq!(unknown_agent.validation, Validation::Unsupported);
    assert_eq!(unknown_agent.metadata["future"]["retained"], 9);
}

#[test]
fn missing_roots_read_limits_cancellation_and_native_snapshot_bytes_are_explicit() {
    let f = Fixture::new();
    write(f.api.join("AGENTS.md"), "Guidance longer than the limit.");
    let mut opts = f.options();
    opts.max_file_bytes = 8;
    let limited = scan(&f.projects, &opts);
    assert_eq!(limited.status, ScanStatus::Partial);
    assert_eq!(
        find(&limited, &f.api.join("AGENTS.md")).validation,
        Validation::Unavailable
    );
    opts = f.options();
    opts.codex.user_skills = Some(f.base.join("missing").into());
    assert_eq!(scan(&f.projects, &opts).status, ScanStatus::Partial);
    let token = CancellationToken::default();
    let cancelled = inventory(&[root(&f.projects)], &f.options(), &token, |p| {
        if p.phase == "artifacts" {
            token.cancel();
        }
    });
    assert_eq!(cancelled.status, ScanStatus::Cancelled);
    let binary = f.api.join(".agents/skills/binary");
    skill(&binary, "binary", "See the asset.");
    write(binary.join("assets/data.bin"), [0, 255, 7, 128]);
    let result = scan(&f.projects, &f.options());
    let asset = find(&result, &binary.join("assets/data.bin"));
    let view = result.inspect(&asset.id).unwrap();
    assert_eq!(view.snapshot.unwrap().bytes, [0, 255, 7, 128]);
    assert!(view.snapshot.unwrap().text.is_none());
    assert!(result.inspect("unknown-id").is_none());
    let empty = inventory(&[], &options(), &CancellationToken::default(), |_| {});
    assert_eq!(empty.status, ScanStatus::Complete);
    assert!(empty.artifacts.is_empty());
}

#[test]
fn references_decode_paths_and_do_not_treat_code_as_links() {
    let f = Fixture::new();
    write(
        f.api.join("AGENTS.md"),
        "[Reference][ref]\n\n[ref]: <docs/with%20space.md#heading>\n\n~~~sh\n[Not link](missing.md)\n~~~\n",
    );
    write(
        f.api.join("docs/with space.md"),
        "# Heading\n[Back](../AGENTS.md)\n",
    );
    let result = scan(&f.projects, &f.options());
    assert_eq!(result.status, ScanStatus::Complete);
    let artifact = find(&result, &f.api.join("AGENTS.md"));
    assert_eq!(artifact.references.len(), 1);
    assert_eq!(artifact.references[0].status, ReferenceStatus::Resolved);
    assert!(!artifact.references[0].fragment_checked);
    assert!(artifact.references[0].target_id.is_some());
}

#[test]
fn old_settings_load_and_new_provider_roots_roundtrip_without_source_writes() {
    let f = Fixture::new();
    let path = f.base.join("rooster.json");
    write(
        &path,
        r#"{"schema_version":1,"next_id":1,"workspaces":[],"excluded_dirs":[]}"#,
    );
    let store = rooster_core::ConfigStore::new(&path).unwrap();
    assert!(store.load().unwrap().codex.include_default_roots);
    let mut config = store.load().unwrap();
    config.codex.home = Some(NativePath::from(f.home));
    config.codex.include_default_roots = false;
    write(&path, serde_json::to_vec(&config).unwrap());
    store.add("Repos", &f.projects).unwrap();
    assert_eq!(store.load().unwrap().codex, config.codex);
}

#[test]
fn relocated_codex_home_inside_a_checkout_retains_personal_scope_and_physical_owner() {
    let f = Fixture::new();
    let home = f.api.join(".codex");
    write(
        home.join("AGENTS.md"),
        "Global guidance from a relocated Codex home.",
    );
    write(
        home.join("agents/global.toml"),
        "name='global-agent'\ndescription='Global role.'\ndeveloper_instructions='Fixture.'\n",
    );
    let mut opts = f.options();
    opts.codex.home = Some(home.clone().into());
    opts.context = Some(f.web.clone());
    let result = scan(&f.projects, &opts);
    let guidance = find(&result, &home.join("AGENTS.md"));
    assert_eq!(guidance.provenance, Provenance::Personal);
    assert!(guidance.owner.checkout_id.is_some());
    assert!(guidance.scope_checkout_id.is_none());
    let scope = result.scope.as_ref().unwrap();
    assert!(
        scope
            .instructions
            .iter()
            .any(|i| i.artifact_id == guidance.id && i.bytes_included > 0)
    );
    let agent = find(&result, &home.join("agents/global.toml"));
    assert_eq!(
        scope
            .agents
            .iter()
            .find(|a| a.artifact_id == agent.id)
            .unwrap()
            .state,
        "candidate"
    );
}

#[test]
fn unknown_policy_shared_role_layers_and_remote_credentials_are_not_flattened() {
    let f = Fixture::new();
    let package = f.api.join(".agents/skills/policy");
    skill(
        &package,
        "policy",
        "[Remote](https://person:password@example.invalid/page?token=secret)",
    );
    write(
        package.join("agents/openai.yaml"),
        "policy:\n  allow_implicit_invocation: sometimes\n",
    );
    write(
        f.api.join(".codex/config.toml"),
        "[agents.first]\nconfig_file = 'shared.toml'\n[agents.second]\nconfig_file = 'shared.toml'\n",
    );
    write(
        f.api.join(".codex/shared.toml"),
        "model = 'configured-model'\n",
    );
    let mut opts = f.options();
    opts.context = Some(f.api.clone());
    let result = scan(&f.projects, &opts);
    let scope = result.scope.as_ref().unwrap();
    assert_eq!(scope.skills[0].implicit_invocation, None);
    let role = find(&result, &f.api.join(".codex/shared.toml"));
    assert_eq!(role.declared_role_names, ["first", "second"]);
    assert!(role.name.is_none());
    assert_eq!(
        scope
            .agents
            .iter()
            .find(|a| a.artifact_id == role.id)
            .unwrap()
            .state,
        "unknown"
    );
    let json = serde_json::to_string(&result).unwrap();
    assert!(!json.contains("password"));
    assert!(!json.contains("token=secret"));
    let source = find(&result, &package.join("SKILL.md"));
    assert!(
        result
            .inspect(&source.id)
            .unwrap()
            .snapshot
            .unwrap()
            .text
            .as_ref()
            .unwrap()
            .contains("token=secret")
    );
}

#[test]
fn referenced_files_keep_their_own_source_classification() {
    let f = Fixture::new();
    let installed = f.base.join("installed");
    write(installed.join("guide.txt"), "Installed content.");
    write(f.api.join("local.txt"), "Repository content.");
    write(
        f.api.join("AGENTS.md"),
        format!("[Installed]({})", installed.join("guide.txt").display()),
    );
    write(
        installed.join("AGENTS.md"),
        format!("[Repository](<{}>)", f.api.join("local.txt").display()),
    );
    let mut opts = f.options();
    opts.codex.extra_roots.push(AdditionalRoot {
        path: installed.clone().into(),
        provenance: Provenance::InstalledPlugin,
    });
    let result = scan(&f.projects, &opts);
    let target = find(&result, &installed.join("guide.txt"));
    assert_eq!(target.provenance, Provenance::InstalledPlugin);
    assert_eq!(target.source_root.as_path(), installed);
    assert!(target.read_only_reason.is_some());
    let local = find(&result, &f.api.join("local.txt"));
    assert_eq!(local.provenance, Provenance::Repository);
    assert_eq!(local.source_root.as_path(), f.api);
    assert!(local.read_only_reason.is_none());
}

#[test]
fn shared_role_names_do_not_hide_multiple_declaration_scopes() {
    let f = Fixture::new();
    let shared = f.projects.join("shared.toml");
    write(&shared, "model = 'fixture'\n");
    for checkout in [&f.api, &f.web] {
        write(
            checkout.join(".codex/config.toml"),
            format!("[agents.reviewer]\nconfig_file = {}\n", json!(shared)),
        );
    }
    let mut opts = f.options();
    opts.context = Some(f.web.clone());
    let result = scan(&f.projects, &opts);
    let artifact = find(&result, &shared);
    assert_eq!(artifact.declared_role_names, ["reviewer"]);
    assert_eq!(artifact.role_declaration_count, 2);
    assert_eq!(
        result
            .scope
            .as_ref()
            .unwrap()
            .agents
            .iter()
            .find(|a| a.artifact_id == artifact.id)
            .unwrap()
            .state,
        "unknown"
    );
}

#[test]
fn fallback_classification_respects_checkout_and_directory_configuration() {
    let f = Fixture::new();
    write(
        f.home.join("config.toml"),
        "project_doc_fallback_filenames = ['GLOBAL.md']\n",
    );
    write(
        f.api.join(".codex/config.toml"),
        "project_doc_fallback_filenames = ['TEAM.md']\n",
    );
    write(f.api.join("TEAM.md"), "Project guidance.");
    write(f.api.join("GLOBAL.md"), "Overridden ordinary document.");
    write(f.web.join("TEAM.md"), "[Ordinary broken link](missing.md)");
    write(f.web.join("GLOBAL.md"), "Personal fallback guidance.");
    write(
        f.web.join("App/.codex/config.toml"),
        "project_doc_fallback_filenames = ['APP.md']\n",
    );
    write(f.web.join("APP.md"), "Ancestor guidance for App contexts.");
    write(f.web.join("Other/APP.md"), "Unrelated directory branch.");
    let result = scan(&f.projects, &f.options());
    assert_eq!(
        find(&result, &f.api.join("TEAM.md")).kind,
        ArtifactKind::Instruction
    );
    assert_eq!(
        find(&result, &f.web.join("GLOBAL.md")).kind,
        ArtifactKind::Instruction
    );
    assert_eq!(
        find(&result, &f.web.join("APP.md")).kind,
        ArtifactKind::Instruction
    );
    for path in [
        f.api.join("GLOBAL.md"),
        f.web.join("TEAM.md"),
        f.web.join("Other/APP.md"),
    ] {
        assert!(
            !result.artifacts.iter().any(|a| a.path.as_path() == path),
            "unexpected guidance: {path:?}"
        );
    }
    assert!(
        !result
            .diagnostics
            .iter()
            .any(|d| d.code == "missing_reference")
    );
}

#[cfg(unix)]
#[test]
fn parent_traversals_cannot_erase_symlinks_in_references_or_legacy_roles() {
    use std::os::unix::fs::symlink;
    let f = Fixture::new();
    fs::create_dir_all(f.api.join("other/nested")).unwrap();
    fs::create_dir_all(f.api.join("plain")).unwrap();
    symlink(f.api.join("other/nested"), f.api.join("link")).unwrap();
    write(
        f.api.join("guide.md"),
        "Wrong for the symlink path; valid for the ordinary path.",
    );
    write(f.api.join("other/guide.md"), "Actual symlink target.");
    write(f.api.join("role.toml"), "model = 'wrong'\n");
    write(f.api.join("other/role.toml"), "model = 'actual'\n");
    write(
        f.api.join("AGENTS.md"),
        "[Linked](link/../guide.md)\n[Ordinary](plain/../guide.md)\n",
    );
    write(
        f.api.join(".codex/config.toml"),
        "[agents.reviewer]\nconfig_file = '../link/../role.toml'\n",
    );
    let result = scan(&f.projects, &f.options());
    let instruction = find(&result, &f.api.join("AGENTS.md"));
    assert_eq!(
        instruction.references[0].status,
        ReferenceStatus::UnsupportedLink
    );
    assert!(instruction.references[0].target_id.is_none());
    assert_eq!(instruction.references[1].status, ReferenceStatus::Resolved);
    assert_eq!(
        instruction.references[1].target.as_ref().unwrap().as_path(),
        f.api.join("guide.md")
    );
    let config = find(&result, &f.api.join(".codex/config.toml"));
    assert_eq!(
        config.references[0].status,
        ReferenceStatus::UnsupportedLink
    );
    assert!(
        !result
            .artifacts
            .iter()
            .any(|a| a.kind == ArtifactKind::LegacyAgent)
    );
    write(f.web.join("guide.md"), "Separately registered checkout.");
    write(f.api.join("AGENTS.md"), "[Sibling](../Web/guide.md)\n");
    let separate = inventory(
        &[root(&f.api), root(&f.web)],
        &f.options(),
        &CancellationToken::default(),
        |_| {},
    );
    let sibling = &find(&separate, &f.api.join("AGENTS.md")).references[0];
    assert_eq!(sibling.status, ReferenceStatus::Resolved);
    assert_eq!(
        sibling.target.as_ref().unwrap().as_path(),
        f.web.join("guide.md")
    );
}

#[cfg(unix)]
#[test]
fn derived_home_links_require_registered_targets_and_preserve_aliases() {
    use std::os::unix::fs::symlink;
    let f = Fixture::new();
    let external = f.base.join("outside");
    write(
        external.join("reviewer.toml"),
        "name='reviewer'\ndescription='Fixture'\ndeveloper_instructions='Fixture'\n",
    );
    symlink(&external, f.home.join("agents")).unwrap();
    let opts = f.options();
    let result = scan(&f.projects, &opts);
    assert!(
        !result
            .artifacts
            .iter()
            .any(|a| a.physical_path.as_path().starts_with(&external))
    );
    let link = result
        .linked_sources
        .iter()
        .find(|l| l.path.as_path() == f.home.join("agents"))
        .unwrap();
    assert!(!link.inspected);
    assert_eq!(link.target.as_ref().unwrap().as_path(), external);
    let registered = inventory(
        &[root(&f.projects), root(&external)],
        &opts,
        &CancellationToken::default(),
        |_| {},
    );
    let alias = find(&registered, &f.home.join("agents/reviewer.toml"));
    assert_eq!(
        alias.physical_path.as_path(),
        external.join("reviewer.toml")
    );
    assert!(alias.read_only_reason.is_some());
    assert!(
        registered
            .linked_sources
            .iter()
            .any(|l| l.path.as_path() == f.home.join("agents") && l.inspected)
    );
    let home_alias = f.base.join("selected-home-alias");
    symlink(&f.home, &home_alias).unwrap();
    let mut selected = opts;
    selected.codex.home = Some(home_alias.into());
    let result = inventory(
        &[root(&external)],
        &selected,
        &CancellationToken::default(),
        |_| {},
    );
    assert_eq!(
        find(&result, &f.home.join("agents/reviewer.toml"))
            .physical_path
            .as_path(),
        external.join("reviewer.toml")
    );
}
