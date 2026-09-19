use rooster_core::{
    ConfigStore,
    changes::{Draft, DraftTarget},
};
use rooster_desktop::session::{EditingRequest, ScanRequest, Session, StructuralRequest};
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::Arc,
    time::{Duration, Instant},
};
fn setup() -> (tempfile::TempDir, PathBuf, ConfigStore, Arc<Session>) {
    let t = tempfile::tempdir().unwrap();
    let base = t.path().canonicalize().unwrap();
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
    fs::write(repo.join("AGENTS.md"), "original").unwrap();
    let config = ConfigStore::new(base.join("config.json")).unwrap();
    config.add("First", &repo).unwrap();
    let mut c: serde_json::Value =
        serde_json::from_slice(&fs::read(config.path()).unwrap()).unwrap();
    c["codex"] = serde_json::json!({"include_default_roots":false});
    fs::write(config.path(), serde_json::to_vec(&c).unwrap()).unwrap();
    let s = Arc::new(Session::with_data(config.clone(), base.join("recovery")));
    (t, repo, config, s)
}
fn scan(s: &Arc<Session>) -> u64 {
    let w = s.settings().unwrap().workspaces[0].id.clone();
    let ticket = s
        .start(ScanRequest {
            provider: Default::default(),
            workspace_id: Some(w),
            all_markdown: true,
            include_personal: false,
        })
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        let status = s.status().unwrap();
        if status.completed == ticket {
            return status.generation;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(20));
    }
}
#[test]
fn desktop_draft_preview_apply_restore_and_reopen_use_shared_service() {
    let (t, repo, config, s) = setup();
    let extra = t.path().join("extra");
    fs::create_dir(&extra).unwrap();
    config.add("Other", &extra).unwrap();
    let generation = scan(&s);
    let inv = s.inventory(generation).unwrap();
    let id = inv.artifacts[0].id.clone();
    let value = s
        .editing(EditingRequest::Open {
            generation,
            target: DraftTarget::Edit { artifact_id: id },
        })
        .unwrap();
    let d: Draft = serde_json::from_value(value).unwrap();
    let value = s
        .editing(EditingRequest::Save {
            id: d.id.clone(),
            revision: d.revision,
            text: "desktop draft".into(),
        })
        .unwrap();
    let saved: Draft = serde_json::from_value(value).unwrap();
    let reopened = Session::with_data(config, t.path().canonicalize().unwrap().join("recovery"));
    assert_eq!(
        reopened
            .editing(EditingRequest::Read { id: d.id.clone() })
            .unwrap()["text"],
        "desktop draft"
    );
    let preview = s
        .editing(EditingRequest::PrepareDraft {
            id: d.id.clone(),
            revision: saved.revision,
        })
        .unwrap();
    assert_eq!(
        fs::read_to_string(repo.join("AGENTS.md")).unwrap(),
        "original"
    );
    let change = preview["id"].as_str().unwrap().to_string();
    assert_eq!(
        s.editing(EditingRequest::Apply { id: change.clone() })
            .unwrap()["status"],
        "completed"
    );
    assert_eq!(
        fs::read_to_string(repo.join("AGENTS.md")).unwrap(),
        "desktop draft"
    );
    assert_eq!(
        s.editing(EditingRequest::Restore { id: change }).unwrap()["status"],
        "restored"
    );
    assert_eq!(
        fs::read_to_string(repo.join("AGENTS.md")).unwrap(),
        "original"
    );
    assert!(
        s.editing(EditingRequest::Read {
            id: "../../AGENTS.md".into()
        })
        .is_err()
    );
}
#[test]
fn desktop_does_not_refresh_away_a_draft_or_prepare_stale_structural_selection() {
    let (_t, repo, _config, s) = setup();
    let generation = scan(&s);
    let inv = s.inventory(generation).unwrap();
    let id = inv.artifacts[0].id.clone();
    let d: Draft = serde_json::from_value(
        s.editing(EditingRequest::Open {
            generation,
            target: DraftTarget::Edit {
                artifact_id: id.clone(),
            },
        })
        .unwrap(),
    )
    .unwrap();
    let d: Draft = serde_json::from_value(
        s.editing(EditingRequest::Save {
            id: d.id,
            revision: d.revision,
            text: "retained draft".into(),
        })
        .unwrap(),
    )
    .unwrap();
    fs::write(repo.join("AGENTS.md"), "external process").unwrap();
    assert!(
        s.editing(EditingRequest::PrepareDraft {
            id: d.id.clone(),
            revision: d.revision
        })
        .is_err()
    );
    assert!(
        s.editing(EditingRequest::Prepare {
            generation,
            request: StructuralRequest::Delete { artifact_id: id }
        })
        .is_err()
    );
    let comparison = s
        .editing(EditingRequest::Compare { id: d.id.clone() })
        .unwrap();
    assert_eq!(comparison["current_text"], "external process");
    assert_eq!(comparison["draft"]["text"], "retained draft");
    assert_eq!(
        fs::read_to_string(repo.join("AGENTS.md")).unwrap(),
        "external process"
    );
    scan(&s);
    assert_eq!(
        s.editing(EditingRequest::Read { id: d.id }).unwrap()["text"],
        "retained draft"
    );
}

#[test]
fn desktop_switch_keeps_draft_provider_and_native_agent_validation() {
    use rooster_core::providers::Provider;
    let (t, repo, config, s) = setup();
    let mut c = config.load().unwrap();
    c.claude.include_default_roots = false;
    fs::write(config.path(), serde_json::to_vec(&c).unwrap()).unwrap();
    let path = repo.join(".claude/agents/reviewer.md");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let original =
        "---\nname: reviewer\ndescription: Review changes.\nfuture: keep\n---\nOriginal\n";
    fs::write(&path, original).unwrap();
    let ticket = s
        .start(ScanRequest {
            provider: Provider::Claude,
            workspace_id: None,
            all_markdown: true,
            include_personal: false,
        })
        .unwrap();
    let until = Instant::now() + Duration::from_secs(15);
    while s.status().unwrap().completed != ticket {
        assert!(Instant::now() < until);
        std::thread::sleep(Duration::from_millis(20));
    }
    let generation = s.status().unwrap().generation;
    let inventory = s.inventory(generation).unwrap();
    assert_eq!(inventory.provider, Provider::Claude);
    let d: Draft = serde_json::from_value(
        s.editing(EditingRequest::Open {
            generation,
            target: DraftTarget::Edit {
                artifact_id: inventory
                    .artifacts
                    .iter()
                    .find(|a| a.path.as_path() == path)
                    .unwrap()
                    .id
                    .clone(),
            },
        })
        .unwrap(),
    )
    .unwrap();
    assert_eq!(d.provider, Provider::Claude);
    let d: Draft = serde_json::from_value(
        s.editing(EditingRequest::Save {
            id: d.id,
            revision: d.revision,
            text: "name='TOML is not a Claude agent'".into(),
        })
        .unwrap(),
    )
    .unwrap();
    scan(&s); // View now shows Codex; the saved draft still requires Claude validation.
    assert!(
        s.editing(EditingRequest::PrepareDraft {
            id: d.id.clone(),
            revision: d.revision
        })
        .is_err()
    );
    let d: Draft = serde_json::from_value(
        s.editing(EditingRequest::Save {
            id: d.id,
            revision: d.revision,
            text: original.replace("Original", "Changed"),
        })
        .unwrap(),
    )
    .unwrap();
    let reopened = Session::with_data(config, t.path().canonicalize().unwrap().join("recovery"));
    assert_eq!(
        reopened
            .editing(EditingRequest::Read { id: d.id.clone() })
            .unwrap()["provider"],
        "claude"
    );
    let p = reopened
        .editing(EditingRequest::PrepareDraft {
            id: d.id,
            revision: d.revision,
        })
        .unwrap();
    let id = p["id"].as_str().unwrap().to_string();
    assert_eq!(
        reopened
            .editing(EditingRequest::Apply { id: id.clone() })
            .unwrap()["status"],
        "completed"
    );
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        original.replace("Original", "Changed")
    );
    assert_eq!(
        fs::read_to_string(repo.join("AGENTS.md")).unwrap(),
        "original"
    );
    assert_eq!(
        reopened.editing(EditingRequest::Restore { id }).unwrap()["status"],
        "restored"
    );
    assert_eq!(fs::read_to_string(path).unwrap(), original);
}
