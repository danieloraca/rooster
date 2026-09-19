use rooster_core::{ConfigStore, artifacts::ArtifactKind};
use rooster_desktop::session::{ScanRequest, Session};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::Arc,
    time::{Duration, Instant},
};
fn fixture() -> (tempfile::TempDir, PathBuf, Arc<Session>) {
    let temp = tempfile::tempdir().unwrap();
    let base = temp.path().canonicalize().unwrap();
    let repo = base.join("Project");
    fs::create_dir(&repo).unwrap();
    assert!(
        Command::new("git")
            .args(["init", "--quiet", "--template=", "--initial-branch=main"])
            .arg(&repo)
            .status()
            .unwrap()
            .success()
    );
    let package = repo.join("App/.agents/skills/example");
    fs::create_dir_all(package.join("references")).unwrap();
    fs::write(package.join("SKILL.md"),"---\nname: example\ndescription: Fixture skill.\n---\n# Example\nPrivate searchable needle.\n[Guide](references/guide.md)\n").unwrap();
    fs::write(
        package.join("references/guide.md"),
        "# Guide\nFixture reference.",
    )
    .unwrap();
    fs::write(repo.join("App/AGENTS.md"), "Nested instruction.").unwrap();
    let store = ConfigStore::new(base.join("config.json")).unwrap();
    store.add("Test", &repo).unwrap();
    (temp, repo, Arc::new(Session::new(store)))
}
fn request() -> ScanRequest {
    ScanRequest {
        provider: Default::default(),
        workspace_id: None,
        all_markdown: false,
        include_personal: false,
    }
}
fn finish(session: &Session, ticket: u64) -> u64 {
    let until = Instant::now() + Duration::from_secs(15);
    loop {
        let status = session.status().unwrap();
        if status.completed == ticket {
            return status.generation;
        }
        assert!(Instant::now() < until, "scan did not complete");
        std::thread::sleep(Duration::from_millis(15));
    }
}
#[test]
fn desktop_uses_inventory_ids_contexts_and_snapshots_without_writing_sources() {
    let (_temp, repo, session) = fixture();
    let path = repo.join("App/.agents/skills/example/SKILL.md");
    let before = fs::read(&path).unwrap();
    let generation = finish(&session, session.start(request()).unwrap());
    let inventory = session.inventory(generation).unwrap();
    let skill = inventory
        .artifacts
        .iter()
        .find(|a| a.kind == ArtifactKind::Skill)
        .unwrap();
    let view = session.inspect(generation, &skill.id).unwrap();
    assert!(view.text.unwrap().contains("Private searchable needle"));
    assert_eq!(view.related.len(), 1);
    let summary = serde_json::to_string(&inventory).unwrap();
    assert!(!summary.contains("Private searchable needle"));
    assert_eq!(
        session.search(generation, "searchable needle").unwrap(),
        vec![skill.id.clone()]
    );
    assert!(session.inspect(generation, path.to_str().unwrap()).is_err());
    assert!(session.assess(generation, "../../outside").is_err());
    let context = inventory
        .contexts
        .iter()
        .find(|c| c.label == "App")
        .unwrap();
    let scope = session.assess(generation, &context.id).unwrap();
    assert_eq!(scope.skills[0].state, "candidate");
    finish(&session, session.start(request()).unwrap());
    assert!(session.inspect(generation, &skill.id).is_err());
    assert_eq!(fs::read(path).unwrap(), before);
}
#[test]
fn cancellation_and_registration_changes_have_explicit_outcomes() {
    let (temp, repo, session) = fixture();
    for i in 0..500 {
        fs::create_dir(repo.join(format!("directory-{i}"))).unwrap();
    }
    let ticket = session.start(request()).unwrap();
    session.cancel(ticket).unwrap();
    let generation = finish(&session, ticket);
    assert_eq!(
        session.inventory(generation).unwrap().status,
        rooster_core::ScanStatus::Cancelled
    );
    assert!(session.cancel(ticket).is_err());
    let new = Path::new(temp.path()).join("Other");
    fs::create_dir(&new).unwrap();
    session.register_selection("Other", &new).unwrap();
    assert!(session.inventory(generation).is_err());
    assert!(
        session
            .start(ScanRequest {
                provider: Default::default(),
                workspace_id: Some("not-a-registered-id".into()),
                ..request()
            })
            .is_err()
    );
}
#[test]
fn filesystem_changes_invalidate_snapshots_and_refresh_observes_them() {
    let (_temp, repo, session) = fixture();
    let generation = finish(&session, session.start(request()).unwrap());
    assert!(session.status().unwrap().watch_errors.is_empty());
    fs::create_dir_all(repo.join("storage/logs")).unwrap();
    fs::write(repo.join("storage/logs/application.log"), "runtime output").unwrap();
    std::thread::sleep(Duration::from_millis(250));
    assert!(!session.status().unwrap().invalidated);
    fs::write(repo.join("App/AGENTS.md"), "Changed externally.").unwrap();
    let until = Instant::now() + Duration::from_secs(5);
    while !session.status().unwrap().invalidated {
        assert!(Instant::now() < until, "watcher did not invalidate");
        std::thread::sleep(Duration::from_millis(25));
    }
    let old = session.inventory(generation).unwrap();
    let id = &old
        .artifacts
        .iter()
        .find(|a| a.kind == ArtifactKind::Instruction)
        .unwrap()
        .id;
    assert_eq!(
        session.inspect(generation, id).unwrap().text.as_deref(),
        Some("Nested instruction.")
    );
    let generation = finish(&session, session.start(request()).unwrap());
    assert_eq!(
        session.inspect(generation, id).unwrap().text.as_deref(),
        Some("Changed externally.")
    );
}

#[test]
fn volatile_provider_runtime_files_do_not_invalidate_personal_artifacts() {
    let temp = tempfile::tempdir().unwrap();
    let base = temp.path().canonicalize().unwrap();
    let codex = base.join(".codex");
    fs::create_dir_all(codex.join("agents")).unwrap();
    fs::create_dir_all(codex.join("skills")).unwrap();
    fs::create_dir_all(codex.join("plugins")).unwrap();
    fs::create_dir_all(codex.join(".tmp")).unwrap();
    fs::write(codex.join("config.toml"), "model = \"fixture\"\n").unwrap();
    fs::write(
        base.join("rooster.json"),
        format!(
            "{{\"schema_version\":1,\"next_id\":1,\"workspaces\":[],\"excluded_dirs\":[],\"codex\":{{\"include_default_roots\":false,\"home\":{},\"user_skills\":null,\"admin_skills\":null,\"extra_roots\":[]}},\"claude\":{{\"include_default_roots\":false,\"home\":null,\"managed\":null,\"extra_roots\":[]}}}}",
            serde_json::to_string(&codex).unwrap()
        ),
    )
    .unwrap();
    let session = Arc::new(Session::new(
        ConfigStore::new(base.join("rooster.json")).unwrap(),
    ));
    let generation = finish(
        &session,
        session
            .start(ScanRequest {
                include_personal: true,
                ..request()
            })
            .unwrap(),
    );
    assert!(session.inventory(generation).is_ok());

    fs::write(codex.join(".tmp/runtime-state"), "changed").unwrap();
    std::thread::sleep(Duration::from_millis(250));
    assert!(!session.status().unwrap().invalidated);

    fs::write(codex.join("config.toml"), "model = \"changed\"\n").unwrap();
    let until = Instant::now() + Duration::from_secs(5);
    while !session.status().unwrap().invalidated {
        assert!(Instant::now() < until, "config change did not invalidate");
        std::thread::sleep(Duration::from_millis(25));
    }
}

#[test]
fn removing_a_location_invalidates_the_snapshot_and_refreshes_without_it() {
    let (_temp, repo, session) = fixture();
    let settings = session.settings().unwrap();
    let workspace = &settings.workspaces[0];
    let generation = finish(&session, session.start(request()).unwrap());
    let inventory = session.inventory(generation).unwrap();
    let artifact_id = inventory.artifacts[0].id.clone();
    let guidance = repo.join("App/AGENTS.md");
    let original = fs::read(&guidance).unwrap();
    assert!(session.remove_location("unknown").is_err());
    assert!(session.inventory(generation).is_ok());

    assert!(
        session
            .remove_location(&workspace.roots[0].id)
            .unwrap()
            .workspaces
            .is_empty()
    );
    assert!(session.status().unwrap().invalidated);
    assert!(session.inspect(generation, &artifact_id).is_err());
    assert!(
        session
            .start(ScanRequest {
                workspace_id: Some(workspace.id.clone()),
                ..request()
            })
            .is_err()
    );
    let generation = finish(&session, session.start(request()).unwrap());
    let refreshed = session.inventory(generation).unwrap();
    assert!(refreshed.repositories.checkouts.is_empty());
    assert!(refreshed.artifacts.is_empty());
    assert_eq!(fs::read(guidance).unwrap(), original);
    assert!(repo.join(".git").is_dir());
}
