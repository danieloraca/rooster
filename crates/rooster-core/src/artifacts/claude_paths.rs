use super::*;

pub(super) fn sources(settings: &ClaudeSettings) -> Vec<Source> {
    let home = settings.home.as_ref().map(|p| p.0.clone()).or_else(|| {
        settings
            .include_default_roots
            .then(|| {
                std::env::var_os("CLAUDE_CONFIG_DIR")
                    .map(PathBuf::from)
                    .or_else(|| directories::BaseDirs::new().map(|d| d.home_dir().join(".claude")))
            })
            .flatten()
    });
    let mut sources = vec![];
    if let Some(path) = home {
        let path = absolute_input(&path).unwrap_or(path);
        let path = path.canonicalize().unwrap_or(path);
        sources.push(source(
            path.clone(),
            Provenance::Personal,
            Mode::Home,
            settings.home.is_none(),
            false,
        ));
        for (name, provenance, mode) in [
            ("skills", Provenance::Personal, Mode::Skills),
            ("agents", Provenance::Personal, Mode::Agents),
            ("rules", Provenance::Personal, Mode::Rules),
            ("commands", Provenance::Personal, Mode::Commands),
            ("skills/synced", Provenance::Synced, Mode::Collection),
            ("plugins", Provenance::InstalledPlugin, Mode::Collection),
        ] {
            sources.push(source(path.join(name), provenance, mode, true, true));
        }
    }
    let managed = settings.managed.as_ref().map(|p| p.0.clone()).or_else(|| {
        settings.include_default_roots.then(|| {
            #[cfg(target_os = "macos")]
            {
                PathBuf::from("/Library/Application Support/ClaudeCode")
            }
            #[cfg(not(target_os = "macos"))]
            {
                PathBuf::from("/etc/claude-code")
            }
        })
    });
    if let Some(path) = managed {
        sources.push(source(
            path,
            Provenance::Managed,
            Mode::Collection,
            settings.managed.is_none(),
            false,
        ));
    }
    sources.extend(settings.extra_roots.iter().map(|r| {
        source(
            r.path.0.clone(),
            r.provenance,
            Mode::Collection,
            false,
            false,
        )
    }));
    sources
}
fn source(
    path: PathBuf,
    provenance: Provenance,
    mode: Mode,
    optional: bool,
    derived: bool,
) -> Source {
    Source {
        path,
        provenance,
        mode,
        optional,
        derived,
        link_target: None,
    }
}
fn native_root(path: &Path, category: &str) -> Option<PathBuf> {
    let mut root = PathBuf::new();
    let parts: Vec<_> = path.components().collect();
    for pair in parts.windows(2) {
        if pair[0].as_os_str() == ".claude" && pair[1].as_os_str() == category {
            return Some(root);
        }
        root.push(pair[0].as_os_str());
    }
    None
}
pub(super) fn collection_scope(path: &Path) -> Option<(PathBuf, bool)> {
    native_root(path, "skills").map(|p| (p, false))
}
/// Reserved account-managed paths must not become authoring destinations, including missing ones.
pub(super) fn reserved(path: &Path) -> bool {
    let parts: Vec<_> = path.components().collect();
    parts.windows(2).any(|p| {
        p[0].as_os_str().eq_ignore_ascii_case("skills")
            && (p[1].as_os_str().eq_ignore_ascii_case("synced")
                || p[1].as_os_str().eq_ignore_ascii_case(".trash"))
    })
}
pub(super) fn classify(c: &Candidate, all_markdown: bool) -> Option<ArtifactKind> {
    let path = &c.path;
    if let Some((root, _)) = &c.package {
        return Some(if path == &root.join("SKILL.md") {
            ArtifactKind::Skill
        } else if markdown(path) {
            ArtifactKind::Reference
        } else {
            ArtifactKind::SupportingFile
        });
    }
    let name = path.file_name()?.to_str()?;
    let in_claude = path
        .parent()
        .is_some_and(|p| p.file_name().is_some_and(|n| n == ".claude"));
    if matches!(name, "settings.json" | "settings.local.json")
        && (in_claude || c.source.mode == Mode::Home)
        || name == "managed-settings.json" && c.source.provenance == Provenance::Managed
    {
        return Some(ArtifactKind::ProviderConfig);
    }
    if matches!(name, "CLAUDE.md" | "CLAUDE.local.md") {
        return Some(ArtifactKind::Instruction);
    }
    if name == "plugin.json"
        && path
            .parent()
            .is_some_and(|p| p.file_name().is_some_and(|n| n == ".claude-plugin"))
    {
        return Some(ArtifactKind::PluginManifest);
    }
    if markdown(path) {
        if native_root(path, "rules").is_some() || c.source.mode == Mode::Rules {
            return Some(ArtifactKind::Rule);
        }
        if native_root(path, "commands").is_some()
            || c.source.mode == Mode::Commands
            || c.source.mode == Mode::Collection
                && path
                    .ancestors()
                    .any(|p| p.file_name().is_some_and(|n| n == "commands"))
        {
            return Some(ArtifactKind::LegacyCommand);
        }
        let agents = native_root(path, "agents").is_some()
            || c.source.mode == Mode::Agents
            || c.source.mode == Mode::Collection
                && path
                    .ancestors()
                    .any(|p| p.file_name().is_some_and(|n| n == "agents"));
        if agents {
            return Some(ArtifactKind::Agent);
        }
        if all_markdown {
            return Some(ArtifactKind::Markdown);
        }
    }
    None
}
pub(super) fn scope(c: &Candidate, kind: ArtifactKind) -> PathBuf {
    for category in ["skills", "agents", "rules", "commands"] {
        if let Some(root) = native_root(&c.path, category) {
            return root;
        }
    }
    if c.source.mode != Mode::Repository {
        return c.source.path.clone();
    }
    let parent = c.path.parent().unwrap_or(&c.source.path);
    if matches!(
        kind,
        ArtifactKind::Instruction | ArtifactKind::ProviderConfig
    ) && parent.file_name().is_some_and(|n| n == ".claude")
    {
        return parent.parent().unwrap_or(parent).into();
    }
    parent.into()
}
