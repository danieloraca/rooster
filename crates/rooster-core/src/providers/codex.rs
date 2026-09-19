use super::{ArtifactProvider, ArtifactTemplate, MetadataIssue, ParsedArtifact};
use crate::artifacts::{ArtifactKind, Severity, Validation};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

pub struct CodexProvider;

impl ArtifactProvider for CodexProvider {
    fn parse(&self, kind: ArtifactKind, bytes: &[u8]) -> ParsedArtifact {
        let mut result = ParsedArtifact::default();
        let Ok(text) = std::str::from_utf8(bytes) else {
            issue(
                &mut result,
                Severity::Error,
                "unsupported_encoding",
                "The source is not UTF-8; its original bytes are preserved.",
            );
            result.validation = Validation::Unsupported;
            return result;
        };
        let text = text.trim_start_matches('\u{feff}');
        let parsed: Result<Value, Validation> = match kind {
            ArtifactKind::Skill => match frontmatter(text) {
                Some((yaml, _)) => parse_yaml(yaml),
                None => {
                    issue(
                        &mut result,
                        Severity::Error,
                        "missing_frontmatter",
                        "SKILL.md needs a delimited YAML header.",
                    );
                    return result;
                }
            },
            ArtifactKind::SkillMetadata => parse_yaml(text),
            ArtifactKind::Agent | ArtifactKind::LegacyAgent | ArtifactKind::ProviderConfig => text
                .parse::<toml::Table>()
                .map_err(|_| Validation::Malformed)
                .and_then(|table| serde_json::to_value(table).map_err(|_| Validation::Unsupported)),
            ArtifactKind::PluginManifest => {
                serde_json::from_str(text).map_err(|_| Validation::Malformed)
            }
            _ => return result,
        };
        result.metadata = match parsed {
            Ok(value) if value.is_object() => value,
            Err(Validation::Unsupported) => {
                issue(
                    &mut result,
                    Severity::Warning,
                    "unsupported_metadata",
                    "Metadata uses an unsupported feature or exceeds parser limits; source bytes are preserved.",
                );
                result.validation = Validation::Unsupported;
                return result;
            }
            _ => {
                issue(
                    &mut result,
                    Severity::Error,
                    "malformed_metadata",
                    "Metadata must be a syntactically valid mapping in its native format.",
                );
                return result;
            }
        };
        let known: &[&str] = match kind {
            ArtifactKind::Skill => {
                required_text(&mut result, "name");
                required_text(&mut result, "description");
                &[
                    "name",
                    "description",
                    "license",
                    "compatibility",
                    "metadata",
                    "allowed-tools",
                ]
            }
            ArtifactKind::Agent => {
                for field in ["name", "description", "developer_instructions"] {
                    required_text(&mut result, field);
                }
                validate_agent_options(&mut result);
                &[
                    "name",
                    "description",
                    "developer_instructions",
                    "model",
                    "model_reasoning_effort",
                    "sandbox_mode",
                    "approval_policy",
                    "mcp_servers",
                    "skills",
                    "agents",
                ]
            }
            ArtifactKind::LegacyAgent => {
                validate_agent_options(&mut result);
                issue(
                    &mut result,
                    Severity::Info,
                    "legacy_role",
                    "This file is a role configuration layer; standalone-agent requirements are not assumed.",
                );
                &[
                    "name",
                    "description",
                    "developer_instructions",
                    "model",
                    "model_reasoning_effort",
                    "sandbox_mode",
                    "approval_policy",
                    "mcp_servers",
                    "skills",
                    "agents",
                ]
            }
            ArtifactKind::SkillMetadata => {
                for field in ["interface", "policy", "dependencies"] {
                    optional_type(&mut result, field, Value::is_object, "mapping");
                }
                if let Some(interface) = result
                    .metadata
                    .get("interface")
                    .and_then(Value::as_object)
                    .cloned()
                {
                    for (key, value) in interface {
                        if [
                            "display_name",
                            "short_description",
                            "icon_small",
                            "icon_large",
                            "brand_color",
                            "default_prompt",
                        ]
                        .contains(&key.as_str())
                        {
                            if !value.is_string() {
                                issue(
                                    &mut result,
                                    Severity::Error,
                                    "invalid_field",
                                    &format!("interface.{key} must be text."),
                                );
                            }
                        } else {
                            result.unknown_fields.push(format!("interface.{key}"));
                        }
                    }
                }
                if let Some(policy) = result
                    .metadata
                    .get("policy")
                    .and_then(Value::as_object)
                    .cloned()
                {
                    for (key, value) in policy {
                        if key == "allow_implicit_invocation" {
                            if !value.is_boolean() {
                                issue(
                                    &mut result,
                                    Severity::Error,
                                    "invalid_field",
                                    "policy.allow_implicit_invocation must be boolean.",
                                );
                            }
                        } else {
                            result.unknown_fields.push(format!("policy.{key}"));
                        }
                    }
                }
                &["interface", "policy", "dependencies"]
            }
            ArtifactKind::ProviderConfig => {
                for field in ["agents", "skills"] {
                    optional_type(&mut result, field, Value::is_object, "mapping");
                }
                if let Some(names) = result.metadata.get("project_doc_fallback_filenames")
                    && !names
                        .as_array()
                        .is_some_and(|a| a.iter().all(|v| v.as_str().is_some_and(valid_filename)))
                {
                    issue(
                        &mut result,
                        Severity::Error,
                        "invalid_fallbacks",
                        "project_doc_fallback_filenames must contain plain filenames.",
                    );
                }
                if let Some(limit) = result.metadata.get("project_doc_max_bytes")
                    && limit.as_u64().is_none()
                {
                    issue(
                        &mut result,
                        Severity::Error,
                        "invalid_byte_limit",
                        "project_doc_max_bytes must be a non-negative integer.",
                    );
                }
                validate_discovery_config(&mut result);
                &[
                    "agents",
                    "skills",
                    "project_doc_fallback_filenames",
                    "project_doc_max_bytes",
                    "projects",
                    "profiles",
                    "profile",
                    "model",
                    "model_reasoning_effort",
                    "sandbox_mode",
                    "approval_policy",
                    "mcp_servers",
                    "plugins",
                ]
            }
            ArtifactKind::PluginManifest => &[
                "name",
                "description",
                "version",
                "skills",
                "agents",
                "mcpServers",
                "author",
                "homepage",
                "repository",
                "license",
                "keywords",
                "logo",
                "interface",
            ],
            _ => &[],
        };
        let unknown: Vec<_> = result
            .metadata
            .as_object()
            .unwrap()
            .keys()
            .filter(|key| !known.contains(&key.as_str()))
            .cloned()
            .collect();
        result.unknown_fields.extend(unknown);
        result.unknown_fields.sort();
        if !result.unknown_fields.is_empty() {
            issue(
                &mut result,
                Severity::Warning,
                "unsupported_fields",
                "Uninterpreted metadata fields are preserved; their provider semantics are unknown.",
            );
            if result.validation == Validation::Valid {
                result.validation = Validation::Unsupported;
            }
        }
        result.name = result
            .metadata
            .get("name")
            .and_then(Value::as_str)
            .map(str::to_owned);
        result.description = result
            .metadata
            .get("description")
            .and_then(Value::as_str)
            .map(str::to_owned);
        result
    }

    fn template(
        &self,
        kind: ArtifactKind,
        name: &str,
        description: &str,
    ) -> Result<ArtifactTemplate, String> {
        if matches!(kind, ArtifactKind::Skill | ArtifactKind::Agent)
            && (name.is_empty()
                || !name
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
                || description.trim().is_empty())
        {
            return Err("A template needs a simple name (letters, digits, hyphen or underscore) and a non-empty description.".into());
        }
        let (filename, content) = match kind {
            ArtifactKind::Instruction => ("AGENTS.md".into(), "# Project guidance\n\nDescribe the project's working conventions here.\n".into()),
            ArtifactKind::Skill => ("SKILL.md".into(), format!("---\nname: {}\ndescription: {}\n---\n\n# Workflow\n\nDescribe the steps and expected result here.\n",
                serde_json::to_string(name).unwrap(), serde_json::to_string(description).unwrap())),
            ArtifactKind::Agent => (format!("{name}.toml"), toml::to_string(&json!({
                "name": name, "description": description, "developer_instructions": "Describe this agent's task, boundaries, and expected result."
            })).map_err(|e| e.to_string())?),
            _ => return Err("Templates are available for instructions, skills, and standalone agents.".into()),
        };
        Ok(ArtifactTemplate {
            provider: "codex".into(),
            kind,
            suggested_filename: filename,
            content,
        })
    }
}

pub(super) fn issue(
    result: &mut ParsedArtifact,
    severity: Severity,
    code: &'static str,
    message: &str,
) {
    if severity == Severity::Error {
        result.validation = Validation::Malformed;
    }
    result.issues.push(MetadataIssue {
        severity,
        code,
        line: None,
        message: message.into(),
    });
}

pub(super) fn required_text(result: &mut ParsedArtifact, field: &str) {
    if result
        .metadata
        .get(field)
        .and_then(Value::as_str)
        .is_none_or(|s| s.trim().is_empty())
    {
        issue(
            result,
            Severity::Error,
            "required_field",
            &format!("{field} must be non-empty text."),
        );
    }
}

pub(super) fn optional_type(
    result: &mut ParsedArtifact,
    field: &str,
    check: fn(&Value) -> bool,
    expected: &str,
) {
    if result
        .metadata
        .get(field)
        .is_some_and(|value| !check(value))
    {
        issue(
            result,
            Severity::Error,
            "invalid_field",
            &format!("{field} must be a {expected}."),
        );
    }
}

fn validate_agent_options(result: &mut ParsedArtifact) {
    for field in [
        "name",
        "description",
        "developer_instructions",
        "model",
        "model_reasoning_effort",
        "sandbox_mode",
    ] {
        optional_type(result, field, Value::is_string, "string");
    }
    for field in ["mcp_servers", "skills", "agents"] {
        optional_type(result, field, Value::is_object, "mapping");
    }
}

fn validate_discovery_config(result: &mut ParsedArtifact) {
    if let Some(overrides) = result.metadata.pointer("/skills/config") {
        let valid = overrides.as_array().is_some_and(|list| {
            list.iter().all(|entry| {
                entry.get("path").and_then(Value::as_str).is_some()
                    && entry.get("enabled").is_some_and(Value::is_boolean)
            })
        });
        if !valid {
            issue(
                result,
                Severity::Error,
                "invalid_skill_override",
                "skills.config entries need a path string and an enabled boolean.",
            );
        }
    }
    if let Some(agents) = result
        .metadata
        .get("agents")
        .and_then(Value::as_object)
        .cloned()
    {
        for (name, role) in agents {
            if !role.is_object() {
                continue;
            }
            if role.get("config_file").is_some_and(|v| !v.is_string())
                || role.get("description").is_some_and(|v| !v.is_string())
            {
                issue(
                    result,
                    Severity::Error,
                    "invalid_role",
                    &format!("agents.{name} role paths and descriptions must be strings."),
                );
            }
        }
    }
}

pub(crate) fn valid_filename(name: &str) -> bool {
    !name.is_empty() && name != "." && name != ".." && !name.contains(['/', '\\', '\0'])
}

pub(super) fn parse_yaml(text: &str) -> Result<Value, Validation> {
    let options = serde_saphyr::options! {
        budget: serde_saphyr::budget! { max_depth: 64, max_nodes: 50_000, max_events: 200_000,
            max_total_scalar_bytes: 2_097_152, max_aliases: 100, max_anchors: 100 },
        strict_booleans: true, reject_unsupported_tags: true, with_snippet: false,
    };
    serde_saphyr::from_str_with_options(text, options).map_err(|error| match error {
        serde_saphyr::Error::UnsupportedTag { .. } | serde_saphyr::Error::Budget { .. } => {
            Validation::Unsupported
        }
        _ => Validation::Malformed,
    })
}

/// Return only the YAML header and the body offset; parsing never rewrites source text.
pub(crate) fn frontmatter(text: &str) -> Option<(&str, usize)> {
    let start = text.find('\n')? + 1;
    if text[..start].trim_end_matches(['\r', '\n']) != "---" {
        return None;
    }
    let mut offset = start;
    for line in text[start..].split_inclusive('\n') {
        if matches!(line.trim_end_matches(['\r', '\n']), "---" | "...") {
            return Some((&text[start..offset], offset + line.len()));
        }
        offset += line.len();
    }
    None
}

#[derive(Clone, Debug)]
pub(crate) struct RoleDeclaration {
    pub name: String,
    pub description: Option<String>,
    pub path: PathBuf,
}

pub(crate) fn roles(metadata: &Value, config_path: &Path) -> Vec<RoleDeclaration> {
    let Some(agents) = metadata.get("agents").and_then(Value::as_object) else {
        return Vec::new();
    };
    agents
        .iter()
        .filter_map(|(name, role)| {
            let path = role.get("config_file")?.as_str()?;
            let path = resolve_config_path(config_path, path);
            Some(RoleDeclaration {
                name: name.clone(),
                description: role
                    .get("description")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
                path,
            })
        })
        .collect()
}

pub(crate) fn fallback_names(metadata: &Value) -> Option<Vec<String>> {
    let values = metadata.get("project_doc_fallback_filenames")?.as_array()?;
    values
        .iter()
        .map(|v| v.as_str().filter(|s| valid_filename(s)).map(str::to_owned))
        .collect()
}

pub(crate) fn resolve_config_path(config_path: &Path, value: &str) -> PathBuf {
    let path = Path::new(value);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        config_path.parent().unwrap_or(Path::new("")).join(path)
    }
}
