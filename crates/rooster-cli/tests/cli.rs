use serde_json::Value;
use std::{
    ffi::OsStr,
    fs,
    path::Path,
    process::{Command, Output},
};
use tempfile::tempdir;

fn invoke(config: &Path, args: &[&OsStr]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_rooster"))
        .arg("--config")
        .arg(config)
        .args(args)
        .output()
        .unwrap()
}

fn json(output: &Output, code: i32) -> Value {
    assert_eq!(
        output.status.code(),
        Some(code),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.stderr.is_empty(),
        "JSON mode polluted stderr: {:?}",
        output.stderr
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn cli_registers_scans_and_relocates_using_only_explicit_settings() {
    let temp = tempdir().unwrap();
    let config = temp.path().join("settings/config.json");
    let empty = invoke(
        &config,
        &["workspace".as_ref(), "list".as_ref(), "--json".as_ref()],
    );
    assert!(json(&empty, 0)["workspaces"].as_array().unwrap().is_empty());
    assert!(!config.parent().unwrap().exists());
    let repository = temp.path().join("my repo — 東京");
    fs::create_dir(&repository).unwrap();
    let initialized = Command::new("git")
        .arg("init")
        .arg("--initial-branch=main")
        .arg("--template=")
        .arg(&repository)
        .output()
        .unwrap();
    assert!(initialized.status.success());
    let output = invoke(
        &config,
        &[
            "workspace".as_ref(),
            "add".as_ref(),
            "Personal".as_ref(),
            repository.as_os_str(),
            "--json".as_ref(),
        ],
    );
    let added = json(&output, 0);
    let root_id = added["root"]["id"].as_str().unwrap();
    let output = invoke(
        &config,
        &[
            "scan".as_ref(),
            "--workspace".as_ref(),
            "Personal".as_ref(),
            "--json".as_ref(),
        ],
    );
    let scanned = json(&output, 0);
    assert_eq!(scanned["status"], "complete");
    assert_eq!(scanned["checkouts"].as_array().unwrap().len(), 1);
    assert_eq!(scanned["checkouts"][0]["head_state"], "unborn");
    let listed = invoke(
        &config,
        &["repos".as_ref(), "list".as_ref(), "--json".as_ref()],
    );
    assert_eq!(json(&listed, 0)["checkouts"], scanned["checkouts"]);
    let moved = temp.path().join("relocated");
    fs::rename(&repository, &moved).unwrap();
    let missing = invoke(&config, &["scan".as_ref(), "--json".as_ref()]);
    assert_eq!(json(&missing, 2)["issues"].as_array().unwrap().len(), 1);
    let output = invoke(
        &config,
        &[
            "workspace".as_ref(),
            "relocate".as_ref(),
            root_id.as_ref(),
            moved.as_os_str(),
            "--json".as_ref(),
        ],
    );
    assert_eq!(json(&output, 0)["root"]["id"], root_id);
    let output = invoke(
        &config,
        &["repos".as_ref(), "list".as_ref(), "--json".as_ref()],
    );
    assert_eq!(json(&output, 0)["checkouts"].as_array().unwrap().len(), 1);
    let unknown = invoke(
        &config,
        &["scan".as_ref(), "--workspace".as_ref(), "unknown".as_ref()],
    );
    assert_eq!(unknown.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&unknown.stderr).contains("workspace not found"));
    let invalid = invoke(
        &config,
        &["scan".as_ref(), "--exclude".as_ref(), "../secret".as_ref()],
    );
    assert_eq!(invalid.status.code(), Some(1));
}

#[test]
fn config_environment_override_and_parse_errors_do_not_write_settings() {
    let temp = tempdir().unwrap();
    let settings = temp.path().join("absent/config.json");
    let output = Command::new(env!("CARGO_BIN_EXE_rooster"))
        .env("ROOSTER_CONFIG", &settings)
        .args(["scan", "--json"])
        .output()
        .unwrap();
    assert_eq!(json(&output, 0)["visited_dirs"], 0);
    assert!(!settings.parent().unwrap().exists());
    let config = temp.path().join("invalid.json");
    fs::write(&config, "invalid").unwrap();
    let output = invoke(
        &config,
        &[
            "workspace".as_ref(),
            "add".as_ref(),
            "Test".as_ref(),
            temp.path().as_os_str(),
        ],
    );
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(fs::read_to_string(&config).unwrap(), "invalid");
}

#[cfg(unix)]
#[test]
fn ctrl_c_emits_cancelled_json_and_exit_130() {
    use std::{
        os::unix::fs::PermissionsExt,
        process::Stdio,
        thread,
        time::{Duration, Instant},
    };
    let temp = tempdir().unwrap();
    let config = temp.path().join("config.json");
    let repository = temp.path().join("repo");
    fs::create_dir_all(repository.join(".git")).unwrap();
    let registered = invoke(
        &config,
        &[
            "workspace".as_ref(),
            "add".as_ref(),
            "Test".as_ref(),
            repository.as_os_str(),
        ],
    );
    assert!(registered.status.success());
    let bin = temp.path().join("bin");
    fs::create_dir(&bin).unwrap();
    // A marker confirms the signal handler is installed and the scanner is waiting on Git.
    let fake = bin.join("git");
    fs::write(
        &fake,
        "#!/bin/sh\nprintf ready > scan-started\nexec /bin/sleep 10\n",
    )
    .unwrap();
    fs::set_permissions(&fake, fs::Permissions::from_mode(0o700)).unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_rooster"))
        .arg("--config")
        .arg(&config)
        .args(["scan", "--json"])
        .env("PATH", &bin)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let start = Instant::now();
    while !repository.join("scan-started").exists() {
        if start.elapsed() > Duration::from_secs(5) {
            let _ = child.kill();
            let output = child.wait_with_output().unwrap();
            panic!(
                "scan did not start: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        thread::sleep(Duration::from_millis(10));
    }
    assert!(
        Command::new("/bin/kill")
            .args(["-INT", &child.id().to_string()])
            .status()
            .unwrap()
            .success()
    );
    let output = child.wait_with_output().unwrap();
    assert_eq!(json(&output, 130)["status"], "cancelled");
    assert!(start.elapsed() < Duration::from_secs(3));
}
