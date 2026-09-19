use super::{ChangeStore, files::*, types::*};
use crate::{CancellationToken, NativePath, artifacts::Inventory, git::Git};
use serde::Serialize;
use std::{
    collections::BTreeSet,
    fs,
    path::Path,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

#[derive(Serialize)]
pub struct CleanupPreview {
    pub days: u32,
    pub candidates: Vec<HistoryEntry>,
}
#[derive(Serialize)]
pub struct CleanupOutcome {
    pub removed: Vec<String>,
    pub pending: Vec<String>,
    pub error: Option<String>,
}
#[derive(Serialize)]
pub struct GitChange {
    pub path: NativePath,
    pub original_path: Option<NativePath>,
    pub index_status: String,
    pub worktree_status: String,
}
#[derive(Serialize)]
pub struct GitChanges {
    pub owner_id: String,
    pub root: NativePath,
    pub branch: Option<String>,
    pub head: Option<String>,
    pub entries: Vec<GitChange>,
}
impl ChangeStore {
    /// Explicit cleanup only. Prepared and unresolved records are never candidates.
    pub fn cleanup_preview(&self, days: u32) -> Result<CleanupPreview> {
        if days > 36500 {
            return Err(invalid("Retention must be at most 36500 days"));
        }
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let age = u64::from(days) * 86400;
        let candidates = self
            .history()?
            .into_iter()
            .filter(|r| {
                matches!(r.status, Status::Completed | Status::Restored)
                    && now.saturating_sub(r.updated_at) >= age
            })
            .collect();
        Ok(CleanupPreview { days, candidates })
    }
    pub fn cleanup(&self, days: u32, ids: Vec<String>) -> Result<CleanupOutcome> {
        let _lock = self.lock()?;
        let eligible: BTreeSet<_> = self
            .cleanup_preview(days)?
            .candidates
            .into_iter()
            .map(|r| r.id)
            .collect();
        let requested: BTreeSet<_> = ids.into_iter().collect();
        if !requested.is_subset(&eligible) {
            return Err(conflict(
                "Cleanup selection changed or includes a prepared/unresolved/recent operation; preview again",
            ));
        }
        let mut outcome = CleanupOutcome {
            removed: vec![],
            pending: requested.iter().cloned().collect(),
            error: None,
        };
        for id in requested {
            let result = (|| {
                let directory = self.directory(&id)?;
                fs::remove_dir_all(directory)?;
                sync_dir(&self.data)
            })();
            match result {
                Ok(()) => {
                    outcome.pending.retain(|p| p != &id);
                    outcome.removed.push(id);
                }
                Err(error) => {
                    outcome.error = Some(error.to_string());
                    break;
                }
            }
        }
        Ok(outcome)
    }
    pub fn git_changes(inventory: &Inventory, owner_id: &str) -> Result<GitChanges> {
        let owner = inventory
            .repositories
            .checkouts
            .iter()
            .find(|c| c.id == owner_id)
            .ok_or_else(|| invalid("Choose a scanned Git checkout"))?;
        no_links(owner.path.as_path())?;
        let before = checkout(owner.path.as_path())?;
        if before.branch != owner.branch
            || before.head != owner.head
            || before.git_dir != owner.git_dir
            || before.common_dir != owner.common_dir
        {
            return Err(conflict(
                "Checkout changed; refresh before inspecting Git changes",
            ));
        }
        let cancel = CancellationToken::default();
        let git = Git {
            executable: Path::new("git"),
            timeout: Duration::from_secs(5),
            cancellation: &cancel,
        };
        let bytes = git
            .status(owner.path.as_path())
            .map_err(|e| invalid(format!("Cannot inspect Git changes: {e:?}")))?;
        if checkout(owner.path.as_path())? != before {
            return Err(conflict("Checkout changed during Git inspection"));
        }
        let mut records = bytes.split(|b| *b == 0).filter(|r| !r.is_empty());
        let mut entries = vec![];
        while let Some(record) = records.next() {
            if record.len() < 4 || record[2] != b' ' {
                return Err(invalid("Unrecognized Git status record"));
            }
            let path = crate::paths::git_path(&record[3..]).map_err(invalid)?;
            let original_path = if record[..2].iter().any(|b| matches!(b, b'R' | b'C')) {
                Some(
                    crate::paths::git_path(
                        records
                            .next()
                            .ok_or_else(|| invalid("Incomplete Git rename record"))?,
                    )
                    .map_err(invalid)?
                    .into(),
                )
            } else {
                None
            };
            entries.push(GitChange {
                path: path.into(),
                original_path,
                index_status: (record[0] as char).to_string(),
                worktree_status: (record[1] as char).to_string(),
            });
        }
        Ok(GitChanges {
            owner_id: owner.id.clone(),
            root: owner.path.clone(),
            branch: before.branch,
            head: before.head,
            entries,
        })
    }
}
