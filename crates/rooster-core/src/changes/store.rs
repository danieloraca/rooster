use super::{files::*, types::*};
use crate::{ConfigStore, artifacts::Inventory};
use std::{
    fs::{self, File, OpenOptions, TryLockError},
    path::{Path, PathBuf},
    sync::Arc,
};

pub struct ChangeStore {
    pub(super) config: ConfigStore,
    pub(super) data: PathBuf,
    fault: Option<Arc<dyn Fn(FaultPoint) -> std::io::Result<()> + Send + Sync>>,
}
impl ChangeStore {
    pub fn new(config: ConfigStore, data: impl AsRef<Path>) -> Result<Self> {
        let data = crate::paths::absolute_input(data.as_ref())?;
        no_links(&data)?;
        Ok(Self {
            config,
            data,
            fault: None,
        })
    }
    pub fn default_path() -> Result<PathBuf> {
        directories::BaseDirs::new()
            .map(|d| d.data_local_dir().join("rooster").join("recovery"))
            .ok_or_else(|| invalid("Cannot resolve application data directory"))
    }
    pub fn path(&self) -> &Path {
        &self.data
    }
    /// Fault injection for deterministic recovery tests; never exposed through IPC or CLI.
    pub fn with_fault_injector(
        mut self,
        fault: impl Fn(FaultPoint) -> std::io::Result<()> + Send + Sync + 'static,
    ) -> Self {
        self.fault = Some(Arc::new(fault));
        self
    }
    pub(super) fn fault(&self, point: FaultPoint) -> Result<()> {
        if let Some(f) = &self.fault {
            f(point)?;
        }
        Ok(())
    }
    pub(super) fn lock(&self) -> Result<File> {
        no_links(&self.data)?;
        if self
            .data
            .ancestors()
            .any(|p| p.join(".git").symlink_metadata().is_ok())
        {
            return Err(invalid("Recovery storage cannot be inside a Git checkout"));
        }
        for root in crate::artifacts::configured_source_paths(&self.config.load()?) {
            if self.data.starts_with(&root) {
                return Err(invalid(
                    "Recovery storage cannot be inside a provider source",
                ));
            }
        }
        fs::create_dir_all(&self.data)?;
        set_mode(&self.data, 0o700)?;
        let path = self.data.join("writer.lock");
        no_links(&path)?;
        let mut options = OpenOptions::new();
        options.read(true).write(true).create(true).truncate(false);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
        }
        let file = options.open(path)?;
        let start = std::time::Instant::now();
        loop {
            match file.try_lock() {
                Ok(()) => break,
                Err(TryLockError::WouldBlock)
                    if start.elapsed() < std::time::Duration::from_secs(2) =>
                {
                    std::thread::sleep(std::time::Duration::from_millis(10))
                }
                Err(TryLockError::WouldBlock) => {
                    return Err(invalid("Another Rooster writer is active; retry later"));
                }
                Err(TryLockError::Error(e)) => return Err(e.into()),
            }
        }
        Ok(file)
    }
    pub(super) fn directory(&self, id: &str) -> Result<PathBuf> {
        if !id.starts_with("change-") || !id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
        {
            return Err(invalid("Invalid change ID"));
        }
        let path = self.data.join(id);
        no_links(&path)?;
        Ok(path)
    }
    pub(super) fn save(&self, record: &Record) -> Result<()> {
        self.fault(FaultPoint::Journal)?;
        atomic(
            &self.directory(&record.id)?.join("record.json"),
            &serde_json::to_vec_pretty(record)?,
            0o600,
            true,
        )
    }
    fn load(&self, id: &str) -> Result<Record> {
        let path = self.directory(id)?.join("record.json");
        no_links(&path)?;
        let bytes = fs::read(path)?;
        if bytes.len() > 32 * 1024 * 1024 {
            return Err(invalid("Oversized change record"));
        }
        let record: Record = serde_json::from_slice(&bytes)?;
        if record.schema_version != 1 || record.id != id {
            return Err(invalid("Unsupported or inconsistent change record"));
        }
        Ok(record)
    }
    pub(super) fn blob(&self, id: &str, digest: &str) -> Result<Vec<u8>> {
        if digest.len() != 64 || !digest.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(invalid("Invalid backup hash"));
        }
        let path = self.directory(id)?.join("blobs").join(digest);
        no_links(&path)?;
        let bytes = fs::read(path)?;
        if hash(&bytes) != digest {
            return Err(conflict("Backup content failed its integrity check"));
        }
        Ok(bytes)
    }
    pub fn history(&self) -> Result<Vec<HistoryEntry>> {
        if !self.data.exists() {
            return Ok(vec![]);
        }
        no_links(&self.data)?;
        let mut entries = vec![];
        for entry in fs::read_dir(&self.data)? {
            let entry = entry?;
            let id = entry.file_name().to_string_lossy().into_owned();
            if !id.starts_with("change-") {
                continue;
            }
            // An interrupted preparation has no published record and touched no source.
            if !entry.path().join("record.json").exists() {
                continue;
            }
            let r = self.load(&id)?;
            entries.push(HistoryEntry {
                updated_at: fs::metadata(self.directory(&id)?.join("record.json"))?
                    .modified()?
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs(),
                id,
                status: r.status,
                changes: r.changes.len(),
                error: r.error,
            });
        }
        entries.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(entries)
    }
    pub(super) fn pending(&self, except: Option<&str>) -> Result<()> {
        if let Some(entry) = self.history()?.iter().find(|r| {
            Some(r.id.as_str()) != except
                && matches!(
                    r.status,
                    Status::Applying | Status::RecoveryRequired | Status::Restoring
                )
        }) {
            return Err(conflict(format!(
                "Resolve interrupted change {} first",
                entry.id
            )));
        }
        Ok(())
    }
    pub fn preview(&self, id: &str) -> Result<Preview> {
        let r = self.load(id)?;
        let mut files = vec![];
        for c in &r.changes {
            let text = |n: &Option<Node>| -> Result<Option<String>> {
                n.as_ref()
                    .and_then(|n| n.hash.as_ref())
                    .map(|h| self.blob(id, h).map(|b| String::from_utf8(b).ok()))
                    .transpose()
                    .map(Option::flatten)
            };
            files.push(FilePreview {
                path: c.path.clone(),
                action: match (&c.before, &c.after) {
                    (None, _) => "create",
                    (_, None) => "delete",
                    _ => "edit",
                }
                .into(),
                directory: c
                    .before
                    .as_ref()
                    .or(c.after.as_ref())
                    .is_some_and(|n| n.directory),
                before_sha256: c.before.as_ref().and_then(|n| n.hash.clone()),
                after_sha256: c.after.as_ref().and_then(|n| n.hash.clone()),
                before_text: text(&c.before)?,
                after_text: text(&c.after)?,
                before_bytes: c.before.as_ref().map(|n| n.size),
                after_bytes: c.after.as_ref().map(|n| n.size),
                before_mode: c.before.as_ref().map(|n| n.mode),
                after_mode: c.after.as_ref().map(|n| n.mode),
            });
        }
        Ok(Preview {
            id: r.id,
            status: r.status,
            files,
            warnings: r.warnings,
        })
    }
    pub fn apply(&self, id: &str) -> Result<Outcome> {
        self.execute(id, false)
    }
    pub fn restore(&self, id: &str) -> Result<Outcome> {
        self.execute(id, true)
    }
    fn validate_guards(&self, r: &Record) -> Result<()> {
        if self.config.load()? != r.config {
            return Err(conflict("Workspace settings changed since preparation"));
        }
        for guard in &r.guards {
            no_links(guard.root.as_path())?;
            if identity(&fs::metadata(guard.root.as_path())?)? != guard.identity {
                return Err(conflict("Owner directory identity changed"));
            }
            if let Some(expected) = &guard.checkout
                && &checkout(guard.root.as_path())? != expected
            {
                return Err(conflict("Checkout branch, HEAD, or Git metadata changed"));
            }
        }
        for (path, expected) in &r.parents {
            no_links(path.as_path())?;
            if identity(&fs::metadata(path.as_path())?)? != *expected {
                return Err(conflict("Destination/source parent was replaced"));
            }
        }
        for change in &r.changes {
            self.validate_path(r, change.path.as_path())?;
        }
        if r.guards
            .iter()
            .any(|g| self.data.starts_with(g.root.as_path()))
        {
            return Err(invalid("Recovery data must be outside source owners"));
        }
        Ok(())
    }
    pub(super) fn validate_path(&self, r: &Record, path: &Path) -> Result<()> {
        no_links(path)?;
        let owner = r
            .guards
            .iter()
            .filter(|g| path.starts_with(g.root.as_path()))
            .max_by_key(|g| g.root.as_path().components().count())
            .ok_or_else(|| invalid("Change outside reviewed owner"))?;
        let parts: Vec<_> = path.components().collect();
        if parts.iter().any(|p| {
            p.as_os_str()
                .to_str()
                .is_some_and(|s| s.eq_ignore_ascii_case(".system"))
        }) || parts.windows(2).any(|p| {
            p[0].as_os_str()
                .to_str()
                .is_some_and(|s| s.eq_ignore_ascii_case(".codex"))
                && p[1]
                    .as_os_str()
                    .to_str()
                    .is_some_and(|s| s.eq_ignore_ascii_case("skills"))
        }) {
            return Err(invalid(
                "System and legacy compatibility locations are read-only",
            ));
        }
        let rel = path.strip_prefix(owner.root.as_path()).unwrap();
        relative(rel)?;
        let mut current = owner.root.0.clone();
        for part in rel.components() {
            current.push(part);
            if current.is_dir() && current.join(".git").symlink_metadata().is_ok() {
                return Err(invalid("Change crosses a nested checkout"));
            }
        }
        if r.protected
            .iter()
            .any(|root| path.starts_with(root.as_path()))
        {
            return Err(invalid(
                "Installed, managed, compatibility, or linked source is read-only",
            ));
        }
        for root in &r.protected {
            if let Ok(meta) = fs::symlink_metadata(root.as_path())
                && meta.is_dir()
            {
                let protected = identity(&meta)?;
                for ancestor in path.ancestors() {
                    if fs::symlink_metadata(ancestor)
                        .ok()
                        .filter(|m| m.is_dir())
                        .is_some_and(|m| identity(&m).ok().as_ref() == Some(&protected))
                    {
                        return Err(invalid("Destination aliases a read-only source"));
                    }
                }
            }
        }
        Ok(())
    }
    fn actual(&self, c: &Change) -> Result<Option<Node>> {
        Ok(read_node(c.path.as_path())?.map(|v| v.0))
    }
    /// Classify a step after a crash without overwriting an unrelated external revision.
    fn reconcile(&self, r: &mut Record) -> Result<()> {
        for c in &mut r.changes {
            let actual = self.actual(c)?;
            if c.undone {
                if actual != c.restored {
                    return Err(conflict(format!("Restored path changed: {}", c.path)));
                }
                continue;
            }
            if c.done {
                if actual == c.applied {
                    continue;
                }
                if r.status == Status::Restoring && desired_matches(&actual, &c.before) {
                    c.undone = true;
                    c.restored = actual;
                    continue;
                }
                return Err(conflict(format!("Applied path changed: {}", c.path)));
            }
            if actual == c.before {
                continue;
            }
            if matches!(
                r.status,
                Status::Applying | Status::RecoveryRequired | Status::Restoring
            ) && desired_matches(&actual, &c.after)
            {
                c.done = true;
                c.applied = actual;
                continue;
            }
            return Err(conflict(format!("Path changed since preview: {}", c.path)));
        }
        Ok(())
    }
    fn execute(&self, id: &str, restore: bool) -> Result<Outcome> {
        let _lock = self.lock()?;
        self.pending(Some(id))?;
        let mut r = self.load(id)?;
        if (!restore && r.status == Status::Completed) || (restore && r.status == Status::Restored)
        {
            return Ok(outcome(&r));
        }
        if (!restore && matches!(r.status, Status::Restoring | Status::Restored))
            || (restore && r.status == Status::Prepared)
        {
            return Err(invalid("Change is not in a state for that operation"));
        }
        self.validate_guards(&r)?;
        // Before the first write, recheck complete package membership as well as each file.
        if r.status == Status::Prepared {
            for (path, expected) in &r.reads {
                if read_node(path.as_path())?.map(|v| v.0).as_ref() != Some(expected) {
                    return Err(conflict("Source dependency changed since preview"));
                }
            }
            for expected in &r.trees {
                if tree(expected.root.as_path())?
                    .into_iter()
                    .collect::<Vec<_>>()
                    != expected.nodes
                {
                    return Err(conflict(
                        "Package membership or bytes changed since preview",
                    ));
                }
            }
        }
        self.reconcile(&mut r)?;
        // Verify every required backup before changing any destination.
        for c in &r.changes {
            for n in [&c.before, &c.after].into_iter().flatten() {
                if let Some(h) = &n.hash {
                    let bytes = self.blob(id, h)?;
                    if bytes.len() as u64 != n.size {
                        return Err(conflict("Backup length changed"));
                    }
                }
            }
        }
        r.status = if restore {
            Status::Restoring
        } else {
            Status::Applying
        };
        r.error = None;
        self.save(&r)?;
        let mut order: (Vec<usize>, Vec<usize>) = (vec![], vec![]);
        for (index, c) in r.changes.iter().enumerate() {
            let desired = if restore { &c.before } else { &c.after };
            if desired.is_some() {
                order.0.push(index)
            } else {
                order.1.push(index)
            }
        }
        order
            .0
            .sort_by_key(|i| r.changes[*i].path.as_path().components().count());
        order
            .1
            .sort_by_key(|i| std::cmp::Reverse(r.changes[*i].path.as_path().components().count()));
        let work = (|| -> Result<()> {
            for index in order.0.into_iter().chain(order.1) {
                let c = &r.changes[index];
                if if restore { c.undone || !c.done } else { c.done } {
                    continue;
                }
                self.validate_guards(&r)?;
                self.fault(FaultPoint::BeforeChange(index))?;
                let expected = if restore { &c.applied } else { &c.before };
                let desired = if restore { &c.before } else { &c.after };
                if self.actual(c)? != *expected {
                    return Err(conflict(format!(
                        "Path changed immediately before write: {}",
                        c.path
                    )));
                }
                self.write_node(id, c.path.as_path(), expected, desired)?;
                self.fault(FaultPoint::AfterChange(index))?;
                let actual = self.actual(&r.changes[index])?;
                if !desired_matches(&actual, desired) {
                    return Err(conflict("Readback did not match the reviewed change"));
                }
                let c = &mut r.changes[index];
                if restore {
                    c.undone = true;
                    c.restored = actual;
                } else {
                    c.done = true;
                    c.applied = actual;
                }
                self.save(&r)?;
            }
            // Recheck all outputs after the complete operation, including earlier steps.
            for c in &r.changes {
                let expected = if restore {
                    if c.undone { &c.restored } else { &c.before }
                } else {
                    &c.applied
                };
                if self.actual(c)? != *expected {
                    return Err(conflict("Output changed before final verification"));
                }
            }
            Ok(())
        })();
        match work {
            Ok(()) => {
                r.status = if restore {
                    Status::Restored
                } else {
                    Status::Completed
                };
                r.error = None;
            }
            Err(error) => {
                r.status = if restore {
                    Status::Restoring
                } else {
                    Status::RecoveryRequired
                };
                r.error = Some(error.to_string());
            }
        }
        if let Err(error) = self.save(&r) {
            r.status = Status::RecoveryRequired;
            r.error = Some(format!(
                "Final journal update failed: {error}. Inspect recovery before proceeding."
            ));
        }
        Ok(outcome(&r))
    }
    fn write_node(
        &self,
        id: &str,
        path: &Path,
        before: &Option<Node>,
        after: &Option<Node>,
    ) -> Result<()> {
        match after {
            Some(n) if n.directory => {
                if before.is_none() {
                    fs::create_dir(path)?;
                    set_mode(path, n.mode)?;
                    sync_dir(path.parent().unwrap())?;
                } else if !before.as_ref().is_some_and(|b| b.directory) {
                    return Err(invalid("Cannot replace a file with a directory"));
                }
            }
            Some(n) => {
                let bytes = self.blob(
                    id,
                    n.hash
                        .as_ref()
                        .ok_or_else(|| invalid("Missing content hash"))?,
                )?;
                atomic(path, &bytes, n.mode, before.is_some())?;
            }
            None => {
                if before.as_ref().is_some_and(|n| n.directory) {
                    fs::remove_dir(path)?;
                } else {
                    fs::remove_file(path)?;
                }
                sync_dir(path.parent().unwrap())?;
            }
        }
        Ok(())
    }
    pub fn prepare(&self, inventory: &Inventory, request: Request) -> Result<Preview> {
        super::prepare::prepare(self, inventory, request)
    }
    pub fn owners(inventory: &Inventory) -> Vec<OwnerChoice> {
        super::prepare::owners(inventory)
    }
}
fn desired_matches(actual: &Option<Node>, desired: &Option<Node>) -> bool {
    match (actual, desired) {
        (None, None) => true,
        (Some(a), Some(d)) => a.content_eq(d),
        _ => false,
    }
}
fn outcome(r: &Record) -> Outcome {
    Outcome {
        id: r.id.clone(),
        status: r.status,
        changed: r
            .changes
            .iter()
            .filter(|c| c.done && !c.undone)
            .map(|c| c.path.clone())
            .collect(),
        pending: r
            .changes
            .iter()
            .filter(|c| !c.done && !c.undone)
            .map(|c| c.path.clone())
            .collect(),
        error: r.error.clone(),
    }
}
