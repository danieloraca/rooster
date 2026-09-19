pub mod claude;
pub(crate) mod claude_scope;
pub mod codex;

use crate::artifacts::{ArtifactKind, Severity, Validation};
use serde::Serialize;
use serde_json::Value;

#[derive(Clone, Debug)]
pub struct MetadataIssue {
    pub severity: Severity,
    pub code: &'static str,
    pub line: Option<usize>,
    pub message: String,
}

#[derive(Clone, Debug)]
pub struct ParsedArtifact {
    pub metadata: Value,
    pub name: Option<String>,
    pub description: Option<String>,
    pub validation: Validation,
    pub unknown_fields: Vec<String>,
    pub issues: Vec<MetadataIssue>,
}

impl Default for ParsedArtifact {
    fn default() -> Self {
        Self {
            metadata: Value::Null,
            name: None,
            description: None,
            validation: Validation::Valid,
            unknown_fields: Vec::new(),
            issues: Vec::new(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ArtifactTemplate {
    pub provider: String,
    pub kind: ArtifactKind,
    pub suggested_filename: String,
    pub content: String,
}

/// Provider syntax and interpretation stay separate from filesystem traversal.
pub trait ArtifactProvider {
    fn parse(&self, kind: ArtifactKind, bytes: &[u8]) -> ParsedArtifact;
    fn template(
        &self,
        kind: ArtifactKind,
        name: &str,
        description: &str,
    ) -> Result<ArtifactTemplate, String>;
}

/// Explicit provider selection; old inventories and drafts default to Codex.
#[derive(Clone, Copy, Debug, Default, serde::Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Provider {
    #[default]
    Codex,
    Claude,
}
impl Provider {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Codex => "codex",
            Self::Claude => "claude",
        }
    }
}
impl std::fmt::Display for Provider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}
impl std::str::FromStr for Provider {
    type Err = String;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "codex" => Ok(Self::Codex),
            "claude" => Ok(Self::Claude),
            _ => Err("Provider must be codex or claude".into()),
        }
    }
}
impl ArtifactProvider for Provider {
    fn parse(&self, kind: ArtifactKind, bytes: &[u8]) -> ParsedArtifact {
        match self {
            Self::Codex => codex::CodexProvider.parse(kind, bytes),
            Self::Claude => claude::ClaudeProvider.parse(kind, bytes),
        }
    }
    fn template(
        &self,
        kind: ArtifactKind,
        name: &str,
        description: &str,
    ) -> Result<ArtifactTemplate, String> {
        match self {
            Self::Codex => codex::CodexProvider.template(kind, name, description),
            Self::Claude => claude::ClaudeProvider.template(kind, name, description),
        }
    }
}

/// Keep an ordinary-Markdown view from bypassing another provider's native validation.
pub(crate) fn foreign_path(provider: Provider, path: &std::path::Path) -> bool {
    let name = path.file_name().and_then(|n| n.to_str());
    match provider {
        Provider::Codex => {
            matches!(name, Some("CLAUDE.md" | "CLAUDE.local.md"))
                || path
                    .components()
                    .any(|p| p.as_os_str().eq_ignore_ascii_case(".claude"))
        }
        Provider::Claude => {
            matches!(name, Some("AGENTS.md" | "AGENTS.override.md"))
                || path.components().any(|p| {
                    p.as_os_str().eq_ignore_ascii_case(".codex")
                        || p.as_os_str().eq_ignore_ascii_case(".agents")
                })
        }
    }
}
