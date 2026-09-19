# Rooster implementation plan

## Outcome and planning depth

Build a Rust core, CLI, and Tauri desktop application that can discover and maintain personal and repository-specific AI guidance across arbitrary local checkout locations. Deliver Codex first and Claude Code local-file support before the complete initial release.

**Depth: full.** The work combines native provider formats, nested repository scope, multiple checkouts, external writers, package mutations, and two user interfaces. The plan therefore specifies the ownership and recovery boundaries that affect correctness.

This is the canonical plan and progress tracker. It was prepared on 17 September 2026. Implementation completion is recorded below against verified checkpoints.

## Progress

Last verified: 18 September 2026.

**Preparation: complete. Implementation: M1–M7 accepted for the local macOS pilot.**

~~~text
Implementation  [████████████████████]  100%
Checkpoints     58 / 58 verified (100%)
Milestones       7 / 7 accepted
Documentation   M0 complete
~~~

The percentage is the fraction of the explicit implementation checkboxes below that are complete and verified. It measures checklist completion, not elapsed time, remaining effort, or release readiness. Documentation is tracked separately and does not inflate implementation progress.

| Milestone | Deliverable | Status | Verified checkpoints |
| --- | --- | --- | --- |
| M1 | Rust core and repository discovery | Complete | 8/8 |
| M2 | Artifact inventory and Codex support | Complete | 9/9 |
| M3 | Desktop browser and preview | Complete | 8/8 |
| M4 | File mutations and recovery | Complete | 9/9 |
| M5 | Desktop editing | Complete | 8/8 |
| M6 | Claude Code support | Complete | 8/8 |
| M7 | Team pilot and release preparation | Complete | 8/8 |

### Immediate next step

The planned initial implementation is complete for a locally built macOS team pilot. Follow the [pilot guide](team-pilot.md) and [build/install instructions](release.md). Next product work is real teammate feedback; public distribution still needs audience/licence/signing decisions and clean-machine validation. This completion bar is not a public-release or all-platform readiness claim.

### Known issues

No unresolved finding from the requested M3/M4 reviews. M3 R1 is resolved by queued refreshes; its regression covers registration during an active scan, rejection of the older inventory, and publication of the follow-up scan. M4 R1 (nested skill destination classification) and R2 (link-title delimiter repair) were independently re-reviewed after correction. M6 was independently reviewed before M7. Nested Claude agent destinations and cross-provider custom-root protection (including containing-package mutations) were corrected and independently re-reviewed. M5 itself has implementation verification but no independent milestone review.

### Updating this tracker

This is a manually maintained progress bar, not a live timer. When working this plan:

1. Tick a work item only after its stated behaviour has been implemented and the relevant checks pass. Code awaiting verification stays unchecked.
2. Mark the milestone acceptance checkbox only after its full verification and exit criteria pass.
3. Before finishing an implementation turn, update the table, counts, percentage, bar, current milestone, and next step to match the checkboxes. Use 20 bar cells, one per 5 percentage points; the exact fraction remains authoritative.
4. Add a short progress-log entry with the checkpoint/milestone, evidence, and any blocker. If acceptance regresses, reopen the affected checkboxes.
5. If scope changes, update the canonical checklist and denominator together and record the change; do not preserve a misleading percentage.

### Progress log

| Date | Progress | Evidence / next action |
| --- | --- | --- |
| 2026-09-17 | M0 complete; implementation 0/58 | Five documents read back, 15 local links checked, R1–R10 mapped, and scaffold hashes unchanged. Next: M1 workspace/core/CLI structure. |
| 2026-09-17 | M1 accepted; implementation 8/58 | Core/CLI workspace, schema-versioned settings, registration/relocation, and repository discovery implemented. Fourteen integration tests pass on Rust 1.90.0 and 1.95.0; formatting and Clippy pass. Fixtures cover the M1 matrix, repository-byte preservation, separate Git metadata, clones, settings contention, and real CLI Ctrl+C. See [CLI behaviour and limits](cli.md). Next: M2 artifact inventory. |
| 2026-09-17 | M2 accepted; implementation 17/58 | Codex inventory/inspection, package snapshots, scope estimates, native metadata/templates, provenance, and link diagnostics implemented. All 26 integration tests pass on Rust 1.90.0 and 1.95.0; formatting and Clippy pass. Includes the nested App matrix, source-byte preservation, shared legacy roles, personal roots inside a checkout, and real CLI list/show/check/template journeys. See [artifact inventory and limits](artifact-inventory.md). Next: M3 Tauri desktop. |
| 2026-09-18 | M2 review repairs; implementation 17/58 | Corrected reference provenance, shared-role declaration counting, fallback configuration isolation, parent-path symlink validation, and derived-home root authorization. Five core regressions failed against the reviewed implementation and pass after correction; a CLI regression covers the false check failure. All 32 integration tests pass on Rust 1.90.0 and 1.95.0; formatting and Clippy pass. Focused reviewer follow-up resolved R1–R5 with no additional supported defects. Scope and checkpoint counts are unchanged. |
| 2026-09-18 | M3 accepted; implementation 25/58 | Built the read-only Tauri browser and verified native folder registration → repository → nested skill → reference using disposable fixtures and embedded app assets. Hostile Markdown stayed inert with no loopback requests; browser checks covered content search, empty states, read-only source, reference navigation, parent isolation, and 1280×850/900×650 layouts. All 35 Rust integration tests pass on 1.90.0 and 1.95.0; formatting, Clippy, three frontend tests, TypeScript, and production build pass. Fixed scan-start overlap and stale preview navigation found during verification. Next: M4 shared mutations and recovery. |
| 2026-09-18 | M4 accepted; M3 acceptance reopened; implementation 33/58 | Shared prepare/preview/apply/verify/restore service and CLI implemented. All 54 Rust integration tests pass on Rust 1.90.0 and 1.95.0; formatting and Clippy pass. Nineteen new tests cover source preservation, complete packages, stale state/ownership, injected backup/write/journal failure, restart recovery, link repairs, and restore conflicts. The independent M3 registration-refresh finding remains open, so its acceptance checkbox is reopened (nine M4 checkpoints added, one M3 acceptance checkpoint removed). Next: resolve M3 R1 and implement M5 desktop editing when requested. |
| 2026-09-18 | M4 review fixes; M3 reaccepted; M5 accepted; implementation 42/58 | Both M4 P2 findings resolved and independently re-reviewed; nested package and misleading link-title regressions pass. Queued refresh resolves M3 R1. M5 adds desktop CRUD/drafts/diffs/conflicts/recovery/retention/Git status. All 63 Rust integration tests pass on 1.90/1.95 with native watcher access, plus documentation checks, formatting, Clippy, seven frontend tests, TypeScript, build, and real native edit/copy/rename/delete/restore/import/restart/conflict journeys. Next: M6 Claude adapter. |
| 2026-09-18 | M6 accepted; implementation 50/58 | Claude local adapter, roots/provenance, native validation/templates, provider-safe drafts, and shared mutation/recovery completed. 73 Rust integration tests on both supported toolchains, eight frontend tests, formatting/Clippy/build, native provider switch/edit/restore/personal-copy journeys, and two-size browser QA pass. Observed Claude Code 2.1.276; structural validation only. Next: M7 team pilot/release preparation. |
| 2026-09-18 | M6 review repaired; M7 accepted; implementation 58/58 | Both M6 P2 findings independently re-reviewed after correction. 77 Rust tests on 1.90/1.95, eight frontend tests, fmt/Clippy/build pass. Two-layout CLI/native pilot, real process-interruption recovery, measured 5,010-artifact discovery, development/release bundles and archive signature readback completed. CI prepared; private local macOS pilot only. Next: teammate feedback and explicit public-distribution decisions if desired. |

## Inspected baseline

- Existing project: a freshly initialised Git repository, on master, with no commits or HEAD revision.
- At planning, [Cargo.toml](../Cargo.toml) defined package rooster 0.1.0, Rust edition 2024, with no dependencies. M1 replaced it with the core/CLI workspace.
- The original src/main.rs printed "Hello, world!". M1 replaced it with the [CLI entry point](../crates/rooster-cli/src/main.rs).
- [.gitignore](../.gitignore): ignores /target.
- Those three scaffold files were untracked at inspection; there is no existing production scanner, caller, data model, test suite, or desktop application to reuse.
- Rust/Cargo 1.95.0 is installed on the planning machine; this is an observation, not yet an MSRV commitment.
- User confirmed Rust + Tauri for the desktop interface.
- Provider documentation and the user's nested-app example were inspected; evidence and limits are recorded in [provider compatibility](provider-compatibility.md) and [the product example](product.md#example-a-nested-web-application).

Before each milestone, recheck the checkout and applicable project guidance. Preserve work added since this historical baseline. M1 was implemented against the 17 September plan whose starting SHA-256 was d2b35b87b282e463e7a428710771c61162b29e8448a5e2e158a79732e8bcfd97. M1–M7 now have recorded implementation and verification evidence.

## Scope and invariants

The [product acceptance criteria](product.md#acceptance-criteria) own the required behaviour. The [architecture](architecture.md) owns the design and write protocol.

Material invariants:

1. Native files remain authoritative; Git remains the initial team sharing mechanism.
2. Workspace names and repository locations are local configuration, never hard-coded company paths.
3. Inventory, directory applicability, and live-session loading are separate concepts.
4. A clone/worktree is a distinct editable owner even if its remote matches another.
5. CLI and desktop use one Rust implementation for every read/validation/write operation.
6. External edits, checkout changes, interrupted operations, and unsupported links have explicit outcomes.
7. Installed and account-managed content is not mutated as an authoring source.
8. Scanning/previewing content does not execute instructions, scripts, hooks, or AI sessions.

## Milestones

### M0 — Establish the project documentation

Deliver the README, product specification, architecture, provider compatibility reference, and this plan. Keep the existing Rust scaffold intact.

Verification for this documentation change: read back the files, check relative links and acceptance-criterion coverage, and confirm scaffold contents are unchanged. Running application tests is unnecessary because no application behaviour changes.

Status: **Complete.** The five documentation files were saved and read back, all 15 local links resolved, R1–R10 had implementation/verification coverage, and the three original scaffold files matched their baseline hashes. No application tests were needed for that documentation-only change.

### M1 — Rust core, registrations, and repository discovery

**Outcome:** a user can register arbitrary parent folders/direct repositories and obtain a reliable inventory of checkouts through a small CLI.

Work:

- [x] Convert the scaffold to the workspace structure in the architecture; add core and CLI crates.
- [x] Implement local settings with a schema version and an explicit override for disposable tests.
- [x] Model workspaces, registered roots, and checkout identity.
- [x] Discover Git directories/pointer files and inspect branch, HEAD, and linked worktrees through argument-array Git calls.
- [x] Handle root overlap, nested repositories/submodules, missing roots, detached HEAD, and no-commit repositories.
- [x] Implement cancellation, exclusions, progress, and partial-error reporting.
- [x] Select the minimum Rust version and necessary dependency versions based on the actual build.

Implemented CLI surface for this slice:

~~~text
rooster workspace add <name> <path>
rooster workspace list
rooster workspace relocate <root-id> <path>
rooster scan [--workspace <id>] [--json]
rooster repos list [--json]
~~~

The [CLI guide](cli.md) documents settings overrides, JSON output, name/ID selection, exclusions, and exit codes. Both inventory commands perform a fresh scan; there is no persisted index.

Verification: temporary fixtures with two sibling repositories, a nested checkout, an initialised submodule, a linked worktree outside the parent root, a no-commit repository, overlapping roots, and a missing directory. Verify distinct ownership and partial outcomes, not internal traversal call counts. Use a fixture path with spaces/non-ASCII characters.

Exit: the CLI presents a stable, deduplicated checkout inventory without reading excluded content or changing Git state.

- [x] **M1 acceptance:** complete the verification above, satisfy the exit criteria, and record the evidence in the progress log.

Implementation decisions: Rust 1.90 is the MSRV (including the standard-library file lock); Cargo.lock fixes the verified dependency resolution. ConfigStore owns Rooster registration settings, while the M4 recovery service remains reserved for guidance/provider-file mutations. Directory exclusions are names, and scan progress reports counts because the tree's total size is not known upfront. Root IDs survive relocation; checkout IDs describe the canonical local location. These details are recorded in the architecture and CLI guide.

Verification: `cargo fmt --all -- --check`; `cargo test --workspace --locked --offline`; `cargo clippy --workspace --all-targets --locked --offline -- -D warnings`; `cargo +1.90.0 test --workspace --locked --offline`. Tests use disposable settings and repositories. Git 2.39.2/macOS Apple Silicon is the verified platform; no Linux/Windows, performance, provider, or desktop acceptance is claimed here.

### M2 — Artifact inventory and Codex interpretation

**Outcome:** discover and inspect personal and nested project Codex guidance.

Work:

- [x] Introduce artifact/package/snapshot models and a small provider module boundary.
- [x] Implement Codex roots, native metadata validation, templates, and source classification from the compatibility reference.
- [x] Index nested instruction files, skill packages, supported agent formats, and linked references; add the optional ordinary-Markdown view.
- [x] Preserve unknown fields/source bytes and distinguish malformed content from unsupported semantics.
- [x] Add scope assessment for a selected working directory, with explicit unknown trust/profile/runtime inputs.
- [x] Build local Markdown-link extraction and diagnostics for missing targets and duplicate identities.
- [x] Recognise system/plugin/compatibility roots with provenance and uncertain activation, rather than declaring all cached content active.
- [x] Add `rooster artifacts list`, `rooster artifacts show <id>`, and `rooster check` using structured results.

Verification: synthetic nested App layout matching the shape of the user-supplied PR; independent personal/config roots; override/fallback guidance; duplicate skill names; a malformed agent; metadata in agents/openai.yaml; installed content; missing references; symlink/cycle boundaries. Test that a selected context in one repository does not inherit another merely because both are in the same workspace.

Exit: all supported Codex artifacts can be inspected, references stay grouped, and unsupported activation is clearly identified.

- [x] **M2 acceptance:** complete the verification above, satisfy the exit criteria, and record the evidence in the progress log.

M2 baseline: the accepted M1 plan, SHA-256 eafa7b08ea19d433cb7b6ade60f1da04d176b7535c5134c71a1d54b15dcedabb. Implementation preserves the shared-core/read-only scope. The additional `artifacts template` command prints native template data without creating files. Settings gained an optional Codex root object; M1 settings still load. Source snapshots and owner revisions prepare the inspection boundary for M4 without implementing writes early.

Verified with the workspace formatting check, full locked/offline test suite on Rust 1.90.0 and 1.95.0, and Clippy across all targets with warnings denied. The provider reference was rechecked against official documentation and the installed CLI version 0.153.4 was observed; provider-runtime acceptance is not claimed. Unknown profile/trust/activation, shared-role scope, Markdown fragments, and resource limits are documented in the artifact guide.

### M3 — Read-only desktop application

**Outcome:** a user can add a workspace and explore its actual configuration in a Tauri window.

Work:

- [x] Scaffold Tauri 2 using the confirmed Rust backend and proposed React/TypeScript/Vite frontend.
- [x] Select an editor component based on Markdown, TOML, JSON, and YAML needs; a source editor plus preview meets the requirement.
- [x] Bind typed commands to the core; use registered IDs and Rust-side path validation.
- [x] Implement workspace/repository navigation, artifact filters, search, preview, and scope/reference details.
- [x] Show progress, cancellation, empty states, partial failures, unknown compatibility, and read-only source reasons.
- [x] Add manual refresh and watcher-driven invalidation without rebuilding domain logic in the frontend.
- [x] Configure capabilities/CSP and ensure Markdown content cannot access privileged commands or fetch remote assets automatically.

Verification: exercise add-root → select repository → find nested skill → inspect reference in a disposable fixture. Test one hostile Markdown preview for script/URL execution and privileged-command exposure. Run the real Tauri shell on macOS: browser-only rendering does not verify native dialogs, IPC, or filesystem boundaries.

Exit: useful Codex browsing is available in the desktop app before the full editing workflow is complete.

- [x] **M3 acceptance:** complete the verification above, satisfy the exit criteria, and record the evidence in the progress log. Reaccepted in M5 after fixing and verifying the registration/focus overlap.

M3 baseline: accepted M2 with review repairs, plan SHA-256 `ac7f6b70c8186c7fab1a7f66dba83a7de2911791a1ef4e121395b61b9935a565`. Tauri 2, React/TypeScript/Vite, npm, and CodeMirror 6 implement the read-only slice. Scope interpretation remains in the Rust core; desktop commands add snapshot generations, opaque context IDs, background scanning, native registration, and watcher invalidation. Direct layout implementation was explicitly selected by the user. See the [desktop guide](desktop.md) for security boundaries, launch commands, and known limits.

### M4 — Shared mutation and recovery service

**Outcome:** the Rust core and CLI can apply explicit, recoverable edits.

Work:

- [x] Implement inspect → prepare → preview → apply → verify as one write path.
- [x] Support ordinary Markdown, user-owned skill source/metadata, and supported native agent definitions.
- [x] Add file/package creation, complete package duplication, content edits, same-owner rename, and recoverable deletion.
- [x] Check expected bytes/hash, file identity, checkout identity, destination collisions, and ownership immediately before applying.
- [x] Preserve unedited text, metadata, encoding/newline details, and relevant permissions.
- [x] Implement the local journal/backups and restore protocol; report partial application precisely.
- [x] Add reviewed repairs for supported local Markdown links; report ambiguous/cross-root references.
- [x] Expose source-file/structured input and preview output through the CLI so automation uses the same validations. Avoid arbitrary shell-command strings or silent overwrite flags.

Verification: meaningful regression tests for an edit that preserves unrelated metadata, stale-content rejection, branch change after opening, an existing destination, full-package copy/delete, interrupted multi-file operation, write/backup failure, and restore colliding with a subsequent external edit. Include containment traversal and unsupported-link cases. Use fault injection for write failure rather than relying on host-specific chmod behaviour.

Exit: failures preserve the draft/originals, applied paths are read back, and the service never reports a partial operation as success. Record the remaining non-cooperating-writer race described in the architecture.

- [x] **M4 acceptance:** complete the verification above, satisfy the exit criteria, and record the evidence in the progress log.

M4 baseline: accepted M3 implementation, plan SHA-256 `33f10a20bbd3dab015c03298f93522d7481081f6915bbf6ab54221625dac96f0`, with the subsequent M3 review finding still open. No frontend behaviour or provider format was changed for M4.

Implementation decisions: `changes::ChangeStore` owns durable opaque change IDs, complete before/proposed blobs, source and checkout guards, explicit inline Markdown repairs, and per-step readback/recovery records. Creation, duplication, edits, same-owner rename, and deletion use this one service. CLI requests supply a source file or structured JSON; preparation never edits the source. Native classification/validation reuses inventory rules. Exact byte-range patches retain unedited bytes, while whole replacements retain BOM and uniform CRLF conventions. Complete packages include ignored resources, scripts, binary files, and empty directories. Recovery records are retained without automatic expiry; M5 supplies retention controls. See the [mutation guide](mutations.md) for commands, constraints, and recovery states.

Verified with `cargo fmt --all -- --check`, `cargo test --workspace --locked --offline` on Rust 1.95.0 and 1.90.0, and `cargo clippy --workspace --all-targets --locked --offline -- -D warnings`. All 54 integration tests pass on each toolchain, including 17 new core and two new CLI tests. Tests use disposable repositories, provider roots, settings, and recovery directories. Fault injection verifies backup failure, write failure, lost journal updates, interrupted apply, and interrupted restore without depending on host permission tricks. The CLI tests exercise saved IDs through preview/apply/history/restore and reject a destination created after preview.

Verified mutation platform: macOS Apple Silicon. Unix identity checks are required; other platforms remain read-only. Only regular-file bytes and Unix rwx modes are covered, not ownership, ACLs, extended attributes, or timestamps. Non-cooperating writers retain the documented final-check/replace race; identical bytes after an unjournalled crash cannot establish their writer. Partial outcomes and ambiguous states remain explicit and preserve the saved proposal/backups. No new desktop UI verification is claimed for M4.

### M5 — Desktop editing and usable Codex milestone

**Outcome:** a Codex user can perform the complete browse/edit/recover workflow from the desktop.

Work:

- [x] Add create/duplicate/edit/rename/delete actions and explicit destination scope.
- [x] Integrate source editing, validation, complete diffs, and save status.
- [x] Persist drafts across navigation/restarts and reconcile external modifications.
- [x] Show package boundaries and incoming-link impacts before package mutation.
- [x] Implement conflict comparison, recovery-required state, restore, and recovery retention controls.
- [x] Display reload guidance without claiming the running coding assistant has consumed the edit.
- [x] Expose resulting Git changes without committing, pushing, or switching branches.

Verification: exercise a representative instruction edit, skill duplication with references, rename/link repair, and delete/restore in the real desktop shell. A second process edits a file while a draft is open; verify the draft survives and the conflict is presented. Verify keyboard navigation, focus, native file selection, and reopening a saved draft.

Exit: the first usable local Codex desktop milestone meets R1–R3 and R5–R10 for the supported Codex formats. R4 remains incomplete until the next milestone.

- [x] **M5 acceptance:** complete the verification above, satisfy the exit criteria, and record the evidence in the progress log.

M5 baseline: plan SHA-256 `216804138f29706a1abe14958c69dfadbee5bb5595440b9fc190368c2c8a2a3f`, following the requested independent M4 review. Two validated M4 P2 findings were repaired and approved in a focused independent follow-up before continuing M5. M3 R1 was separately corrected with a queued-refresh regression.

Implementation decisions: durable drafts live in private versioned records alongside recovery data; opening/reloading inventory never overwrites saved draft text. Source-only conflicts can be explicitly rebased against the exact displayed current revision; changed settings, owner, or checkout remain rejected. The desktop uses typed Rust requests and shared native validation for every prepare/apply/restore. Retention is explicit, defaults to 30 days for completed/restored journals, and excludes drafts and unresolved/prepared changes. Git status is read-only. See the [desktop guide](desktop.md) and [mutation guide](mutations.md).

Verification: all 63 Rust integration tests pass on Rust 1.90.0 and 1.95.0, with native filesystem-event tests run outside the event-restricting sandbox; documentation checks, formatting, and Clippy with warnings denied pass. Seven frontend tests, TypeScript, and the production build pass. Native macOS journeys covered instruction edit with undo/redo, exact whole-skill copy, rename and selected link repair, delete/restore, native source import, new-file creation, app restart and draft reopening, external source conflict with retained text, Git changes, and retention preview. Readbacks verified actual source bytes and complete copied/restored trees. Supplemental 1280×850 and 900×650 browser checks passed; they are presentation evidence, not native write verification. The optional development favicon 404 and existing lazy CodeMirror chunk warning do not affect those journeys.

Verification limitations: macOS Apple Silicon only, no signed release or Claude support yet. No claim that an assistant consumed the changed files. The first full check overlapped a feature-varied bundle build in the same Cargo target and required an isolated desktop documentation rerun; subsequent verification separated those builds. Restricted-shell watcher failures were rerun successfully with native event access. M5 itself has not received an independent code review.

### M6 — Claude Code local-file support

**Outcome:** the same application manages both providers' local artifacts.

Work:

- [x] Implement the Claude adapter from the compatibility reference.
- [x] Support local skills, Markdown/YAML agents, instruction files, rules, and legacy command classification.
- [x] Resolve configured roots and separate personal/project/installed/synced sources.
- [x] Preserve native metadata and expose uncertain import/path-condition/runtime behaviour.
- [x] Reuse inventory, preview, mutation, and recovery services; add only provider-specific validation/templates.
- [x] Present a personal copy as independent. Do not introduce automatic conversion or bidirectional synchronisation.
- [x] Capture the installed/provider version actually used for compatibility verification.

Verification: disposable Claude fixtures with multiple roots, scoped rules, imported references, unknown frontmatter, installed/synced content, and a native agent edit. Test provider-specific same-name handling separately. Where available, use a non-executing provider validation facility; otherwise report local structural checks as such.

Exit: R4 is complete for documented local support; no shared UI/service is silently assuming Codex semantics.

- [x] **M6 acceptance:** complete the verification above, satisfy the exit criteria, and record the evidence in the progress log.

M6 baseline: accepted M5 plan SHA-256 `43eaa4b8dc99a8bb5932113d11f5c96e95520fe5dabb31f6d3149e6193787367`. One provider-selected inventory reuses traversal, snapshots, and the full write/recovery service. Claude adds native classification, bounded YAML/JSON validation, templates, roots/provenance, and separate scope estimates. Private drafts persist their provider; old settings and draft/journal records retain schema-1 defaults. Codex stays the default. See [provider compatibility](provider-compatibility.md#claude-code) and [Claude inventory usage](artifact-inventory.md#claude-code-inventories-m6).

Verification: 73 Rust integration tests passed on Rust 1.90.0 and 1.95.0, including ten new core/CLI/desktop cases; affected inventory/Claude tests were rerun after the final relocated-home boundary correction. Formatting and Clippy across all targets with warnings denied pass. Eight frontend tests, TypeScript, production assets, and the rebuilt macOS Tauri bundle pass. Native disposable journeys verified provider switching, a Claude agent draft reopened from the Codex view, metadata-preserving reviewed apply, exact restore, synced read-only restrictions, complete independent personal copy with binary/reference readback, and restoration of that copy. Read-only browser checks at 1280×850 and 900×650 verified provider isolation, source navigation, conditional-rule details, no page overflow or application errors; screenshot inspection led to compact sidebar spacing and reachable scrolling. These browser checks supplement native IPC verification.

Compatibility was checked against official Claude documentation and observed CLI 2.1.276. The original verification did not identify a suitable non-executing validator. The review follow-up tried `claude plugin validate` with isolated valid and malformed agent fixtures; both returned empty reports, so native metadata checks remain local structural validation. No assistant session, skill script, hook, or remote reference ran. macOS Apple Silicon only; runtime loading, managed/plugin activation, dynamic/quoted imports, and path-condition evaluation remain explicitly unknown. The subsequent independent M6 review found two P2 defects: nested Claude agent destinations and foreign custom-root protections. Both were corrected and independently re-reviewed before M7; three added core regressions cover recursive CRUD, direct/reference/destination protection, and containing-package operations/stale provider settings. The 11-test Claude suite passes after repair.

### M7 — Team pilot and release preparation

**Outcome:** a teammate can use the app with a different folder structure and review its resulting changes through Git.

Work:

- [x] Walk through onboarding on two independent root layouts using the same sample guidance.
- [x] Verify local settings, recovery copies, and personal drafts stay outside shared repositories.
- [x] Exercise moving/offlining a registered root, opening a second checkout, and restarting after an interrupted operation.
- [x] Measure discovery responsiveness with representative generated repositories; address observed bottlenecks and publish tested limits.
- [x] Package the macOS development/release build and document build/install requirements, recovery behaviour, compatibility, and known limits.
- [x] Decide distribution signing/notarisation based on the intended audience before public release. Add other operating-system claims only after their builds and file operations pass.
- [x] Update the README with working commands and desktop instructions, and add CI checks appropriate to the implemented Rust/frontend components.

Exit: the complete initial release satisfies the acceptance matrix below with recorded evidence. A server, marketplace, or shared absolute-path configuration is not required for the pilot.

- [x] **M7 acceptance:** complete the verification above, satisfy the exit criteria, and record the evidence in the progress log.

M7 baseline: accepted M6 plan SHA-256 `3f3cebd801c9e9fbf0ede67493acbffe6d172322552de92dee512c9d71bfe0e1`, followed by the requested independent M6 review and its two resolved P2 findings. No milestone scope or checkpoint denominator changed.

The working distribution choice is a private, locally built macOS pilot with explicit ad-hoc signing. Public signing/notarization, final bundle identity, licence, and other-platform claims remain outside this pilot. No public upload, commit, push, or hosted service was introduced.

Verification: all 77 Rust integration tests pass on Rust 1.90.0 and 1.95.0, including an abrupt child-process exit before journaling and fresh-process finish/restore with a retained private draft. Formatting, Clippy across all targets with warnings denied, eight frontend tests, TypeScript, and production assets pass. The repeatable two-layout CLI pilot verifies identical guidance, parent/direct registrations, both providers, Git diffs, restoration, private storage, offline/moved roots, and distinct second clones. Real native onboarding in both layouts covers Unicode paths, nested skill/reference inspection, provider selection, private draft restart, reviewed apply with exact readback/Git diff, and restore to a clean checkout. See [pilot evidence](team-pilot.md#evidence-and-limits).

The representative release workload contains ten repositories and 5,010 selected-provider artifacts. Three-run means are 4.296 s (Codex) and 4.000 s (Claude), first progress under 1 ms, cancellation during repository discovery under 8 ms, and no inventory diagnostics. Existing background scanning/cancellation sufficed at that measured scale; no unmeasured indexing or concurrency optimization was introduced. See [reproducible limits and timing](performance.md).

Both development and optimized macOS app bundles build and pass signature verification. The optimized app was exercised natively; its ZIP extraction preserves the verified signature and executable hash. Release artifacts are ad-hoc signed, arm64, locally verified on macOS 26.6.2, with no notarization or public trust claim. CI YAML and local commands are verified; no hosted CI run is claimed before a repository push. The [release guide](release.md) records commands, hashes, compatibility, installation, and remaining distribution decisions.

Limits: the two developers were simulated with isolated local layouts, not a second human/physical Mac. Native console/network instrumentation, 5,010-row UI rendering, cold/network storage, peak memory, other OS/CPU combinations, and provider runtime loading were not established. The first targeted M6 repair run under concurrent build/test load had two incomplete-inventory failures; the corrected focused run and both complete suites passed with native access and bounded concurrency. Existing lazy-editor chunk warnings remain. These limitations are recorded rather than represented as additional completed claims.

### Post-M7: install and launch command (18 September 2026)

Added `./scripts/install.sh` to build the native macOS release, verify and install it into the user's Applications folder, and launch it. It also accepts an existing local bundle, an alternate install folder, and installation without launch. Updates retain the previous verified app, reject symbolic-link destinations and damaged candidates, and require Rooster to be quit first. README, desktop/release instructions, and macOS CI include the command and its focused checks. M1–M7 remain 58/58 complete; this follow-up does not change that milestone denominator.

Verification: the final script built from locked, cached dependencies and installed successfully from outside the repository using a relative destination. Real-bundle smoke checks passed for fresh installation, update backup preservation, corrupted-candidate rejection without changing the installed executable, and symbolic-link rejection, including a Unicode destination containing spaces. Shell syntax and CI YAML checks passed. Hosted CI and public signing/notarization are not claimed.

### Post-M7: remove registered locations from the sidebar (18 September 2026)

The sidebar now separates registered folders from discovered repositories. Each registration has a Remove location action and confirmation with the exact workspace and path. The shared settings store unregisters only that root ID, including offline roots, and removes its workspace if it has no roots left. Files, private drafts, recovery history, and other registrations remain intact. The desktop refreshes its inventory, clears obsolete selections, and returns a removed workspace selection to All workspaces. Pending scan results cannot restore the previous registration list.

Verification: six core configuration tests, four native-session tests, twelve frontend tests, formatting, workspace Clippy, TypeScript, and production frontend build pass. Coverage includes overlapping roots, the same folder in separate workspaces, offline removal, unknown IDs, last-root removal, exact source preservation, cancellation/confirmation, error retry, active scans, and stale asynchronous inventory reads. M1–M7 remain 58/58 complete.

The packaged macOS app also passed native cancellation, selected-repository/last-workspace removal, and offline-location removal. Its refreshed sidebar and inspector cleared the removed selection. Configuration readback retained only the expected root, and hashes confirmed that every fixture source and Git metadata file stayed unchanged.

### Post-M7: group skill metadata with its package (18 September 2026)

Settings now lists provider configuration and plugin manifests, excluding skill metadata. All artifacts and Skills & commands display expandable skill packages with metadata, references, scripts, and assets under the owning skill. Package identity determines ownership, so repeated `openai.yaml` filenames remain separate. Search and Needs attention retain a matching child’s parent as context and expand matching files automatically. Related-file navigation reveals a selected child; incomplete inventories retain orphaned metadata as an inspectable file. The on-disk inventory and editing services are unchanged.

Verification: all eighteen frontend tests, TypeScript, and the production build pass. Focused coverage includes Settings classification, expansion/collapse, exact-ID selection for repeated filenames, child-only search results, malformed child metadata, related-file navigation, and missing parents. M1–M7 remain 58/58 complete.

Native verification of the packaged app passed Settings exclusion, skill expansion with metadata/reference/script rows, exact source selection for two different `openai.yaml` files, malformed-child filtering, and child-only content search. Fixture file and Git metadata hashes stayed unchanged; the release bundle passed signature verification.

### Post-M7: simplify supporting-file navigation (18 September 2026)

Removed the redundant References sidebar category and its unused category filter. Supporting files remain discoverable in All artifacts, inside their owning skill packages, through search, and through Details links. No files, inventory kinds, or editing capabilities are removed. The desktop guide reflects the simpler navigation. All eighteen existing frontend tests, TypeScript, and production build pass; M1–M7 remain 58/58 complete.

### Post-M7: improve desktop readability (18 September 2026)

Raised the default interface text from 13px to 15px, with main controls and file names at 14–15px and secondary labels at least 12px. Source editing uses 15px monospace text; Markdown body text uses 16px with 15px code. Darkened faint secondary text and widened the sidebar and file list, including their narrower-window layouts. Short windows use tighter header spacing and a shorter scrollable summary to preserve the document reading area; Markdown previews fit the available height. Existing scrolling and wrapping remain available. All eighteen frontend tests, TypeScript, and production build pass. M1–M7 remain 58/58 complete.

### Post-M7: ignore volatile provider runtime changes (18 September 2026)

Provider homes and repositories also contain runtime state that does not affect Rooster's inventory. Filter delivered filesystem events to direct provider settings, artifact collections, existing artifacts and packages, instruction filenames, relevant Git references, and optional ordinary Markdown. Codex activity in runtime folders and application logs in a registered repository no longer show a false stale-snapshot warning when the user switches applications. Real settings, agent, skill, plugin, instruction, package, and branch changes still invalidate the inventory.

## Acceptance and verification map

| Requirement | Implementation | Evidence |
| --- | --- | --- |
| R1: arbitrary locations | M1, M3, M7 | Independent layouts, overlap, relocation, missing-root journey |
| R2: Git ownership | M1, M4 | Worktree/submodule/detached/no-commit fixtures; stale checkout rejection |
| R3: nested content | M2, M3 | Nested-app inventory and grouped references |
| R4: both providers | M2, M6 | Separate native-format and scope fixtures |
| R5: CRUD/packages | M4, M5 | Core mutation tests and one desktop workflow per distinct operation |
| R6: preservation/recovery | M4, M5, M7 | External edit, interrupted write, restore conflict, draft restart |
| R7: source editability | M2, M4, M6 | Managed/cache/synced mutation rejection and explicit personal copy |
| R8: diagnostics | M2, M4, M6 | Broken link, duplicate identity, malformed/unknown metadata |
| R9: local/team boundaries | M1, M7 | Offline workflow and two-user layout pilot |
| R10: desktop/CLI parity | M3–M6 | Both interfaces exercise the same services; real Tauri-shell checks |

## Verification policy

Use focused core integration tests for filesystem/provider invariants, CLI smoke checks for command/output contracts, and desktop journeys for user-visible/native behaviour. Do not reproduce every parser case in the UI or add tests for static documentation wording.

Once code exists, use applicable formatting, Rust tests/Clippy, frontend type/lint/build checks, and the relevant targeted journeys. Make CI commands match actual package scripts and workspace members. Do not claim browser-only verification proves desktop IPC, or that mocked provider fixtures prove an AI client loaded a file.

All automated writes use temporary fixture roots and local application-data overrides. Real personal/team configuration is read-only during development unless the user deliberately selects an edit in a product pilot.

## Assumptions and decisions still to settle

| Item | Current direction | Resolve before |
| --- | --- | --- |
| Desktop framework | Rust + Tauri confirmed by the user | Resolved |
| Frontend stack/editor | React + TypeScript + Vite with CodeMirror 6 source/preview | Resolved M3/M5 |
| Supported platforms | macOS first; portable design | Release packaging and support claims |
| Provider versions | Record actual compatibility at implementation time; do not hard-code today's installed version as a universal contract | M2/M6 write support |
| Recovery retention | Explicit cleanup, default minimum age 30 days for completed/restored entries; prepared/unresolved entries and drafts excluded, no automatic expiry | Resolved M5 |
| Distribution/licence | M7 targets a private, locally built macOS pilot with ad-hoc signing. Public audience, Developer ID/notarization, final identifier, and licence remain unselected | Public distribution |

The M7 pilot is local macOS/Apple Silicon; resolve public distribution and broader platform claims separately.

## Material risks and limits

- Provider schema/discovery changes can invalidate assumptions: isolate adapters and maintain a tested compatibility record.
- Filesystem operations cannot provide a universal atomic transaction across packages, volumes, or non-cooperating editors: retain recovery state and honest failure outcomes.
- Branch/worktree changes and moved roots invalidate drafts: require revalidation instead of saving to a newly resolved owner.
- Linked resources and installed caches can have surprising ownership: expose their origin and begin with read-only handling.
- Large trees can overwhelm a synchronous UI: scan in background with pruning, cancellation, and measured follow-up optimisation.
- Guidance written in prose may conflict in ways a deterministic validator cannot establish: present concrete references and metadata diagnostics, without inventing semantic certainty.
