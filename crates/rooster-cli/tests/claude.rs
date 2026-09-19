use serde_json::{Value, json};
use std::{
    fs,
    path::Path,
    process::{Command, Output},
};
fn cmd(config: &Path, args: &[&str]) -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_rooster"));
    c.arg("--config").arg(config).args(args);
    c
}
fn value(o: Output, code: i32) -> Value {
    assert_eq!(
        o.status.code(),
        Some(code),
        "{}",
        String::from_utf8_lossy(&o.stderr)
    );
    serde_json::from_slice(&o.stdout).unwrap()
}
#[test]
fn cli_claude_roots_environment_templates_checks_and_mutations() {
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
    let config = base.join("config.json");
    let store = rooster_core::ConfigStore::new(&config).unwrap();
    store.add("Test", &repo).unwrap();
    let home = base.join("custom home");
    fs::create_dir_all(home.join("agents")).unwrap();
    let original = "---\nname: reviewer\ndescription: Review.\n---\nOriginal\n";
    let path = home.join("agents/reviewer.md");
    fs::write(&path, original).unwrap();
    let mut c = store.load().unwrap();
    c.codex.include_default_roots = false;
    c.claude.include_default_roots = false;
    c.claude.home = Some(home.clone().into());
    fs::write(&config, serde_json::to_vec(&c).unwrap()).unwrap();
    let list = value(
        cmd(
            &config,
            &["artifacts", "list", "--provider", "claude", "--json"],
        )
        .output()
        .unwrap(),
        0,
    );
    assert_eq!(list["provider"], "claude");
    let id = list["artifacts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["kind"] == "agent")
        .unwrap()["id"]
        .as_str()
        .unwrap();
    let shown = value(
        cmd(
            &config,
            &["artifacts", "show", id, "--provider", "claude", "--json"],
        )
        .output()
        .unwrap(),
        0,
    );
    assert_eq!(shown["snapshot"]["text"], original);
    let template = value(
        cmd(
            &config,
            &[
                "artifacts",
                "template",
                "agent",
                "--provider",
                "claude",
                "--json",
            ],
        )
        .output()
        .unwrap(),
        0,
    );
    assert_eq!(template["suggested_filename"], "example.md");
    assert!(
        cmd(
            &config,
            &[
                "artifacts",
                "list",
                "--provider",
                "codex",
                "--claude-home",
                home.to_str().unwrap()
            ]
        )
        .output()
        .unwrap()
        .status
        .code()
        .unwrap()
            != 0
    );
    let data = base.join("recovery");
    let source = base.join("edited.md");
    fs::write(&source, original.replace("Original", "Revised")).unwrap();
    let p = value(
        cmd(
            &config,
            &[
                "changes",
                "--provider",
                "claude",
                "--data-dir",
                data.to_str().unwrap(),
                "edit",
                id,
                "--source",
                source.to_str().unwrap(),
            ],
        )
        .output()
        .unwrap(),
        0,
    );
    let change = p["id"].as_str().unwrap();
    assert_eq!(
        value(
            cmd(
                &config,
                &[
                    "changes",
                    "--data-dir",
                    data.to_str().unwrap(),
                    "apply",
                    change
                ]
            )
            .output()
            .unwrap(),
            0
        )["status"],
        "completed"
    );
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        original.replace("Original", "Revised")
    );
    assert_eq!(
        value(
            cmd(
                &config,
                &[
                    "changes",
                    "--data-dir",
                    data.to_str().unwrap(),
                    "restore",
                    change
                ]
            )
            .output()
            .unwrap(),
            0
        )["status"],
        "restored"
    );
    fs::write(&path, "---\nname: invalid\n---\n").unwrap();
    value(
        cmd(&config, &["check", "--provider", "claude", "--json"])
            .output()
            .unwrap(),
        3,
    );
    // Process-local environment override must discover only the explicitly supplied home.
    let empty = base.join("empty.json");
    fs::write(&empty,serde_json::to_vec(&json!({"schema_version":1,"next_id":1,"workspaces":[],"excluded_dirs":[],"codex":{"include_default_roots":false}})).unwrap()).unwrap();
    let inv = value(
        cmd(
            &empty,
            &["artifacts", "list", "--provider", "claude", "--json"],
        )
        .env("CLAUDE_CONFIG_DIR", &home)
        .output()
        .unwrap(),
        0,
    );
    assert!(
        inv["artifacts"]
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["path"] == path.to_str().unwrap())
    );
}
