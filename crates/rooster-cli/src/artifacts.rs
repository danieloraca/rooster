use clap::{Args, Subcommand, ValueEnum};
use rooster_core::{
    CancellationToken, ConfigStore, NativePath, ScanOptions, ScanStatus,
    artifacts::{
        AdditionalRoot, ArtifactKind, Inventory, InventoryOptions, Provenance, Severity, inventory,
    },
    providers::{ArtifactProvider, Provider},
};
use std::{
    io::{self, IsTerminal},
    path::PathBuf,
    time::{Duration, Instant},
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[derive(Subcommand)]
pub(super) enum ArtifactCommand {
    /// Discover artifacts; source bodies are omitted from list output.
    List(ArtifactArgs),
    /// Rescan and inspect a stable artifact ID, including the exact source snapshot.
    Show {
        id: String,
        #[command(flatten)]
        args: ArtifactArgs,
    },
    /// Print a native template without writing files.
    Template {
        #[arg(long, default_value = "codex")]
        provider: Provider,
        #[arg(value_enum)]
        kind: TemplateKind,
        #[arg(long, default_value = "example")]
        name: String,
        #[arg(long, default_value = "Describe when to use this definition.")]
        description: String,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Clone, Copy, ValueEnum)]
pub(super) enum TemplateKind {
    Instruction,
    Skill,
    Agent,
    Rule,
    LegacyCommand,
}

#[derive(Args)]
pub(super) struct ArtifactArgs {
    #[arg(long, default_value = "codex")]
    provider: Provider,
    #[arg(long)]
    claude_home: Option<PathBuf>,
    #[arg(long)]
    managed_root: Option<PathBuf>,
    #[arg(long)]
    synced_root: Vec<PathBuf>,
    #[arg(long)]
    workspace: Option<String>,
    #[arg(long)]
    json: bool,
    /// Also include ordinary Markdown, respecting Git ignore rules.
    #[arg(long)]
    all_markdown: bool,
    /// Working directory for an estimated scope; does not start an assistant.
    #[arg(long)]
    context: Option<PathBuf>,
    /// Disable ambient home, admin, compatibility, and plugin discovery. Explicit overrides still apply.
    #[arg(long)]
    no_default_roots: bool,
    #[arg(long)]
    codex_home: Option<PathBuf>,
    #[arg(long)]
    user_skills: Option<PathBuf>,
    #[arg(long)]
    admin_skills: Option<PathBuf>,
    /// Additional compatibility root; repeat as needed. Activation is unknown.
    #[arg(long)]
    compat_root: Vec<PathBuf>,
    /// Additional installed-plugin root; read-only and not assumed active.
    #[arg(long)]
    installed_root: Vec<PathBuf>,
    #[arg(long)]
    exclude: Vec<String>,
}

pub(super) fn run(store: &ConfigStore, command: ArtifactCommand) -> Result<u8> {
    match command {
        ArtifactCommand::Template {
            provider,
            kind,
            name,
            description,
            json,
        } => {
            let kind = match kind {
                TemplateKind::Instruction => ArtifactKind::Instruction,
                TemplateKind::Skill => ArtifactKind::Skill,
                TemplateKind::Agent => ArtifactKind::Agent,
                TemplateKind::Rule => ArtifactKind::Rule,
                TemplateKind::LegacyCommand => ArtifactKind::LegacyCommand,
            };
            let template = provider.template(kind, &name, &description)?;
            if json {
                super::print_json(&template)?;
            } else {
                print!("{}", template.content);
            }
            Ok(0)
        }
        ArtifactCommand::List(args) => {
            let result = discover(store, &args)?;
            if args.json {
                super::print_json(&result)?;
            } else {
                for artifact in &result.artifacts {
                    println!(
                        "{}  {:?}  {:?}  {:?}",
                        artifact.id,
                        artifact.kind,
                        artifact.provenance,
                        artifact.path.as_path()
                    );
                }
                print_diagnostics(&result);
                print_summary(&result);
            }
            Ok(exit_status(&result, false))
        }
        ArtifactCommand::Show { id, args } => {
            let result = discover(store, &args)?;
            if result.status == ScanStatus::Cancelled {
                if args.json {
                    super::print_json(&result)?;
                } else {
                    print_summary(&result);
                }
                return Ok(130);
            }
            let view = result.inspect(&id).ok_or_else(|| {
                format!(
                    "artifact not found in the current {:?} inventory: {id}",
                    result.status
                )
            })?;
            if args.json {
                super::print_json(&view)?;
            } else {
                println!(
                    "{:?} · {:?} · {:?}",
                    view.artifact.kind,
                    view.artifact.validation,
                    view.artifact.path.as_path()
                );
                if let Some(reason) = &view.artifact.read_only_reason {
                    println!("{}", terminal_text(reason));
                }
                if let Some(snapshot) = view.snapshot {
                    if let Some(text) = &snapshot.text {
                        println!("\n{}", terminal_text(text));
                    } else {
                        println!(
                            "Binary source: {} bytes; use --json for the lossless snapshot.",
                            snapshot.bytes.len()
                        );
                    }
                }
                for related in view.related {
                    println!("Related: {}  {:?}", related.id, related.path.as_path());
                }
                for diagnostic in view.diagnostics {
                    println!(
                        "{:?}: {} — {}",
                        diagnostic.severity,
                        diagnostic.code,
                        terminal_text(&diagnostic.message)
                    );
                }
            }
            Ok(exit_status(&result, false))
        }
    }
}

pub(super) fn check(store: &ConfigStore, args: ArtifactArgs) -> Result<u8> {
    let result = discover(store, &args)?;
    if args.json {
        super::print_json(&result)?;
    } else {
        print_diagnostics(&result);
        print_summary(&result);
    }
    Ok(exit_status(&result, true))
}

fn discover(store: &ConfigStore, args: &ArtifactArgs) -> Result<Inventory> {
    let config = store.load()?;
    let roots = config.roots(args.workspace.as_deref())?;
    let mut codex = config.codex;
    let mut claude = config.claude;
    if args.provider == Provider::Codex
        && (args.claude_home.is_some()
            || args.managed_root.is_some()
            || !args.synced_root.is_empty())
    {
        return Err("Claude root options require --provider claude".into());
    }
    if args.provider == Provider::Claude
        && (args.codex_home.is_some() || args.user_skills.is_some() || args.admin_skills.is_some())
    {
        return Err("Codex root options require --provider codex".into());
    }
    if let Some(path) = &args.claude_home {
        claude.home = Some(NativePath::resolve(path)?);
    }
    if let Some(path) = &args.managed_root {
        claude.managed = Some(NativePath::resolve(path)?);
    }
    for path in &args.synced_root {
        claude.extra_roots.push(AdditionalRoot {
            path: NativePath::resolve(path)?,
            provenance: Provenance::Synced,
        });
    }
    if args.no_default_roots {
        codex.include_default_roots = false;
        claude.include_default_roots = false;
    }
    if let Some(path) = &args.codex_home {
        codex.home = Some(NativePath::resolve(path)?);
    }
    if let Some(path) = &args.user_skills {
        codex.user_skills = Some(NativePath::resolve(path)?);
    }
    if let Some(path) = &args.admin_skills {
        codex.admin_skills = Some(NativePath::resolve(path)?);
    }
    for (paths, provenance) in [
        (&args.compat_root, Provenance::Compatibility),
        (&args.installed_root, Provenance::InstalledPlugin),
    ] {
        for path in paths {
            let extras = if args.provider == Provider::Codex {
                &mut codex.extra_roots
            } else {
                &mut claude.extra_roots
            };
            extras.push(AdditionalRoot {
                path: NativePath::resolve(path)?,
                provenance,
            });
        }
    }
    let mut exclusions = config.excluded_dirs;
    for name in &args.exclude {
        if name.is_empty() || name == "." || name == ".." || name.contains(['/', '\\']) {
            return Err("--exclude expects a directory name".into());
        }
        if !exclusions.contains(name) {
            exclusions.push(name.clone());
        }
    }
    let options = InventoryOptions {
        provider: args.provider,
        claude,
        scan: ScanOptions {
            excluded_dirs: exclusions,
            ..ScanOptions::default()
        },
        codex,
        all_markdown: args.all_markdown,
        context: args
            .context
            .as_ref()
            .map(NativePath::resolve)
            .transpose()?
            .map(|p| p.0),
        ..InventoryOptions::default()
    };
    let cancellation = CancellationToken::default();
    let signal = cancellation.clone();
    ctrlc::set_handler(move || signal.cancel())?;
    let interactive = !args.json && io::stderr().is_terminal();
    let mut last = Instant::now();
    let result = inventory(&roots, &options, &cancellation, |progress| {
        if interactive && last.elapsed() >= Duration::from_millis(250) {
            eprintln!(
                "Scanning {}: {} folders, {} artifacts",
                progress.phase, progress.visited, progress.artifacts
            );
            last = Instant::now();
        }
    });
    Ok(result)
}

fn exit_status(result: &Inventory, check: bool) -> u8 {
    match result.status {
        ScanStatus::Cancelled => 130,
        ScanStatus::Partial => 2,
        ScanStatus::Complete
            if check
                && result
                    .diagnostics
                    .iter()
                    .any(|d| d.severity == Severity::Error) =>
        {
            3
        }
        ScanStatus::Complete => 0,
    }
}

fn print_diagnostics(result: &Inventory) {
    for issue in &result.repositories.issues {
        println!(
            "Repository issue: {:?}: {}",
            issue.path.as_path(),
            terminal_text(&issue.message)
        );
    }
    for diagnostic in &result.diagnostics {
        println!(
            "{:?}: {} {:?} — {}",
            diagnostic.severity,
            diagnostic.code,
            diagnostic.path.as_path(),
            terminal_text(&diagnostic.message)
        );
    }
}

fn print_summary(result: &Inventory) {
    println!(
        "{:?}: {} artifacts, {} skill packages, {} diagnostics.",
        result.status,
        result.artifacts.len(),
        result.packages.len(),
        result.diagnostics.len()
    );
    if let Some(scope) = &result.scope {
        println!(
            "Scope estimate for {:?}: {} instruction candidates. Live-session loading is unknown.",
            scope.context.as_path(),
            scope
                .instructions
                .iter()
                .filter(|i| i.bytes_included > 0)
                .count()
        );
    }
}

fn terminal_text(text: &str) -> String {
    text.chars()
        .flat_map(|c| {
            if c.is_control() && !matches!(c, '\n' | '\r' | '\t') {
                c.escape_default().collect::<Vec<_>>()
            } else {
                vec![c]
            }
        })
        .collect()
}
