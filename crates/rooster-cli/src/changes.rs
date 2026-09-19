use clap::{Args, Subcommand};
use rooster_core::{
    CancellationToken, ConfigStore, ScanOptions,
    artifacts::{Inventory, InventoryOptions, inventory},
    changes::{ChangeStore, FileKind, Request, Status},
    providers::Provider,
};
use std::path::PathBuf;
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
#[derive(Args)]
pub(super) struct ChangeArgs {
    #[arg(long, global = true, default_value = "codex")]
    provider: Provider,
    /// Private recovery directory, outside repositories/provider roots.
    #[arg(long, env = "ROOSTER_DATA_DIR", global = true)]
    data_dir: Option<PathBuf>,
    #[command(subcommand)]
    command: ChangeCommand,
}
#[derive(Subcommand)]
enum ChangeCommand {
    /// Inspect staged and working-tree paths in a registered checkout.
    Git { owner_id: String },
    /// Preview completed/restored recovery records eligible for explicit cleanup.
    CleanupPreview {
        #[arg(long, default_value_t = 30)]
        days: u32,
    },
    /// Permanently remove only the listed eligible recovery records, never source files.
    Cleanup {
        #[arg(long, default_value_t = 30)]
        days: u32,
        #[arg(long, required = true)]
        id: Vec<String>,
    },
    /// List valid destination owner IDs from current registered/configured sources.
    Owners,
    /// Save a reviewed proposal from a structured JSON request; does not change source files.
    Prepare {
        #[arg(long)]
        request: PathBuf,
    },
    /// Prepare a full UTF-8 replacement from a file, preserving BOM/CRLF convention.
    Edit {
        artifact_id: String,
        #[arg(long)]
        source: PathBuf,
    },
    /// Prepare an ordinary Markdown file from source text.
    Create {
        owner_id: String,
        path: PathBuf,
        #[arg(long)]
        source: PathBuf,
    },
    /// Show complete before/after text, hashes, paths, and warnings for a saved change.
    Preview { id: String },
    /// Apply (or finish) a saved preview after revalidating files and checkout.
    Apply { id: String },
    /// Restore a completed or interrupted change without overwriting newer edits.
    Restore { id: String },
    /// List saved proposals and recovery-required operations.
    List,
}
pub(super) fn run(config: &ConfigStore, args: ChangeArgs) -> Result<u8> {
    let store = ChangeStore::new(
        config.clone(),
        match args.data_dir {
            Some(path) => path,
            None => ChangeStore::default_path()?,
        },
    )?;
    let restoring = matches!(args.command, ChangeCommand::Restore { .. });
    let request = match args.command {
        ChangeCommand::Git { owner_id } => {
            super::print_json(&ChangeStore::git_changes(
                &discover(config, args.provider)?,
                &owner_id,
            )?)?;
            return Ok(0);
        }
        ChangeCommand::CleanupPreview { days } => {
            super::print_json(&store.cleanup_preview(days)?)?;
            return Ok(0);
        }
        ChangeCommand::Cleanup { days, id } => {
            let result = store.cleanup(days, id)?;
            let code = if result.error.is_some() { 2 } else { 0 };
            super::print_json(&result)?;
            return Ok(code);
        }
        ChangeCommand::Owners => {
            super::print_json(&ChangeStore::owners(&discover(config, args.provider)?))?;
            return Ok(0);
        }
        ChangeCommand::Preview { id } => {
            super::print_json(&store.preview(&id)?)?;
            return Ok(0);
        }
        ChangeCommand::List => {
            super::print_json(&store.history()?)?;
            return Ok(0);
        }
        ChangeCommand::Apply { id } | ChangeCommand::Restore { id } => {
            let result = if restoring {
                store.restore(&id)?
            } else {
                store.apply(&id)?
            };
            let code = if matches!(result.status, Status::Completed | Status::Restored) {
                0
            } else {
                2
            };
            super::print_json(&result)?;
            return Ok(code);
        }
        ChangeCommand::Prepare { request } => {
            let bytes = std::fs::read(request)?;
            if bytes.len() > 64 * 1024 * 1024 {
                return Err("Request exceeds 64 MiB".into());
            }
            serde_json::from_slice(&bytes)?
        }
        ChangeCommand::Edit {
            artifact_id,
            source,
        } => Request::Replace {
            artifact_id,
            text: std::fs::read_to_string(source)?,
        },
        ChangeCommand::Create {
            owner_id,
            path,
            source,
        } => Request::Create {
            owner_id,
            path: path.into(),
            kind: FileKind::Markdown,
            text: std::fs::read_to_string(source)?,
        },
    };
    let inv = discover(config, args.provider)?;
    let preview = store.prepare(&inv, request)?;
    super::print_json(&preview)?;
    Ok(0)
}
fn discover(store: &ConfigStore, provider: Provider) -> Result<Inventory> {
    let config = store.load()?;
    let roots = config.roots(None)?;
    let options = InventoryOptions {
        provider,
        claude: config.claude,
        scan: ScanOptions {
            excluded_dirs: config.excluded_dirs,
            ..ScanOptions::default()
        },
        codex: config.codex,
        all_markdown: true,
        ..InventoryOptions::default()
    };
    let cancellation = CancellationToken::default();
    let token = cancellation.clone();
    ctrlc::set_handler(move || token.cancel())?;
    Ok(inventory(&roots, &options, &cancellation, |_| {}))
}
