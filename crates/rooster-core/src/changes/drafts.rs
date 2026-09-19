use super::{ChangeStore, files::*, types::*};
use crate::{
    Config, NativePath,
    artifacts::{Artifact, ArtifactKind, Inventory},
    providers::Provider,
};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum DraftTarget {
    Edit {
        artifact_id: String,
    },
    Create {
        owner_id: String,
        path: NativePath,
        kind: FileKind,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Draft {
    #[serde(default)]
    pub provider: Provider,
    pub id: String,
    pub revision: u64,
    pub target: DraftTarget,
    pub path: NativePath,
    pub original: Option<String>,
    pub text: String,
    pub updated_at: u64,
}
#[derive(Clone, Debug, Serialize)]
pub struct DraftSummary {
    pub provider: Provider,
    pub id: String,
    pub revision: u64,
    pub path: NativePath,
    pub updated_at: u64,
}
#[derive(Clone, Debug, Serialize)]
pub struct DraftComparison {
    pub draft: Draft,
    pub current_text: Option<String>,
    pub current_token: Option<String>,
    pub conflict: Option<String>,
    pub can_rebase: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
struct Baseline {
    #[serde(default)]
    provider: Provider,
    config: Config,
    owner: Guard,
    parents: Vec<(NativePath, Identity)>,
    source: Option<Node>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SavedDraft {
    schema_version: u32,
    draft: Draft,
    baseline: Baseline,
}

pub fn edit_reason(a: &Artifact) -> Option<String> {
    if let Some(reason) = &a.read_only_reason {
        return Some(reason.clone());
    }
    if !a.provenance.is_authoring() {
        return Some("Installed or managed source is read-only.".into());
    }
    if !matches!(
        a.kind,
        ArtifactKind::Instruction
            | ArtifactKind::Skill
            | ArtifactKind::SkillMetadata
            | ArtifactKind::Agent
            | ArtifactKind::Reference
            | ArtifactKind::Markdown
            | ArtifactKind::Rule
            | ArtifactKind::LegacyCommand
    ) {
        return Some("This file type is available for inspection only.".into());
    }
    if a.snapshot.as_ref().is_none_or(|s| !s.utf8) {
        return Some("A complete UTF-8 source snapshot is required.".into());
    }
    None
}
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
impl ChangeStore {
    fn draft_directory(&self, id: &str) -> Result<PathBuf> {
        if !id.starts_with("draft-") || !id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
        {
            return Err(invalid("Invalid draft ID"));
        }
        let path = self.data.join(id);
        no_links(&path)?;
        Ok(path)
    }
    fn read_draft(&self, id: &str) -> Result<SavedDraft> {
        let path = self.draft_directory(id)?.join("draft.json");
        no_links(&path)?;
        if fs::metadata(&path)?.len() > 128 * 1024 * 1024 {
            return Err(invalid("Oversized draft"));
        }
        let record: SavedDraft = serde_json::from_slice(&fs::read(path)?)?;
        if record.schema_version != 1 || record.draft.id != id {
            return Err(invalid("Unsupported draft schema or inconsistent ID"));
        }
        Ok(record)
    }
    fn write_draft(&self, record: &SavedDraft) -> Result<()> {
        atomic(
            &self.draft_directory(&record.draft.id)?.join("draft.json"),
            &serde_json::to_vec(record)?,
            0o600,
            true,
        )
    }
    pub fn drafts(&self) -> Result<Vec<DraftSummary>> {
        if !self.data.exists() {
            return Ok(vec![]);
        }
        no_links(&self.data)?;
        let mut result = vec![];
        for e in fs::read_dir(&self.data)? {
            let e = e?;
            let id = e.file_name().to_string_lossy().into_owned();
            if !id.starts_with("draft-") {
                continue;
            }
            let directory = self.draft_directory(&id)?;
            if !directory.join("draft.json").exists() {
                continue;
            }
            let d = self.read_draft(&id)?.draft;
            result.push(DraftSummary {
                provider: d.provider,
                id: d.id,
                revision: d.revision,
                path: d.path,
                updated_at: d.updated_at,
            });
        }
        result.sort_by(|a, b| b.updated_at.cmp(&a.updated_at).then(a.id.cmp(&b.id)));
        Ok(result)
    }
    pub fn draft(&self, id: &str) -> Result<Draft> {
        Ok(self.read_draft(id)?.draft)
    }
    fn draft_baseline(
        &self,
        inventory: &Inventory,
        target: &DraftTarget,
    ) -> Result<(Baseline, NativePath, Option<String>)> {
        let config = self.config.load()?;
        let choices = Self::owners(inventory);
        let (owner_id, path, source, text) = match target {
            DraftTarget::Edit { artifact_id } => {
                let view = inventory
                    .inspect(artifact_id)
                    .ok_or_else(|| conflict("Draft source is no longer in the inventory"))?;
                let a = view.artifact;
                if let Some(reason) = edit_reason(a) {
                    return Err(invalid(reason));
                }
                if a.path != a.physical_path {
                    return Err(invalid("Linked files are read-only"));
                }
                let (node, bytes) = read_node(a.path.as_path())?
                    .ok_or_else(|| conflict("Draft source disappeared"))?;
                let expected = a
                    .snapshot
                    .as_ref()
                    .ok_or_else(|| invalid("Missing source snapshot"))?;
                if node.hash.as_deref() != Some(expected.sha256.as_str())
                    || node.identity.as_ref().map(|i| (i.device, i.inode))
                        != expected.identity.device.zip(expected.identity.inode)
                {
                    return Err(conflict(
                        "Source changed since inspection; refresh before opening or comparing the draft",
                    ));
                }
                (
                    a.owner.id.clone(),
                    a.path.clone(),
                    Some(node),
                    Some(
                        String::from_utf8(bytes)
                            .map_err(|_| invalid("Only UTF-8 text can be edited"))?,
                    ),
                )
            }
            DraftTarget::Create { owner_id, path, .. } => {
                relative(path.as_path())?;
                let owner = choices
                    .iter()
                    .find(|o| &o.id == owner_id)
                    .ok_or_else(|| invalid("Unknown destination owner"))?;
                let path: NativePath = owner.root.as_path().join(path.as_path()).into();
                no_links(path.as_path())?;
                if read_node(path.as_path())?.is_some() {
                    return Err(conflict("Creation destination already exists"));
                }
                (owner_id.clone(), path, None, None)
            }
        };
        let owner = choices
            .iter()
            .find(|o| o.id == owner_id)
            .ok_or_else(|| invalid("Draft owner is unavailable or read-only"))?;
        let root = owner.root.as_path();
        no_links(root)?;
        let checkout = if owner.checkout {
            let observed = checkout(root)?;
            let expected = inventory
                .repositories
                .checkouts
                .iter()
                .find(|c| c.path == owner.root)
                .ok_or_else(|| invalid("Checkout is missing"))?;
            if observed.branch != expected.branch
                || observed.head != expected.head
                || observed.git_dir != expected.git_dir
                || observed.common_dir != expected.common_dir
            {
                return Err(conflict("Checkout changed since inspection"));
            }
            Some(observed)
        } else {
            None
        };
        let guard = Guard {
            root: owner.root.clone(),
            identity: identity(&fs::metadata(root)?)?,
            checkout,
        };
        let mut parents = vec![];
        for parent in path.as_path().ancestors().skip(1) {
            if parent == root {
                break;
            }
            if parent.exists() {
                no_links(parent)?;
                parents.push((
                    parent.to_path_buf().into(),
                    identity(&fs::metadata(parent)?)?,
                ));
            }
        }
        Ok((
            Baseline {
                provider: inventory.options.provider,
                config,
                owner: guard,
                parents,
                source,
            },
            path,
            text,
        ))
    }
    pub fn open_draft(
        &self,
        inventory: &Inventory,
        target: DraftTarget,
        text: Option<String>,
    ) -> Result<Draft> {
        let _lock = self.lock()?;
        let (baseline, path, original) = self.draft_baseline(inventory, &target)?;
        // Reopening always preserves an existing draft, including a stale one.
        for item in self.drafts()? {
            if item.path == path && item.provider == inventory.options.provider {
                return self.draft(&item.id);
            }
        }
        let text = text.unwrap_or_else(|| original.clone().unwrap_or_default());
        if text.len() as u64 > MAX_FILE {
            return Err(invalid("Draft exceeds 16 MiB"));
        }
        let directory = tempfile::Builder::new()
            .prefix("draft-")
            .tempdir_in(&self.data)?;
        set_mode(directory.path(), 0o700)?;
        let id = directory
            .path()
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let record = SavedDraft {
            schema_version: 1,
            draft: Draft {
                provider: inventory.options.provider,
                id,
                revision: 1,
                target,
                path,
                original,
                text,
                updated_at: now(),
            },
            baseline,
        };
        self.write_draft(&record)?;
        sync_dir(directory.path())?;
        let _ = directory.keep();
        sync_dir(&self.data)?;
        Ok(record.draft)
    }
    pub fn save_draft(&self, id: &str, revision: u64, text: String) -> Result<Draft> {
        if text.len() as u64 > MAX_FILE {
            return Err(invalid("Draft exceeds 16 MiB"));
        }
        let _lock = self.lock()?;
        let mut record = self.read_draft(id)?;
        if record.draft.revision != revision {
            return Err(conflict(
                "Draft changed in another window; your local text has been retained",
            ));
        }
        record.draft.text = text;
        record.draft.revision += 1;
        record.draft.updated_at = now();
        self.write_draft(&record)?;
        Ok(record.draft)
    }
    pub fn compare_draft(&self, inventory: &Inventory, id: &str) -> Result<DraftComparison> {
        let record = self.read_draft(id)?;
        match self.draft_baseline(inventory, &record.draft.target) {
            Ok((current, _, text)) => {
                let conflict=(current!=record.baseline).then(||"The file, its owner, checkout, or settings changed since this draft was opened. Compare before continuing.".into());
                let can_rebase = current.provider == record.baseline.provider
                    && matches!(record.draft.target, DraftTarget::Edit { .. })
                    && current.config == record.baseline.config
                    && current.owner == record.baseline.owner
                    && current.parents == record.baseline.parents;
                let token = hash(&serde_json::to_vec(&current)?);
                Ok(DraftComparison {
                    draft: record.draft,
                    current_text: text,
                    current_token: Some(token),
                    conflict,
                    can_rebase,
                })
            }
            Err(error) => Ok(DraftComparison {
                draft: record.draft,
                current_text: None,
                current_token: None,
                conflict: Some(error.to_string()),
                can_rebase: false,
            }),
        }
    }
    pub fn rebase_draft(
        &self,
        inventory: &Inventory,
        id: &str,
        revision: u64,
        current_token: &str,
    ) -> Result<Draft> {
        let _lock = self.lock()?;
        let comparison = self.compare_draft(inventory, id)?;
        if !comparison.can_rebase || comparison.current_token.as_deref() != Some(current_token) {
            return Err(conflict(
                "The compared revision changed or belongs to a different checkout. Keep the draft and reopen explicitly.",
            ));
        }
        let mut record = self.read_draft(id)?;
        if record.draft.revision != revision {
            return Err(conflict("Draft changed in another window"));
        }
        let (baseline, _, text) = self.draft_baseline(inventory, &record.draft.target)?;
        if hash(&serde_json::to_vec(&baseline)?) != current_token {
            return Err(conflict("Source changed after comparison"));
        }
        record.baseline = baseline;
        record.draft.original = text;
        record.draft.revision += 1;
        record.draft.updated_at = now();
        self.write_draft(&record)?;
        Ok(record.draft)
    }
    pub fn prepare_draft(&self, inventory: &Inventory, id: &str, revision: u64) -> Result<Preview> {
        let record = {
            let _lock = self.lock()?;
            let record = self.read_draft(id)?;
            if record.draft.revision != revision {
                return Err(conflict("Draft changed since it was reviewed"));
            }
            let (current, _, _) = self.draft_baseline(inventory, &record.draft.target)?;
            if current != record.baseline {
                return Err(conflict(
                    "Draft base changed. Compare the current source before reviewing an apply.",
                ));
            }
            record
        };
        let request = match record.draft.target {
            DraftTarget::Edit { artifact_id } => Request::Replace {
                artifact_id,
                text: record.draft.text,
            },
            DraftTarget::Create {
                owner_id,
                path,
                kind,
            } => Request::Create {
                owner_id,
                path,
                kind,
                text: record.draft.text,
            },
        };
        self.prepare(inventory, request)
    }
    pub fn discard_draft(&self, id: &str, revision: u64) -> Result<()> {
        let _lock = self.lock()?;
        let record = self.read_draft(id)?;
        if record.draft.revision != revision {
            return Err(conflict(
                "Draft changed in another window; it was not discarded",
            ));
        }
        fs::remove_file(self.draft_directory(id)?.join("draft.json"))?;
        fs::remove_dir(self.draft_directory(id)?)?;
        sync_dir(&self.data)
    }
}
