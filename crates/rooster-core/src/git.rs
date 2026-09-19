use crate::{CancellationToken, CheckoutKind, HeadState, NativePath, paths::git_path};
use std::{
    ffi::OsStr,
    fs::{self, File},
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

const OUTPUT_LIMIT: u64 = 4 * 1024 * 1024;

#[derive(Debug)]
pub(crate) enum GitError {
    Cancelled,
    Failed(String),
}

impl From<std::io::Error> for GitError {
    fn from(error: std::io::Error) -> Self {
        Self::Failed(error.to_string())
    }
}

pub(crate) struct Git<'a> {
    pub executable: &'a Path,
    pub timeout: Duration,
    pub cancellation: &'a CancellationToken,
}

struct Output {
    code: i32,
    stdout: Vec<u8>,
}

pub(crate) struct Metadata {
    pub git_dir: PathBuf,
    pub common_dir: PathBuf,
    pub kind: CheckoutKind,
    pub branch: Option<String>,
    pub head: Option<String>,
    pub head_state: HeadState,
    pub superproject: Option<NativePath>,
}

impl Git<'_> {
    pub fn status(&self, directory: &Path) -> Result<Vec<u8>, GitError> {
        Ok(self
            .run(
                directory,
                &[
                    "status",
                    "--porcelain=v1",
                    "-z",
                    "--untracked-files=all",
                    "--ignored=no",
                ],
                false,
            )?
            .stdout)
    }
    pub fn visible_files(
        &self,
        directory: &Path,
    ) -> Result<std::collections::HashSet<PathBuf>, GitError> {
        let output = self.run(
            directory,
            &[
                "ls-files",
                "--cached",
                "--others",
                "--exclude-standard",
                "-z",
            ],
            false,
        )?;
        output
            .stdout
            .split(|b| *b == 0)
            .filter(|path| !path.is_empty())
            .map(|bytes| {
                git_path(bytes)
                    .map(|path| directory.join(path))
                    .map_err(GitError::Failed)
            })
            .collect()
    }
    fn run(
        &self,
        directory: &Path,
        args: &[&str],
        allow_missing: bool,
    ) -> Result<Output, GitError> {
        if self.cancellation.is_cancelled() {
            return Err(GitError::Cancelled);
        }
        // Files avoid pipe deadlocks and allow cancellation without reader threads.
        let mut stdout = tempfile::tempfile()?;
        let mut stderr = tempfile::tempfile()?;
        let mut command = Command::new(self.executable);
        command
            .current_dir(directory)
            .args(["--no-pager", "-c", "core.fsmonitor=false"])
            .args(args)
            .stdin(Stdio::null())
            .stdout(stdout.try_clone()?)
            .stderr(stderr.try_clone()?);
        // A parent agent may itself be running inside a Git-specific environment.
        for (key, _) in std::env::vars_os() {
            if key.to_string_lossy().starts_with("GIT_") {
                command.env_remove(key);
            }
        }
        command
            .env("GIT_OPTIONAL_LOCKS", "0")
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("LC_ALL", "C");
        let mut child = command.spawn()?;
        let start = Instant::now();
        let status = loop {
            let failure = if self.cancellation.is_cancelled() {
                Some(GitError::Cancelled)
            } else if start.elapsed() >= self.timeout {
                Some(GitError::Failed(format!(
                    "Git timed out after {:?}",
                    self.timeout
                )))
            } else {
                match stdout
                    .metadata()
                    .and_then(|out| stderr.metadata().map(|err| (out, err)))
                {
                    Ok((out, err)) if out.len() > OUTPUT_LIMIT || err.len() > OUTPUT_LIMIT => {
                        Some(GitError::Failed("Git output exceeded 4 MiB".into()))
                    }
                    Ok(_) => None,
                    Err(error) => Some(error.into()),
                }
            };
            if let Some(error) = failure {
                let _ = child.kill();
                let _ = child.wait();
                return Err(error);
            }
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) => thread::sleep(Duration::from_millis(10)),
                Err(error) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(error.into());
                }
            }
        };
        let stdout = read_output(&mut stdout)?;
        let stderr = read_output(&mut stderr)?;
        let code = status.code().unwrap_or(-1);
        if code != 0 && !(allow_missing && code == 1) {
            return Err(GitError::Failed(format!(
                "git {} failed ({code}): {}",
                args.join(" "),
                String::from_utf8_lossy(&stderr).trim()
            )));
        }
        Ok(Output { code, stdout })
    }

    fn path(&self, directory: &Path, flag: &str) -> Result<PathBuf, GitError> {
        let output = self.run(directory, &["rev-parse", flag], false)?;
        let path = git_path(without_line_ending(&output.stdout)).map_err(GitError::Failed)?;
        let path = if path.is_absolute() {
            path
        } else {
            directory.join(path)
        };
        path.canonicalize().map_err(Into::into)
    }

    pub fn is_bare(&self, directory: &Path) -> Result<bool, GitError> {
        let output = self.run(directory, &["rev-parse", "--is-bare-repository"], false)?;
        Ok(without_line_ending(&output.stdout) == b"true")
    }

    pub fn is_metadata(&self, directory: &Path) -> Result<bool, GitError> {
        let output = self.run(directory, &["rev-parse", "--is-inside-git-dir"], false)?;
        Ok(without_line_ending(&output.stdout) == b"true")
    }

    pub fn inspect(&self, directory: &Path) -> Result<Metadata, GitError> {
        let top = self.path(directory, "--show-toplevel")?;
        if top != directory {
            return Err(GitError::Failed(
                "Git metadata points to a different checkout directory".into(),
            ));
        }
        let git_dir = self.path(directory, "--absolute-git-dir")?;
        let common_dir = self.path(directory, "--git-common-dir")?;
        let symbolic = self.run(
            directory,
            &["symbolic-ref", "--quiet", "--short", "HEAD"],
            true,
        )?;
        let branch = (symbolic.code == 0)
            .then(|| String::from_utf8_lossy(without_line_ending(&symbolic.stdout)).into_owned());
        let revision = self.run(
            directory,
            &["rev-parse", "--verify", "--quiet", "HEAD"],
            true,
        )?;
        let head = (revision.code == 0)
            .then(|| String::from_utf8_lossy(without_line_ending(&revision.stdout)).into_owned());
        let head_state = match (&branch, &head) {
            (Some(_), None) => HeadState::Unborn,
            (Some(_), Some(_)) => HeadState::Branch,
            (None, Some(_)) => HeadState::Detached,
            (None, None) => {
                return Err(GitError::Failed(
                    "HEAD has neither a branch nor a commit".into(),
                ));
            }
        };
        let output = self.run(
            directory,
            &["rev-parse", "--show-superproject-working-tree"],
            false,
        )?;
        let superproject = if output.stdout.is_empty() {
            None
        } else {
            Some(
                git_path(without_line_ending(&output.stdout))
                    .map_err(GitError::Failed)?
                    .into(),
            )
        };
        let kind = if superproject.is_some() {
            CheckoutKind::Submodule
        } else if git_dir != common_dir {
            CheckoutKind::LinkedWorktree
        } else {
            CheckoutKind::Repository
        };
        Ok(Metadata {
            git_dir,
            common_dir,
            kind,
            branch,
            head,
            head_state,
            superproject,
        })
    }

    pub fn worktrees(&self, directory: &Path) -> Result<Vec<PathBuf>, GitError> {
        let output = self.run(directory, &["worktree", "list", "--porcelain", "-z"], false)?;
        let fields: Vec<_> = output.stdout.split(|b| *b == 0).collect();
        fields
            .split(|field| field.is_empty())
            .filter(|record| !record.contains(&b"bare".as_slice()))
            .filter_map(|record| {
                record
                    .iter()
                    .find_map(|field| field.strip_prefix(b"worktree "))
            })
            .map(|path| git_path(path).map_err(GitError::Failed))
            .collect()
    }
}

fn without_line_ending(bytes: &[u8]) -> &[u8] {
    // Remove only Git's terminator: a path may itself end in a newline.
    bytes.strip_suffix(b"\n").unwrap_or(bytes)
}

fn read_output(file: &mut File) -> Result<Vec<u8>, GitError> {
    file.seek(SeekFrom::Start(0))?;
    let mut bytes = Vec::new();
    file.take(OUTPUT_LIMIT + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > OUTPUT_LIMIT {
        return Err(GitError::Failed("Git output exceeded 4 MiB".into()));
    }
    Ok(bytes)
}

pub(crate) fn has_git_marker(directory: &Path) -> Result<bool, std::io::Error> {
    match fs::symlink_metadata(directory.join(OsStr::new(".git"))) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            Err(std::io::Error::other(".git is a symlink; not inspected"))
        }
        Ok(metadata) if metadata.is_file() || metadata.is_dir() => Ok(true),
        Ok(_) => Err(std::io::Error::other(
            ".git is not a regular file or directory",
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}
