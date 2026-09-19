use serde_json::Value;
use std::{
    ffi::OsStr,
    fs,
    path::Path,
    process::{Command, Output},
};
use tempfile::tempdir;

fn run(config: &Path, args: &[&OsStr]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_rooster"))
        .arg("--config")
        .arg(config)
        .args(args)
        .output()
        .unwrap()
}

fn value(output: &Output, code: i32) -> Value {
    assert_eq!(
        output.status.code(),
        Some(code),
        "stderr: {} stdout: {}",
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );
    assert!(
        output.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn list_show_and_check_use_real_core_results_without_ambient_provider_roots() {
    let temp = tempdir().unwrap();
    let project = temp.path().join("project");
    let config = temp.path().join("rooster.json");
    fs::create_dir(&project).unwrap();
    let initialized = Command::new("git")
        .args(["init", "--quiet", "--template=", "--initial-branch=main"])
        .arg(&project)
        .output()
        .unwrap();
    assert!(initialized.status.success());
    let package = project.join("App/.agents/skills/example");
    fs::create_dir_all(package.join("references")).unwrap();
    fs::write(package.join("SKILL.md"), "---\nname: example\ndescription: Fixture skill.\nfuture: {preserved: 7}\n---\n\nPRIVATE_SOURCE_BODY\n[Guide](references/guide.md)\n[Missing](references/missing.md)\n").unwrap();
    fs::write(package.join("references/guide.md"), "Companion reference.").unwrap();
    let registered = run(
        &config,
        &[
            "workspace".as_ref(),
            "add".as_ref(),
            "Test".as_ref(),
            project.as_os_str(),
        ],
    );
    assert!(registered.status.success());
    let listed = run(
        &config,
        &[
            "artifacts".as_ref(),
            "list".as_ref(),
            "--no-default-roots".as_ref(),
            "--context".as_ref(),
            project.join("App").as_os_str(),
            "--json".as_ref(),
        ],
    );
    let result = value(&listed, 0);
    assert!(!String::from_utf8_lossy(&listed.stdout).contains("PRIVATE_SOURCE_BODY"));
    assert_eq!(result["packages"].as_array().unwrap().len(), 1);
    assert!(
        result["sources"]
            .as_array()
            .unwrap()
            .iter()
            .all(|s| s["provenance"] == "repository")
    );
    let skill = result["artifacts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["kind"] == "skill")
        .unwrap();
    let id = skill["id"].as_str().unwrap();
    let shown = run(
        &config,
        &[
            "artifacts".as_ref(),
            "show".as_ref(),
            id.as_ref(),
            "--no-default-roots".as_ref(),
            "--json".as_ref(),
        ],
    );
    let view = value(&shown, 0);
    assert_eq!(view["metadata"]["future"]["preserved"], 7);
    assert!(
        view["snapshot"]["text"]
            .as_str()
            .unwrap()
            .contains("PRIVATE_SOURCE_BODY")
    );
    assert_eq!(view["related"].as_array().unwrap().len(), 1);
    let checked = run(
        &config,
        &[
            "check".as_ref(),
            "--no-default-roots".as_ref(),
            "--json".as_ref(),
        ],
    );
    assert!(
        value(&checked, 3)["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d["code"] == "missing_reference")
    );
    fs::write(package.join("references/missing.md"), "Now present.").unwrap();
    let checked = run(
        &config,
        &[
            "check".as_ref(),
            "--no-default-roots".as_ref(),
            "--json".as_ref(),
        ],
    );
    assert_eq!(value(&checked, 0)["status"], "complete");
    fs::remove_file(package.join("SKILL.md")).unwrap();
    let stale = run(
        &config,
        &[
            "artifacts".as_ref(),
            "show".as_ref(),
            id.as_ref(),
            "--no-default-roots".as_ref(),
            "--json".as_ref(),
        ],
    );
    assert_eq!(stale.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&stale.stderr).contains("artifact not found"));
}

#[test]
fn explicit_roots_templates_and_empty_installation_do_not_create_settings() {
    let temp = tempdir().unwrap();
    let config = temp.path().join("not-created/config.json");
    let root = temp.path().join("personal-skills");
    fs::create_dir_all(root.join("test")).unwrap();
    fs::write(
        root.join("test/SKILL.md"),
        "---\nname: test\ndescription: Root override.\n---\n",
    )
    .unwrap();
    let listed = run(
        &config,
        &[
            "artifacts".as_ref(),
            "list".as_ref(),
            "--no-default-roots".as_ref(),
            "--user-skills".as_ref(),
            root.as_os_str(),
            "--json".as_ref(),
        ],
    );
    assert_eq!(value(&listed, 0)["artifacts"].as_array().unwrap().len(), 1);
    assert!(!config.parent().unwrap().exists());
    let template = run(
        &config,
        &[
            "artifacts".as_ref(),
            "template".as_ref(),
            "agent".as_ref(),
            "--name".as_ref(),
            "reviewer".as_ref(),
            "--json".as_ref(),
        ],
    );
    assert!(
        value(&template, 0)["content"]
            .as_str()
            .unwrap()
            .contains("developer_instructions")
    );
    assert!(!config.parent().unwrap().exists());
    let missing = run(
        &config,
        &[
            "artifacts".as_ref(),
            "list".as_ref(),
            "--no-default-roots".as_ref(),
            "--user-skills".as_ref(),
            temp.path().join("missing").as_os_str(),
            "--json".as_ref(),
        ],
    );
    assert_eq!(value(&missing, 2)["status"], "partial");
    let empty = run(
        &config,
        &[
            "check".as_ref(),
            "--no-default-roots".as_ref(),
            "--json".as_ref(),
        ],
    );
    assert!(value(&empty, 0)["artifacts"].as_array().unwrap().is_empty());
}

#[test]
fn check_ignores_other_checkouts_fallback_names() {
    let temp = tempdir().unwrap();
    let base = temp.path().canonicalize().unwrap();
    let config = base.join("rooster.json");
    for name in ["A", "B"] {
        let project = base.join(name);
        fs::create_dir(&project).unwrap();
        assert!(
            Command::new("git")
                .args(["init", "--quiet", "--template=", "--initial-branch=main"])
                .arg(&project)
                .status()
                .unwrap()
                .success()
        );
    }
    fs::create_dir(base.join("A/.codex")).unwrap();
    fs::write(
        base.join("A/.codex/config.toml"),
        "project_doc_fallback_filenames=['TEAM.md']\n",
    )
    .unwrap();
    fs::write(base.join("A/TEAM.md"), "Actual guidance.\n").unwrap();
    fs::write(
        base.join("B/TEAM.md"),
        "[Ordinary broken link](missing.md)\n",
    )
    .unwrap();
    assert!(
        run(
            &config,
            &[
                "workspace".as_ref(),
                "add".as_ref(),
                "Test".as_ref(),
                base.as_os_str()
            ]
        )
        .status
        .success()
    );
    let checked = run(
        &config,
        &[
            "check".as_ref(),
            "--no-default-roots".as_ref(),
            "--json".as_ref(),
            "--context".as_ref(),
            base.join("B").as_os_str(),
        ],
    );
    let report = value(&checked, 0);
    assert_eq!(report["status"], "complete");
    assert!(
        !report["artifacts"]
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["path"] == base.join("B/TEAM.md").to_str().unwrap())
    );
    assert!(
        !report["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d["code"] == "missing_reference")
    );
}
