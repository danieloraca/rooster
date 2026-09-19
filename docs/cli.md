# CLI guide

M1 supports workspace registration and repository discovery. M2 adds Codex artifact inventory and inspection, described in the [artifact guide](artifact-inventory.md). M4 adds reviewed file/package mutations and recovery through `changes`, described in the [mutation guide](mutations.md). M5 adds read-only `changes git OWNER_ID`, `changes cleanup-preview --days 30`, and explicit `changes cleanup --days 30 --id CHANGE_ID` using the shared core. Agent execution remains outside the CLI.

## Commands

Run from the repository with `cargo run -- <command>`, or install the local binary with `cargo install --path crates/rooster-cli --locked`.

~~~text
rooster [--config <file>] workspace add <name> <path> [--json]
rooster [--config <file>] workspace list [--json]
rooster [--config <file>] workspace relocate <root-id> <path> [--json]
rooster [--config <file>] scan [--workspace <id-or-name>] [--exclude <directory-name>] [--json]
rooster [--config <file>] repos list [--workspace <id-or-name>] [--exclude <directory-name>] [--json]
~~~

The config flag can appear after the subcommand too. Quote paths containing spaces. Relative paths resolve from the caller's current directory; a leading literal `~` resolves to the current user's home. Registration requires an existing directory and stores its canonical path.

Reuse a workspace name to append a root. Adding the same path to that workspace again returns its existing root ID. Different workspaces may contain the same path. Workspace selectors prefer an exact ID, then an exact name. Omitting the selector scans all workspaces.

Relocation updates a root's registered path and preserves its ID; it does not move files. A registered folder that later goes offline or disappears remains in settings and produces a scan issue until it becomes available or is relocated.

## Settings

Resolution order is `--config`, then `ROOSTER_CONFIG`, then the platform configuration directory plus `rooster/config.json`. On macOS the default is `~/Library/Application Support/rooster/config.json`.

The default file is created on the first successful registration. Listing or scanning an unconfigured installation produces an empty result without creating files. Keep real settings outside shared repositories; an explicit override can point anywhere the user chooses.

Example schema:

~~~json
{
  "schema_version": 1,
  "next_id": 3,
  "workspaces": [
    {
      "id": "workspace-1",
      "name": "Personal",
      "roots": [
        { "id": "root-2", "path": "/absolute/path/to/repos" }
      ]
    }
  ],
  "excluded_dirs": ["node_modules", "vendor", "target", "dist", "build"]
}
~~~

IDs and the allocation counter are managed by Rooster. Exclusions may be edited in this file; unknown fields/schema versions are rejected. Mutations lock `<config-file>.lock`, reload current settings, atomically replace the JSON, and verify the result. A cooperating writer is waited for up to five seconds. The empty lock file is retained for stable coordination. Direct external edits should finish before a CLI mutation; they cannot be protected by Rooster's cooperative lock.

## Scan behaviour

The scanner traverses selected folders without following directory symlinks and recognizes both `.git` directories and pointer files. Nested repositories and initialized submodules are separate checkout records. Overlapping roots produce one record per canonical checkout with all applicable root IDs.

Default excluded directory names are case-sensitive. Repeat `--exclude` to add names for one scan; edit `excluded_dirs` in settings to change persisted defaults. Paths and globs are not supported. Git internals are always pruned. Directly registering a checkout under an excluded directory opts that checkout into discovery.

Git worktree metadata can advertise paths outside selected folders. These appear as candidates, with no automatic filesystem traversal or availability claim. Register the candidate explicitly to scan it. A skipped or undiscovered worktree inside selected roots can also remain a candidate.

Each Git command has a five-second timeout and a 4 MiB output limit. A failed command or unreadable/missing folder adds an issue while other folders continue. Ctrl+C stops traversal and running Git inspection and emits a cancelled report containing results collected so far. A scan is a best-effort observation; files or branches may change during it.

Human output includes branch/HEAD state, counts, issues, and worktree candidates. Interactive human scans show count-based progress on stderr: the total tree size is unknown until traversal finishes. JSON mode writes one report to stdout with no progress messages.

## JSON and exit codes

`scan` and `repos list` return the same schema-versioned report: `status`, `roots`, `visited_dirs`, `checkouts`, `issues`, `skipped`, and `worktree_candidates`. There is no persistent inventory cache.

A checkout includes its ID, path, Git/common directories, root IDs, kind (`repository`, `linked_worktree`, or `submodule`), branch, HEAD commit, HEAD state (`branch`, `detached`, or `unborn`), and optional superproject. Checkout IDs remain stable at the same canonical location; they change if the physical checkout moves. Clones and worktrees have distinct IDs.

Unicode paths are JSON strings. Non-Unicode native paths use `{"unix_bytes":[...]}` or `{"windows_wide":[...]}` so consumers never reconstruct a path from a lossy display string. Encoded paths are local to their platform.

| Exit | Meaning |
| --- | --- |
| 0 | Complete scan or successful workspace command |
| 1 | Configuration/registration error or other fatal application error |
| 2 | Partial scan; inspect issues. Clap also uses 2 for invalid command syntax. |
| 3 | `check` completed its inventory and found validation errors; warnings alone do not fail it. |
| 130 | Cancelled scan; collected results are included |

Configuration and argument errors use stderr and do not produce a scan report.

## Tested limits

Verified on macOS Apple Silicon, Git 2.39.2, and Rust 1.90.0/1.95.0. Fixtures cover sibling/nested repositories, independent clones, submodules, linked worktrees, separate Git metadata, detached/unborn HEAD, overlap, relocation, missing roots, exclusions, symlinks, Unicode/newline paths, timeouts, concurrent settings writers, and Ctrl+C.

Non-Unicode path serialization is tested independently because the development filesystem rejects invalid UTF-8 filenames. Linux/Windows filesystem behaviour, workloads beyond the published measurements, and provider-client runtime acceptance are not yet verified. Native desktop verification is recorded in the desktop guide. See [discovery measurements and tested limits](performance.md) for M7 workload details.

M2's provider validation is structural and uses disposable fixtures; the observed Codex CLI version is recorded in the [compatibility document](provider-compatibility.md). It is not a certification that Codex accepted or loaded those fixtures.

## Reviewed mutations and local recovery (M4–M5)

`rooster changes` exposes the shared prepare → preview → apply → verify protocol and conflict-aware restore. `owners` lists valid destination IDs, `edit --source` and `create --source` prepare text files, `prepare --request` accepts structured operations, `preview` shows a saved change, `apply` applies/finishes it, `restore` reverses it, and `list` reports recovery state. All outputs are JSON.

Use `ROOSTER_DATA_DIR` or `changes --data-dir` for private recovery storage, independently of `ROOSTER_CONFIG`. Complete configuration drives mutation discovery; read-only inventory overrides do not silently authorize writes. See the [mutation guide](mutations.md) for complete requests, preview fields, package scope, exit codes, and safety limits.

## Provider selection

Codex remains the default. Use `--provider claude` on artifact/check commands and `changes` preparation commands for Claude Code local files. The optional `claude` settings object preserves schema-1 backward compatibility and resolves independent home/managed/extra roots. See [Claude inventories](artifact-inventory.md#claude-code-inventories-m6) and [Claude edits](mutations.md#claude-local-files-m6) for examples, JSON additions, supported types, and provider-specific restrictions.
