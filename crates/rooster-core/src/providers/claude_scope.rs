use crate::artifacts::*;
use std::path::Path;

pub(crate) fn identity(a: &Artifact) -> Option<String> {
    match a.kind {
        ArtifactKind::Skill => a
            .path
            .as_path()
            .parent()?
            .file_name()
            .map(|n| n.to_string_lossy().into_owned()),
        ArtifactKind::LegacyCommand => a
            .path
            .as_path()
            .file_stem()
            .map(|n| n.to_string_lossy().into_owned()),
        _ => a.name.clone(),
    }
}
fn rank(a: &Artifact) -> (bool, bool, usize) {
    if a.kind == ArtifactKind::Agent {
        (
            a.scope_checkout_id.is_some(),
            true,
            a.scope_directory.as_path().components().count(),
        )
    } else {
        (
            a.kind == ArtifactKind::Skill,
            a.scope_checkout_id.is_none(),
            0,
        )
    }
}
pub(crate) fn assess(inventory: &Inventory, context: &Path) -> ScopeAssessment {
    let checkout = inventory
        .repositories
        .checkouts
        .iter()
        .filter(|c| context.starts_with(c.path.as_path()))
        .max_by_key(|c| c.path.as_path().components().count());
    let checkout_id = checkout.map(|c| c.id.clone());
    let mut result=ScopeAssessment {provider:"claude".into(),context:context.to_path_buf().into(),checkout_id:checkout_id.clone(),assumed_config_layers:vec![],instructions:vec![],skills:vec![],agents:vec![],instruction_byte_limit:None,unknowns:vec![
        "Disk presence does not establish what a running Claude Code session loaded.".into(),
        "Session start directory, accessed files, --add-dir, setting sources, trust, safe/bare mode, managed policy and plugin enablement are unknown.".into(),
        "Rule/skill paths, claudeMdExcludes, skillOverrides, runtime import approval and import depth are not evaluated. Literal references do not add documents to the estimated instruction chain.".into(),
        "Git roots bound this estimate; memory outside registered checkouts and auto-memory are not inventoried as project guidance.".into(),
        "Installed and synced namespaces/activation remain unknown; independent copies preserve bytes without inheriting their source activation.".into(),
    ]};
    let in_scope = |a: &Artifact| {
        a.scope_checkout_id.is_none()
            || checkout_id.is_some()
                && a.scope_checkout_id == checkout_id
                && context.starts_with(a.scope_directory.as_path())
    };
    let local = |a: &Artifact| {
        a.provenance.is_authoring()
            && !matches!(
                a.validation,
                Validation::Malformed | Validation::Unavailable
            )
    };
    for a in &inventory.artifacts {
        if a.kind == ArtifactKind::ProviderConfig && in_scope(a) && a.provenance.is_authoring() {
            result.assumed_config_layers.push(a.id.clone());
        }
        if matches!(a.kind, ArtifactKind::Instruction | ArtifactKind::Rule) && in_scope(a) {
            let conditional = a.kind == ArtifactKind::Rule && a.metadata.get("paths").is_some();
            let selected = local(a) && !conditional;
            result.instructions.push(InstructionDecision {artifact_id:a.id.clone(),bytes_included:if selected {a.snapshot.as_ref().map_or(0,|s|s.identity.size as usize)} else {0},reason:if conditional {"Conditional rule: accessed files and path matching are unknown."} else if !local(a) {"Source is managed/installed, malformed, or unavailable; activation is unknown."} else {"Directory candidate; Claude combines applicable instruction/rule files. Runtime loading and overrides remain unknown."}.into()});
        }
        if !matches!(
            a.kind,
            ArtifactKind::Skill | ArtifactKind::Agent | ArtifactKind::LegacyCommand
        ) {
            continue;
        }
        let mut availability = Availability {
            artifact_id: a.id.clone(),
            state: "unknown".into(),
            reason: "Installed/synced namespace and runtime activation are unknown.".into(),
            implicit_invocation: None,
        };
        if !in_scope(a) {
            availability.state = "out_of_scope".into();
            availability.reason=if a.scope_checkout_id==checkout_id {"Outside the chosen directory chain; nested definitions may load later when files are accessed."} else {"Definition belongs to another checkout."}.into();
        } else if matches!(
            a.validation,
            Validation::Malformed | Validation::Unavailable
        ) {
            availability.state = "invalid".into();
            availability.reason = "Malformed or unavailable local definition.".into();
        } else if local(a) {
            availability.state =
                if a.metadata.get("paths").is_some() && a.kind != ArtifactKind::LegacyCommand {
                    "conditional"
                } else {
                    "candidate"
                }
                .into();
            availability.reason = if availability.state == "conditional" {
                "Conditional paths depend on files accessed in the session; activation is unknown."
            } else {
                "Local directory candidate; runtime loading and permissions remain unknown."
            }
            .into();
            let name = identity(a);
            let shadow = inventory.artifacts.iter().find(|b| {
                b.id != a.id
                    && matches!(
                        b.kind,
                        ArtifactKind::Skill | ArtifactKind::LegacyCommand | ArtifactKind::Agent
                    )
                    && local(b)
                    && in_scope(b)
                    && (a.kind == ArtifactKind::Agent) == (b.kind == ArtifactKind::Agent)
                    && name.is_some()
                    && identity(b) == name
                    && rank(b) > rank(a)
            });
            if let Some(b) = shadow {
                availability.state = "shadowed_candidate".into();
                availability.reason = format!(
                    "Local same-name precedence favors {} ({}); both files are retained. Runtime overrides remain unknown.",
                    b.id,
                    if a.kind == ArtifactKind::Agent {
                        "project agent over personal"
                    } else {
                        "personal skill over project, skill over legacy command"
                    }
                );
            } else if a.kind != ArtifactKind::Agent && availability.state != "conditional" {
                availability.reason="Local skill/command candidate. Nested same-name skills can coexist as directory-qualified commands; display name does not redefine the folder's invocation identity.".into();
            }
            if a.kind != ArtifactKind::Agent {
                availability.implicit_invocation = Some(
                    !a.metadata
                        .get("disable-model-invocation")
                        .and_then(super::claude::boolean)
                        .unwrap_or(false),
                );
            }
        }
        if a.kind == ArtifactKind::Agent {
            result.agents.push(availability);
        } else {
            result.skills.push(availability);
        }
    }
    result
}
