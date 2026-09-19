use super::codex::{frontmatter, issue, optional_type, parse_yaml, required_text};
use super::{ArtifactProvider, ArtifactTemplate, ParsedArtifact};
use crate::artifacts::{ArtifactKind, Severity, Validation};
use pulldown_cmark::{Event, Parser, Tag, TagEnd};
use serde_json::{Value, json};

pub struct ClaudeProvider;

/// Claude accepts these boolean spellings without requiring a source rewrite.
pub fn boolean(value: &Value) -> Option<bool> {
    value.as_bool().or_else(
        || match value.as_str().map(str::to_ascii_lowercase).as_deref() {
            Some("true" | "yes" | "on" | "1") => Some(true),
            Some("false" | "no" | "off" | "0") => Some(false),
            _ => match value.as_u64() {
                Some(1) => Some(true),
                Some(0) => Some(false),
                _ => None,
            },
        },
    )
}
fn text_list(v: &Value) -> bool {
    v.is_string() || v.as_array().is_some_and(|a| a.iter().all(Value::is_string))
}
impl ArtifactProvider for ClaudeProvider {
    fn parse(&self, kind: ArtifactKind, bytes: &[u8]) -> ParsedArtifact {
        let mut result = ParsedArtifact::default();
        let Ok(text) = std::str::from_utf8(bytes) else {
            issue(
                &mut result,
                Severity::Error,
                "unsupported_encoding",
                "Source is not UTF-8; original bytes are retained.",
            );
            result.validation = Validation::Unsupported;
            return result;
        };
        let text = text.trim_start_matches('\u{feff}');
        let header_kind = matches!(
            kind,
            ArtifactKind::Skill
                | ArtifactKind::Agent
                | ArtifactKind::Rule
                | ArtifactKind::LegacyCommand
        );
        let mut body = text;
        let parsed = if header_kind {
            if let Some((yaml, offset)) = frontmatter(text) {
                body = &text[offset..];
                if yaml.trim().is_empty() {
                    Ok(json!({}))
                } else {
                    parse_yaml(yaml)
                }
            } else if kind == ArtifactKind::Agent || text.lines().next() == Some("---") {
                issue(
                    &mut result,
                    Severity::Error,
                    "missing_frontmatter",
                    "A delimited YAML header is required or its closing delimiter is missing.",
                );
                return result;
            } else {
                Ok(json!({}))
            }
        } else if matches!(
            kind,
            ArtifactKind::ProviderConfig | ArtifactKind::PluginManifest
        ) {
            serde_json::from_str(text).map_err(|_| Validation::Malformed)
        } else {
            return result;
        };
        result.metadata = match parsed {
            Ok(value) if value.is_object() => value,
            Err(Validation::Unsupported) => {
                issue(
                    &mut result,
                    Severity::Warning,
                    "unsupported_metadata",
                    "Metadata exceeds parser limits or uses unsupported YAML features; bytes are retained.",
                );
                result.validation = Validation::Unsupported;
                return result;
            }
            _ => {
                issue(
                    &mut result,
                    Severity::Error,
                    "malformed_metadata",
                    "Metadata must be a valid mapping in its native YAML or strict JSON format.",
                );
                return result;
            }
        };
        let known: &[&str] = match kind {
            ArtifactKind::Agent => {
                required_text(&mut result, "name");
                required_text(&mut result, "description");
                if result
                    .metadata
                    .get("name")
                    .and_then(Value::as_str)
                    .is_some_and(|n| n.contains(':') || n.starts_with('-'))
                {
                    issue(
                        &mut result,
                        Severity::Error,
                        "invalid_agent_name",
                        "Agent names cannot start with a hyphen or contain a colon.",
                    );
                }
                for field in ["tools", "disallowedTools", "skills"] {
                    optional_type(&mut result, field, text_list, "text or a list of text");
                }
                optional_type(
                    &mut result,
                    "maxTurns",
                    |v| v.as_u64().is_some_and(|n| n > 0),
                    "positive integer",
                );
                optional_type(&mut result, "hooks", Value::is_object, "mapping");
                optional_type(&mut result, "experimental", Value::is_object, "mapping");
                for field in ["background", "omitClaudeMd"] {
                    optional_type(&mut result, field, |v| boolean(v).is_some(), "boolean");
                }
                &[
                    "name",
                    "description",
                    "tools",
                    "disallowedTools",
                    "model",
                    "permissionMode",
                    "maxTurns",
                    "skills",
                    "mcpServers",
                    "hooks",
                    "memory",
                    "background",
                    "omitClaudeMd",
                    "effort",
                    "isolation",
                    "color",
                    "initialPrompt",
                    "experimental",
                ]
            }
            ArtifactKind::Skill | ArtifactKind::LegacyCommand => {
                for field in ["disable-model-invocation", "user-invocable", "background"] {
                    optional_type(&mut result, field, |v| boolean(v).is_some(), "boolean");
                }
                for field in ["allowed-tools", "disallowed-tools", "arguments", "paths"] {
                    optional_type(&mut result, field, text_list, "text or a list of text");
                }
                for field in ["hooks", "metadata"] {
                    optional_type(&mut result, field, Value::is_object, "mapping");
                }
                &[
                    "name",
                    "description",
                    "when_to_use",
                    "argument-hint",
                    "arguments",
                    "disable-model-invocation",
                    "user-invocable",
                    "allowed-tools",
                    "disallowed-tools",
                    "model",
                    "effort",
                    "context",
                    "agent",
                    "background",
                    "hooks",
                    "paths",
                    "shell",
                    "metadata",
                    "license",
                    "compatibility",
                ]
            }
            ArtifactKind::Rule => {
                optional_type(&mut result, "paths", text_list, "text or a list of text");
                &["paths"]
            }
            ArtifactKind::ProviderConfig => {
                for field in [
                    "permissions",
                    "hooks",
                    "env",
                    "enabledPlugins",
                    "skillOverrides",
                ] {
                    optional_type(&mut result, field, Value::is_object, "mapping");
                }
                for field in ["syncClaudeAiSkills", "strictPluginOnlyCustomization"] {
                    optional_type(&mut result, field, Value::is_boolean, "boolean");
                }
                &[
                    "permissions",
                    "hooks",
                    "env",
                    "enabledPlugins",
                    "extraKnownMarketplaces",
                    "skillOverrides",
                    "syncClaudeAiSkills",
                    "strictPluginOnlyCustomization",
                    "claudeMdExcludes",
                    "model",
                    "agent",
                    "availableModels",
                    "disableAllHooks",
                    "disableBundledSkills",
                    "sandbox",
                    "outputStyle",
                    "statusLine",
                    "language",
                    "effortLevel",
                ]
            }
            ArtifactKind::PluginManifest => &[
                "name",
                "description",
                "version",
                "author",
                "homepage",
                "repository",
                "license",
                "keywords",
                "commands",
                "agents",
                "skills",
                "hooks",
                "mcpServers",
                "outputStyles",
                "lspServers",
            ],
            _ => &[],
        };
        for field in ["name", "description", "model"] {
            optional_type(&mut result, field, Value::is_string, "text");
        }
        result.unknown_fields = result
            .metadata
            .as_object()
            .unwrap()
            .keys()
            .filter(|k| !known.contains(&k.as_str()))
            .cloned()
            .collect();
        if !result.unknown_fields.is_empty() {
            issue(
                &mut result,
                Severity::Warning,
                "unsupported_fields",
                "Uninterpreted Claude metadata is preserved; its semantics are unknown.",
            );
            if result.validation == Validation::Valid {
                result.validation = Validation::Unsupported;
            }
        }
        if result.metadata.get("paths").is_some() {
            issue(
                &mut result,
                Severity::Info,
                "conditional_paths",
                "Path conditions depend on files accessed by the session; Rooster does not evaluate their runtime activation.",
            );
        }
        if kind == ArtifactKind::LegacyCommand {
            issue(
                &mut result,
                Severity::Info,
                "legacy_command",
                "Legacy command retained as its native file; no conversion to a skill is performed. Filename determines invocation; name and paths metadata are not applied.",
            );
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
            .map(str::to_owned)
            .or_else(|| {
                matches!(kind, ArtifactKind::Skill | ArtifactKind::LegacyCommand).then(|| {
                    body.lines()
                        .find(|line| !line.trim().is_empty())
                        .unwrap_or("")
                        .trim()
                        .to_string()
                })
            });
        result
    }
    fn template(
        &self,
        kind: ArtifactKind,
        name: &str,
        description: &str,
    ) -> Result<ArtifactTemplate, String> {
        if name.is_empty()
            || !name
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
            || description.trim().is_empty()
        {
            return Err(
                "Use a lowercase name with digits/hyphens and a non-empty description.".into(),
            );
        }
        let (filename, content) = match kind {
            ArtifactKind::Instruction => ("CLAUDE.md".into(), "# Project guidance\n\nDescribe working conventions here.\n".into()),
            ArtifactKind::Rule => (format!("{name}.md"), "---\npaths:\n  - \"src/**\"\n---\n\n# Scoped guidance\n\nDescribe the convention here.\n".into()),
            ArtifactKind::Skill | ArtifactKind::Agent | ArtifactKind::LegacyCommand => (
                if kind == ArtifactKind::Skill {"SKILL.md".into()} else {format!("{name}.md")},
                format!("---\nname: {}\ndescription: {}\n---\n\nDescribe the task, boundaries, and expected result.\n", serde_json::to_string(name).unwrap(), serde_json::to_string(description).unwrap())),
            _ => return Err("No Claude template for this artifact type.".into()),
        };
        Ok(ArtifactTemplate {
            provider: "claude".into(),
            kind,
            suggested_filename: filename,
            content,
        })
    }
}

/// Literal @file references outside code. Dynamic expressions/quoted paths are not expanded.
pub(crate) fn imports(text: &str) -> Vec<(String, usize)> {
    let mut out = vec![];
    let mut code = false;
    for (event, range) in Parser::new(text).into_offset_iter() {
        match event {
            Event::Start(Tag::CodeBlock(_)) => code = true,
            Event::End(TagEnd::CodeBlock) => code = false,
            Event::Text(_) if !code => {
                let source = &text[range.clone()];
                for (offset, _) in source.match_indices('@') {
                    if offset > 0 && !source[..offset].ends_with(char::is_whitespace) {
                        continue;
                    }
                    let path = source[offset + 1..].split_whitespace().next().unwrap_or("");
                    if path.is_empty() || path.contains(['"', '\'', '`', '{', '}', '<', '>']) {
                        continue;
                    }
                    out.push((
                        path.to_string(),
                        text[..range.start + offset]
                            .bytes()
                            .filter(|b| *b == b'\n')
                            .count()
                            + 1,
                    ));
                }
            }
            _ => {}
        }
    }
    out
}
