use clap::{Args, Parser, Subcommand};
mod artifacts;
mod changes;
use rooster_core::{CancellationToken, ConfigStore, ScanOptions, ScanStatus, scan};
use serde::Serialize;
use std::{
    io::{self, IsTerminal, Write},
    path::PathBuf,
    process::ExitCode,
    time::{Duration, Instant},
};

#[derive(Parser)]
#[command(
    name = "rooster",
    version,
    about = "Discover local repositories for AI configuration management"
)]
struct Cli {
    /// Rooster settings file; overrides the platform user configuration directory.
    #[arg(long, global = true, env = "ROOSTER_CONFIG")]
    config: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Prepare, review, apply, and recover local file changes.
    Changes(changes::ChangeArgs),
    /// Discover and inspect provider instructions, skills, agents, and references.
    Artifacts {
        #[command(subcommand)]
        command: artifacts::ArtifactCommand,
    },
    /// Diagnose local provider artifacts; returns 3 when validation errors are found.
    Check(artifacts::ArtifactArgs),
    /// Register or relocate folders and list workspaces.
    Workspace {
        #[command(subcommand)]
        command: WorkspaceCommand,
    },
    /// Discover repositories beneath registered folders.
    Scan(ScanArgs),
    /// Inspect the repository inventory.
    Repos {
        #[command(subcommand)]
        command: ReposCommand,
    },
}

#[derive(Subcommand)]
enum WorkspaceCommand {
    /// Add a parent folder or direct repository; reuse a name to add another root.
    Add {
        name: String,
        path: PathBuf,
        #[arg(long)]
        json: bool,
    },
    /// Show workspace IDs, root IDs and their registered paths.
    List {
        #[arg(long)]
        json: bool,
    },
    /// Change a registered root's path while retaining its ID.
    Relocate {
        root_id: String,
        path: PathBuf,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
enum ReposCommand {
    /// Scan again and list current repositories (no persistent inventory cache).
    List(ScanArgs),
}

#[derive(Args)]
struct ScanArgs {
    /// Workspace ID or name; omit to scan all workspaces.
    #[arg(long)]
    workspace: Option<String>,
    /// Emit one structured report to stdout, including partial results and errors.
    #[arg(long)]
    json: bool,
    /// Additional directory name to skip; repeat for multiple names.
    #[arg(long)]
    exclude: Vec<String>,
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            eprintln!("rooster: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<u8, Box<dyn std::error::Error>> {
    let store = ConfigStore::new(match cli.config {
        Some(path) => path,
        None => ConfigStore::default_path()?,
    })?;
    match cli.command {
        Command::Changes(args) => return changes::run(&store, args),
        Command::Artifacts { command } => return artifacts::run(&store, command),
        Command::Check(args) => return artifacts::check(&store, args),
        Command::Workspace { command } => match command {
            WorkspaceCommand::Add { name, path, json } => {
                let registration = store.add(&name, path)?;
                if json {
                    print_json(&registration)?;
                } else {
                    println!(
                        "{} ({}) — {}: {}",
                        registration.workspace_name,
                        registration.workspace_id,
                        registration.root.id,
                        registration.root.path
                    );
                }
            }
            WorkspaceCommand::Relocate {
                root_id,
                path,
                json,
            } => {
                let registration = store.relocate(&root_id, path)?;
                if json {
                    print_json(&registration)?;
                } else {
                    println!("{} → {}", registration.root.id, registration.root.path);
                }
            }
            WorkspaceCommand::List { json } => {
                let config = store.load()?;
                if json {
                    print_json(&config)?;
                } else if config.workspaces.is_empty() {
                    println!("No workspaces. Add one with: rooster workspace add <name> <path>");
                } else {
                    for workspace in config.workspaces {
                        println!("{} ({})", workspace.name, workspace.id);
                        for root in workspace.roots {
                            println!("  {}  {}", root.id, root.path);
                        }
                    }
                }
            }
        },
        Command::Scan(args)
        | Command::Repos {
            command: ReposCommand::List(args),
        } => {
            let config = store.load()?;
            let roots = config.roots(args.workspace.as_deref())?;
            let mut exclusions = config.excluded_dirs;
            for name in args.exclude {
                if name.is_empty() || name == "." || name == ".." || name.contains(['/', '\\']) {
                    return Err("--exclude expects a directory name, not a path or pattern".into());
                }
                if !exclusions.contains(&name) {
                    exclusions.push(name);
                }
            }
            let options = ScanOptions {
                excluded_dirs: exclusions,
                ..ScanOptions::default()
            };
            let cancellation = CancellationToken::default();
            let signal_token = cancellation.clone();
            ctrlc::set_handler(move || signal_token.cancel())?;
            let show_progress = !args.json && io::stderr().is_terminal();
            let mut last = Instant::now();
            let report = scan(&roots, &options, &cancellation, |progress| {
                if show_progress && last.elapsed() >= Duration::from_millis(200) {
                    eprint!(
                        "\rScanning: {} folders · {} repos · {} issues    ",
                        progress.visited_dirs, progress.checkouts, progress.issues
                    );
                    let _ = io::stderr().flush();
                    last = Instant::now();
                }
            });
            if show_progress {
                eprintln!();
            }
            if args.json {
                print_json(&report)?;
            } else {
                for checkout in &report.checkouts {
                    let revision = match checkout.head_state {
                        rooster_core::HeadState::Unborn => {
                            format!("{} (no commits)", checkout.branch.as_deref().unwrap_or(""))
                        }
                        rooster_core::HeadState::Branch => {
                            checkout.branch.clone().unwrap_or_default()
                        }
                        rooster_core::HeadState::Detached => {
                            format!("detached {}", checkout.head.as_deref().unwrap_or(""))
                        }
                    };
                    println!("{}  [{:?}; {}]", checkout.path, checkout.kind, revision);
                }
                for issue in &report.issues {
                    eprintln!("Issue: {}: {}", issue.path, issue.message);
                }
                for candidate in &report.worktree_candidates {
                    println!("Worktree candidate (not scanned): {}", candidate.path);
                }
                println!(
                    "{:?}: {} repositories, {} folders visited, {} skipped, {} issues.",
                    report.status,
                    report.checkouts.len(),
                    report.visited_dirs,
                    report.skipped.len(),
                    report.issues.len()
                );
                if roots.is_empty() {
                    println!("Add a workspace with: rooster workspace add <name> <path>");
                }
            }
            return Ok(match report.status {
                ScanStatus::Complete => 0,
                ScanStatus::Partial => 2,
                ScanStatus::Cancelled => 130,
            });
        }
    }
    Ok(0)
}

fn print_json(value: &impl Serialize) -> Result<(), Box<dyn std::error::Error>> {
    let mut out = io::stdout().lock();
    serde_json::to_writer_pretty(&mut out, value)?;
    writeln!(out)?;
    Ok(())
}
