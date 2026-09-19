use crate::{Config, NativePath};
use serde::{Deserialize, Serialize};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Invalid(String),
    #[error("conflict: {0}")]
    Conflict(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Config(#[from] crate::Error),
}
pub type Result<T> = std::result::Result<T, Error>;
#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FileKind {
    Markdown,
    Instruction,
    Skill,
    SkillMetadata,
    Agent,
    Rule,
    LegacyCommand,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TextEdit {
    pub start: usize,
    pub end: usize,
    pub replacement: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct NewFile {
    pub path: NativePath,
    pub bytes: Vec<u8>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum Request {
    Edit {
        artifact_id: String,
        edits: Vec<TextEdit>,
    },
    Replace {
        artifact_id: String,
        text: String,
    },
    Create {
        owner_id: String,
        path: NativePath,
        kind: FileKind,
        text: String,
    },
    CreatePackage {
        owner_id: String,
        path: NativePath,
        files: Vec<NewFile>,
    },
    Duplicate {
        artifact_id: String,
        owner_id: String,
        path: NativePath,
    },
    Rename {
        artifact_id: String,
        path: NativePath,
        #[serde(default)]
        repair_links: Vec<String>,
    },
    Delete {
        artifact_id: String,
    },
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Prepared,
    Applying,
    Completed,
    RecoveryRequired,
    Restoring,
    Restored,
}
#[derive(Clone, Debug, Serialize)]
pub struct OwnerChoice {
    pub id: String,
    pub root: NativePath,
    pub checkout: bool,
}
#[derive(Clone, Debug, Serialize)]
pub struct FilePreview {
    pub path: NativePath,
    pub action: String,
    pub directory: bool,
    pub before_sha256: Option<String>,
    pub after_sha256: Option<String>,
    pub before_text: Option<String>,
    pub after_text: Option<String>,
    pub before_bytes: Option<u64>,
    pub after_bytes: Option<u64>,
    pub before_mode: Option<u32>,
    pub after_mode: Option<u32>,
}
#[derive(Clone, Debug, Serialize)]
pub struct Preview {
    pub id: String,
    pub status: Status,
    pub files: Vec<FilePreview>,
    pub warnings: Vec<String>,
}
#[derive(Clone, Debug, Serialize)]
pub struct Outcome {
    pub id: String,
    pub status: Status,
    pub changed: Vec<NativePath>,
    pub pending: Vec<NativePath>,
    pub error: Option<String>,
}
#[derive(Clone, Debug, Serialize)]
pub struct HistoryEntry {
    pub updated_at: u64,
    pub id: String,
    pub status: Status,
    pub changes: usize,
    pub error: Option<String>,
}
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub(super) struct Identity {
    pub device: u64,
    pub inode: u64,
}
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub(super) struct Node {
    pub directory: bool,
    pub hash: Option<String>,
    pub size: u64,
    pub mode: u32,
    pub identity: Option<Identity>,
}
impl Node {
    pub fn content_eq(&self, other: &Self) -> bool {
        self.directory == other.directory
            && self.hash == other.hash
            && self.size == other.size
            && self.mode == other.mode
    }
    pub fn desired(&self) -> Self {
        let mut node = self.clone();
        node.identity = None;
        node
    }
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub(super) struct Guard {
    pub root: NativePath,
    pub identity: Identity,
    pub checkout: Option<CheckoutGuard>,
}
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub(super) struct CheckoutGuard {
    pub git_dir: NativePath,
    pub common_dir: NativePath,
    pub branch: Option<String>,
    pub head: Option<String>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub(super) struct TreeGuard {
    pub root: NativePath,
    pub nodes: Vec<(NativePath, Node)>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub(super) struct Change {
    pub path: NativePath,
    pub before: Option<Node>,
    pub after: Option<Node>,
    pub applied: Option<Node>,
    pub restored: Option<Node>,
    pub done: bool,
    pub undone: bool,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Record {
    pub schema_version: u32,
    pub id: String,
    pub config: Config,
    pub status: Status,
    pub guards: Vec<Guard>,
    pub parents: Vec<(NativePath, Identity)>,
    pub reads: Vec<(NativePath, Node)>,
    pub protected: Vec<NativePath>,
    pub trees: Vec<TreeGuard>,
    pub changes: Vec<Change>,
    pub warnings: Vec<String>,
    pub error: Option<String>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FaultPoint {
    Backup,
    Journal,
    BeforeChange(usize),
    AfterChange(usize),
}
