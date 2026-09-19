#[path = "claude_paths.rs"]
mod claude_paths;
use super::{links, scope, snapshot, *};
use crate::{
    CancellationToken, Root,
    git::{Git, GitError, has_git_marker},
    paths::absolute_input,
    providers::{
        ArtifactProvider, Provider,
        codex::{self},
    },
};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, Eq, PartialEq)]
enum Mode {
    Repository,
    Home,
    Skills,
    Agents,
    Collection,
    Rules,
    Commands,
}

#[derive(Clone)]
struct Source {
    path: PathBuf,
    provenance: Provenance,
    mode: Mode,
    optional: bool,
    derived: bool,
    link_target: Option<PathBuf>,
}

impl Source {
    fn physical_root(&self) -> &Path {
        self.link_target.as_deref().unwrap_or(&self.path)
    }
}

#[derive(Clone)]
struct Candidate {
    path: PathBuf,
    physical: PathBuf,
    source: Source,
    package: Option<(PathBuf, PathBuf)>,
    linked: bool,
}

struct Builder<'a, F> {
    result: Inventory,
    options: &'a InventoryOptions,
    cancel: &'a CancellationToken,
    progress: F,
    boundaries: Vec<PathBuf>,
    foreign_sources: Vec<PathBuf>,
    candidates: BTreeMap<PathBuf, Candidate>,
    sources: Vec<Source>,
    bytes_read: u64,
    visible: HashMap<String, HashSet<PathBuf>>,
}

/// Inventory local artifacts without executing provider code or writing source files.
pub fn inventory(
    roots: &[Root],
    options: &InventoryOptions,
    cancel: &CancellationToken,
    mut on_progress: impl FnMut(&InventoryProgress),
) -> Inventory {
    let repositories = crate::scan(roots, &options.scan, cancel, |p| {
        on_progress(&InventoryProgress {
            phase: "repositories".into(),
            path: p.current_path.clone(),
            visited: p.visited_dirs,
            artifacts: 0,
        })
    });
    let status = repositories.status;
    let mut builder = Builder {
        result: Inventory {
            schema_version: 1,
            provider: options.provider.to_string(),
            status,
            repositories,
            sources: Vec::new(),
            artifacts: Vec::new(),
            packages: Vec::new(),
            linked_sources: Vec::new(),
            diagnostics: Vec::new(),
            scope: None,
            snapshots: HashMap::new(),
            options: options.clone(),
        },
        options,
        cancel,
        progress: on_progress,
        boundaries: Vec::new(),
        foreign_sources: foreign_sources(options),
        candidates: BTreeMap::new(),
        sources: Vec::new(),
        bytes_read: 0,
        visible: HashMap::new(),
    };
    // Selected parent roots authorize explicit linked references as well as checkouts.
    builder.boundaries.extend(
        roots
            .iter()
            .filter_map(|r| r.path.as_path().canonicalize().ok()),
    );
    let mut sources: Vec<Source> = builder
        .result
        .repositories
        .checkouts
        .iter()
        .map(|c| Source {
            path: c.path.0.clone(),
            provenance: Provenance::Repository,
            mode: Mode::Repository,
            optional: false,
            derived: false,
            link_target: None,
        })
        .collect();
    sources.extend(provider_sources(options));
    // Explicit selections establish boundaries before derived home children are checked.
    sources.sort_by_key(|source| source.derived);
    let mut available = Vec::new();
    for mut source in sources {
        let linked = source.derived
            && fs::symlink_metadata(&source.path)
                .is_ok_and(|metadata| metadata.file_type().is_symlink());
        let resolved = source.path.canonicalize().ok().filter(|path| path.is_dir());
        if linked {
            let allowed = resolved.as_ref().is_some_and(|target| {
                !source.path.starts_with(target)
                    && links::bounded_path(target, &builder.boundaries, &options.scan.excluded_dirs)
                        .is_ok()
            });
            builder.result.linked_sources.push(LinkedSource {
                path: source.path.clone().into(),
                target: resolved.clone().map(Into::into),
                inspected: allowed,
                reason: if allowed {
                    "Registered derived-root target; linked content is read-only."
                } else {
                    "Derived-root link not followed: target is unregistered, unavailable, excluded, or cyclic."
                }.into(),
            });
            if !allowed {
                builder.result.sources.push(SourceRoot {
                    path: source.path.clone().into(),
                    provenance: source.provenance,
                    available: false,
                });
                builder.diagnostic(&source.path, None, Severity::Warning, "unsupported_link",
                    "Derived provider root requires an independently registered target before inspection.", None);
                continue;
            }
        }
        match resolved {
            Some(path) => {
                if linked {
                    source.link_target = Some(path);
                } else {
                    source.path = path;
                }
                if !source.derived {
                    builder.boundaries.push(source.path.clone());
                }
                builder.result.sources.push(SourceRoot {
                    path: source.path.clone().into(),
                    provenance: source.provenance,
                    available: true,
                });
                available.push(source);
            }
            None => {
                builder.result.sources.push(SourceRoot {
                    path: source.path.clone().into(),
                    provenance: source.provenance,
                    available: false,
                });
                if !source.optional {
                    builder.io_issue(
                        &source.path,
                        "unavailable_source",
                        "Configured artifact root is missing or not a readable directory.",
                    );
                }
            }
        }
    }
    builder.sources = available.clone();
    builder.boundaries.sort();
    builder.boundaries.dedup();
    for source in available {
        if cancel.is_cancelled() {
            break;
        }
        if source.mode == Mode::Home {
            // Never walk sessions, credentials, logs, or the rest of Codex home.
            for name in home_files(options.provider) {
                let path = source.path.join(name);
                if path.exists() || fs::symlink_metadata(&path).is_ok() {
                    builder.add_candidate(Candidate {
                        physical: path.clone(),
                        path,
                        source: source.clone(),
                        package: None,
                        linked: false,
                    });
                }
            }
        } else {
            builder.walk(&source);
        }
    }
    builder.load_visible_files();
    builder.build_artifacts();
    if let Some(context) = &options.context {
        match context.canonicalize() {
            Ok(path) if path.is_dir() => {
                builder.result.scope = Some(scope::assess(&builder.result, &path))
            }
            _ => builder.io_issue(
                context,
                "invalid_context",
                "Scope context must be an existing directory.",
            ),
        }
    }
    builder.duplicates();
    builder.result.artifacts.sort_by(|a, b| a.path.cmp(&b.path));
    if cancel.is_cancelled() {
        builder.result.status = crate::ScanStatus::Cancelled;
    }
    builder.result
}

fn resolve_sources(settings: &CodexSettings) -> Vec<Source> {
    let dirs = directories::BaseDirs::new();
    let home = settings.home.as_ref().map(|p| p.0.clone()).or_else(|| {
        settings
            .include_default_roots
            .then(|| {
                std::env::var_os("CODEX_HOME")
                    .map(PathBuf::from)
                    .or_else(|| dirs.as_ref().map(|d| d.home_dir().join(".codex")))
            })
            .flatten()
    });
    let mut sources = Vec::new();
    if let Some(path) = home {
        let path = absolute_input(&path).unwrap_or(path);
        let path = path.canonicalize().unwrap_or(path);
        sources.push(Source {
            path: path.clone(),
            provenance: Provenance::Personal,
            mode: Mode::Home,
            optional: settings.home.is_none(),
            derived: false,
            link_target: None,
        });
        sources.push(Source {
            path: path.join("agents"),
            provenance: Provenance::Personal,
            mode: Mode::Agents,
            optional: true,
            derived: true,
            link_target: None,
        });
        sources.push(Source {
            path: path.join("skills"),
            provenance: Provenance::Compatibility,
            mode: Mode::Collection,
            optional: true,
            derived: true,
            link_target: None,
        });
        sources.push(Source {
            path: path.join("plugins"),
            provenance: Provenance::InstalledPlugin,
            mode: Mode::Collection,
            optional: true,
            derived: true,
            link_target: None,
        });
    }
    let user = settings
        .user_skills
        .as_ref()
        .map(|p| p.0.clone())
        .or_else(|| {
            settings
                .include_default_roots
                .then(|| dirs.as_ref().map(|d| d.home_dir().join(".agents/skills")))
                .flatten()
        });
    if let Some(path) = user {
        sources.push(Source {
            path,
            provenance: Provenance::Personal,
            mode: Mode::Skills,
            optional: settings.user_skills.is_none(),
            derived: false,
            link_target: None,
        });
    }
    let admin = settings
        .admin_skills
        .as_ref()
        .map(|p| p.0.clone())
        .or_else(|| {
            settings
                .include_default_roots
                .then(|| PathBuf::from("/etc/codex/skills"))
        });
    if let Some(path) = admin {
        sources.push(Source {
            path,
            provenance: Provenance::Managed,
            mode: Mode::Skills,
            optional: settings.admin_skills.is_none(),
            derived: false,
            link_target: None,
        });
    }
    sources.extend(settings.extra_roots.iter().map(|root| Source {
        path: root.path.0.clone(),
        provenance: root.provenance,
        mode: Mode::Collection,
        optional: false,
        derived: false,
        link_target: None,
    }));
    sources
}

impl<F: FnMut(&InventoryProgress)> Builder<'_, F> {
    fn diagnostic(
        &mut self,
        path: &Path,
        id: Option<String>,
        severity: Severity,
        code: &str,
        message: &str,
        line: Option<usize>,
    ) {
        self.result.diagnostics.push(Diagnostic {
            severity,
            code: code.into(),
            path: path.to_path_buf().into(),
            line,
            artifact_id: id,
            message: message.into(),
        });
    }

    fn io_issue(&mut self, path: &Path, code: &str, message: &str) {
        self.result.status = crate::ScanStatus::Partial;
        self.diagnostic(path, None, Severity::Error, code, message, None);
    }

    fn add_candidate(&mut self, candidate: Candidate) {
        if self.candidates.len() >= self.options.max_files {
            if !self
                .result
                .diagnostics
                .iter()
                .any(|d| d.code == "file_limit")
            {
                self.io_issue(
                    &candidate.path,
                    "file_limit",
                    "Inventory file limit reached; remaining entries were not inspected.",
                );
            }
            return;
        }
        let entry = self.candidates.entry(candidate.path.clone());
        match entry {
            std::collections::btree_map::Entry::Occupied(mut entry)
                if entry.get().source.mode == Mode::Repository
                    && candidate.source.mode != Mode::Repository =>
            {
                entry.insert(candidate);
            }
            std::collections::btree_map::Entry::Vacant(entry) => {
                entry.insert(candidate);
            }
            _ => {}
        }
    }

    fn walk(&mut self, source: &Source) {
        let mut pending = vec![(
            source.path.clone(),
            source.physical_root().to_path_buf(),
            None,
            source.link_target.is_some(),
            Vec::<PathBuf>::new(),
        )];
        let mut visited = 0;
        while let Some((logical, physical, mut package, linked, mut ancestors)) = pending.pop() {
            if self.cancel.is_cancelled() {
                break;
            }
            if self.candidates.len() >= self.options.max_files {
                self.io_issue(
                    &logical,
                    "file_limit",
                    "Inventory file limit reached; remaining entries were not inspected.",
                );
                break;
            }
            if self.result.repositories.skipped.iter().any(|skip| {
                skip.path.as_path() == physical
                    && matches!(
                        skip.reason,
                        crate::SkipReason::BareRepository | crate::SkipReason::GitMetadata
                    )
            }) {
                continue;
            }
            if physical != source.physical_root() && has_git_marker(&physical).unwrap_or(true) {
                continue;
            }
            if ancestors.contains(&physical) {
                continue;
            }
            ancestors.push(physical.clone());
            if links::bounded_path(
                &physical,
                &self.boundaries,
                &self.options.scan.excluded_dirs,
            )
            .is_err()
            {
                continue;
            }
            visited += 1;
            (self.progress)(&InventoryProgress {
                phase: "artifacts".into(),
                path: logical.clone().into(),
                visited,
                artifacts: self.result.artifacts.len(),
            });
            if package.is_none()
                && (source.mode == Mode::Skills
                    || source.mode == Mode::Collection
                    || provider_collection_scope(self.options.provider, &logical).is_some())
                && fs::symlink_metadata(physical.join("SKILL.md"))
                    .is_ok_and(|m| m.file_type().is_file())
            {
                package = Some((logical.clone(), physical.clone()));
            }
            let entries = match fs::read_dir(&physical) {
                Ok(entries) => entries,
                Err(_) => {
                    self.io_issue(
                        &physical,
                        "unreadable_directory",
                        "Directory could not be read.",
                    );
                    continue;
                }
            };
            let mut children = Vec::new();
            for entry in entries {
                if self.cancel.is_cancelled() {
                    break;
                }
                let entry = match entry {
                    Ok(entry) => entry,
                    Err(_) => {
                        self.io_issue(
                            &physical,
                            "unreadable_entry",
                            "A directory entry could not be read.",
                        );
                        continue;
                    }
                };
                let name = entry.file_name();
                if name == ".git"
                    || self.options.provider == Provider::Claude && name == ".trash"
                    || crate::providers::foreign_path(self.options.provider, &logical.join(&name))
                    || self
                        .foreign_sources
                        .iter()
                        .any(|p| logical.join(&name).starts_with(p))
                {
                    continue;
                }
                let path = logical.join(&name);
                let target = entry.path();
                match entry.file_type() {
                    Ok(kind) if kind.is_symlink() => {
                        let resolved = target.canonicalize().ok();
                        let recognized = package.is_none()
                            && (logical.file_name().is_some_and(|n| n == "skills")
                                || (logical == source.path && source.mode == Mode::Skills)
                                || (name == "skills"
                                    && logical.file_name().is_some_and(|n| {
                                        n == if self.options.provider == Provider::Claude {
                                            ".claude"
                                        } else {
                                            ".agents"
                                        }
                                    })));
                        let allowed = resolved.as_ref().is_some_and(|p| {
                            self.boundaries.iter().any(|r| p.starts_with(r))
                                && links::bounded_path(
                                    p,
                                    &self.boundaries,
                                    &self.options.scan.excluded_dirs,
                                )
                                .is_ok()
                                && p.is_dir()
                                && !ancestors.contains(p)
                        });
                        let inspect = recognized && allowed;
                        self.result.linked_sources.push(LinkedSource { path: path.clone().into(),
                            target: resolved.clone().map(Into::into), inspected: inspect,
                            reason: if inspect { "Registered skill target; linked content is read-only." }
                                else { "Link not followed: target is outside registered roots, cyclic, or not a supported skill-folder link." }.into() });
                        if inspect {
                            children.push((path, resolved.unwrap(), None, true, ancestors.clone()));
                        } else {
                            self.diagnostic(
                                &path,
                                None,
                                Severity::Warning,
                                "unsupported_link",
                                "Link was inventoried without following its contents.",
                                None,
                            );
                        }
                    }
                    Ok(kind) if kind.is_dir() => {
                        if !self
                            .options
                            .scan
                            .excluded_dirs
                            .iter()
                            .any(|n| name == n.as_str())
                        {
                            children.push((
                                path,
                                target,
                                package.clone(),
                                linked,
                                ancestors.clone(),
                            ));
                        }
                    }
                    Ok(kind) if kind.is_file() => self.add_candidate(Candidate {
                        path,
                        physical: target,
                        source: source.clone(),
                        package: package.clone(),
                        linked,
                    }),
                    Ok(_) => self.diagnostic(
                        &path,
                        None,
                        Severity::Warning,
                        "special_file",
                        "Non-regular filesystem entry was not read.",
                        None,
                    ),
                    Err(_) => self.io_issue(
                        &path,
                        "unreadable_entry",
                        "File type could not be inspected.",
                    ),
                }
            }
            children.sort_by(|a, b| a.0.cmp(&b.0));
            pending.extend(children.into_iter().rev());
        }
    }

    fn load_visible_files(&mut self) {
        let checkouts = self.result.repositories.checkouts.clone();
        for checkout in checkouts {
            let git = Git {
                executable: &self.options.scan.git_executable,
                timeout: self.options.scan.git_timeout,
                cancellation: self.cancel,
            };
            match git.visible_files(checkout.path.as_path()) {
                Ok(files) => { self.visible.insert(checkout.id, files); }
                Err(GitError::Cancelled) => break,
                Err(GitError::Failed(_)) => self.io_issue(checkout.path.as_path(), "ignore_status_unavailable", "Git ignore/tracked-file status could not be determined; ordinary Markdown is omitted."),
            }
        }
    }

    fn owner(&self, path: &Path, source: &Source) -> Owner {
        if let Some(checkout) = self
            .result
            .repositories
            .checkouts
            .iter()
            .filter(|c| path.starts_with(c.path.as_path()))
            .max_by_key(|c| c.path.as_path().components().count())
        {
            return Owner {
                id: checkout.id.clone(),
                root: checkout.path.clone(),
                checkout_id: Some(checkout.id.clone()),
                head: checkout.head.clone(),
                branch: checkout.branch.clone(),
            };
        }
        let root = self
            .boundaries
            .iter()
            .filter(|r| path.starts_with(r))
            .max_by_key(|r| r.components().count())
            .unwrap_or(&source.path);
        Owner {
            id: snapshot::stable_id("owner", root),
            root: root.clone().into(),
            checkout_id: None,
            head: None,
            branch: None,
        }
    }

    fn add_artifact(&mut self, candidate: &Candidate, kind: ArtifactKind) -> Option<String> {
        let id = snapshot::stable_id("artifact", &candidate.path);
        if self.result.artifacts.iter().any(|a| a.id == id) {
            return Some(id);
        }
        if self.cancel.is_cancelled() {
            return None;
        }
        if self.result.artifacts.len() >= self.options.max_files {
            self.io_issue(
                &candidate.path,
                "file_limit",
                "Artifact limit reached; remaining references were not inspected.",
            );
            return None;
        }
        let owner = self.owner(&candidate.physical, &candidate.source);
        let scope_owner = self.owner(&candidate.path, &candidate.source);
        let ignored = owner
            .checkout_id
            .as_ref()
            .and_then(|id| self.visible.get(id))
            .map(|files| !files.contains(&candidate.physical));
        if kind == ArtifactKind::Markdown && owner.checkout_id.is_some() && ignored != Some(false) {
            return None;
        }
        let mut provenance = candidate.source.provenance;
        if candidate
            .path
            .strip_prefix(&candidate.source.path)
            .ok()
            .is_some_and(|p| p.components().any(|c| c.as_os_str() == ".system"))
        {
            provenance = Provenance::System;
        } else if self.options.provider == Provider::Claude
            && claude_paths::reserved(&candidate.path)
        {
            provenance = Provenance::Synced;
        } else if provider_collection_scope(self.options.provider, &candidate.path)
            .is_some_and(|(_, legacy)| legacy)
        {
            provenance = Provenance::Compatibility;
        }
        let scope_directory = if self.options.provider == Provider::Claude {
            claude_paths::scope(candidate, kind)
        } else {
            match kind {
                ArtifactKind::Skill
                | ArtifactKind::SkillMetadata
                | ArtifactKind::SupportingFile
                | ArtifactKind::Reference => candidate
                    .package
                    .as_ref()
                    .and_then(|(root, _)| collection_scope(root).map(|(scope, _)| scope))
                    .unwrap_or_else(|| candidate.source.path.clone()),
                ArtifactKind::Agent => candidate
                    .path
                    .parent()
                    .and_then(Path::parent)
                    .filter(|p| p.file_name().is_some_and(|n| n == ".codex"))
                    .and_then(Path::parent)
                    .unwrap_or(&candidate.source.path)
                    .to_path_buf(),
                ArtifactKind::ProviderConfig => candidate
                    .path
                    .parent()
                    .filter(|p| p.file_name().is_some_and(|n| n == ".codex"))
                    .and_then(Path::parent)
                    .unwrap_or(&candidate.source.path)
                    .to_path_buf(),
                _ => candidate
                    .path
                    .parent()
                    .unwrap_or(&candidate.source.path)
                    .to_path_buf(),
            }
        };
        let mut artifact = Artifact {
            id: id.clone(),
            provider: (!matches!(
                kind,
                ArtifactKind::Markdown | ArtifactKind::Reference | ArtifactKind::SupportingFile
            ))
            .then(|| self.options.provider.to_string()),
            kind,
            path: candidate.path.clone().into(),
            physical_path: candidate.physical.clone().into(),
            owner,
            provenance,
            source_root: candidate.source.path.clone().into(),
            scope_directory: scope_directory.into(),
            scope_checkout_id: if candidate.source.mode == Mode::Repository {
                scope_owner.checkout_id
            } else {
                None
            },
            package_id: candidate
                .package
                .as_ref()
                .map(|(root, _)| snapshot::stable_id("package", root)),
            name: None,
            declared_role_names: Vec::new(),
            role_declaration_count: 0,
            description: None,
            validation: Validation::Unavailable,
            unknown_fields: Vec::new(),
            read_only_reason: if crate::providers::foreign_path(
                self.options.provider,
                &candidate.path,
            ) || self
                .foreign_sources
                .iter()
                .any(|p| candidate.path.starts_with(p))
            {
                Some("This source belongs to another provider; switch provider to edit it with its native validation.".into())
            } else if candidate.linked {
                Some(
                    "Linked source; edit the explicitly registered authoring source instead."
                        .into(),
                )
            } else if !provenance.is_authoring() {
                Some(format!(
                    "{provenance:?} source; availability and activation are not established by disk presence."
                ))
            } else if kind == ArtifactKind::LegacyAgent {
                Some("Legacy role configuration; native editing is deferred.".into())
            } else {
                None
            },
            ignored,
            snapshot: None,
            references: Vec::new(),
            metadata: serde_json::Value::Null,
        };
        let remaining = self.options.max_total_bytes.saturating_sub(self.bytes_read);
        let bounded = links::bounded_path(
            &candidate.physical,
            &self.boundaries,
            &self.options.scan.excluded_dirs,
        );
        let read = bounded
            .map_err(|s| format!("Source boundary rejected ({s:?})."))
            .and_then(|path| {
                snapshot::read(&path, self.options.max_file_bytes.min(remaining))
                    .map_err(|e| e.to_string())
            });
        match read {
            Ok(document) => {
                self.bytes_read += document.bytes.len() as u64;
                artifact.snapshot = Some(document.info.clone());
                let parsed = if matches!(kind, ArtifactKind::SupportingFile) {
                    crate::providers::ParsedArtifact::default()
                } else {
                    self.options.provider.parse(kind, &document.bytes)
                };
                artifact.metadata = parsed.metadata;
                artifact.name = parsed.name;
                if self.options.provider == Provider::Claude {
                    // Command identity follows the path; a skill's name field is only display text.
                    if kind == ArtifactKind::Skill {
                        if artifact.name.is_none() {
                            artifact.name = candidate
                                .path
                                .parent()
                                .and_then(Path::file_name)
                                .map(|n| n.to_string_lossy().into_owned());
                        }
                    } else if kind == ArtifactKind::LegacyCommand {
                        artifact.name = candidate
                            .path
                            .file_stem()
                            .map(|n| n.to_string_lossy().into_owned());
                    }
                }
                artifact.description = parsed.description;
                artifact.validation = parsed.validation;
                artifact.unknown_fields = parsed.unknown_fields;
                for issue in parsed.issues {
                    self.diagnostic(
                        &candidate.path,
                        Some(id.clone()),
                        issue.severity,
                        issue.code,
                        &issue.message,
                        issue.line,
                    );
                }
                if document.info.identity.size > 0 {
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::MetadataExt;
                        if fs::metadata(&candidate.physical).is_ok_and(|m| m.nlink() > 1) {
                            artifact.read_only_reason = Some(
                                "Hard-linked source; mutation semantics are not supported.".into(),
                            );
                        }
                    }
                }
                self.result.snapshots.insert(id.clone(), document);
            }
            Err(message) => {
                self.result.status = crate::ScanStatus::Partial;
                artifact.read_only_reason = Some("Source could not be snapshotted.".into());
                self.diagnostic(
                    &candidate.path,
                    Some(id.clone()),
                    Severity::Error,
                    "snapshot_unavailable",
                    &message,
                    None,
                );
            }
        }
        self.result.artifacts.push(artifact);
        Some(id)
    }

    fn reference_candidate(&self, path: &Path) -> Candidate {
        // Reuse the discovered target, including its package and source, never the referrer.
        if let Some(candidate) = self.candidates.get(path).or_else(|| {
            self.candidates
                .values()
                .find(|candidate| candidate.physical == path)
        }) {
            return candidate.clone();
        }
        let source = self
            .sources
            .iter()
            .filter(|source| path.starts_with(source.physical_root()))
            .max_by_key(|source| {
                (
                    source.physical_root().components().count(),
                    source.mode != Mode::Repository,
                    !source.provenance.is_authoring(),
                )
            })
            .cloned()
            .unwrap_or_else(|| Source {
                path: self
                    .boundaries
                    .iter()
                    .filter(|root| path.starts_with(root))
                    .max_by_key(|root| root.components().count())
                    .unwrap()
                    .clone(),
                provenance: Provenance::Repository,
                mode: Mode::Repository,
                optional: false,
                derived: false,
                link_target: None,
            });
        Candidate {
            path: path.to_path_buf(),
            physical: path.to_path_buf(),
            linked: source.link_target.is_some(),
            source,
            package: None,
        }
    }

    fn fallback_names_for(&self, candidate: &Candidate, configs: &[Artifact]) -> HashSet<String> {
        if candidate.source.mode != Mode::Repository {
            return HashSet::new();
        }
        let parent = candidate.path.parent().unwrap();
        let owner = self.owner(&candidate.path, &candidate.source);
        let mut contexts = vec![parent];
        // A descendant config can select fallback guidance in its ancestors, but not siblings.
        contexts.extend(
            configs
                .iter()
                .filter(|config| {
                    config.scope_checkout_id == owner.checkout_id
                        && config.scope_directory.as_path().starts_with(parent)
                })
                .map(|config| config.scope_directory.as_path()),
        );
        contexts
            .into_iter()
            .flat_map(|context| {
                scope::applicable_configs(&self.result, context)
                    .into_iter()
                    .filter_map(|config| codex::fallback_names(&config.metadata))
                    .next_back()
                    .unwrap_or_default()
            })
            .collect()
    }

    fn build_artifacts(&mut self) {
        let candidates: Vec<_> = self.candidates.values().cloned().collect();
        for candidate in candidates.iter().filter(|c| {
            provider_classify(self.options.provider, c, &HashSet::new(), false)
                == Some(ArtifactKind::ProviderConfig)
        }) {
            self.add_artifact(candidate, ArtifactKind::ProviderConfig);
        }
        let configs: Vec<_> = self
            .result
            .artifacts
            .iter()
            .filter(|a| a.kind == ArtifactKind::ProviderConfig)
            .cloned()
            .collect();
        let mut fallback_names: HashSet<String> = HashSet::new();
        let mut roles: BTreeMap<PathBuf, Vec<(codex::RoleDeclaration, Artifact)>> = BTreeMap::new();
        for config in configs
            .iter()
            .filter(|_| self.options.provider == Provider::Codex)
        {
            fallback_names.extend(codex::fallback_names(&config.metadata).unwrap_or_default());
            for role in codex::roles(&config.metadata, config.physical_path.as_path()) {
                let mut target = role.path.clone();
                let status = match links::bounded_path(
                    &target,
                    &self.boundaries,
                    &self.options.scan.excluded_dirs,
                ) {
                    Ok(physical) => {
                        target = physical;
                        ReferenceStatus::Resolved
                    }
                    Err(status) => status,
                };
                let reference = Reference {
                    destination: role.path.display().to_string(),
                    line: 1,
                    target: Some(target.clone().into()),
                    target_id: None,
                    status,
                    fragment_checked: false,
                };
                if reference.status == ReferenceStatus::Resolved {
                    roles
                        .entry(target)
                        .or_default()
                        .push((role.clone(), config.clone()));
                } else {
                    self.reference_diagnostic(&config.id, config.path.as_path(), &reference);
                }
                if let Some(artifact) = self.result.artifacts.iter_mut().find(|a| a.id == config.id)
                {
                    artifact.references.push(reference);
                }
            }
        }
        for candidate in &candidates {
            if self.cancel.is_cancelled() {
                break;
            }
            let kind = if roles.contains_key(&candidate.physical) {
                Some(ArtifactKind::LegacyAgent)
            } else {
                let scoped_fallbacks = if candidate
                    .path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|name| fallback_names.contains(name))
                {
                    self.fallback_names_for(candidate, &configs)
                } else {
                    HashSet::new()
                };
                provider_classify(
                    self.options.provider,
                    candidate,
                    &scoped_fallbacks,
                    self.options.all_markdown,
                )
            };
            if let Some(kind) = kind {
                self.add_artifact(candidate, kind);
            }
        }
        for (path, declarations) in roles {
            let (role, config) = &declarations[0];
            let candidate = self.reference_candidate(&path);
            if let Some(id) = self.add_artifact(&candidate, ArtifactKind::LegacyAgent) {
                if let Some(artifact) = self.result.artifacts.iter_mut().find(|a| a.id == id) {
                    artifact.role_declaration_count = declarations.len();
                    artifact.declared_role_names = declarations
                        .iter()
                        .map(|(role, _)| role.name.clone())
                        .collect();
                    artifact.declared_role_names.sort();
                    artifact.declared_role_names.dedup();
                    artifact.name = (declarations.len() == 1).then(|| role.name.clone());
                    artifact.description = if declarations.len() == 1 {
                        role.description.clone()
                    } else {
                        None
                    };
                    artifact.scope_directory = config.scope_directory.clone();
                    artifact.scope_checkout_id = config.scope_checkout_id.clone();
                }
                if declarations.len() > 1 {
                    self.diagnostic(&candidate.path, Some(id), Severity::Warning, "shared_role_layer",
                        "Several role declarations share this source. Their names and config references are retained; combined scope is unknown.", None);
                }
            }
        }
        self.resolve_references();
        let package_roots: BTreeMap<_, _> = candidates
            .iter()
            .filter_map(|c| c.package.clone())
            .collect();
        for (root, physical) in package_roots {
            let skill_id = snapshot::stable_id("artifact", &root.join("SKILL.md"));
            if !self.result.artifacts.iter().any(|a| a.id == skill_id) {
                continue;
            }
            let id = snapshot::stable_id("package", &root);
            let members = self
                .result
                .artifacts
                .iter()
                .filter(|a| a.package_id.as_deref() == Some(id.as_str()))
                .map(|a| a.id.clone())
                .collect();
            if self
                .result
                .linked_sources
                .iter()
                .any(|link| link.path.as_path().starts_with(&root))
            {
                for artifact in self
                    .result
                    .artifacts
                    .iter_mut()
                    .filter(|a| a.package_id.as_deref() == Some(id.as_str()))
                {
                    artifact.read_only_reason = Some(
                        "Package contains linked content; mutation semantics are not supported."
                            .into(),
                    );
                }
            }
            self.result.packages.push(SkillPackage {
                id,
                root: root.into(),
                physical_root: physical.into(),
                skill_id,
                member_ids: members,
            });
        }
    }

    fn reference_diagnostic(&mut self, id: &str, path: &Path, reference: &Reference) {
        let (severity, code, message) = match reference.status {
            ReferenceStatus::Missing => (
                Severity::Error,
                "missing_reference",
                "Local reference target does not exist.",
            ),
            ReferenceStatus::OutsideRoots => (
                Severity::Warning,
                "unregistered_reference",
                "Target is outside registered roots; its contents were not inspected.",
            ),
            ReferenceStatus::Excluded => (
                Severity::Info,
                "excluded_reference",
                "Target is beneath an excluded directory.",
            ),
            ReferenceStatus::UnsupportedLink => (
                Severity::Warning,
                "linked_reference",
                "Target uses an unsupported link or cannot be inspected.",
            ),
            ReferenceStatus::UnsupportedScheme => (
                Severity::Warning,
                "unsupported_reference",
                "URL scheme or path encoding is not supported.",
            ),
            _ => return,
        };
        self.diagnostic(
            path,
            Some(id.into()),
            severity,
            code,
            message,
            Some(reference.line),
        );
    }

    fn resolve_references(&mut self) {
        let mut pending: std::collections::VecDeque<_> = (0..self.result.artifacts.len()).collect();
        let mut visited = HashSet::new();
        while let Some(index) = pending.pop_front() {
            if self.cancel.is_cancelled() || index >= self.options.max_files {
                break;
            }
            if !visited.insert(index) {
                continue;
            }
            let artifact = self.result.artifacts[index].clone();
            let mut destinations = Vec::new();
            if matches!(
                artifact.kind,
                ArtifactKind::Instruction
                    | ArtifactKind::Skill
                    | ArtifactKind::Reference
                    | ArtifactKind::Markdown
                    | ArtifactKind::Rule
                    | ArtifactKind::LegacyCommand
                    | ArtifactKind::Agent
            ) && let Some(text) = self
                .result
                .snapshots
                .get(&artifact.id)
                .and_then(|s| s.text.as_deref())
            {
                let text = text.trim_start_matches('\u{feff}');
                let offset = if matches!(
                    artifact.kind,
                    ArtifactKind::Skill
                        | ArtifactKind::Agent
                        | ArtifactKind::Rule
                        | ArtifactKind::LegacyCommand
                ) {
                    codex::frontmatter(text)
                        .map(|(_, offset)| offset)
                        .unwrap_or(0)
                } else {
                    0
                };
                let lines = text[..offset].bytes().filter(|b| *b == b'\n').count();
                destinations.extend(
                    links::extract(&text[offset..])
                        .into_iter()
                        .map(|(url, line)| (url, line + lines, false)),
                );
                if self.options.provider == Provider::Claude
                    && matches!(
                        artifact.kind,
                        ArtifactKind::Instruction
                            | ArtifactKind::Rule
                            | ArtifactKind::Reference
                            | ArtifactKind::Skill
                            | ArtifactKind::LegacyCommand
                    )
                {
                    let imports = crate::providers::claude::imports(&text[offset..]);
                    if !imports.is_empty() {
                        self.diagnostic(artifact.path.as_path(),Some(artifact.id.clone()),Severity::Info,"claude_imports","Literal @file targets are inspected within registered roots. Runtime import approval, recursion depth, dynamic expressions, and synced-skill import behavior remain unknown; rename repairs do not rewrite @imports.",None);
                    }
                    destinations.extend(imports.into_iter().map(|(p, line)| {
                        let p = if let Some(rest) = p.strip_prefix("~/") {
                            directories::BaseDirs::new()
                                .map(|d| d.home_dir().join(rest).display().to_string())
                                .unwrap_or(p)
                        } else {
                            p
                        };
                        (p, line + lines, true)
                    }));
                }
            }
            let mut parent = artifact
                .physical_path
                .as_path()
                .parent()
                .unwrap()
                .to_path_buf();
            if artifact.kind == ArtifactKind::SkillMetadata {
                parent = parent.parent().unwrap_or(&parent).to_path_buf();
                for key in ["icon_small", "icon_large"] {
                    if let Some(path) = artifact
                        .metadata
                        .pointer(&format!("/interface/{key}"))
                        .and_then(serde_json::Value::as_str)
                    {
                        destinations.push((path.into(), 1, false));
                    }
                }
            }
            let mut references = artifact.references.clone();
            for (url, line, import) in destinations {
                let resolve = if import {
                    links::resolve_import
                } else {
                    links::resolve
                };
                let reference = resolve(
                    &url,
                    line,
                    &parent,
                    &self.boundaries,
                    &self.options.scan.excluded_dirs,
                );
                if references
                    .iter()
                    .any(|r| r.destination == reference.destination && r.line == reference.line)
                {
                    continue;
                }
                self.reference_diagnostic(&artifact.id, artifact.path.as_path(), &reference);
                if reference.status == ReferenceStatus::Resolved
                    && let Some(target) = &reference.target
                    && target.as_path().is_file()
                    && !self
                        .result
                        .artifacts
                        .iter()
                        .any(|a| a.physical_path == *target)
                {
                    let candidate = self.reference_candidate(target.as_path());
                    let kind = if markdown(target.as_path()) {
                        ArtifactKind::Reference
                    } else {
                        ArtifactKind::SupportingFile
                    };
                    if self.add_artifact(&candidate, kind).is_some() {
                        pending.push_back(self.result.artifacts.len() - 1);
                    }
                }
                // All-Markdown discovery may have visited this imported document already.
                // Promote it once and follow its imports too, without looping on cycles.
                if import
                    && reference.status == ReferenceStatus::Resolved
                    && let Some(target) = &reference.target
                    && let Some(other) = self.result.artifacts.iter().position(|a| {
                        a.physical_path == *target && a.kind == ArtifactKind::Markdown
                    })
                {
                    self.result.artifacts[other].kind = ArtifactKind::Reference;
                    visited.remove(&other);
                    pending.push_back(other);
                }
                references.push(reference);
            }
            self.result.artifacts[index].references = references;
        }
        let targets: HashMap<_, _> = self
            .result
            .artifacts
            .iter()
            .map(|a| (a.physical_path.clone(), a.id.clone()))
            .collect();
        for artifact in &mut self.result.artifacts {
            for reference in &mut artifact.references {
                reference.target_id = reference
                    .target
                    .as_ref()
                    .and_then(|p| targets.get(p))
                    .cloned();
            }
        }
    }

    fn duplicates(&mut self) {
        if self.options.provider == Provider::Claude {
            self.claude_duplicates();
            return;
        }
        let named: Vec<_> = self
            .result
            .artifacts
            .iter()
            .filter(|a| {
                matches!(
                    a.kind,
                    ArtifactKind::Skill | ArtifactKind::Agent | ArtifactKind::LegacyAgent
                ) && a.name.is_some()
            })
            .cloned()
            .collect();
        for (index, a) in named.iter().enumerate() {
            for b in &named[index + 1..] {
                let same_kind = a.kind == b.kind
                    || matches!(
                        (a.kind, b.kind),
                        (ArtifactKind::Agent, ArtifactKind::LegacyAgent)
                            | (ArtifactKind::LegacyAgent, ArtifactKind::Agent)
                    );
                let overlap = a.scope_checkout_id.is_none()
                    || b.scope_checkout_id.is_none()
                    || (a.scope_checkout_id == b.scope_checkout_id
                        && (a
                            .scope_directory
                            .as_path()
                            .starts_with(b.scope_directory.as_path())
                            || b.scope_directory
                                .as_path()
                                .starts_with(a.scope_directory.as_path())));
                if same_kind && a.name == b.name && overlap && a.physical_path != b.physical_path {
                    self.diagnostic(a.path.as_path(), Some(a.id.clone()), Severity::Warning, "duplicate_identity",
                        &format!("Another potentially co-visible definition has the same name ({}); both sources are retained.", b.id), None);
                }
            }
        }
    }
}

fn is_config(candidate: &Candidate) -> bool {
    candidate
        .path
        .file_name()
        .is_some_and(|n| n == "config.toml")
        && (candidate.source.mode == Mode::Home
            || candidate
                .path
                .parent()
                .is_some_and(|p| p.file_name().is_some_and(|n| n == ".codex")))
        && candidate.package.is_none()
}

fn classify(
    candidate: &Candidate,
    fallbacks: &HashSet<String>,
    all_markdown: bool,
) -> Option<ArtifactKind> {
    let path = &candidate.path;
    let name = path.file_name()?.to_str();
    if is_config(candidate) {
        return Some(ArtifactKind::ProviderConfig);
    }
    if let Some((root, _)) = &candidate.package {
        if path == &root.join("SKILL.md") {
            return Some(ArtifactKind::Skill);
        }
        if path == &root.join("agents/openai.yaml") {
            return Some(ArtifactKind::SkillMetadata);
        }
        return Some(if markdown(path) {
            ArtifactKind::Reference
        } else {
            ArtifactKind::SupportingFile
        });
    }
    if name
        .is_some_and(|n| matches!(n, "AGENTS.md" | "AGENTS.override.md") || fallbacks.contains(n))
    {
        return Some(ArtifactKind::Instruction);
    }
    if path.extension().is_some_and(|e| e == "toml")
        && (candidate.source.mode == Mode::Agents
            || path.parent().is_some_and(|p| {
                p.file_name().is_some_and(|n| n == "agents")
                    && (p
                        .parent()
                        .is_some_and(|p| p.file_name().is_some_and(|n| n == ".codex"))
                        || candidate.source.mode == Mode::Collection)
            }))
    {
        return Some(ArtifactKind::Agent);
    }
    if name == Some("plugin.json")
        && path
            .parent()
            .is_some_and(|p| p.file_name().is_some_and(|n| n == ".codex-plugin"))
    {
        return Some(ArtifactKind::PluginManifest);
    }
    (all_markdown && markdown(path)).then_some(ArtifactKind::Markdown)
}

pub(super) fn collection_scope(path: &Path) -> Option<(PathBuf, bool)> {
    let mut root = PathBuf::new();
    let parts: Vec<_> = path.components().collect();
    for pair in parts.windows(2) {
        if (pair[0].as_os_str() == ".agents" || pair[0].as_os_str() == ".codex")
            && pair[1].as_os_str() == "skills"
        {
            return Some((root, pair[0].as_os_str() == ".codex"));
        }
        root.push(pair[0].as_os_str());
    }
    None
}

fn markdown(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("md") || e.eq_ignore_ascii_case("markdown"))
}

/// Classify a proposed path using the same source modes and native filename rules as discovery.
pub(crate) fn proposed_kind(
    inventory: &Inventory,
    path: &Path,
    package: Option<&Path>,
) -> Option<ArtifactKind> {
    if crate::providers::foreign_path(inventory.options.provider, path)
        || foreign_sources(&inventory.options)
            .iter()
            .any(|p| path.starts_with(p))
        || path.components().any(|p| p.as_os_str() == ".system")
        || provider_collection_scope(inventory.options.provider, path)
            .is_some_and(|(_, legacy)| legacy)
        || inventory.options.provider == Provider::Claude && claude_paths::reserved(path)
    {
        return None;
    }
    let mut sources = provider_sources(&inventory.options);
    for source in &mut sources {
        source.path = source
            .path
            .canonicalize()
            .unwrap_or_else(|_| source.path.clone());
    }
    sources.extend(inventory.repositories.checkouts.iter().map(|c| Source {
        path: c.path.0.clone(),
        provenance: Provenance::Repository,
        mode: Mode::Repository,
        optional: false,
        derived: false,
        link_target: None,
    }));
    sources.retain(|s| path.starts_with(&s.path));
    sources.sort_by_key(|s| (s.mode != Mode::Repository, s.path.components().count()));
    let source = sources.pop()?;
    if !source.provenance.is_authoring() {
        return None;
    }
    if source.mode == Mode::Home
        && !(path.parent() == Some(source.path.as_path())
            && path.file_name().is_some_and(|n| {
                home_files(inventory.options.provider)
                    .iter()
                    .any(|name| n == *name)
            }))
    {
        return None;
    }
    // Traversal never starts an independent package inside an existing package.
    let package = inventory
        .packages
        .iter()
        .find(|p| path.starts_with(p.root.as_path()))
        .map(|p| p.root.0.clone())
        .or_else(|| package.map(Path::to_path_buf));
    if let Some(root) = &package {
        if !matches!(source.mode, Mode::Skills | Mode::Collection)
            && provider_collection_scope(inventory.options.provider, root).is_none()
        {
            return None;
        }
        if !path.starts_with(root) {
            return None;
        }
    }
    let candidate = Candidate {
        path: path.into(),
        physical: path.into(),
        source,
        package: package.map(|p| (p.clone(), p)),
        linked: false,
    };
    provider_classify(
        inventory.options.provider,
        &candidate,
        &HashSet::new(),
        true,
    )
}

/// Provider roots used to keep recovery state outside source directories.
pub(crate) fn configured_source_paths(config: &crate::Config) -> Vec<PathBuf> {
    resolve_sources(&config.codex)
        .into_iter()
        .chain(claude_paths::sources(&config.claude))
        .map(|s| s.path.canonicalize().unwrap_or(s.path))
        .collect()
}

fn provider_sources(options: &InventoryOptions) -> Vec<Source> {
    match options.provider {
        Provider::Codex => resolve_sources(&options.codex),
        Provider::Claude => claude_paths::sources(&options.claude),
    }
}
fn home_files(provider: Provider) -> &'static [&'static str] {
    match provider {
        Provider::Codex => &["AGENTS.override.md", "AGENTS.md", "config.toml"],
        Provider::Claude => &[
            "CLAUDE.md",
            "CLAUDE.local.md",
            "settings.json",
            "settings.local.json",
        ],
    }
}
fn provider_collection_scope(provider: Provider, path: &Path) -> Option<(PathBuf, bool)> {
    match provider {
        Provider::Codex => collection_scope(path),
        Provider::Claude => claude_paths::collection_scope(path),
    }
}
fn provider_classify(
    provider: Provider,
    c: &Candidate,
    fallbacks: &HashSet<String>,
    all: bool,
) -> Option<ArtifactKind> {
    if crate::providers::foreign_path(provider, &c.path) {
        return None;
    }
    match provider {
        Provider::Codex => classify(c, fallbacks, all),
        Provider::Claude => claude_paths::classify(c, all),
    }
}
impl<F: FnMut(&InventoryProgress)> Builder<'_, F> {
    fn claude_duplicates(&mut self) {
        let named: Vec<_> = self
            .result
            .artifacts
            .iter()
            .filter(|a| {
                matches!(
                    a.kind,
                    ArtifactKind::Skill | ArtifactKind::Agent | ArtifactKind::LegacyCommand
                )
            })
            .cloned()
            .collect();
        for (i, a) in named.iter().enumerate() {
            for b in &named[i + 1..] {
                let a_name = crate::providers::claude_scope::identity(a);
                let b_name = crate::providers::claude_scope::identity(b);
                let family = (a.kind == ArtifactKind::Agent) == (b.kind == ArtifactKind::Agent);
                let overlap = a.scope_checkout_id.is_none()
                    || b.scope_checkout_id.is_none()
                    || a.scope_checkout_id == b.scope_checkout_id;
                if a_name.is_some()
                    && a_name == b_name
                    && family
                    && overlap
                    && a.physical_path != b.physical_path
                {
                    self.diagnostic(a.path.as_path(),Some(a.id.clone()),Severity::Info,"claude_name_overlap",&format!("Same Claude invocation identity as {}. Sources stay separate; inspect directory scope for provider-specific precedence and namespace uncertainty.",b.id),None);
                }
            }
        }
    }
}

/// Every configured provider source retains its boundary across provider switches.
pub(crate) fn foreign_sources(options: &InventoryOptions) -> Vec<PathBuf> {
    let sources = match options.provider {
        Provider::Codex => claude_paths::sources(&options.claude),
        Provider::Claude => resolve_sources(&options.codex),
    };
    sources
        .into_iter()
        .map(|s| s.path.canonicalize().unwrap_or(s.path))
        .collect()
}
