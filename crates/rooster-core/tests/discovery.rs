use rooster_core::{
    CancellationToken, CheckoutKind, HeadState, Root, ScanOptions, ScanStatus, SkipReason, scan,
};
use std::{
    collections::BTreeMap,
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
    process::Command,
};
use tempfile::tempdir;

fn git(directory: &Path, args: &[&OsStr]) {
    let mut command = Command::new("git");
    for (key, _) in std::env::vars_os() {
        if key.to_string_lossy().starts_with("GIT_") {
            command.env_remove(key);
        }
    }
    let output = command
        .current_dir(directory)
        .args(args)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", directory.join("absent-global-config"))
        .env("GIT_AUTHOR_NAME", "Rooster Test")
        .env("GIT_AUTHOR_EMAIL", "test@example.invalid")
        .env("GIT_COMMITTER_NAME", "Rooster Test")
        .env("GIT_COMMITTER_EMAIL", "test@example.invalid")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn repo(path: &Path, commit: bool) {
    fs::create_dir_all(path).unwrap();
    git(
        path,
        &[
            "init".as_ref(),
            "--initial-branch=main".as_ref(),
            "--template=".as_ref(),
        ],
    );
    if commit {
        fs::write(path.join("README.md"), "fixture\n").unwrap();
        git(path, &["add".as_ref(), "README.md".as_ref()]);
        git(
            path,
            &[
                "-c".as_ref(),
                "commit.gpgsign=false".as_ref(),
                "commit".as_ref(),
                "-m".as_ref(),
                "fixture".as_ref(),
            ],
        );
    }
}

fn root(id: &str, path: &Path) -> Root {
    Root {
        id: id.into(),
        path: path.to_path_buf().into(),
    }
}

fn snapshot(directory: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut files = BTreeMap::new();
    let mut pending = vec![directory.to_path_buf()];
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

#[test]
fn discovers_nested_submodule_and_worktrees_without_modifying_repositories() {
    let temp = tempdir().unwrap();
    let base = temp.path().canonicalize().unwrap();
    let parent = base.join("workspace with spaces — 東京");
    let main = parent.join("API");
    let unborn = parent.join("Web");
    let nested = main.join("nested/checkout");
    let detached = parent.join("Detached");
    let source = base.join("submodule-source");
    let external = base.join("external worktree — café");
    for path in [&main, &nested, &detached, &source] {
        repo(path, true);
    }
    repo(&unborn, false);
    git(&detached, &["checkout".as_ref(), "--detach".as_ref()]);
    git(
        &main,
        &[
            "worktree".as_ref(),
            "add".as_ref(),
            "-b".as_ref(),
            "linked".as_ref(),
            external.as_os_str(),
        ],
    );
    git(
        &main,
        &[
            "-c".as_ref(),
            "protocol.file.allow=always".as_ref(),
            "submodule".as_ref(),
            "add".as_ref(),
            source.as_os_str(),
            "modules/local".as_ref(),
        ],
    );
    let submodule = main.join("modules/local");
    let before = snapshot(&base);
    let mut progress = Vec::new();
    let roots = [
        root("parent", &parent),
        root("direct", &main),
        root("duplicate", &parent),
    ];
    let report = scan(
        &roots,
        &ScanOptions::default(),
        &CancellationToken::default(),
        |p| progress.push(p.clone()),
    );
    assert_eq!(report.status, ScanStatus::Complete, "{:?}", report.issues);
    assert_eq!(report.checkouts.len(), 5);
    let checkout = |path: &Path| {
        report
            .checkouts
            .iter()
            .find(|c| c.path.as_path() == path)
            .unwrap()
    };
    assert_eq!(checkout(&main).root_ids, ["direct", "duplicate", "parent"]);
    assert_eq!(
        checkout(&nested).root_ids,
        ["direct", "duplicate", "parent"]
    );
    assert_eq!(checkout(&unborn).head_state, HeadState::Unborn);
    assert_eq!(checkout(&unborn).branch.as_deref(), Some("main"));
    assert!(checkout(&unborn).head.is_none());
    assert_eq!(checkout(&detached).head_state, HeadState::Detached);
    assert!(checkout(&detached).branch.is_none());
    assert_eq!(checkout(&submodule).kind, CheckoutKind::Submodule);
    assert_eq!(
        checkout(&submodule)
            .superproject
            .as_ref()
            .unwrap()
            .as_path(),
        main
    );
    assert_eq!(
        report.worktree_candidates.len(),
        1,
        "{:?}",
        report.worktree_candidates
    );
    assert_eq!(report.worktree_candidates[0].path.as_path(), external);
    assert!(
        progress
            .iter()
            .all(|p| p.current_path.as_path().starts_with(&parent))
    );
    assert!(
        progress
            .windows(2)
            .all(|p| p[1].visited_dirs > p[0].visited_dirs)
    );
    let included = scan(
        &[root("linked", &external)],
        &ScanOptions::default(),
        &CancellationToken::default(),
        |_| {},
    );
    assert_eq!(
        included.status,
        ScanStatus::Complete,
        "{:?}",
        included.issues
    );
    assert_eq!(included.checkouts.len(), 1);
    assert_eq!(included.checkouts[0].kind, CheckoutKind::LinkedWorktree);
    assert_ne!(included.checkouts[0].id, checkout(&main).id);
    assert_eq!(included.checkouts[0].common_dir, checkout(&main).common_dir);
    assert_eq!(included.worktree_candidates[0].path.as_path(), main);
    assert_eq!(
        snapshot(&base),
        before,
        "scan changed repository files or Git metadata"
    );
}

#[test]
fn exclusions_direct_roots_bare_stores_and_partial_failures() {
    let temp = tempdir().unwrap();
    let base = temp.path().canonicalize().unwrap();
    let good = base.join("good");
    let dependency = base.join("node_modules/dependency");
    let excluded = base.join("generated/private");
    repo(&good, false);
    repo(&dependency, false);
    repo(&excluded, false);
    fs::create_dir(base.join("broken")).unwrap();
    fs::write(base.join("broken/.git"), "gitdir: missing-metadata\n").unwrap();
    fs::create_dir(base.join("bare.git")).unwrap();
    git(
        &base.join("bare.git"),
        &["init".as_ref(), "--bare".as_ref(), "--template=".as_ref()],
    );
    let mut options = ScanOptions::default();
    options.excluded_dirs.push("generated".into());
    let report = scan(
        &[
            root("parent", &base),
            root("direct", &dependency),
            root("missing", &base.join("absent")),
        ],
        &options,
        &CancellationToken::default(),
        |_| {},
    );
    assert_eq!(report.status, ScanStatus::Partial);
    assert_eq!(report.issues.len(), 2, "{:?}", report.issues);
    assert_eq!(report.checkouts.len(), 2);
    assert!(report.checkouts.iter().any(|c| c.path.as_path() == good));
    assert!(
        report
            .checkouts
            .iter()
            .any(|c| c.path.as_path() == dependency)
    );
    assert!(
        report
            .skipped
            .iter()
            .any(|p| p.reason == SkipReason::BareRepository)
    );
    assert!(
        report
            .skipped
            .iter()
            .any(|p| p.path.as_path() == base.join("generated"))
    );
    let repeated = scan(
        &[root("direct", &dependency)],
        &options,
        &CancellationToken::default(),
        |_| {},
    );
    assert_eq!(
        repeated.checkouts[0].id,
        report
            .checkouts
            .iter()
            .find(|c| c.path.as_path() == dependency)
            .unwrap()
            .id
    );
}

#[test]
fn cancellation_returns_explicit_partial_inventory() {
    let temp = tempdir().unwrap();
    repo(&temp.path().join("one"), false);
    repo(&temp.path().join("two"), false);
    let token = CancellationToken::default();
    let report = scan(
        &[root("root", temp.path())],
        &ScanOptions::default(),
        &token,
        |p| {
            if p.visited_dirs == 3 {
                token.cancel();
            }
        },
    );
    assert_eq!(report.status, ScanStatus::Cancelled);
    assert_eq!(report.checkouts.len(), 1);
    let report = scan(
        &[root("root", temp.path())],
        &ScanOptions::default(),
        &token,
        |_| panic!("cancelled scan progressed"),
    );
    assert_eq!(report.status, ScanStatus::Cancelled);
    assert_eq!(report.visited_dirs, 0);
}

#[test]
fn separate_git_metadata_and_clones_keep_checkout_ownership() {
    let temp = tempdir().unwrap();
    let base = temp.path().canonicalize().unwrap();
    let original = base.join("original");
    let cloned = base.join("clone");
    repo(&original, true);
    git(
        &base,
        &["clone".as_ref(), original.as_os_str(), cloned.as_os_str()],
    );
    let separate = base.join("separate");
    let metadata = base.join("metadata");
    git(
        &base,
        &[
            "init".as_ref(),
            "--initial-branch=main".as_ref(),
            "--template=".as_ref(),
            "--separate-git-dir".as_ref(),
            metadata.as_os_str(),
            separate.as_os_str(),
        ],
    );
    let report = scan(
        &[
            root("parent", &base),
            root("internal", &original.join(".git/objects")),
        ],
        &ScanOptions::default(),
        &CancellationToken::default(),
        |_| {},
    );
    assert_eq!(report.status, ScanStatus::Complete, "{:?}", report.issues);
    assert_eq!(report.checkouts.len(), 3);
    let get = |path: &Path| {
        report
            .checkouts
            .iter()
            .find(|c| c.path.as_path() == path)
            .unwrap()
    };
    assert_ne!(get(&original).id, get(&cloned).id);
    assert_eq!(get(&original).head, get(&cloned).head);
    assert_eq!(get(&separate).git_dir.as_path(), metadata);
    assert!(
        report.worktree_candidates.is_empty(),
        "{:?}",
        report.worktree_candidates
    );
    assert!(
        report
            .skipped
            .iter()
            .any(|p| p.path.as_path() == metadata && p.reason == SkipReason::GitMetadata)
    );
    assert!(
        report
            .skipped
            .iter()
            .any(|p| p.path.as_path() == original.join(".git/objects")
                && p.reason == SkipReason::GitMetadata)
    );
}

#[cfg(unix)]
#[test]
fn symbolic_links_are_not_traversed_and_native_names_remain_lossless() {
    use std::os::unix::fs::symlink;
    let temp = tempdir().unwrap();
    let base = temp.path().canonicalize().unwrap();
    let odd = base.join("native — café\n");
    repo(&odd, false);
    symlink(&base, base.join("cycle")).unwrap();
    symlink(&odd, base.join("alias")).unwrap();
    let report = scan(
        &[root("root", &base)],
        &ScanOptions::default(),
        &CancellationToken::default(),
        |_| {},
    );
    assert_eq!(report.status, ScanStatus::Complete, "{:?}", report.issues);
    assert_eq!(report.checkouts.len(), 1);
    assert_eq!(report.checkouts[0].path.as_path(), odd);
    assert_eq!(
        report
            .skipped
            .iter()
            .filter(|p| p.reason == SkipReason::SymbolicLink)
            .count(),
        2
    );
    let json = serde_json::to_value(&report.checkouts[0]).unwrap();
    let decoded: rooster_core::NativePath = serde_json::from_value(json["path"].clone()).unwrap();
    assert_eq!(decoded.as_path(), odd);
    assert!(report.worktree_candidates.is_empty());
}

#[cfg(unix)]
#[test]
fn stalled_git_can_time_out_or_be_cancelled() {
    use std::{
        os::unix::fs::PermissionsExt,
        thread,
        time::{Duration, Instant},
    };
    let temp = tempdir().unwrap();
    let directory = temp.path().join("repo");
    fs::create_dir_all(directory.join(".git")).unwrap();
    let fake_git = temp.path().join("slow-git");
    fs::write(&fake_git, "#!/bin/sh\nexec sleep 10\n").unwrap();
    fs::set_permissions(&fake_git, fs::Permissions::from_mode(0o700)).unwrap();
    let mut options = ScanOptions {
        git_executable: fake_git,
        git_timeout: Duration::from_millis(60),
        ..ScanOptions::default()
    };
    let start = Instant::now();
    let report = scan(
        &[root("root", &directory)],
        &options,
        &CancellationToken::default(),
        |_| {},
    );
    assert_eq!(report.status, ScanStatus::Partial);
    assert!(report.issues[0].message.contains("timed out"));
    assert!(start.elapsed() < Duration::from_secs(2));
    options.git_timeout = Duration::from_secs(10);
    let token = CancellationToken::default();
    let signal = token.clone();
    let handle = thread::spawn(move || {
        thread::sleep(Duration::from_millis(60));
        signal.cancel();
    });
    let start = Instant::now();
    let report = scan(&[root("root", &directory)], &options, &token, |_| {});
    handle.join().unwrap();
    assert_eq!(report.status, ScanStatus::Cancelled);
    assert!(start.elapsed() < Duration::from_secs(2));
}
