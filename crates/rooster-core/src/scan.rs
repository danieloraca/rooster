use crate::{
    NativePath, Root,
    git::{Git, GitError, has_git_marker},
    paths::checkout_id,
};
use serde::Serialize;
use std::{
    collections::{BTreeMap, BTreeSet, HashSet},
    fs,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

#[derive(Clone, Debug, Default)]
pub struct CancellationToken(Arc<AtomicBool>);

impl CancellationToken {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

#[derive(Clone, Debug)]
pub struct ScanOptions {
    pub excluded_dirs: Vec<String>,
    pub git_timeout: Duration,
    /// Allows callers to select a trusted Git installation.
    pub git_executable: PathBuf,
}

impl Default for ScanOptions {
    fn default() -> Self {
        Self {
            excluded_dirs: crate::Config::default().excluded_dirs,
            git_timeout: Duration::from_secs(5),
            git_executable: PathBuf::from("git"),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckoutKind {
    Repository,
    LinkedWorktree,
    Submodule,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HeadState {
    Branch,
    Detached,
    Unborn,
}

#[derive(Clone, Debug, Serialize)]
pub struct Checkout {
    pub id: String,
    pub path: NativePath,
    pub git_dir: NativePath,
    pub common_dir: NativePath,
    pub root_ids: Vec<String>,
    pub kind: CheckoutKind,
    pub branch: Option<String>,
    pub head: Option<String>,
    pub head_state: HeadState,
    pub superproject: Option<NativePath>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ScanStatus {
    Complete,
    Partial,
    Cancelled,
}

#[derive(Clone, Debug, Serialize)]
pub struct ScanIssue {
    pub path: NativePath,
    pub message: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SkipReason {
    ExcludedDirectory,
    SymbolicLink,
    BareRepository,
    GitMetadata,
}

#[derive(Clone, Debug, Serialize)]
pub struct SkippedPath {
    pub path: NativePath,
    pub reason: SkipReason,
}

#[derive(Clone, Debug, Serialize)]
pub struct WorktreeCandidate {
    pub path: NativePath,
    pub common_dir: NativePath,
}

#[derive(Clone, Debug, Serialize)]
pub struct ScanReport {
    pub schema_version: u32,
    pub status: ScanStatus,
    pub roots: Vec<Root>,
    pub visited_dirs: usize,
    pub checkouts: Vec<Checkout>,
    pub issues: Vec<ScanIssue>,
    pub skipped: Vec<SkippedPath>,
    /// Paths advertised by Git but not discovered in this scan; never traversed automatically.
    pub worktree_candidates: Vec<WorktreeCandidate>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ScanProgress {
    pub current_path: NativePath,
    pub visited_dirs: usize,
    pub checkouts: usize,
    pub issues: usize,
    pub skipped: usize,
}

impl ScanReport {
    fn issue(&mut self, path: &Path, message: impl ToString) {
        self.issues.push(ScanIssue {
            path: path.to_path_buf().into(),
            message: message.to_string(),
        });
    }
    fn skip(&mut self, path: PathBuf, reason: SkipReason) {
        self.skipped.push(SkippedPath {
            path: path.into(),
            reason,
        });
    }
}

/// Discover only beneath the supplied roots. Git metadata outside them is read
/// when needed to identify a worktree; advertised checkout paths remain candidates.
pub fn scan(
    roots: &[Root],
    options: &ScanOptions,
    cancellation: &CancellationToken,
    mut on_progress: impl FnMut(&ScanProgress),
) -> ScanReport {
    let mut report = ScanReport {
        schema_version: 1,
        status: ScanStatus::Complete,
        roots: roots.to_vec(),
        visited_dirs: 0,
        checkouts: Vec::new(),
        issues: Vec::new(),
        skipped: Vec::new(),
        worktree_candidates: Vec::new(),
    };
    let git = Git {
        executable: &options.git_executable,
        timeout: options.git_timeout,
        cancellation,
    };
    let mut registered = Vec::new();
    for root in roots {
        if cancellation.is_cancelled() {
            break;
        }
        match root.path.as_path().canonicalize() {
            Ok(path) if path.is_dir() => registered.push((root.id.clone(), path)),
            Ok(_) => report.issue(root.path.as_path(), "registered root is not a directory"),
            Err(error) => report.issue(root.path.as_path(), error),
        }
    }
    let mut pending: Vec<PathBuf> = registered.iter().map(|(_, path)| path.clone()).collect();
    pending.sort();
    pending.dedup();
    pending.reverse();
    let mut visited = HashSet::new();
    let mut common_dirs = HashSet::new();
    let mut candidates = BTreeMap::new();
    while let Some(directory) = pending.pop() {
        if cancellation.is_cancelled() {
            break;
        }
        if directory
            .components()
            .any(|part| part.as_os_str() == ".git")
        {
            report.skip(directory, SkipReason::GitMetadata);
            continue;
        }
        match fs::symlink_metadata(&directory) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                report.skip(directory, SkipReason::SymbolicLink);
                continue;
            }
            Ok(metadata) if metadata.is_dir() => {}
            Ok(_) => {
                report.issue(&directory, "directory changed during scan");
                continue;
            }
            Err(error) => {
                report.issue(&directory, error);
                continue;
            }
        }
        let directory = match directory.canonicalize() {
            Ok(path) if registered.iter().any(|(_, root)| path.starts_with(root)) => path,
            Ok(_) => {
                report.issue(&directory, "directory moved outside registered roots");
                continue;
            }
            Err(error) => {
                report.issue(&directory, error);
                continue;
            }
        };
        if !visited.insert(directory.clone()) {
            continue;
        }
        report.visited_dirs += 1;
        on_progress(&ScanProgress {
            current_path: directory.clone().into(),
            visited_dirs: report.visited_dirs,
            checkouts: report.checkouts.len(),
            issues: report.issues.len(),
            skipped: report.skipped.len(),
        });
        if cancellation.is_cancelled() {
            break;
        }
        match has_git_marker(&directory) {
            Ok(true) => match git.inspect(&directory) {
                Ok(metadata) => {
                    if common_dirs.insert(metadata.common_dir.clone()) {
                        match git.worktrees(&directory) {
                            Ok(paths) => {
                                for path in paths {
                                    // Git 2.39 can advertise a submodule's common
                                    // directory as its main worktree. It is metadata,
                                    // not another editable checkout.
                                    if path != metadata.common_dir {
                                        candidates.insert(path, metadata.common_dir.clone());
                                    }
                                }
                            }
                            Err(GitError::Cancelled) => break,
                            Err(GitError::Failed(message)) => report.issue(&directory, message),
                        }
                    }
                    let root_ids: BTreeSet<_> = registered
                        .iter()
                        .filter(|(_, root)| directory.starts_with(root))
                        .map(|(id, _)| id.clone())
                        .collect();
                    report.checkouts.push(Checkout {
                        id: checkout_id(&directory),
                        path: directory.clone().into(),
                        git_dir: metadata.git_dir.into(),
                        common_dir: metadata.common_dir.into(),
                        root_ids: root_ids.into_iter().collect(),
                        kind: metadata.kind,
                        branch: metadata.branch,
                        head: metadata.head,
                        head_state: metadata.head_state,
                        superproject: metadata.superproject,
                    });
                }
                Err(GitError::Cancelled) => break,
                Err(GitError::Failed(message)) => report.issue(&directory, message),
            },
            Ok(false) => {
                // Recognize bare stores without walking their object database.
                if directory.join("HEAD").is_file()
                    && directory.join("objects").is_dir()
                    && directory.join("refs").is_dir()
                {
                    match git.is_bare(&directory) {
                        Ok(true) => {
                            report.skip(directory, SkipReason::BareRepository);
                            continue;
                        }
                        Err(GitError::Cancelled) => break,
                        Err(GitError::Failed(message)) => report.issue(&directory, message),
                        Ok(false) => match git.is_metadata(&directory) {
                            Ok(true) => {
                                report.skip(directory, SkipReason::GitMetadata);
                                continue;
                            }
                            Err(GitError::Cancelled) => break,
                            Err(GitError::Failed(message)) => report.issue(&directory, message),
                            Ok(false) => {}
                        },
                    }
                }
            }
            Err(error) => report.issue(&directory, error),
        }
        let entries = match fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(error) => {
                report.issue(&directory, error);
                continue;
            }
        };
        let mut children = Vec::new();
        for entry in entries {
            if cancellation.is_cancelled() {
                break;
            }
            let entry = match entry {
                Ok(entry) => entry,
                Err(error) => {
                    report.issue(&directory, error);
                    continue;
                }
            };
            if entry.file_name() == ".git" {
                continue;
            }
            let path = entry.path();
            match entry.file_type() {
                Ok(kind) if kind.is_symlink() => report.skip(path, SkipReason::SymbolicLink),
                Ok(kind) if kind.is_dir() => {
                    if options
                        .excluded_dirs
                        .iter()
                        .any(|name| entry.file_name() == name.as_str())
                    {
                        report.skip(path, SkipReason::ExcludedDirectory);
                    } else {
                        children.push(path);
                    }
                }
                Ok(_) => {}
                Err(error) => report.issue(&path, error),
            }
        }
        children.sort();
        pending.extend(children.into_iter().rev());
    }
    report.checkouts.sort_by(|a, b| a.path.cmp(&b.path));
    let known: HashSet<_> = report.checkouts.iter().map(|c| c.path.as_path()).collect();
    report.worktree_candidates = candidates
        .into_iter()
        .filter(|(path, _)| !known.contains(path.as_path()))
        .map(|(path, common_dir)| WorktreeCandidate {
            path: path.into(),
            common_dir: common_dir.into(),
        })
        .collect();
    report.status = if cancellation.is_cancelled() {
        ScanStatus::Cancelled
    } else if report.issues.is_empty() {
        ScanStatus::Complete
    } else {
        ScanStatus::Partial
    };
    report
}
