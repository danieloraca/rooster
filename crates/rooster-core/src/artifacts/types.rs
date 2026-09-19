use crate::{NativePath, ScanOptions, ScanReport, ScanStatus};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::HashMap, path::PathBuf};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Provenance {
    Repository,
    Personal,
    System,
    InstalledPlugin,
    Compatibility,
    Managed,
    Synced,
}

impl Provenance {
    pub fn is_authoring(self) -> bool {
        matches!(self, Self::Repository | Self::Personal)
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AdditionalRoot {
    pub path: NativePath,
    pub provenance: Provenance,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct CodexSettings {
    pub include_default_roots: bool,
    pub home: Option<NativePath>,
    pub user_skills: Option<NativePath>,
    pub admin_skills: Option<NativePath>,
    pub extra_roots: Vec<AdditionalRoot>,
}

impl Default for CodexSettings {
    fn default() -> Self {
        Self {
            include_default_roots: true,
            home: None,
            user_skills: None,
            admin_skills: None,
            extra_roots: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct ClaudeSettings {
    pub include_default_roots: bool,
    pub home: Option<NativePath>,
    pub managed: Option<NativePath>,
    pub extra_roots: Vec<AdditionalRoot>,
}
impl Default for ClaudeSettings {
    fn default() -> Self {
        Self {
            include_default_roots: true,
            home: None,
            managed: None,
            extra_roots: Vec::new(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct InventoryOptions {
    pub provider: crate::providers::Provider,
    pub claude: ClaudeSettings,
    pub scan: ScanOptions,
    pub codex: CodexSettings,
    pub all_markdown: bool,
    pub context: Option<PathBuf>,
    pub max_file_bytes: u64,
    pub max_total_bytes: u64,
    pub max_files: usize,
}

impl Default for InventoryOptions {
    fn default() -> Self {
        Self {
            provider: Default::default(),
            claude: ClaudeSettings::default(),
            scan: ScanOptions::default(),
            codex: CodexSettings::default(),
            all_markdown: false,
            context: None,
            max_file_bytes: 1024 * 1024,
            max_total_bytes: 64 * 1024 * 1024,
            max_files: 20_000,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactKind {
    Instruction,
    Skill,
    SkillMetadata,
    Agent,
    LegacyAgent,
    Rule,
    LegacyCommand,
    ProviderConfig,
    PluginManifest,
    Reference,
    SupportingFile,
    Markdown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Validation {
    Valid,
    Malformed,
    Unsupported,
    Unavailable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Info,
    Warning,
    Error,
}

#[derive(Clone, Debug, Serialize)]
pub struct Diagnostic {
    pub severity: Severity,
    pub code: String,
    pub path: NativePath,
    pub line: Option<usize>,
    pub artifact_id: Option<String>,
    pub message: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct Owner {
    pub id: String,
    pub root: NativePath,
    pub checkout_id: Option<String>,
    pub head: Option<String>,
    pub branch: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Artifact {
    pub id: String,
    pub provider: Option<String>,
    pub kind: ArtifactKind,
    /// Discovery path and physical target are separate for linked skills.
    pub path: NativePath,
    pub physical_path: NativePath,
    pub owner: Owner,
    pub provenance: Provenance,
    pub source_root: NativePath,
    pub scope_directory: NativePath,
    pub scope_checkout_id: Option<String>,
    pub package_id: Option<String>,
    pub name: Option<String>,
    pub declared_role_names: Vec<String>,
    pub role_declaration_count: usize,
    pub description: Option<String>,
    pub validation: Validation,
    pub unknown_fields: Vec<String>,
    pub read_only_reason: Option<String>,
    pub ignored: Option<bool>,
    pub snapshot: Option<SnapshotInfo>,
    pub references: Vec<Reference>,
    /// Full provider metadata is returned only by explicit inspection.
    #[serde(skip_serializing)]
    pub metadata: Value,
}

#[derive(Clone, Debug, Serialize)]
pub struct SkillPackage {
    pub id: String,
    pub root: NativePath,
    pub physical_root: NativePath,
    pub skill_id: String,
    pub member_ids: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct FileIdentity {
    pub device: Option<u64>,
    pub inode: Option<u64>,
    pub size: u64,
    pub modified_nanos: Option<u64>,
}

#[derive(Clone, Debug, Serialize)]
pub struct SnapshotInfo {
    pub sha256: String,
    pub identity: FileIdentity,
    pub utf8: bool,
    pub has_bom: bool,
    pub newline: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct DocumentSnapshot {
    pub info: SnapshotInfo,
    pub bytes: Vec<u8>,
    pub text: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReferenceStatus {
    Resolved,
    Missing,
    OutsideRoots,
    Excluded,
    UnsupportedLink,
    Remote,
    UnsupportedScheme,
    FragmentOnly,
}

#[derive(Clone, Debug, Serialize)]
pub struct Reference {
    pub destination: String,
    pub line: usize,
    pub target: Option<NativePath>,
    pub target_id: Option<String>,
    pub status: ReferenceStatus,
    pub fragment_checked: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct LinkedSource {
    pub path: NativePath,
    pub target: Option<NativePath>,
    pub inspected: bool,
    pub reason: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct SourceRoot {
    pub path: NativePath,
    pub provenance: Provenance,
    pub available: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct InventoryProgress {
    pub phase: String,
    pub path: NativePath,
    pub visited: usize,
    pub artifacts: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct InstructionDecision {
    pub artifact_id: String,
    pub bytes_included: usize,
    pub reason: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct Availability {
    pub artifact_id: String,
    pub state: String,
    pub reason: String,
    pub implicit_invocation: Option<bool>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ScopeAssessment {
    pub provider: String,
    pub context: NativePath,
    pub checkout_id: Option<String>,
    pub assumed_config_layers: Vec<String>,
    pub instructions: Vec<InstructionDecision>,
    pub skills: Vec<Availability>,
    pub agents: Vec<Availability>,
    pub instruction_byte_limit: Option<usize>,
    pub unknowns: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct Inventory {
    pub schema_version: u32,
    pub provider: String,
    pub status: ScanStatus,
    pub repositories: ScanReport,
    pub sources: Vec<SourceRoot>,
    pub artifacts: Vec<Artifact>,
    pub packages: Vec<SkillPackage>,
    pub linked_sources: Vec<LinkedSource>,
    pub diagnostics: Vec<Diagnostic>,
    pub scope: Option<ScopeAssessment>,
    #[serde(skip)]
    pub(crate) snapshots: HashMap<String, DocumentSnapshot>,
    #[serde(skip)]
    pub(crate) options: InventoryOptions,
}

#[derive(Serialize)]
pub struct ArtifactView<'a> {
    pub artifact: &'a Artifact,
    pub metadata: &'a Value,
    pub snapshot: Option<&'a DocumentSnapshot>,
    pub package: Option<&'a SkillPackage>,
    pub related: Vec<&'a Artifact>,
    pub diagnostics: Vec<&'a Diagnostic>,
    pub scope: Option<&'a ScopeAssessment>,
}

impl Inventory {
    pub fn selected_provider(&self) -> crate::providers::Provider {
        self.options.provider
    }
    pub fn inspect(&self, id: &str) -> Option<ArtifactView<'_>> {
        let artifact = self.artifacts.iter().find(|a| a.id == id)?;
        let package = artifact
            .package_id
            .as_ref()
            .and_then(|id| self.packages.iter().find(|p| &p.id == id));
        let related = self
            .artifacts
            .iter()
            .filter(|a| {
                a.id != id
                    && (package.is_some_and(|p| p.member_ids.contains(&a.id))
                        || artifact
                            .references
                            .iter()
                            .any(|r| r.target_id.as_deref() == Some(a.id.as_str())))
            })
            .collect();
        Some(ArtifactView {
            artifact,
            metadata: &artifact.metadata,
            snapshot: self.snapshots.get(id),
            package,
            related,
            diagnostics: self
                .diagnostics
                .iter()
                .filter(|d| d.artifact_id.as_deref() == Some(id))
                .collect(),
            scope: self.scope.as_ref(),
        })
    }
}
