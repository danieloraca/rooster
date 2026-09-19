use super::{files::*, store::ChangeStore, types::*};
use crate::{
    NativePath, ScanStatus,
    artifacts::{Artifact, ArtifactKind, Inventory, Severity, Validation},
    providers::{ArtifactProvider, Provider},
};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

pub(super) fn owners(inventory: &Inventory) -> Vec<OwnerChoice> {
    let mut owners: BTreeMap<String, OwnerChoice> = inventory
        .repositories
        .checkouts
        .iter()
        .map(|c| {
            (
                c.id.clone(),
                OwnerChoice {
                    id: c.id.clone(),
                    root: c.path.clone(),
                    checkout: true,
                },
            )
        })
        .collect();
    for a in inventory
        .artifacts
        .iter()
        .filter(|a| a.provenance.is_authoring() && a.read_only_reason.is_none())
    {
        owners.entry(a.owner.id.clone()).or_insert(OwnerChoice {
            id: a.owner.id.clone(),
            root: a.owner.root.clone(),
            checkout: a.owner.checkout_id.is_some(),
        });
    }
    for s in inventory
        .sources
        .iter()
        .filter(|s| s.available && s.provenance.is_authoring())
    {
        if inventory
            .repositories
            .checkouts
            .iter()
            .any(|c| s.path.as_path().starts_with(c.path.as_path()))
        {
            continue;
        }
        let id = crate::paths::checkout_id(s.path.as_path()).replacen("checkout-", "owner-", 1);
        owners.entry(id.clone()).or_insert(OwnerChoice {
            id,
            root: s.path.clone(),
            checkout: false,
        });
    }
    owners.into_values().collect()
}
struct Builder<'a> {
    store: &'a ChangeStore,
    inventory: &'a Inventory,
    record: Record,
    blobs: BTreeMap<String, Vec<u8>>,
    choices: Vec<OwnerChoice>,
}
pub(super) fn prepare(
    store: &ChangeStore,
    inventory: &Inventory,
    request: Request,
) -> Result<Preview> {
    if inventory.status != ScanStatus::Complete {
        return Err(invalid("Prepare requires a complete inventory"));
    }
    let config = store.config.load()?;
    if inventory.options.codex != config.codex
        || inventory.options.claude != config.claude
        || inventory.options.scan.excluded_dirs != config.excluded_dirs
    {
        return Err(conflict(
            "Inventory source settings differ from the saved configuration; rescan before preparing",
        ));
    }
    let roots = config.roots(None)?;
    if inventory.repositories.roots != roots {
        return Err(conflict(
            "Prepare requires an inventory of the current complete registration set",
        ));
    }
    let choices = owners(inventory);
    if choices
        .iter()
        .any(|o| store.data.starts_with(o.root.as_path()))
        || inventory
            .sources
            .iter()
            .any(|s| store.data.starts_with(s.path.as_path()))
    {
        return Err(invalid(
            "Recovery directory must be outside repositories and provider sources",
        ));
    }
    let _lock = store.lock()?;
    store.pending(None)?;
    let temp = tempfile::Builder::new()
        .prefix("change-")
        .tempdir_in(&store.data)?;
    set_mode(temp.path(), 0o700)?;
    let id = temp
        .path()
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    let protected = inventory
        .sources
        .iter()
        .filter(|s| !s.provenance.is_authoring())
        .map(|s| s.path.clone())
        .chain(inventory.linked_sources.iter().map(|l| l.path.clone()))
        .chain(
            crate::artifacts::foreign_sources(&inventory.options)
                .into_iter()
                .map(Into::into),
        )
        .collect();
    let mut b = Builder {
        store,
        inventory,
        choices,
        blobs: BTreeMap::new(),
        record: Record {
            schema_version: 1,
            id: id.clone(),
            config,
            status: Status::Prepared,
            guards: vec![],
            parents: vec![],
            reads: vec![],
            protected,
            trees: vec![],
            changes: vec![],
            warnings: vec![],
            error: None,
        },
    };
    b.build(request)?;
    // Validate every resulting native definition, including copied packages and link repairs.
    let packages: Vec<_> = b
        .record
        .changes
        .iter()
        .filter(|c| {
            c.after.is_some()
                && c.path
                    .as_path()
                    .file_name()
                    .is_some_and(|n| n == "SKILL.md")
        })
        .map(|c| c.path.as_path().parent().unwrap().to_path_buf())
        .collect();
    for c in &b.record.changes {
        if let Some(node) = &c.after
            && let Some(hash) = &node.hash
        {
            let package = packages
                .iter()
                .find(|p| c.path.as_path().starts_with(p))
                .map(|p| p.as_path());
            if let Some(kind) =
                crate::artifacts::proposed_kind(inventory, c.path.as_path(), package)
                && matches!(
                    kind,
                    ArtifactKind::Skill
                        | ArtifactKind::SkillMetadata
                        | ArtifactKind::Agent
                        | ArtifactKind::Rule
                        | ArtifactKind::LegacyCommand
                )
            {
                validate(
                    inventory.options.provider,
                    kind,
                    b.blobs
                        .get(hash)
                        .ok_or_else(|| invalid("Missing proposed bytes"))?,
                    &mut b.record.warnings,
                )?;
            }
        }
    }
    b.record.warnings.sort();
    b.record.warnings.dedup();
    if b.record.changes.is_empty() {
        return Err(invalid("The proposal makes no changes"));
    }
    let changing: BTreeSet<_> = b.record.changes.iter().map(|c| c.path.0.clone()).collect();
    let mut parents = BTreeMap::new();
    for c in &b.record.changes {
        for parent in c.path.as_path().ancestors().skip(1) {
            if b.record.guards.iter().any(|g| g.root.as_path() == parent) {
                break;
            }
            if parent.exists() && !changing.contains(parent) {
                no_links(parent)?;
                parents.insert(
                    parent.to_path_buf().into(),
                    identity(&fs::metadata(parent)?)?,
                );
            }
        }
    }
    b.record.parents = parents.into_iter().collect();
    b.record.changes.sort_by(|a, b| a.path.cmp(&b.path));
    // Backups and proposals become durable before any source write can be requested.
    store.fault(FaultPoint::Backup)?;
    let blob_dir = temp.path().join("blobs");
    fs::create_dir(&blob_dir)?;
    set_mode(&blob_dir, 0o700)?;
    for (h, bytes) in &b.blobs {
        atomic(&blob_dir.join(h), bytes, 0o600, false)?;
    }
    sync_dir(&blob_dir)?;
    store.save(&b.record)?;
    sync_dir(&store.data)?;
    let _ = temp.keep();
    store.preview(&id)
}
impl Builder<'_> {
    fn guard(&mut self, owner: &OwnerChoice) -> Result<()> {
        if self.record.guards.iter().any(|g| g.root == owner.root) {
            return Ok(());
        }
        no_links(owner.root.as_path())?;
        let meta = fs::metadata(owner.root.as_path())?;
        let git = if owner.checkout {
            let now = checkout(owner.root.as_path())?;
            let expected = self
                .inventory
                .repositories
                .checkouts
                .iter()
                .find(|c| c.id == owner.id)
                .ok_or_else(|| invalid("Unknown checkout"))?;
            if now.branch != expected.branch
                || now.head != expected.head
                || now.git_dir != expected.git_dir
                || now.common_dir != expected.common_dir
            {
                return Err(conflict("Checkout changed since inspection"));
            }
            Some(now)
        } else {
            None
        };
        self.record.guards.push(Guard {
            root: owner.root.clone(),
            identity: identity(&meta)?,
            checkout: git,
        });
        Ok(())
    }
    fn owner(&mut self, id: &str) -> Result<OwnerChoice> {
        let owner = self
            .choices
            .iter()
            .find(|o| o.id == id)
            .cloned()
            .ok_or_else(|| invalid("Unknown authoring owner; use changes owners"))?;
        self.guard(&owner)?;
        Ok(owner)
    }
    fn artifact(&mut self, id: &str, copy: bool) -> Result<Artifact> {
        let a = self
            .inventory
            .artifacts
            .iter()
            .find(|a| a.id == id)
            .cloned()
            .ok_or_else(|| invalid("Artifact not present in this inventory"))?;
        if !copy && (!a.provenance.is_authoring() || a.read_only_reason.is_some()) {
            return Err(invalid(
                a.read_only_reason
                    .clone()
                    .unwrap_or_else(|| "Read-only source".into()),
            ));
        }
        if a.path != a.physical_path {
            return Err(invalid("Linked artifact cannot be mutated or copied"));
        }
        supported(&a)?;
        if copy && !a.provenance.is_authoring() {
            let o = OwnerChoice {
                id: a.owner.id.clone(),
                root: a.owner.root.clone(),
                checkout: a.owner.checkout_id.is_some(),
            };
            self.guard(&o)?;
        } else {
            self.owner(&a.owner.id)?;
        }
        let (node, _) =
            read_node(a.path.as_path())?.ok_or_else(|| conflict("Source disappeared"))?;
        let snapshot = a
            .snapshot
            .as_ref()
            .ok_or_else(|| invalid("Source snapshot unavailable"))?;
        if node.hash.as_deref() != Some(snapshot.sha256.as_str())
            || node.identity.as_ref().is_none_or(|i| {
                Some(i.device) != snapshot.identity.device
                    || Some(i.inode) != snapshot.identity.inode
            })
        {
            return Err(conflict(
                "Source bytes or file identity changed since inspection",
            ));
        }
        if let Some((_, expected)) = self.record.reads.iter().find(|(p, _)| p == &a.path) {
            if *expected != node {
                return Err(conflict("Source changed during preparation"));
            }
        } else {
            self.record.reads.push((a.path.clone(), node));
        }
        Ok(a)
    }
    fn destination(&mut self, owner: &OwnerChoice, path: &Path) -> Result<PathBuf> {
        relative(path)?;
        if path.components().any(|p| {
            self.record
                .config
                .excluded_dirs
                .iter()
                .any(|e| p.as_os_str() == e.as_str())
        }) {
            return Err(invalid("Destination is under an excluded directory"));
        }
        let dest = owner.root.as_path().join(path);
        self.store.validate_path(&self.record, &dest)?;
        if read_node(&dest)?.is_some() {
            return Err(conflict(format!(
                "Destination already exists: {}",
                dest.display()
            )));
        }
        Ok(dest)
    }
    fn stash(&mut self, bytes: Vec<u8>) -> Result<String> {
        let h = hash(&bytes);
        self.blobs.entry(h.clone()).or_insert(bytes);
        if self.blobs.values().map(Vec::len).sum::<usize>() > MAX_TOTAL {
            return Err(invalid("Change exceeds 64 MiB recovery limit"));
        }
        Ok(h)
    }
    fn change(&mut self, path: &Path, after: Option<Node>, bytes: Option<Vec<u8>>) -> Result<()> {
        self.store.validate_path(&self.record, path)?;
        if self.record.changes.iter().any(|c| c.path.as_path() == path) {
            return Err(invalid(format!(
                "Overlapping changes for {}",
                path.display()
            )));
        }
        let before = read_node(path)?;
        if before.is_some()
            && after.is_some()
            && !self.record.reads.iter().any(|(p, _)| p.as_path() == path)
        {
            return Err(conflict("Destination appeared while preparing a creation"));
        }
        if before.is_none() && after.is_none() {
            return Err(conflict("Source disappeared while preparing deletion"));
        }
        if let Some((_, expected)) = self.record.reads.iter().find(|(p, _)| p.as_path() == path)
            && before.as_ref().map(|n| &n.0) != Some(expected)
        {
            return Err(conflict("Source changed while preparing the proposal"));
        }
        if let (Some(node), Some(bytes)) = (&after, &bytes)
            && node.hash.as_deref() != Some(hash(bytes).as_str())
        {
            return Err(conflict("Copied content changed during preparation"));
        }
        if before
            .as_ref()
            .map(|v| &v.0)
            .zip(after.as_ref())
            .is_some_and(|(b, a)| b.content_eq(a))
        {
            return Ok(());
        }
        if let Some((n, b)) = &before
            && !n.directory
        {
            self.stash(b.clone())?;
        }
        if let Some(bytes) = bytes {
            self.stash(bytes)?;
        }
        self.record.changes.push(Change {
            path: path.to_path_buf().into(),
            before: before.map(|v| v.0),
            after,
            applied: None,
            restored: None,
            done: false,
            undone: false,
        });
        Ok(())
    }
    fn parents(&mut self, path: &Path) -> Result<()> {
        let parent = path.parent().ok_or_else(|| invalid("No parent"))?;
        if parent.exists() {
            return Ok(());
        }
        self.parents(parent)?;
        if !self
            .record
            .changes
            .iter()
            .any(|c| c.path.as_path() == parent)
        {
            self.change(
                parent,
                Some(Node {
                    directory: true,
                    hash: None,
                    size: 0,
                    mode: 0o755,
                    identity: None,
                }),
                None,
            )?;
        }
        Ok(())
    }
    fn write(&mut self, path: &Path, bytes: Vec<u8>, permissions: u32) -> Result<()> {
        if bytes.len() > MAX_FILE as usize {
            return Err(invalid("File exceeds 16 MiB limit"));
        }
        self.parents(path)?;
        let node = Node {
            directory: false,
            hash: Some(hash(&bytes)),
            size: bytes.len() as u64,
            mode: permissions,
            identity: None,
        };
        self.change(path, Some(node), Some(bytes))
    }
    fn package(&mut self, a: &Artifact) -> Result<Option<(PathBuf, BTreeMap<NativePath, Node>)>> {
        if a.kind != ArtifactKind::Skill {
            return Ok(None);
        }
        let p = self
            .inventory
            .packages
            .iter()
            .find(|p| p.skill_id == a.id)
            .ok_or_else(|| invalid("Skill package unavailable"))?;
        if p.root != p.physical_root {
            return Err(invalid("Linked package is read-only"));
        }
        let nodes = tree(p.root.as_path())?;
        for member in self
            .inventory
            .artifacts
            .iter()
            .filter(|m| m.package_id.as_deref() == Some(&p.id))
        {
            let n = nodes
                .get(&member.physical_path)
                .ok_or_else(|| conflict("Package member disappeared"))?;
            let snap = member
                .snapshot
                .as_ref()
                .ok_or_else(|| invalid("A package member has no complete snapshot"))?;
            if n.hash.as_deref() != Some(&snap.sha256)
                || n.identity.as_ref().is_none_or(|i| {
                    Some(i.device) != snap.identity.device || Some(i.inode) != snap.identity.inode
                })
            {
                return Err(conflict(
                    "Package content or file identity changed since inspection",
                ));
            }
        }
        self.record.trees.push(TreeGuard {
            root: p.root.clone(),
            nodes: nodes.iter().map(|(p, n)| (p.clone(), n.clone())).collect(),
        });
        Ok(Some((p.root.0.clone(), nodes)))
    }
    fn build(&mut self, request: Request) -> Result<()> {
        match request {
            Request::Edit { artifact_id, edits } => {
                let a = self.artifact(&artifact_id, false)?;
                let (n, bytes) = read_node(a.path.as_path())?.unwrap();
                let text = String::from_utf8(bytes)
                    .map_err(|_| invalid("Only UTF-8 text edits are supported"))?;
                let mut edits = edits;
                edits.sort_by_key(|e| e.start);
                let mut end = 0;
                for e in &edits {
                    if e.start < end
                        || e.start > e.end
                        || e.end > text.len()
                        || !text.is_char_boundary(e.start)
                        || !text.is_char_boundary(e.end)
                    {
                        return Err(invalid(
                            "Text edit ranges must be ordered, non-overlapping UTF-8 byte boundaries",
                        ));
                    }
                    end = e.end;
                }
                let mut out = text;
                for e in edits.into_iter().rev() {
                    out.replace_range(e.start..e.end, &e.replacement);
                }
                validate(
                    self.inventory.options.provider,
                    a.kind,
                    out.as_bytes(),
                    &mut self.record.warnings,
                )?;
                self.write(a.path.as_path(), out.into_bytes(), n.mode)?;
            }
            Request::Replace { artifact_id, text } => {
                let a = self.artifact(&artifact_id, false)?;
                let (n, bytes) = read_node(a.path.as_path())?.unwrap();
                let old = String::from_utf8(bytes)
                    .map_err(|_| invalid("Only UTF-8 replacements are supported"))?;
                let mut text = text.trim_start_matches('\u{feff}').to_string();
                if old.contains("\r\n") && !old.replace("\r\n", "").contains('\n') {
                    text = text.replace("\r\n", "\n").replace('\n', "\r\n");
                }
                if old.starts_with('\u{feff}') {
                    text.insert(0, '\u{feff}');
                }
                validate(
                    self.inventory.options.provider,
                    a.kind,
                    text.as_bytes(),
                    &mut self.record.warnings,
                )?;
                self.write(a.path.as_path(), text.into_bytes(), n.mode)?;
            }
            Request::Create {
                owner_id,
                path,
                kind,
                text,
            } => {
                let owner = self.owner(&owner_id)?;
                let dest = self.destination(&owner, path.as_path())?;
                let kind = kind.into();
                valid_location(self.inventory.options.provider, kind, &dest)?;
                self.location(kind, &dest, None)?;
                validate(
                    self.inventory.options.provider,
                    kind,
                    text.as_bytes(),
                    &mut self.record.warnings,
                )?;
                self.write(&dest, text.into_bytes(), 0o644)?;
            }
            Request::CreatePackage {
                owner_id,
                path,
                files,
            } => {
                let owner = self.owner(&owner_id)?;
                let dest = self.destination(&owner, path.as_path())?;
                self.location(ArtifactKind::Skill, &dest.join("SKILL.md"), Some(&dest))?;
                if !files
                    .iter()
                    .any(|f| f.path.as_path() == Path::new("SKILL.md"))
                {
                    return Err(invalid("A package must include SKILL.md"));
                }
                if files.len() > 20_000 {
                    return Err(invalid("Too many package files"));
                }
                for f in files {
                    relative(f.path.as_path())?;
                    let target = dest.join(f.path.as_path());
                    let kind = if f.path.as_path() == Path::new("SKILL.md") {
                        ArtifactKind::Skill
                    } else if f.path.as_path() == Path::new("agents/openai.yaml") {
                        ArtifactKind::SkillMetadata
                    } else {
                        ArtifactKind::SupportingFile
                    };
                    if kind != ArtifactKind::SupportingFile {
                        validate(
                            self.inventory.options.provider,
                            kind,
                            &f.bytes,
                            &mut self.record.warnings,
                        )?;
                    }
                    self.write(&target, f.bytes, 0o644)?;
                }
            }
            Request::Duplicate {
                artifact_id,
                owner_id,
                path,
            } => {
                let a = self.artifact(&artifact_id, true)?;
                let owner = self.owner(&owner_id)?;
                let dest = self.destination(&owner, path.as_path())?;
                if let Some((root, nodes)) = self.package(&a)? {
                    self.location(ArtifactKind::Skill, &dest.join("SKILL.md"), Some(&dest))?;
                    for (p, n) in nodes {
                        let target = dest.join(p.as_path().strip_prefix(&root).unwrap());
                        self.parents(&target)?;
                        let bytes = if n.directory {
                            None
                        } else {
                            Some(read_node(p.as_path())?.unwrap().1)
                        };
                        self.change(&target, Some(n.desired()), bytes)?;
                    }
                } else {
                    valid_location(self.inventory.options.provider, a.kind, &dest)?;
                    self.location(a.kind, &dest, None)?;
                    let (n, bytes) = read_node(a.path.as_path())?.unwrap();
                    self.write(&dest, bytes, n.mode)?;
                }
                self.record.warnings.push("This is an independent copy. Provider identity and activation are not changed automatically.".into());
            }
            Request::Rename {
                artifact_id,
                path,
                repair_links,
            } => {
                let a = self.artifact(&artifact_id, false)?;
                let owner = self.owner(&a.owner.id)?;
                let dest = self.destination(&owner, path.as_path())?;
                let old = if a.kind == ArtifactKind::Skill {
                    self.inventory
                        .packages
                        .iter()
                        .find(|p| p.skill_id == a.id)
                        .ok_or_else(|| invalid("Missing package"))?
                        .root
                        .0
                        .clone()
                } else {
                    a.path.0.clone()
                };
                if dest.starts_with(&old) || old.starts_with(&dest) {
                    return Err(invalid("Overlapping rename paths"));
                }
                if let Some((root, nodes)) = self.package(&a)? {
                    self.location(ArtifactKind::Skill, &dest.join("SKILL.md"), Some(&dest))?;
                    for (p, n) in nodes {
                        let target = dest.join(p.as_path().strip_prefix(&root).unwrap());
                        self.parents(&target)?;
                        let bytes = if n.directory {
                            None
                        } else {
                            Some(read_node(p.as_path())?.unwrap().1)
                        };
                        self.change(&target, Some(n.desired()), bytes)?;
                        self.change(p.as_path(), None, None)?;
                    }
                } else {
                    valid_location(self.inventory.options.provider, a.kind, &dest)?;
                    self.location(a.kind, &dest, None)?;
                    let (n, bytes) = read_node(a.path.as_path())?.unwrap();
                    self.write(&dest, bytes, n.mode)?;
                    self.change(a.path.as_path(), None, None)?;
                }
                self.repair(&a.owner.id, &old, &dest, &repair_links)?;
            }
            Request::Delete { artifact_id } => {
                let a = self.artifact(&artifact_id, false)?;
                if let Some((root, nodes)) = self.package(&a)? {
                    for p in nodes.keys() {
                        self.change(p.as_path(), None, None)?;
                    }
                    self.incoming(&root);
                } else {
                    self.change(a.path.as_path(), None, None)?;
                    self.incoming(a.path.as_path());
                }
            }
        }
        Ok(())
    }
    fn location(&self, kind: ArtifactKind, path: &Path, package: Option<&Path>) -> Result<()> {
        let package = package.or_else(|| {
            (kind == ArtifactKind::Skill)
                .then(|| path.parent())
                .flatten()
        });
        let actual = crate::artifacts::proposed_kind(self.inventory, path, package);
        let accepted = actual == Some(kind)
            || matches!(
                (kind, actual),
                (
                    ArtifactKind::Markdown | ArtifactKind::Reference,
                    Some(
                        ArtifactKind::Markdown
                            | ArtifactKind::Reference
                            | ArtifactKind::Instruction
                    )
                )
            );
        if !accepted {
            return Err(invalid(
                "Destination would not be discovered as the intended native artifact type",
            ));
        }
        Ok(())
    }
    fn incoming(&mut self, path: &Path) {
        for a in &self.inventory.artifacts {
            for r in &a.references {
                if r.target
                    .as_ref()
                    .is_some_and(|t| t.as_path().starts_with(path))
                {
                    self.record.warnings.push(format!(
                        "Known reference needs review: {} line {}",
                        a.path, r.line
                    ));
                }
            }
        }
    }
    fn repair(
        &mut self,
        owner_id: &str,
        old: &Path,
        new: &Path,
        selected: &[String],
    ) -> Result<()> {
        let selected: BTreeSet<_> = selected.iter().collect();
        let mut found = BTreeSet::new();
        for a in self.inventory.artifacts.clone() {
            let new_path = if a.path.as_path().starts_with(old) {
                new.join(a.path.as_path().strip_prefix(old).unwrap())
            } else {
                a.path.0.clone()
            };
            let affected = a.references.iter().any(|r| {
                r.target
                    .as_ref()
                    .is_some_and(|t| t.as_path().starts_with(old))
            }) || new_path != a.path.0;
            if !affected {
                continue;
            }
            if !selected.contains(&a.id) {
                if !a.references.is_empty() {
                    self.record
                        .warnings
                        .push(format!("References not repaired; review {}", a.path));
                }
                continue;
            }
            if a.owner.id != owner_id {
                return Err(invalid("Cross-owner reference repair is not supported"));
            }
            let a = self.artifact(&a.id, false)?;
            let (node, bytes) = read_node(a.path.as_path())?.unwrap();
            let text =
                String::from_utf8(bytes).map_err(|_| invalid("Reference repair requires UTF-8"))?;
            if self.inventory.options.provider == Provider::Claude
                && !crate::providers::claude::imports(&text).is_empty()
            {
                self.record.warnings.push(format!(
                    "Claude @imports require manual repair; review {}",
                    a.path
                ));
            }
            let (updated, warnings) =
                super::references::repair(&text, a.path.as_path(), &new_path, old, new)?;
            self.record.warnings.extend(warnings);
            found.insert(a.id.clone());
            if text == updated {
                continue;
            }
            let bytes = updated.into_bytes();
            let h = self.stash(bytes.clone())?;
            if let Some(change) = self
                .record
                .changes
                .iter_mut()
                .find(|c| c.path.as_path() == new_path)
            {
                let after = change
                    .after
                    .as_mut()
                    .ok_or_else(|| invalid("Reference repair conflicts with deletion"))?;
                after.hash = Some(h);
                after.size = bytes.len() as u64;
            } else {
                self.write(&new_path, bytes, node.mode)?;
            }
        }
        if selected.iter().any(|id| !found.contains(*id)) {
            return Err(invalid(
                "A selected reference file has no supported relationship to this rename",
            ));
        }
        Ok(())
    }
}
fn supported(a: &Artifact) -> Result<()> {
    match a.kind {
        ArtifactKind::Instruction
        | ArtifactKind::Skill
        | ArtifactKind::SkillMetadata
        | ArtifactKind::Agent
        | ArtifactKind::Markdown
        | ArtifactKind::Rule
        | ArtifactKind::LegacyCommand => Ok(()),
        ArtifactKind::Reference | ArtifactKind::SupportingFile if markdown(a.path.as_path()) => {
            Ok(())
        }
        _ => Err(invalid(
            "This artifact format is read-only; scripts, provider settings, and legacy roles are not edited",
        )),
    }
}
fn markdown(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("md") || e.eq_ignore_ascii_case("markdown"))
}
fn valid_location(provider: Provider, kind: ArtifactKind, path: &Path) -> Result<()> {
    if path.file_name().is_some_and(|s| s == "SKILL.md") && kind != ArtifactKind::Skill {
        return Err(invalid("SKILL.md must use skill validation"));
    }
    let valid = match kind {
        ArtifactKind::Skill => path.file_name().is_some_and(|s| s == "SKILL.md"),
        ArtifactKind::SkillMetadata => path.ends_with("agents/openai.yaml"),
        ArtifactKind::Agent => {
            path.extension().is_some_and(|s| {
                s == if provider == Provider::Claude {
                    "md"
                } else {
                    "toml"
                }
            }) && (provider == Provider::Claude
                || path.parent().is_some_and(|p| p.ends_with("agents")))
        }
        _ => markdown(path),
    };
    if !valid {
        return Err(invalid(
            "Destination does not match the native artifact format",
        ));
    }
    Ok(())
}
fn validate(
    provider: Provider,
    kind: ArtifactKind,
    bytes: &[u8],
    warnings: &mut Vec<String>,
) -> Result<()> {
    let parsed = provider.parse(kind, bytes);
    if parsed.validation == Validation::Malformed
        || parsed.issues.iter().any(|i| i.severity == Severity::Error)
    {
        return Err(invalid(format!(
            "Proposed content is invalid: {}",
            parsed
                .issues
                .iter()
                .map(|i| i.message.as_str())
                .collect::<Vec<_>>()
                .join("; ")
        )));
    }
    if parsed.validation == Validation::Unsupported && parsed.unknown_fields.is_empty() {
        return Err(invalid("Unsupported source encoding or metadata syntax"));
    }
    warnings.extend(parsed.issues.iter().map(|i| i.message.clone()));
    Ok(())
}
impl From<FileKind> for ArtifactKind {
    fn from(k: FileKind) -> Self {
        match k {
            FileKind::Markdown => Self::Markdown,
            FileKind::Instruction => Self::Instruction,
            FileKind::Skill => Self::Skill,
            FileKind::SkillMetadata => Self::SkillMetadata,
            FileKind::Agent => Self::Agent,
            FileKind::Rule => Self::Rule,
            FileKind::LegacyCommand => Self::LegacyCommand,
        }
    }
}
