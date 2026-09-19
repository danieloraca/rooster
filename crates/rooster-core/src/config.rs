use crate::paths::{NativePath, absolute_input};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    fs::{self, File, OpenOptions, TryLockError},
    io::{self, Write},
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant},
};
use tempfile::NamedTempFile;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("invalid configuration: {0}")]
    InvalidConfig(String),
    #[error("workspace not found: {0}")]
    WorkspaceNotFound(String),
    #[error("root not found: {0}")]
    RootNotFound(String),
    #[error("configuration is busy; retry after the other Rooster writer finishes")]
    Busy,
    #[error("configuration changed while updating it; retry with the current file")]
    Conflict,
}

fn io_error(path: &Path, source: io::Error) -> Error {
    Error::Io {
        path: path.to_path_buf(),
        source,
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Root {
    pub id: String,
    pub path: NativePath,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Workspace {
    pub id: String,
    pub name: String,
    pub roots: Vec<Root>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub schema_version: u32,
    next_id: u64,
    pub workspaces: Vec<Workspace>,
    pub excluded_dirs: Vec<String>,
    #[serde(default)]
    pub codex: crate::artifacts::CodexSettings,
    #[serde(default)]
    pub claude: crate::artifacts::ClaudeSettings,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            schema_version: 1,
            next_id: 1,
            codex: crate::artifacts::CodexSettings::default(),
            claude: crate::artifacts::ClaudeSettings::default(),
            workspaces: Vec::new(),
            excluded_dirs: ["node_modules", "vendor", "target", "dist", "build"]
                .map(String::from)
                .to_vec(),
        }
    }
}

impl Config {
    pub fn roots(&self, selector: Option<&str>) -> Result<Vec<Root>, Error> {
        self.validate()?;
        match selector {
            None => Ok(self
                .workspaces
                .iter()
                .flat_map(|w| w.roots.clone())
                .collect()),
            Some(value) => self
                .workspaces
                .iter()
                .find(|w| w.id == value)
                .or_else(|| self.workspaces.iter().find(|w| w.name == value))
                .map(|w| w.roots.clone())
                .ok_or_else(|| Error::WorkspaceNotFound(value.into())),
        }
    }

    fn validate(&self) -> Result<(), Error> {
        for path in self
            .codex
            .home
            .iter()
            .chain(self.codex.user_skills.iter())
            .chain(self.codex.admin_skills.iter())
            .chain(self.codex.extra_roots.iter().map(|r| &r.path))
            .chain(self.claude.home.iter())
            .chain(self.claude.managed.iter())
            .chain(self.claude.extra_roots.iter().map(|r| &r.path))
        {
            if !path.as_path().is_absolute() {
                return Err(Error::InvalidConfig(
                    "Provider root overrides need absolute native paths".into(),
                ));
            }
        }
        if self.schema_version != 1 {
            return Err(Error::InvalidConfig(format!(
                "unsupported schema version {}; expected 1",
                self.schema_version
            )));
        }
        let mut ids = HashSet::new();
        let mut names = HashSet::new();
        for workspace in &self.workspaces {
            if workspace.name.trim().is_empty()
                || workspace.name.chars().any(char::is_control)
                || !names.insert(&workspace.name)
                || workspace.id.is_empty()
                || !ids.insert(&workspace.id)
            {
                return Err(Error::InvalidConfig(
                    "invalid or duplicate workspace identity".into(),
                ));
            }
            for root in &workspace.roots {
                if root.id.is_empty() || !ids.insert(&root.id) || !root.path.as_path().is_absolute()
                {
                    return Err(Error::InvalidConfig(
                        "roots need unique IDs and absolute native paths".into(),
                    ));
                }
            }
        }
        for name in &self.excluded_dirs {
            if name.is_empty() || name == "." || name == ".." || name.contains(['/', '\\']) {
                return Err(Error::InvalidConfig(
                    "exclusions must be directory names".into(),
                ));
            }
        }
        Ok(())
    }

    fn allocate_id(&mut self, prefix: &str) -> Result<String, Error> {
        loop {
            let id = format!("{prefix}-{}", self.next_id);
            self.next_id = self
                .next_id
                .checked_add(1)
                .ok_or_else(|| Error::InvalidConfig("identity counter exhausted".into()))?;
            if !self
                .workspaces
                .iter()
                .any(|w| w.id == id || w.roots.iter().any(|r| r.id == id))
            {
                return Ok(id);
            }
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct Registration {
    pub workspace_id: String,
    pub workspace_name: String,
    pub root: Root,
}

/// Mutations lock, reload, and atomically replace Rooster's own settings.
/// Reads do not create settings, lock files, or directories.
#[derive(Clone, Debug)]
pub struct ConfigStore {
    path: PathBuf,
}

impl ConfigStore {
    pub fn new(path: impl AsRef<Path>) -> Result<Self, Error> {
        let input = path.as_ref();
        let path = absolute_input(input).map_err(|e| io_error(input, e))?;
        Ok(Self { path })
    }

    pub fn default_path() -> Result<PathBuf, Error> {
        directories::BaseDirs::new()
            .map(|dirs| dirs.config_dir().join("rooster").join("config.json"))
            .ok_or_else(|| {
                Error::InvalidConfig("cannot resolve user config directory; use --config".into())
            })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn load(&self) -> Result<Config, Error> {
        Self::decode(self.read_bytes()?)
    }

    pub fn add(&self, name: &str, path: impl AsRef<Path>) -> Result<Registration, Error> {
        let name = name.trim();
        if name.is_empty() || name.chars().any(char::is_control) {
            return Err(Error::InvalidConfig(
                "workspace name must be non-empty text".into(),
            ));
        }
        let path = registered_path(path.as_ref())?;
        self.update(|config| {
            let index = match config.workspaces.iter().position(|w| w.name == name) {
                Some(index) => index,
                None => {
                    let id = config.allocate_id("workspace")?;
                    config.workspaces.push(Workspace {
                        id,
                        name: name.into(),
                        roots: Vec::new(),
                    });
                    config.workspaces.len() - 1
                }
            };
            let root = match config.workspaces[index]
                .roots
                .iter()
                .find(|r| r.path == path)
            {
                Some(root) => root.clone(),
                None => {
                    let root = Root {
                        id: config.allocate_id("root")?,
                        path,
                    };
                    config.workspaces[index].roots.push(root.clone());
                    root
                }
            };
            let workspace = &config.workspaces[index];
            Ok(Registration {
                workspace_id: workspace.id.clone(),
                workspace_name: workspace.name.clone(),
                root,
            })
        })
    }

    pub fn relocate(&self, root_id: &str, path: impl AsRef<Path>) -> Result<Registration, Error> {
        let path = registered_path(path.as_ref())?;
        self.update(|config| {
            for workspace in &mut config.workspaces {
                if let Some(index) = workspace.roots.iter().position(|r| r.id == root_id) {
                    if workspace
                        .roots
                        .iter()
                        .any(|r| r.id != root_id && r.path == path)
                    {
                        return Err(Error::InvalidConfig(
                            "destination already registered in this workspace".into(),
                        ));
                    }
                    workspace.roots[index].path = path;
                    return Ok(Registration {
                        workspace_id: workspace.id.clone(),
                        workspace_name: workspace.name.clone(),
                        root: workspace.roots[index].clone(),
                    });
                }
            }
            Err(Error::RootNotFound(root_id.into()))
        })
    }

    /// Forget one registration without accessing its folder. Drop only its
    /// workspace when that registration was the workspace's last root.
    pub fn remove_root(&self, root_id: &str) -> Result<(), Error> {
        self.update(|config| {
            let (workspace_index, root_index) = config
                .workspaces
                .iter()
                .enumerate()
                .find_map(|(index, workspace)| {
                    workspace
                        .roots
                        .iter()
                        .position(|root| root.id == root_id)
                        .map(|root_index| (index, root_index))
                })
                .ok_or_else(|| Error::RootNotFound(root_id.into()))?;
            let workspace = &mut config.workspaces[workspace_index];
            workspace.roots.remove(root_index);
            if workspace.roots.is_empty() {
                config.workspaces.remove(workspace_index);
            }
            Ok(())
        })
    }

    fn read_bytes(&self) -> Result<Option<Vec<u8>>, Error> {
        match fs::symlink_metadata(&self.path) {
            Ok(metadata) if !metadata.file_type().is_file() => {
                return Err(Error::InvalidConfig(
                    "settings must be a regular, non-symlink file".into(),
                ));
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(io_error(&self.path, e)),
            _ => {}
        }
        fs::read(&self.path)
            .map(Some)
            .map_err(|e| io_error(&self.path, e))
    }

    fn decode(bytes: Option<Vec<u8>>) -> Result<Config, Error> {
        let config: Config = match bytes {
            None => Config::default(),
            Some(bytes) => {
                serde_json::from_slice(&bytes).map_err(|e| Error::InvalidConfig(e.to_string()))?
            }
        };
        config.validate()?;
        Ok(config)
    }

    fn update<T>(&self, mutate: impl FnOnce(&mut Config) -> Result<T, Error>) -> Result<T, Error> {
        let parent = self
            .path
            .parent()
            .ok_or_else(|| Error::InvalidConfig("missing config parent".into()))?;
        let mut builder = fs::DirBuilder::new();
        builder.recursive(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder.create(parent).map_err(|e| io_error(parent, e))?;
        let mut lock_name = self.path.as_os_str().to_os_string();
        lock_name.push(".lock");
        let lock_path = PathBuf::from(lock_name);
        let lock = open_lock(&lock_path)?;
        let start = Instant::now();
        loop {
            match lock.try_lock() {
                Ok(()) => break,
                Err(TryLockError::WouldBlock) if start.elapsed() < Duration::from_secs(5) => {
                    thread::sleep(Duration::from_millis(20))
                }
                Err(TryLockError::WouldBlock) => return Err(Error::Busy),
                Err(TryLockError::Error(error)) => return Err(io_error(&lock_path, error)),
            }
        }
        let original = self.read_bytes()?;
        let mut config = Self::decode(original.clone())?;
        let result = mutate(&mut config)?;
        config.validate()?;
        let mut bytes =
            serde_json::to_vec_pretty(&config).map_err(|e| Error::InvalidConfig(e.to_string()))?;
        bytes.push(b'\n');
        let mut temp = NamedTempFile::new_in(parent).map_err(|e| io_error(parent, e))?;
        temp.write_all(&bytes)
            .map_err(|e| io_error(temp.path(), e))?;
        temp.as_file()
            .sync_all()
            .map_err(|e| io_error(temp.path(), e))?;
        if self.read_bytes()? != original {
            return Err(Error::Conflict);
        }
        temp.persist(&self.path)
            .map_err(|e| io_error(&self.path, e.error))?;
        #[cfg(unix)]
        File::open(parent)
            .and_then(|file| file.sync_all())
            .map_err(|e| io_error(parent, e))?;
        if self.read_bytes()?.as_deref() != Some(bytes.as_slice()) {
            return Err(Error::Conflict);
        }
        Ok(result)
    }
}

fn open_lock(path: &Path) -> Result<File, Error> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if !metadata.file_type().is_file() => {
            return Err(Error::InvalidConfig(
                "settings lock must be a regular file".into(),
            ));
        }
        Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(io_error(path, e)),
        _ => {}
    }
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path).map_err(|e| io_error(path, e))
}

fn registered_path(path: &Path) -> Result<NativePath, Error> {
    let absolute = absolute_input(path).map_err(|e| io_error(path, e))?;
    let path = absolute
        .canonicalize()
        .map_err(|e| io_error(&absolute, e))?;
    if !path.is_dir() {
        return Err(Error::InvalidConfig(format!(
            "not a directory: {}",
            path.display()
        )));
    }
    Ok(path.into())
}
