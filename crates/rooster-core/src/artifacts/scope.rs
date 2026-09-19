use super::*;
use crate::NativePath;
use crate::providers::codex;
use std::{collections::BTreeMap, path::Path};

pub(super) fn applicable_configs<'a>(
    inventory: &'a Inventory,
    context: &Path,
) -> Vec<&'a Artifact> {
    let checkout = inventory
        .repositories
        .checkouts
        .iter()
        .filter(|c| context.starts_with(c.path.as_path()))
        .max_by_key(|c| c.path.as_path().components().count());
    let checkout_id = checkout.map(|c| c.id.clone());
    let mut configs: Vec<_> = inventory
        .artifacts
        .iter()
        .filter(|a| {
            a.kind == ArtifactKind::ProviderConfig
                && a.provenance.is_authoring()
                && (a.scope_checkout_id.is_none()
                    || (a.scope_checkout_id == checkout_id
                        && context.starts_with(a.scope_directory.as_path())))
        })
        .collect();
    configs.sort_by_key(|a| {
        (
            a.scope_checkout_id.is_some(),
            a.scope_directory.as_path().components().count(),
            a.path.clone(),
        )
    });
    configs
}

pub fn assess(inventory: &Inventory, context: &Path) -> ScopeAssessment {
    if inventory.selected_provider() == crate::providers::Provider::Claude {
        return crate::providers::claude_scope::assess(inventory, context);
    }
    let checkout = inventory
        .repositories
        .checkouts
        .iter()
        .filter(|c| context.starts_with(c.path.as_path()))
        .max_by_key(|c| c.path.as_path().components().count());
    let checkout_id = checkout.map(|c| c.id.clone());
    let configs = applicable_configs(inventory, context);
    let mut fallback_names = Vec::new();
    let mut byte_limit = 32 * 1024;
    let mut overrides = BTreeMap::new();
    let mut agents_enabled = true;
    for config in &configs {
        if let Some(names) = codex::fallback_names(&config.metadata) {
            fallback_names = names;
        }
        if let Some(limit) = config
            .metadata
            .get("project_doc_max_bytes")
            .and_then(serde_json::Value::as_u64)
        {
            byte_limit = usize::try_from(limit).unwrap_or(usize::MAX);
        }
        if let Some(enabled) = config
            .metadata
            .pointer("/agents/enabled")
            .and_then(serde_json::Value::as_bool)
        {
            agents_enabled = enabled;
        }
        if let Some(entries) = config
            .metadata
            .pointer("/skills/config")
            .and_then(serde_json::Value::as_array)
        {
            for entry in entries {
                if let (Some(path), Some(enabled)) = (
                    entry.get("path").and_then(serde_json::Value::as_str),
                    entry.get("enabled").and_then(serde_json::Value::as_bool),
                ) {
                    let path = super::links::lexical(&codex::resolve_config_path(
                        config.physical_path.as_path(),
                        path,
                    ));
                    overrides.insert(path, enabled);
                }
            }
        }
    }
    let mut assessment = ScopeAssessment { provider:"codex".into(), context: context.to_path_buf().into(), checkout_id: checkout_id.clone(),
        assumed_config_layers: configs.iter().map(|a| a.id.clone()).collect(), instructions: Vec::new(),
        skills: Vec::new(), agents: Vec::new(), instruction_byte_limit: Some(byte_limit),
        unknowns: vec![
            "Disk inventory does not establish what any running Codex session loaded.".into(),
            "Project trust, selected profile, command-line overrides, managed policy, and model/catalog budgets are unknown.".into(),
            "Applicable project config layers are assumed trusted for this estimate; a different runtime can select different guidance.".into(),
            "Git roots are used as project boundaries; custom project-root markers and non-repository lookup are not evaluated.".into(),
            "Plugin installation/enablement and compatibility-cache activation are not inferred from directory presence.".into(),
        ] };
    if checkout.is_none() {
        assessment.unknowns.push(
            "Context is outside the scanned checkouts; repository guidance was not assessed."
                .into(),
        );
    }
    if configs.iter().any(|c| c.validation != Validation::Valid) {
        assessment.unknowns.push("Some settings have malformed or uninterpreted fields; the estimate uses only recognized values.".into());
    }
    let mut instruction_groups: BTreeMap<(bool, usize, NativePath), Vec<&Artifact>> =
        BTreeMap::new();
    for artifact in inventory
        .artifacts
        .iter()
        .filter(|a| a.kind == ArtifactKind::Instruction)
    {
        let personal = artifact.provenance == Provenance::Personal
            && artifact.scope_checkout_id.is_none()
            && artifact.path.as_path().parent() == Some(artifact.source_root.as_path());
        let project = artifact.provenance == Provenance::Repository
            && checkout_id.is_some()
            && artifact.scope_checkout_id == checkout_id
            && context.starts_with(artifact.scope_directory.as_path());
        if personal || project {
            instruction_groups
                .entry((
                    !personal,
                    artifact.scope_directory.as_path().components().count(),
                    artifact.scope_directory.clone(),
                ))
                .or_default()
                .push(artifact);
        }
    }
    let mut remaining = byte_limit;
    let mut selected_any = false;
    for ((project, _, _), files) in instruction_groups {
        let mut names = vec!["AGENTS.override.md".to_string(), "AGENTS.md".to_string()];
        if project {
            names.extend(fallback_names.clone());
        }
        let mut selected = false;
        for name in names {
            let Some(artifact) = files.iter().find(|a| {
                a.path
                    .as_path()
                    .file_name()
                    .is_some_and(|n| n == name.as_str())
            }) else {
                continue;
            };
            let Some(text) = inventory
                .snapshots
                .get(&artifact.id)
                .and_then(|s| s.text.as_deref())
            else {
                assessment.instructions.push(InstructionDecision {
                    artifact_id: artifact.id.clone(),
                    bytes_included: 0,
                    reason: "Unavailable source; selection is uncertain.".into(),
                });
                assessment.unknowns.push(
                    "An instruction could not be read; the displayed chain may be incomplete."
                        .into(),
                );
                continue;
            };
            let text = text.trim_start_matches('\u{feff}').trim();
            let (bytes, reason) = if text.is_empty() {
                (0, "Empty instruction skipped.")
            } else if selected {
                (0, "A higher-priority file in this directory was selected.")
            } else {
                selected = true;
                let available = remaining.saturating_sub(if selected_any { 2 } else { 0 });
                let bytes = text.len().min(available);
                let mut boundary = bytes;
                while !text.is_char_boundary(boundary) {
                    boundary -= 1;
                }
                remaining = available.saturating_sub(boundary);
                if boundary > 0 {
                    selected_any = true;
                }
                (
                    boundary,
                    if boundary == 0 {
                        "Instruction byte budget exhausted."
                    } else if boundary < text.len() {
                        "Candidate is truncated by the estimated instruction byte budget."
                    } else {
                        "Candidate selected by directory and filename precedence."
                    },
                )
            };
            assessment.instructions.push(InstructionDecision {
                artifact_id: artifact.id.clone(),
                bytes_included: bytes,
                reason: reason.into(),
            });
        }
    }
    for artifact in inventory.artifacts.iter().filter(|a| {
        matches!(
            a.kind,
            ArtifactKind::Skill | ArtifactKind::Agent | ArtifactKind::LegacyAgent
        )
    }) {
        let mut availability = Availability {
            artifact_id: artifact.id.clone(),
            state: "unknown".into(),
            reason: "Activation requires provider/runtime information.".into(),
            implicit_invocation: None,
        };
        if artifact.kind == ArtifactKind::LegacyAgent && artifact.role_declaration_count > 1 {
            availability.reason = "Shared role configuration layer; inspect declaring configs to assess each role's scope.".into();
            assessment.agents.push(availability);
            continue;
        }
        let in_scope = artifact.scope_checkout_id.is_none()
            || (checkout_id.is_some()
                && artifact.scope_checkout_id == checkout_id
                && context.starts_with(artifact.scope_directory.as_path()));
        if !in_scope {
            availability.state = "out_of_scope".into();
            availability.reason =
                "Definition belongs to a different checkout or directory branch.".into();
        } else if matches!(
            artifact.validation,
            Validation::Malformed | Validation::Unavailable
        ) {
            availability.state = "invalid".into();
            availability.reason = "Definition is malformed or could not be read.".into();
        } else if artifact.provenance.is_authoring()
            && artifact.metadata.is_object()
            && artifact.kind != ArtifactKind::LegacyAgent
        {
            availability.state = "candidate".into();
            availability.reason =
                "Local definition is in the estimated directory scope; runtime loading is unknown."
                    .into();
        }
        if in_scope && artifact.kind == ArtifactKind::Skill {
            let enabled = overrides.get(artifact.physical_path.as_path()).or_else(|| {
                artifact
                    .physical_path
                    .as_path()
                    .parent()
                    .and_then(|p| overrides.get(p))
            });
            if enabled == Some(&false) {
                availability.state = "disabled_by_config".into();
                availability.reason = "A selected local config layer explicitly disables this skill; runtime overrides remain unknown.".into();
            }
            let policy = inventory.artifacts.iter().find(|a| {
                a.kind == ArtifactKind::SkillMetadata && a.package_id == artifact.package_id
            });
            availability.implicit_invocation = match policy {
                None => Some(true),
                Some(a)
                    if matches!(
                        a.validation,
                        Validation::Malformed | Validation::Unavailable
                    ) || !a.metadata.is_object() =>
                {
                    None
                }
                Some(a) => a
                    .metadata
                    .pointer("/policy/allow_implicit_invocation")
                    .and_then(serde_json::Value::as_bool)
                    .or(Some(true)),
            };
        } else if in_scope && !agents_enabled {
            availability.state = "disabled_by_config".into();
            availability.reason = "The selected local config layers disable subagent tools.".into();
        }
        if artifact.kind == ArtifactKind::Skill {
            assessment.skills.push(availability);
        } else {
            assessment.agents.push(availability);
        }
    }
    assessment
}
