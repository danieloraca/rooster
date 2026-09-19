//! A real child-process exit after a filesystem step, before its journal update.
use rooster_core::{
    CancellationToken, ConfigStore,
    artifacts::{self, InventoryOptions},
    changes::{ChangeStore, DraftTarget, FaultPoint, NewFile, Request},
};
use serde_json::{Value, json};
use std::{fs, path::PathBuf, process::Command};

#[test]
#[ignore = "invoked only by the restart regression with isolated fixture arguments"]
fn interrupted_writer_child() {
    let config = ConfigStore::new(std::env::var_os("ROOSTER_TEST_CONFIG").unwrap()).unwrap();
    let data = PathBuf::from(std::env::var_os("ROOSTER_TEST_DATA").unwrap());
    let id = std::env::var("ROOSTER_TEST_CHANGE").unwrap();
    let store = ChangeStore::new(config, data)
        .unwrap()
        .with_fault_injector(|point| {
            if matches!(point, FaultPoint::AfterChange(_)) {
                std::process::exit(73);
            }
            Ok(())
        });
    store.apply(&id).unwrap();
    panic!("the writer must stop during apply");
}

#[test]
fn fresh_cli_recovers_an_abruptly_exited_writer_and_private_draft() {
    for finish in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let base = temp.path().canonicalize().unwrap();
        let repo = base.join("Team checkout ü");
        fs::create_dir(&repo).unwrap();
        assert!(
            Command::new("git")
                .args(["init", "--quiet", "--template=", "--initial-branch=main"])
                .arg(&repo)
                .status()
                .unwrap()
                .success()
        );
        fs::write(repo.join("AGENTS.md"), "Shared guidance\n").unwrap();
        let config = ConfigStore::new(base.join("private/config.json")).unwrap();
        config.add("Pilot", &repo).unwrap();
        let mut c = config.load().unwrap();
        c.codex.include_default_roots = false;
        c.claude.include_default_roots = false;
        fs::write(config.path(), serde_json::to_vec(&c).unwrap()).unwrap();
        let inv = artifacts::inventory(
            &c.roots(None).unwrap(),
            &InventoryOptions {
                codex: c.codex,
                claude: c.claude,
                ..Default::default()
            },
            &CancellationToken::default(),
            |_| {},
        );
        let data = base.join("private/recovery");
        let store = ChangeStore::new(config.clone(), &data).unwrap();
        let artifact = inv
            .artifacts
            .iter()
            .find(|a| a.path.as_path() == repo.join("AGENTS.md"))
            .unwrap();
        let draft = store
            .open_draft(
                &inv,
                DraftTarget::Edit {
                    artifact_id: artifact.id.clone(),
                },
                None,
            )
            .unwrap();
        let draft = store
            .save_draft(
                &draft.id,
                draft.revision,
                "Unpublished personal draft\n".into(),
            )
            .unwrap();
        let p = store
            .prepare(
                &inv,
                Request::CreatePackage {
                    owner_id: inv.repositories.checkouts[0].id.clone(),
                    path: PathBuf::from(".agents/skills/pilot").into(),
                    files: vec![
                        NewFile {
                            path: PathBuf::from("SKILL.md").into(),
                            bytes: b"---\nname: pilot\ndescription: Pilot guidance.\n---\nShared\n"
                                .to_vec(),
                        },
                        NewFile {
                            path: PathBuf::from("references/check.md").into(),
                            bytes: b"Check\n".to_vec(),
                        },
                        NewFile {
                            path: PathBuf::from("assets/data.bin").into(),
                            bytes: vec![0, 255, 17],
                        },
                    ],
                },
            )
            .unwrap();
        let out = Command::new(std::env::current_exe().unwrap())
            .args(["--ignored", "--exact", "interrupted_writer_child"])
            .env("ROOSTER_TEST_CONFIG", config.path())
            .env("ROOSTER_TEST_DATA", &data)
            .env("ROOSTER_TEST_CHANGE", &p.id)
            .output()
            .unwrap();
        assert_eq!(
            out.status.code(),
            Some(73),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let record: Value =
            serde_json::from_slice(&fs::read(data.join(&p.id).join("record.json")).unwrap())
                .unwrap();
        assert_eq!(record["status"], "applying");
        let cli = |action: &str| {
            let out = Command::new(env!("CARGO_BIN_EXE_rooster"))
                .arg("--config")
                .arg(config.path())
                .args(["changes", "--data-dir"])
                .arg(&data)
                .args([action, &p.id])
                .output()
                .unwrap();
            assert!(
                out.status.success(),
                "{}",
                String::from_utf8_lossy(&out.stderr)
            );
            serde_json::from_slice::<Value>(&out.stdout).unwrap()
        };
        if finish {
            assert_eq!(cli("apply")["status"], json!("completed"));
            assert_eq!(
                fs::read(repo.join(".agents/skills/pilot/assets/data.bin")).unwrap(),
                [0, 255, 17]
            );
            assert_eq!(
                fs::read_to_string(repo.join(".agents/skills/pilot/references/check.md")).unwrap(),
                "Check\n"
            );
        }
        assert_eq!(cli("restore")["status"], json!("restored"));
        assert!(!repo.join(".agents").exists());
        assert_eq!(
            fs::read_to_string(repo.join("AGENTS.md")).unwrap(),
            "Shared guidance\n"
        );
        assert_eq!(
            ChangeStore::new(config, data)
                .unwrap()
                .draft(&draft.id)
                .unwrap()
                .text,
            "Unpublished personal draft\n"
        );
        let paths: Vec<_> = fs::read_dir(&repo)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(paths.len(), 2, "only .git and shared guidance remain");
    }
}
