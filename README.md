# Rooster

**Your agents, skills, and repository guidance in one place.**

Rooster is a desktop application and CLI in development for browsing and maintaining the files that configure AI coding assistants. It brings personal configuration and guidance from multiple repositories into one interface, while keeping the existing files authoritative.

The project uses Rust, with a Tauri desktop interface. Codex and Claude Code use provider adapters through the same core. The name and Rust + Tauri direction were agreed on 17 September 2026.

## Current status

M1–M7 are complete for the local macOS pilot: the shared Rust core, CLI, and Tauri desktop can register folders, discover Git checkouts, and inspect Codex and Claude Code instructions, skills, agents, rules, legacy commands, and references. The desktop includes content search, source/Markdown preview, scope details, and refresh after external changes.

Both interfaces now prepare, preview, apply, and restore supported file/package changes through the shared Rust service. The desktop adds private saved drafts, source editing, conflict comparison, complete package previews, recovery controls, and read-only Git status. The two-layout pilot, crash/restart recovery, discovery measurements, local macOS packaging, and CI preparation are verified. Public distribution, signing identity, and licence selection remain open.

See the [progress bar, milestone status, and next step](docs/implementation-plan.md#progress). The implementation plan is the single source of progress; checked items represent verified work.

## What Rooster does

- Discover Git checkouts inside folders you choose, add individual repositories, and remove registered locations from the sidebar without deleting files.
- Show personal and repository-specific instructions, skills, custom agents, and supporting Markdown.
- Recognise nested guidance, such as an application's AGENTS.md inside a larger repository.
- Create, duplicate, edit, rename, and recoverably delete supported files in their original checkout.
- Show provenance, local branch/worktree, file changes, references, and configuration diagnostics.
- Let teams maintain shared guidance in Git while each developer keeps their own checkout locations and preferences.

Repository folders are user-selected. A workspace named "Gecko" is one person's grouping, not a built-in path or provider feature.

Rooster distinguishes configuration found on disk from configuration expected to apply in a particular directory. It will not describe a file as loaded by a live AI session without evidence from that session.

## Documentation

| Document | Purpose |
| --- | --- |
| [Product specification](docs/product.md) | Users, workflows, scope, and acceptance criteria |
| [Architecture](docs/architecture.md) | Rust core, Tauri boundary, discovery, persistence, and editing |
| [Provider compatibility](docs/provider-compatibility.md) | Native formats, discovery rules, evidence, and compatibility limits |
| [Implementation plan](docs/implementation-plan.md) | The canonical sequence of work and verification |
| [CLI guide](docs/cli.md) | Working commands, settings, scan output, and current limits |
| [Mutation and recovery guide](docs/mutations.md) | Saved previews, supported operations, conflicts, and recovery |
| [Desktop guide](docs/desktop.md) | Launching the app, browsing, preview isolation, and current limits |
| [Artifact inventory](docs/artifact-inventory.md) | Provider roots, packages, inspection, scope estimates, and diagnostics |
| [Team pilot](docs/team-pilot.md) | Portable onboarding, local state, relocation, and repeatable fixtures |
| [Release preparation](docs/release.md) | Local macOS bundles, installation, signing boundaries, and CI |
| [Discovery measurements](docs/performance.md) | Reproducible workloads, timings, and tested limits |

The product specification owns requirements, the architecture owns design decisions, and the implementation plan owns delivery sequencing. Provider-specific discovery facts belong in the compatibility document.

## Run the CLI

Requires Rust **1.90 or newer** and Git on PATH. The Cargo workspace contains `rooster-core`, `rooster-cli`, and `rooster-desktop`; its binary is `rooster`. Dependencies are pinned by Cargo.lock. Verified on macOS Apple Silicon with Git 2.39.2; other platforms are not yet validated.

From this repository, replace the example path with a folder containing your repositories:

~~~sh
cargo run -- --help
cargo run -- workspace add Personal /absolute/path/to/repos
cargo run -- scan --workspace Personal
cargo run -- repos list --json
cargo run -- artifacts list --workspace Personal
cargo run -- artifacts list --provider claude --workspace Personal
cargo run -- artifacts list --workspace Personal --context /absolute/path/to/repos/your-repo/App --json
cargo run -- artifacts show ARTIFACT_ID --json
cargo run -- check --workspace Personal
~~~

Repeat `workspace add` with the same name to add another root. `workspace list` shows IDs for `workspace relocate <root-id> <new-path>`. Both scan commands rebuild the inventory from disk.

Registration writes only Rooster's local settings; discovery does not edit repository content or Git state. Use `--config /path/to/disposable/config.json` or `ROOSTER_CONFIG` to isolate a trial. See the [CLI guide](docs/cli.md) for default locations, JSON fields, exclusions, and exit codes.

Artifact commands default to Codex; `--provider claude` selects Claude Code. Each adapter also inspects its conventional personal roots. Add `--no-default-roots` for a repository-only trial; explicit `--codex-home`, `--user-skills`, and additional source overrides still work. `--all-markdown` includes ordinary documentation. `--context` provides a directory-based estimate with explicit runtime unknowns; it does not establish what a running assistant loaded. Claude root flags and local support limits are described in the [artifact guide](docs/artifact-inventory.md#claude-code-inventories-m6).

## Install and launch the desktop

On macOS, from the repository root:

~~~sh
./scripts/install.sh
~~~

This builds Rooster, verifies the bundle, installs it at `~/Applications/Rooster.app`, and opens it. No sudo is needed. Building requires Rust 1.90+, Git, Node 22.12+, npm, and Xcode Command Line Tools. Quit Rooster before updating; the installer retains the previous app in a backup folder beside it.

If you already have a locally built or extracted app, install and launch it without the build tools:

~~~sh
./scripts/install.sh --app /absolute/path/to/Rooster.app
~~~

To open the installed version later:

~~~sh
open "$HOME/Applications/Rooster.app"
~~~

Use **Add workspace** to choose any repository or parent folder, and **Provider** to switch between Codex and Claude Code. Both interfaces share Rooster settings. See [installer options and build requirements](docs/release.md) and the [desktop guide](docs/desktop.md). The pilot uses ad-hoc signing; it is not a notarized public download.

For development with live reload:

~~~sh
cd apps/desktop
npm ci
npm run desktop
~~~

## Prepare and recover edits

~~~sh
cargo run -- changes owners
cargo run -- changes edit ARTIFACT_ID --source /absolute/path/to/draft.md
cargo run -- changes preview CHANGE_ID
cargo run -- changes apply CHANGE_ID
cargo run -- changes restore CHANGE_ID
~~~

Preparation saves a proposal and backups without changing source files. Apply and restore reject stale files, changed checkouts, and conflicting destinations. The [mutation guide](docs/mutations.md) covers package operations, explicit link repair, structured requests, and recovery limits. The desktop exposes the same reviewed workflow through its file actions and Recovery panel.

## Verify

~~~sh
cargo fmt --all -- --check
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo +1.90.0 test --workspace --locked
cd apps/desktop
npm test
npm run build
~~~

The minimum-version check requires the 1.90.0 toolchain. Tests create disposable repositories and settings, including local submodule/worktree fixtures; they do not need an AI account, private repositories, or network access.

## Product direction

The first usable release is a local desktop application with a shared Rust core and a small CLI. It includes repository discovery, personal and project configuration, a source editor with Markdown preview, and recoverable file changes. The complete initial release also supports Claude Code's local files.

Git remains the team collaboration mechanism. Rooster's local catalogue stores where files are and how to display them; it does not become a second copy of the team's instructions.

Cloud synchronisation, agent execution, automatic cross-provider conversion, GitHub PR browsing, and central policy enforcement are later possibilities, outside the first release.
# rooster
