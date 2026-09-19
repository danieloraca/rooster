# Local team pilot

The pilot uses Git to share guidance. Each developer registers their own folders; nobody needs another person's absolute paths, application data, AI account, or a Rooster server. The supported pilot is a locally built macOS app. Public binary distribution is a separate decision described in [release preparation](release.md).

## Onboard with your own folders

1. Build and launch the app using the [desktop guide](desktop.md), or install the CLI with `cargo install --path crates/rooster-cli --locked`.
2. Choose **Add workspace**. Select a parent containing repositories, or select individual checkouts. Reuse a label to group unrelated locations. An external drive or a folder containing spaces works the same way.
3. Select a repository and provider. Open its instructions or a nested skill, then inspect the references, provenance, and editable/read-only explanation.
4. Edit a shared instruction, choose **Review changes**, inspect the complete diff, and apply. Use **Git changes** and your usual Git diff/review workflow. Rooster does not commit or push.
5. Open **Recovery**, review the original bytes, and restore the trial edit. Keep a private saved draft, restart the app, and reopen it through **Drafts**.

For a repository-only trial, turn off **Personal & installed sources**. Provider settings can also disable ambient roots in Rooster's local config. Installed, managed, synced, and foreign-provider sources are protected; an independent copy needs an explicit authoring destination.

## Local state stays local

On macOS the default config is `~/Library/Application Support/rooster/config.json`; drafts and recovery live under `~/Library/Application Support/rooster/recovery`. `ROOSTER_CONFIG` and `ROOSTER_DATA_DIR` permit isolated trials. Set them outside every shared checkout and provider source. Recovery rejects placement within those boundaries. An explicit config override is user-controlled, so do not place it in a repository or commit it.

Share native instruction/agent/skill files through Git. Do not share settings, lock files, private drafts, recovery records/blobs, or machine-specific absolute paths. Drafts may contain unpublished text; recovery copies retain original and proposed source. They use private local permissions and have no automatic expiry. Completed/restored recovery entries can be explicitly removed with the retention controls; unresolved entries and drafts are excluded. See [recovery behavior](mutations.md).

A missing root remains registered and produces a partial scan with an issue. Bring it online or use `rooster workspace relocate ROOT_ID /new/path`, then refresh. Relocation updates the registration, not the filesystem. The root ID survives; checkout IDs change with location. Open a fresh draft after moving a checkout or changing branch/HEAD. Separate clones and worktrees remain separate owners even when their commits match.

If the app stops during a multi-file operation, reopen **Recovery** and inspect the saved proposal. Finish or restore only after reviewing it. Rooster reconciles observed bytes against its journal and refuses to overwrite conflicting external changes. Filesystem-wide atomicity and protection against the final non-cooperating-writer race are not promised.

## Repeatable pilot fixture

This harness creates two independent developer layouts containing identical guidance, one registered through a parent and one through individual checkouts. It creates disposable Git commits with fixture-only author identity, exercises both providers through the CLI, saves ordinary Git diffs, restores originals, adds a second clone, and checks offline/relocated roots. It refuses an existing destination and leaves the fixtures available for desktop walkthroughs.

From the repository root, with Python 3.9+ and Git available:

~~~sh
cargo build -p rooster-cli --locked
python3 scripts/pilot.py --binary target/debug/rooster --directory /absolute/path/to/new-disposable-pilot
~~~

Use a NEW location outside the Rooster repository. Inspect its `result.json` for each layout's config, recovery, and repository paths. Launch the built app executable with the corresponding `ROOSTER_CONFIG` and `ROOSTER_DATA_DIR`. Those two local settings files are deliberately different; the guidance hashes are equal.

The automated restart regression `fresh_cli_recovers_an_abruptly_exited_writer_and_private_draft` terminates a child process after a filesystem step, before its journal update. A fresh CLI process separately verifies finish-and-restore and direct-restore paths, with binary bytes and a private draft retained. Fault injection exists only in the Rust test API, not production IPC or CLI flags.

## Evidence and limits

Verified on 18 September 2026 using the optimized Rooster 0.1.0 macOS bundle with embedded assets and isolated application data. The repeatable CLI harness passed both layouts for Codex and Claude, Git diff/restore, distinct second-clone ownership, missing-root partial results, and relocation with retained registration IDs. Shared guidance hashes matched across layouts, and both finished clean in Git.

The real native shell separately registered a parent folder for developer A and a direct checkout under a different hierarchy containing spaces/Unicode for developer B. Both discovered the same nested skill. Native checks covered skill → reference navigation, a saved private instruction draft surviving process restart, complete before/after review, apply with exact source/Git-diff readback, and restore to original bytes/clean Git. Developer B also switched to Claude and inspected its nested agent. Screenshots of the reopened editor and second-layout library were visually checked; there were no blank screens or framework overlays. Native console/network instrumentation was not collected. Native picker automation needed a direct Unicode value rather than simulated typing; this was a test-tool input issue.

The full 77-test Rust integration suite passed on 1.90.0 and 1.95.0, including the real child-process crash/recovery regression. Its ignored helper is explicitly invoked twice by the parent test; it is not skipped behavior. Eight frontend tests, TypeScript/production build, formatting, and Clippy with warnings denied pass. M6's two independent-review findings were fixed and independently re-reviewed before this pilot.

 Two local simulated developer layouts exercise portability; they do not constitute feedback from a second human or a second physical Mac. Existing M1–M6 fixtures cover submodules, worktrees, nested guidance, conflicts, package operations, native dialogs, and both providers. See the [canonical acceptance map](implementation-plan.md#acceptance-and-verification-map).
