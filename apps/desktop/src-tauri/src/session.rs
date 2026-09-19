mod editing;
use crate::rerank;
pub use editing::{EditingRequest, StructuralRequest};
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use rooster_core::{
    CancellationToken, Config, ConfigStore, NativePath, ScanOptions, ScanReport, Workspace,
    artifacts::{
        self, Artifact, ArtifactKind, ClaudeSettings, CodexSettings, Diagnostic, Inventory,
        InventoryOptions, InventoryProgress, LinkedSource, ScopeAssessment, SkillPackage,
        SnapshotInfo, SourceRoot,
    },
    providers::Provider,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

type Result<T> = std::result::Result<T, String>;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ScanRequest {
    #[serde(default)]
    pub provider: Provider,
    pub workspace_id: Option<String>,
    pub all_markdown: bool,
    pub include_personal: bool,
}

#[derive(Serialize)]
pub struct Settings {
    pub workspaces: Vec<Workspace>,
    pub config_path: String,
    pub include_personal: bool,
}

#[derive(Clone, Serialize)]
pub struct ContextEntry {
    pub id: String,
    pub checkout_id: String,
    pub label: String,
    pub path: NativePath,
}

#[derive(Serialize)]
pub struct DesktopInventory {
    pub provider: Provider,
    pub generation: u64,
    pub status: rooster_core::ScanStatus,
    pub repositories: ScanReport,
    pub sources: Vec<SourceRoot>,
    pub artifacts: Vec<Artifact>,
    pub packages: Vec<SkillPackage>,
    pub linked_sources: Vec<LinkedSource>,
    pub diagnostics: Vec<Diagnostic>,
    pub contexts: Vec<ContextEntry>,
}

#[derive(Serialize)]
pub struct Inspection {
    pub generation: u64,
    pub artifact: Artifact,
    pub metadata: serde_json::Value,
    pub text: Option<String>,
    pub snapshot: Option<SnapshotInfo>,
    pub package: Option<SkillPackage>,
    pub related: Vec<Artifact>,
    pub diagnostics: Vec<Diagnostic>,
    pub edit_reason: Option<String>,
}

#[derive(Serialize)]
pub struct Status {
    pub running: Option<u64>,
    pub completed: u64,
    pub generation: u64,
    pub progress: Option<InventoryProgress>,
    pub invalidated: bool,
    pub error: Option<String>,
    pub watch_errors: Vec<String>,
}

struct ActiveScan {
    ticket: u64,
    cancellation: CancellationToken,
}
#[derive(Default)]
struct Inner {
    next_ticket: u64,
    active: Option<ActiveScan>,
    completed: u64,
    generation: u64,
    inventory: Option<Arc<Inventory>>,
    config: Option<Config>,
    contexts: Vec<ContextEntry>,
    progress: Option<InventoryProgress>,
    observed_revision: u64,
    error: Option<String>,
}
#[derive(Default)]
struct Watches {
    watcher: Option<RecommendedWatcher>,
    paths: BTreeSet<(PathBuf, bool)>,
    errors: Vec<String>,
}

pub struct Session {
    store: ConfigStore,
    data: Option<PathBuf>,
    inner: Mutex<Inner>,
    revision: Arc<AtomicU64>,
    watches: Mutex<Watches>,
}

impl Session {
    pub fn new(store: ConfigStore) -> Self {
        Self {
            store,
            data: None,
            inner: Mutex::new(Inner::default()),
            revision: Arc::new(AtomicU64::new(0)),
            watches: Mutex::new(Watches::default()),
        }
    }
    pub fn with_data(store: ConfigStore, data: PathBuf) -> Self {
        let mut session = Self::new(store);
        session.data = Some(data);
        session
    }
    pub fn from_environment() -> Result<Self> {
        let path = std::env::var_os("ROOSTER_CONFIG")
            .map(PathBuf::from)
            .map(Ok)
            .unwrap_or_else(ConfigStore::default_path)
            .map_err(|e| e.to_string())?;
        let mut session = Self::new(ConfigStore::new(path).map_err(|e| e.to_string())?);
        session.data = std::env::var_os("ROOSTER_DATA_DIR").map(PathBuf::from);
        Ok(session)
    }
    pub fn settings(&self) -> Result<Settings> {
        let config = self.store.load().map_err(|e| e.to_string())?;
        let include_personal = config.codex.include_default_roots
            || config.codex.home.is_some()
            || config.codex.user_skills.is_some()
            || config.codex.admin_skills.is_some()
            || !config.codex.extra_roots.is_empty()
            || config.claude.include_default_roots
            || config.claude.home.is_some()
            || config.claude.managed.is_some()
            || !config.claude.extra_roots.is_empty();
        Ok(Settings {
            workspaces: config.workspaces,
            config_path: self.store.path().display().to_string(),
            include_personal,
        })
    }
    // Called only with a native-picker result, never a frontend-supplied filesystem path.
    pub fn register_selection(&self, name: &str, path: &Path) -> Result<Settings> {
        self.store.add(name, path).map_err(|e| e.to_string())?;
        self.revision.fetch_add(1, Ordering::SeqCst);
        self.settings()
    }
    pub fn remove_location(&self, root_id: &str) -> Result<Settings> {
        self.store.remove_root(root_id).map_err(|e| e.to_string())?;
        self.revision.fetch_add(1, Ordering::SeqCst);
        self.settings()
    }
    pub fn start(self: &Arc<Self>, request: ScanRequest) -> Result<u64> {
        let config = self.store.load().map_err(|e| e.to_string())?;
        if let Some(id) = &request.workspace_id
            && !config
                .workspaces
                .iter()
                .any(|workspace| &workspace.id == id)
        {
            return Err("Unknown workspace ID; reload the workspace list.".into());
        }
        let roots = config
            .roots(request.workspace_id.as_deref())
            .map_err(|e| e.to_string())?;
        let options = InventoryOptions {
            provider: request.provider,
            claude: if request.include_personal {
                config.claude.clone()
            } else {
                ClaudeSettings {
                    include_default_roots: false,
                    ..Default::default()
                }
            },
            scan: ScanOptions {
                excluded_dirs: config.excluded_dirs.clone(),
                ..ScanOptions::default()
            },
            codex: if request.include_personal {
                config.codex.clone()
            } else {
                CodexSettings {
                    include_default_roots: false,
                    ..CodexSettings::default()
                }
            },
            all_markdown: request.all_markdown,
            ..InventoryOptions::default()
        };
        let cancellation = CancellationToken::default();
        let ticket = {
            let mut state = self
                .inner
                .lock()
                .map_err(|_| "Inventory state unavailable")?;
            if state.active.is_some() {
                return Err("A scan is already running.".into());
            }
            state.next_ticket += 1;
            let ticket = state.next_ticket;
            state.active = Some(ActiveScan {
                ticket,
                cancellation: cancellation.clone(),
            });
            state.error = None;
            state.progress = None;
            ticket
        };
        let session = Arc::clone(self);
        std::thread::spawn(move || {
            let revision = session.revision.load(Ordering::SeqCst);
            let mut last_progress = Instant::now() - Duration::from_secs(1);
            let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                artifacts::inventory(&roots, &options, &cancellation, |progress| {
                    if last_progress.elapsed() >= Duration::from_millis(80) {
                        if let Ok(mut state) = session.inner.lock() {
                            state.progress = Some(progress.clone());
                        }
                        last_progress = Instant::now();
                    }
                })
            }));
            match outcome {
                Ok(inventory) => {
                    session.update_watches(&inventory, request.all_markdown);
                    let contexts = contexts(&inventory);
                    if let Ok(mut state) = session.inner.lock() {
                        state.generation += 1;
                        state.inventory = Some(Arc::new(inventory));
                        state.config = Some(config);
                        state.contexts = contexts;
                        state.observed_revision = revision;
                        state.completed = ticket;
                        state.active = None;
                        state.progress = None;
                    }
                }
                Err(_) => {
                    if let Ok(mut state) = session.inner.lock() {
                        state.error =
                            Some("The scan stopped unexpectedly. Refresh to retry.".into());
                        state.completed = ticket;
                        state.active = None;
                        state.progress = None;
                    }
                }
            }
        });
        Ok(ticket)
    }
    pub fn cancel(&self, ticket: u64) -> Result<()> {
        let state = self
            .inner
            .lock()
            .map_err(|_| "Inventory state unavailable")?;
        if let Some(active) = &state.active
            && active.ticket == ticket
        {
            active.cancellation.cancel();
            Ok(())
        } else {
            Err("That scan is no longer running.".into())
        }
    }
    pub fn status(&self) -> Result<Status> {
        let state = self
            .inner
            .lock()
            .map_err(|_| "Inventory state unavailable")?;
        let watches = self
            .watches
            .lock()
            .map_err(|_| "Watcher state unavailable")?;
        Ok(Status {
            running: state.active.as_ref().map(|active| active.ticket),
            completed: state.completed,
            generation: state.generation,
            progress: state.progress.clone(),
            invalidated: state.inventory.is_some()
                && state.observed_revision != self.revision.load(Ordering::SeqCst),
            error: state.error.clone(),
            watch_errors: watches.errors.clone(),
        })
    }
    fn current(&self, generation: u64) -> Result<(Arc<Inventory>, Vec<ContextEntry>)> {
        let config = self.store.load().map_err(|e| e.to_string())?;
        let state = self
            .inner
            .lock()
            .map_err(|_| "Inventory state unavailable")?;
        if generation != state.generation {
            return Err("This inventory was replaced. Select the file again.".into());
        }
        if state.config.as_ref() != Some(&config) {
            self.revision.fetch_add(1, Ordering::SeqCst);
            return Err("Workspace settings changed. Refresh before inspecting files.".into());
        }
        Ok((
            Arc::clone(state.inventory.as_ref().ok_or("Scan a workspace first.")?),
            state.contexts.clone(),
        ))
    }
    pub fn inventory(&self, generation: u64) -> Result<DesktopInventory> {
        let (inventory, contexts) = self.current(generation)?;
        Ok(DesktopInventory {
            provider: inventory.selected_provider(),
            generation,
            status: inventory.status,
            repositories: inventory.repositories.clone(),
            sources: inventory.sources.clone(),
            artifacts: inventory.artifacts.clone(),
            packages: inventory.packages.clone(),
            linked_sources: inventory.linked_sources.clone(),
            diagnostics: inventory.diagnostics.clone(),
            contexts,
        })
    }
    pub fn inspect(&self, generation: u64, id: &str) -> Result<Inspection> {
        let (inventory, _) = self.current(generation)?;
        let view = inventory
            .inspect(id)
            .ok_or("Unknown artifact ID; refresh the inventory.")?;
        Ok(Inspection {
            edit_reason: rooster_core::changes::edit_reason(view.artifact),
            generation,
            artifact: view.artifact.clone(),
            metadata: view.metadata.clone(),
            text: view.snapshot.and_then(|snapshot| snapshot.text.clone()),
            snapshot: view.snapshot.map(|snapshot| snapshot.info.clone()),
            package: view.package.cloned(),
            related: view.related.into_iter().cloned().collect(),
            diagnostics: view.diagnostics.into_iter().cloned().collect(),
        })
    }
    pub fn search(&self, generation: u64, query: &str) -> Result<Vec<String>> {
        if query.len() > 512 {
            return Err("Search is limited to 512 bytes.".into());
        }
        let (inventory, _) = self.current(generation)?;
        let lower = query.to_lowercase();
        let mut matches = Vec::new();
        let mut candidates = Vec::new();
        for artifact in &inventory.artifacts {
            let text = inventory
                .inspect(&artifact.id)
                .and_then(|view| view.snapshot)
                .and_then(|snapshot| snapshot.text.clone());
            let matched = artifact.path.to_string().to_lowercase().contains(&lower)
                || artifact
                    .name
                    .as_ref()
                    .is_some_and(|name| name.to_lowercase().contains(&lower))
                || artifact
                    .description
                    .as_ref()
                    .is_some_and(|description| description.to_lowercase().contains(&lower))
                || text
                    .as_ref()
                    .is_some_and(|text| text.to_lowercase().contains(&lower));
            if matched {
                matches.push(artifact.id.clone());
                candidates.push(rerank::Candidate {
                    id: artifact.id.clone(),
                    kind: serde_json::to_value(artifact.kind)
                        .ok()
                        .and_then(|value| value.as_str().map(str::to_owned))
                        .unwrap_or_default(),
                    name: artifact.name.clone().unwrap_or_default(),
                    description: artifact.description.clone().unwrap_or_default(),
                    path: artifact.path.to_string(),
                    text: text.unwrap_or_default(),
                });
            }
        }
        Ok(rerank::rerank(query, &candidates).unwrap_or(matches))
    }
    pub fn assess(&self, generation: u64, id: &str) -> Result<ScopeAssessment> {
        let (inventory, contexts) = self.current(generation)?;
        let context = contexts
            .iter()
            .find(|entry| entry.id == id)
            .ok_or("Unknown directory context ID.")?;
        let physical = context
            .path
            .as_path()
            .canonicalize()
            .map_err(|_| "Directory is unavailable; refresh the inventory.")?;
        if physical != context.path.as_path() {
            return Err("Directory target changed; refresh the inventory.".into());
        }
        Ok(artifacts::assess_scope(&inventory, &physical))
    }
    fn update_watches(&self, inventory: &Inventory, all_markdown: bool) {
        let mut paths = BTreeSet::new();
        let mut repository_roots = Vec::new();
        for root in &inventory.repositories.roots {
            if let Ok(path) = root.path.as_path().canonicalize() {
                repository_roots.push(path.clone());
                paths.insert((path, true));
            } else if let Some(parent) = root.path.as_path().parent().filter(|p| p.is_dir()) {
                paths.insert((parent.into(), false));
            }
        }
        let source_paths: Vec<_> = inventory
            .sources
            .iter()
            .map(|source| {
                source
                    .path
                    .as_path()
                    .canonicalize()
                    .unwrap_or_else(|_| source.path.as_path().to_path_buf())
            })
            .collect();
        for (source, path) in inventory.sources.iter().zip(&source_paths) {
            if source.available {
                paths.insert((path.clone(), true));
            }
        }
        for link in inventory
            .linked_sources
            .iter()
            .filter(|link| link.inspected)
        {
            if let Some(target) = &link.target {
                paths.insert((target.0.clone(), true));
            }
        }
        if let Some(parent) = self.store.path().parent().filter(|p| p.is_dir()) {
            paths.insert((parent.into(), false));
        }
        let Ok(mut watches) = self.watches.lock() else {
            return;
        };
        if watches.paths == paths && watches.watcher.is_some() {
            return;
        }
        let provider_homes: Vec<_> = source_paths
            .iter()
            .filter_map(|root| {
                let children: Vec<_> = source_paths
                    .iter()
                    .filter(|child| *child != root && child.starts_with(root))
                    .cloned()
                    .collect();
                (!children.is_empty()).then(|| (root.clone(), children))
            })
            .collect();
        let artifact_paths: Vec<_> = inventory
            .artifacts
            .iter()
            .map(|artifact| artifact.path.as_path().to_path_buf())
            .collect();
        let package_paths: Vec<_> = inventory
            .packages
            .iter()
            .map(|package| package.physical_root.as_path().to_path_buf())
            .collect();
        let instruction_names: BTreeSet<_> = inventory
            .artifacts
            .iter()
            .filter(|artifact| artifact.kind == ArtifactKind::Instruction)
            .filter_map(|artifact| {
                artifact
                    .path
                    .as_path()
                    .file_name()
                    .map(|name| name.to_owned())
            })
            .collect();
        let filter = WatchFilter {
            provider_homes,
            home_files: provider_home_files(&inventory.provider),
            repository_roots,
            artifact_paths,
            package_paths,
            instruction_names,
            all_markdown,
        };
        let revision = Arc::clone(&self.revision);
        let watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
            if event.as_ref().is_ok_and(|event| {
                !matches!(event.kind, EventKind::Access(_))
                    && event_affects_inventory(event, &filter)
            }) || event.is_err()
            {
                revision.fetch_add(1, Ordering::SeqCst);
            }
        });
        watches.errors.clear();
        match watcher {
            Ok(mut watcher) => {
                if let Err(error) =
                    watcher.configure(notify::Config::default().with_follow_symlinks(false))
                {
                    watches
                        .errors
                        .push(format!("Watcher configuration: {error}"));
                }
                for (path, recursive) in &paths {
                    let mode = if *recursive {
                        RecursiveMode::Recursive
                    } else {
                        RecursiveMode::NonRecursive
                    };
                    if let Err(error) = watcher.watch(path, mode) {
                        watches
                            .errors
                            .push(format!("Cannot watch {}: {error}", path.display()));
                    }
                }
                watches.watcher = Some(watcher);
                watches.paths = paths;
            }
            Err(error) => {
                watches.watcher = None;
                watches
                    .errors
                    .push(format!("File watching unavailable: {error}"));
            }
        }
    }
}

fn provider_home_files(provider: &str) -> &'static [&'static str] {
    match provider {
        "claude" => &[
            "CLAUDE.md",
            "CLAUDE.local.md",
            "settings.json",
            "settings.local.json",
        ],
        _ => &["AGENTS.override.md", "AGENTS.md", "config.toml"],
    }
}

struct WatchFilter {
    provider_homes: Vec<(PathBuf, Vec<PathBuf>)>,
    home_files: &'static [&'static str],
    repository_roots: Vec<PathBuf>,
    artifact_paths: Vec<PathBuf>,
    package_paths: Vec<PathBuf>,
    instruction_names: BTreeSet<std::ffi::OsString>,
    all_markdown: bool,
}

fn event_affects_inventory(event: &Event, filter: &WatchFilter) -> bool {
    event.paths.iter().any(|path| {
        if let Some((home, children)) = filter
            .provider_homes
            .iter()
            .find(|(home, _)| path.starts_with(home))
        {
            return children.iter().any(|child| path.starts_with(child))
                || (path.parent() == Some(home.as_path())
                    && path
                        .file_name()
                        .and_then(|name| name.to_str())
                        .is_some_and(|name| filter.home_files.contains(&name)));
        }
        let Some(root) = filter
            .repository_roots
            .iter()
            .find(|root| path.starts_with(root))
        else {
            return true;
        };
        repository_event_affects_inventory(
            root,
            path,
            &filter.artifact_paths,
            &filter.package_paths,
            &filter.instruction_names,
            filter.all_markdown,
        )
    })
}

fn repository_event_affects_inventory(
    root: &Path,
    path: &Path,
    artifact_paths: &[PathBuf],
    package_paths: &[PathBuf],
    instruction_names: &BTreeSet<std::ffi::OsString>,
    all_markdown: bool,
) -> bool {
    if artifact_paths
        .iter()
        .any(|artifact| artifact == path || (path != root && artifact.starts_with(path)))
        || package_paths
            .iter()
            .any(|package| path.starts_with(package) || (path != root && package.starts_with(path)))
    {
        return true;
    }
    let Ok(relative) = path.strip_prefix(root) else {
        return true;
    };
    let components: Vec<_> = relative.iter().collect();
    let in_collection = components.windows(2).any(|pair| {
        matches!(pair[0].to_str(), Some(".agents" | ".codex" | ".claude"))
            && matches!(
                pair[1].to_str(),
                Some("skills" | "agents" | "rules" | "commands" | "plugins")
            )
    });
    if in_collection
        || relative
            .file_name()
            .is_some_and(|name| instruction_names.contains(name))
        || relative.file_name().is_some_and(|name| {
            matches!(
                name.to_str(),
                Some(
                    "AGENTS.md"
                        | "AGENTS.override.md"
                        | "CLAUDE.md"
                        | "CLAUDE.local.md"
                        | "config.toml"
                        | "settings.json"
                        | "settings.local.json"
                        | "plugin.json"
                )
            )
        })
        || (all_markdown
            && relative.extension().is_some_and(|extension| {
                extension.eq_ignore_ascii_case("md") || extension.eq_ignore_ascii_case("markdown")
            }))
    {
        return true;
    }
    components
        .first()
        .is_some_and(|component| *component == ".git")
        && components.get(1).is_some_and(|component| {
            *component == "HEAD" || *component == "refs" || *component == "packed-refs"
        })
}

fn contexts(inventory: &Inventory) -> Vec<ContextEntry> {
    let mut entries = Vec::new();
    for checkout in &inventory.repositories.checkouts {
        let root = checkout.path.as_path();
        let mut paths = BTreeSet::from([root.to_path_buf()]);
        for artifact in inventory
            .artifacts
            .iter()
            .filter(|artifact| artifact.scope_checkout_id.as_deref() == Some(&checkout.id))
        {
            let path = artifact.scope_directory.as_path();
            if path.starts_with(root) && path.is_dir() {
                paths.insert(path.to_path_buf());
            }
            if matches!(
                artifact.kind,
                ArtifactKind::Instruction | ArtifactKind::Rule | ArtifactKind::ProviderConfig
            ) && let Some(parent) = artifact.path.as_path().parent()
                && parent.starts_with(root)
            {
                paths.insert(parent.to_path_buf());
            }
        }
        for path in paths {
            let native = NativePath::from(path.clone());
            let hash =
                Sha256::digest(serde_json::to_vec(&native).expect("native path serialization"));
            entries.push(ContextEntry {
                id: format!("context-{hash:x}"),
                checkout_id: checkout.id.clone(),
                label: if path == root {
                    "Repository root".into()
                } else {
                    path.strip_prefix(root).unwrap().display().to_string()
                },
                path: native,
            });
        }
    }
    entries
}
