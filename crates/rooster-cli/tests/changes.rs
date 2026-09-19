use serde_json::{Value, json};
use std::{
    fs,
    path::Path,
    process::{Command, Output},
};
fn command(config: &Path, data: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_rooster"))
        .arg("--config")
        .arg(config)
        .args(["changes", "--data-dir"])
        .arg(data)
        .args(args)
        .output()
        .unwrap()
}
fn value(out: Output, code: i32) -> Value {
    assert_eq!(
        out.status.code(),
        Some(code),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}
fn setup() -> (
    tempfile::TempDir,
    std::path::PathBuf,
    std::path::PathBuf,
    std::path::PathBuf,
) {
    let temp = tempfile::tempdir().unwrap();
    let base = temp.path().canonicalize().unwrap();
    let repo = base.join("repo");
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
    let mut v: Value = serde_json::from_slice(&fs::read(&config).unwrap()).unwrap();
    v["codex"]["include_default_roots"] = json!(false);
    fs::write(&config, serde_json::to_vec(&v).unwrap()).unwrap();
    (temp, repo, config, base.join("recovery"))
}
#[test]
fn cli_prepares_previews_applies_and_restores_using_durable_ids() {
    let (_temp, repo, config, data) = setup();
    let owner = value(command(&config, &data, &["owners"]), 0)[0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let source = config.with_file_name("source.md");
    fs::write(&source, "# Original source\n").unwrap();
    let p = value(
        command(
            &config,
            &data,
            &[
                "create",
                &owner,
                "docs/new.md",
                "--source",
                source.to_str().unwrap(),
            ],
        ),
        0,
    );
    let id = p["id"].as_str().unwrap();
    assert!(!repo.join("docs/new.md").exists());
    let preview = value(command(&config, &data, &["preview", id]), 0);
    assert!(
        preview["files"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f["after_text"] == "# Original source\n")
    );
    assert_eq!(
        value(command(&config, &data, &["apply", id]), 0)["status"],
        "completed"
    );
    assert_eq!(
        fs::read_to_string(repo.join("docs/new.md")).unwrap(),
        "# Original source\n"
    );
    assert_eq!(
        value(command(&config, &data, &["list"]), 0)
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        value(command(&config, &data, &["restore", id]), 0)["status"],
        "restored"
    );
    assert!(!repo.join("docs").exists());
}
#[test]
fn cli_structured_package_preview_and_conflict_do_not_overwrite_sources() {
    let (_temp, repo, config, data) = setup();
    let owner = value(command(&config, &data, &["owners"]), 0)[0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let request = config.with_file_name("request.json");
    fs::write(&request,serde_json::to_vec(&json!({"operation":"create_package","owner_id":owner,"path":".agents/skills/example","files":[{"path":"SKILL.md","bytes":b"---\nname: example\ndescription: Example.\n---\nBody\n".to_vec()}]})).unwrap()).unwrap();
    let p = value(
        command(
            &config,
            &data,
            &["prepare", "--request", request.to_str().unwrap()],
        ),
        0,
    );
    let id = p["id"].as_str().unwrap();
    fs::create_dir_all(repo.join(".agents/skills/example")).unwrap();
    fs::write(
        repo.join(".agents/skills/example/SKILL.md"),
        "External source",
    )
    .unwrap();
    let out = command(&config, &data, &["apply", id]);
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("conflict"));
    assert_eq!(
        fs::read_to_string(repo.join(".agents/skills/example/SKILL.md")).unwrap(),
        "External source"
    );
}
