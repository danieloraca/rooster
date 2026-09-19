use super::*;
use rooster_core::{
    changes::{ChangeStore, DraftTarget, FileKind, Request},
    providers::{ArtifactProvider, Provider},
};
use serde_json::{Value, to_value};

#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum EditingRequest {
    State {
        generation: Option<u64>,
    },
    Open {
        generation: u64,
        target: DraftTarget,
    },
    Read {
        id: String,
    },
    Save {
        id: String,
        revision: u64,
        text: String,
    },
    Compare {
        id: String,
    },
    Rebase {
        id: String,
        revision: u64,
        current_token: String,
    },
    PrepareDraft {
        id: String,
        revision: u64,
    },
    Discard {
        id: String,
        revision: u64,
    },
    Prepare {
        generation: u64,
        request: StructuralRequest,
    },
    Preview {
        id: String,
    },
    Apply {
        id: String,
    },
    Restore {
        id: String,
    },
    CleanupPreview {
        days: u32,
    },
    Cleanup {
        days: u32,
        ids: Vec<String>,
    },
    Git {
        generation: u64,
        owner_id: String,
    },
}
#[derive(Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum StructuralRequest {
    Duplicate {
        artifact_id: String,
        owner_id: String,
        path: NativePath,
    },
    Rename {
        artifact_id: String,
        path: NativePath,
        repair_links: Vec<String>,
    },
    Delete {
        artifact_id: String,
    },
}
impl Session {
    fn changes(&self) -> Result<ChangeStore> {
        let path = self
            .data
            .clone()
            .map(Ok)
            .unwrap_or_else(ChangeStore::default_path)
            .map_err(|e| e.to_string())?;
        ChangeStore::new(self.store.clone(), path).map_err(|e| e.to_string())
    }
    fn mutation_inventory(&self, provider: Provider) -> Result<Inventory> {
        let config = self.store.load().map_err(|e| e.to_string())?;
        Ok(artifacts::inventory(
            &config.roots(None).map_err(|e| e.to_string())?,
            &InventoryOptions {
                provider,
                claude: config.claude,
                scan: ScanOptions {
                    excluded_dirs: config.excluded_dirs,
                    ..Default::default()
                },
                codex: config.codex,
                all_markdown: true,
                ..Default::default()
            },
            &CancellationToken::default(),
            |_| {},
        ))
    }
    pub fn editing(&self, request: EditingRequest) -> Result<Value> {
        let store = self.changes()?;
        let restoring = matches!(&request, EditingRequest::Restore { .. });
        match request {
            EditingRequest::State { generation } => {
                let owners = if let Some(g) = generation {
                    ChangeStore::owners(&self.current(g)?.0)
                } else {
                    vec![]
                };
                Ok(
                    serde_json::json!({"owners":owners,"drafts":store.drafts().map_err(|e|e.to_string())?,"history":store.history().map_err(|e|e.to_string())?,"data_path":store.path().display().to_string()}),
                )
            }
            EditingRequest::Open { generation, target } => {
                let (inventory, _) = self.current(generation)?;
                let text=match &target {
                    DraftTarget::Edit{..}=>None,
                    DraftTarget::Create{kind,..}=>Some(match kind {
                        FileKind::Markdown=>"# New document\n\n".into(),
                        FileKind::SkillMetadata=>"interface:\n  display_name: Example skill\n  short_description: Describe the skill here\n".into(),
                        kind=>inventory.selected_provider().template((*kind).into(),"example","Describe the purpose here.")?.content,
                    }),
                };
                to_value(
                    store
                        .open_draft(&inventory, target, text)
                        .map_err(|e| e.to_string())?,
                )
                .map_err(|e| e.to_string())
            }
            EditingRequest::Read { id } => {
                to_value(store.draft(&id).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
            }
            EditingRequest::Save { id, revision, text } => to_value(
                store
                    .save_draft(&id, revision, text)
                    .map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string()),
            EditingRequest::Compare { id } => to_value(
                store
                    .compare_draft(
                        &self.mutation_inventory(
                            store.draft(&id).map_err(|e| e.to_string())?.provider,
                        )?,
                        &id,
                    )
                    .map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string()),
            EditingRequest::Rebase {
                id,
                revision,
                current_token,
            } => to_value(
                store
                    .rebase_draft(
                        &self.mutation_inventory(
                            store.draft(&id).map_err(|e| e.to_string())?.provider,
                        )?,
                        &id,
                        revision,
                        &current_token,
                    )
                    .map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string()),
            EditingRequest::PrepareDraft { id, revision } => to_value(
                store
                    .prepare_draft(
                        &self.mutation_inventory(
                            store.draft(&id).map_err(|e| e.to_string())?.provider,
                        )?,
                        &id,
                        revision,
                    )
                    .map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string()),
            EditingRequest::Discard { id, revision } => {
                store
                    .discard_draft(&id, revision)
                    .map_err(|e| e.to_string())?;
                Ok(Value::Null)
            }
            EditingRequest::Prepare {
                generation,
                request,
            } => {
                let (view, _) = self.current(generation)?;
                let (artifact_id, request) = match request {
                    StructuralRequest::Duplicate {
                        artifact_id,
                        owner_id,
                        path,
                    } => (
                        artifact_id.clone(),
                        Request::Duplicate {
                            artifact_id,
                            owner_id,
                            path,
                        },
                    ),
                    StructuralRequest::Rename {
                        artifact_id,
                        path,
                        repair_links,
                    } => (
                        artifact_id.clone(),
                        Request::Rename {
                            artifact_id,
                            path,
                            repair_links,
                        },
                    ),
                    StructuralRequest::Delete { artifact_id } => {
                        (artifact_id.clone(), Request::Delete { artifact_id })
                    }
                };
                let full = self.mutation_inventory(view.selected_provider())?;
                let old = view
                    .artifacts
                    .iter()
                    .find(|a| a.id == artifact_id)
                    .ok_or("Unknown selected artifact")?;
                let current = full
                    .artifacts
                    .iter()
                    .find(|a| a.id == artifact_id)
                    .ok_or("Source disappeared; refresh")?;
                if old.snapshot.as_ref().map(|s| (&s.sha256, &s.identity))
                    != current.snapshot.as_ref().map(|s| (&s.sha256, &s.identity))
                    || old.owner.head != current.owner.head
                    || old.owner.branch != current.owner.branch
                    || old.owner.root != current.owner.root
                    || old.physical_path != current.physical_path
                {
                    return Err(
                        "Source or checkout changed since inspection. Refresh before preparing."
                            .into(),
                    );
                }
                to_value(store.prepare(&full, request).map_err(|e| e.to_string())?)
                    .map_err(|e| e.to_string())
            }
            EditingRequest::Preview { id } => {
                to_value(store.preview(&id).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
            }
            EditingRequest::Apply { id } | EditingRequest::Restore { id } => {
                let result = if restoring {
                    store.restore(&id)
                } else {
                    store.apply(&id)
                }
                .map_err(|e| e.to_string())?;
                self.revision.fetch_add(1, Ordering::SeqCst);
                to_value(result).map_err(|e| e.to_string())
            }
            EditingRequest::CleanupPreview { days } => {
                to_value(store.cleanup_preview(days).map_err(|e| e.to_string())?)
                    .map_err(|e| e.to_string())
            }
            EditingRequest::Cleanup { days, ids } => {
                to_value(store.cleanup(days, ids).map_err(|e| e.to_string())?)
                    .map_err(|e| e.to_string())
            }
            EditingRequest::Git {
                generation,
                owner_id,
            } => to_value(
                ChangeStore::git_changes(&self.current(generation)?.0, &owner_id)
                    .map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string()),
        }
    }
}
