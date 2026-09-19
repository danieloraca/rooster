# Architecture

Status: M1–M6 implemented: repository discovery, Codex/Claude local inventory, desktop browsing/editing, private drafts, and shared mutation/recovery services. Last updated: 18 September 2026.

## Design decisions

| Decision | Reason |
| --- | --- |
| Rust core shared by CLI and desktop | Discovery and file mutations must behave the same from both interfaces. |
| Tauri 2 desktop shell | Confirmed by the user; supports a Rust backend and web-based editor interface. |
| React, TypeScript, and Vite frontend | Implemented in M3; presentation stays separate from Rust domain services. |
| Native files as authoritative data | Edits from other applications remain visible; users retain ordinary Git workflows. |
| Local application state, rebuildable inventory | Avoid a second editable representation of instructions and a required hosted service. |
| Provider-specific interpretation | Similar file formats do not imply identical activation or permission semantics. |
| macOS first, portable core | Start with the user's platform while keeping other operating systems possible. |

The framework choice follows [Tauri's documented Rust/web architecture](https://v2.tauri.app/start/). M1 uses Rust edition 2024 with MSRV 1.90, verified with that toolchain. M3 uses npm, React 19, TypeScript 7, Vite 8, and CodeMirror 6; exact versions are locked in package-lock.json.

## Repository layout

~~~text
Cargo.toml                 # implemented workspace, MSRV 1.90
crates/
  rooster-core/            # discovery, providers, documents, edits, diagnostics
  rooster-cli/             # command parsing and output; binary named rooster
apps/                      # implemented in M3
  desktop/
    src/                   # frontend
    src-tauri/             # thin desktop bindings to rooster-core
docs/
~~~

The initial single-package scaffold was replaced in M1. The core contains config, paths, Git inspection, scan, artifacts, and providers modules. M2 separates inventory traversal, snapshots, Markdown links, scope estimates, and Codex parsing/templates. M4 adds changes modules for preparation, native path checks, link repairs, private record/blob storage, application, and recovery. M5 adds private versioned drafts, explicit recovery cleanup, and read-only Git status in the core, with typed desktop commands. Separate crates or a dynamic plugin system are unnecessary for two integrations.

~~~mermaid
flowchart TD
    UI[Tauri desktop UI] --> Bridge[Typed desktop commands]
    Bridge --> Core[Rust application services]
    CLI[Rooster CLI] --> Core
    Core --> Discovery[Repository and file discovery]
    Core --> Providers[Codex and Claude adapters]
    Core --> Edits[Validation and recoverable edits]
    Discovery --> Files[Selected local checkouts and personal roots]
    Providers --> Files
    Edits --> Files
    Core --> State[Local settings, drafts, recovery]
    Discovery --> Git[Read-only Git inspection]
~~~

## Domain model

| Object | Responsibility |
| --- | --- |
| Workspace | User label and selected scan roots; grouping has no provider scope semantics. |
| Checkout | Canonical working-tree path, Git directory/common directory, optional branch and HEAD, and display name. |
| Personal root | A discovered or explicitly configured provider directory, with provenance and access classification. |
| Artifact | Identity, kind, provider, owner, relative path, physical target, metadata, and diagnostics. |
| Skill package | SKILL.md plus its supporting files; metadata files are not mistaken for custom agents. |
| Document snapshot | Original bytes, content hash, filesystem identity, owner identity, checkout revision, and encoding/newline information. |
| Scope assessment | Source chain and directory context, known visibility, and explicit reasons for uncertainty. |
| Change set | Concrete source/destination paths, expected revisions, new contents, backups, and operation status. |

Checkout identity is local. A sanitised Git remote can help label related clones, but it is optional, mutable, and not a unique checkout key. Separate worktrees and clones must stay separate even when their remotes match.

M1 hashes the platform and canonical checkout path into an opaque ID. A rescan at the same location retains that ID; relocating the physical checkout changes it. Registered workspace/root IDs are persisted separately and survive root relocation. Each checkout records every selected root containing it. Remote URLs are not read. Paths provide the initial display label.

Keep native paths as Rust PathBuf/OsString values. Use opaque identifiers across the UI boundary; do not reconstruct a filesystem path from a lossy display string. Home-directory and provider-root resolution must work for the current user and configured environment, not the planning machine.

## Discovery

Use two passes:

1. Walk registered parent folders for repository candidates, including .git directories and .git pointer files. Directly added checkouts are also accepted.
2. For each checkout, inventory supported content recursively, respecting ownership boundaries and provider classification.

Verify candidates with Git commands invoked as argument arrays, such as `git -C <path> rev-parse --show-toplevel` and `git -C <path> worktree list --porcelain -z`. Handle detached HEAD and missing HEAD separately. A nested repository/submodule owns its content; the parent scanner must not index it a second time. Enumerating linked worktrees may reveal paths outside selected roots: offer them as candidates and scan their content only after registration.

Git's [repository introspection](https://git-scm.com/docs/git-rev-parse) and [worktree format](https://git-scm.com/docs/git-worktree) support this distinction. Bare repositories can be reported as lacking editable working-tree content.

Prune .git internals, node_modules, vendor, target, dist, and build by default. Users can adjust exclusions or directly select a repository beneath an excluded parent. Do not hide recognised configuration simply because a generic hidden-file or Git-ignore filter would omit it; show ignored/local-only status and let explicit exclusions win. "All Markdown" honours normal ignore rules by default.

Overlapping roots are deduplicated by canonical checkout identity. Do not follow arbitrary directory symlinks while walking. Recognised linked skill roots may be indexed with their source and target displayed; targets outside selected roots require an explicit root registration to read content. Unavailable drives and permission failures produce partial results, not an empty success.

Scanning is cancellable and reports discovered counts, skipped paths, and issues. Start with a rebuildable in-memory index. Watchers invalidate entries and request rescans; they are not the only source of truth. A manual rescan and refresh-on-focus cover missed events.

M1 implements the first pass only. Both CLI inventory commands rescan synchronously; the core accepts a cancellation token and progress callback so M3 can run the same service on a background worker. Git calls use a five-second timeout, bounded output, disabled optional locks/fsmonitor, and no inherited Git redirection variables. Bare stores and Git metadata are skipped. Exclusions are directory names, not glob patterns. A directly registered root beneath an excluded directory is still scanned independently.

M2 implements the content pass with the same repository owners and registered boundaries. It stops at nested checkout boundaries, retains complete discovered skill packages, and follows local references without fetching URLs or running supporting scripts. Recognized guidance bypasses generic Git ignores; ordinary Markdown uses Git's tracked/unignored-file list. Personal root provenance can override a repository discovery at the same path without changing physical checkout ownership.

Inventory snapshots retain bytes, hashes, file identity, BOM/newline details, and the checkout revision observed during discovery. Inspection returns the snapshot from that inventory; each CLI invocation rebuilds it. Read limits, linked targets, and changes during a snapshot are explicit outcomes. The [artifact guide](artifact-inventory.md) owns the concrete CLI/schema limits and unsupported scope cases.

## Providers and interpretation

The [compatibility document](provider-compatibility.md) owns native paths and source evidence. Each adapter supplies:

- Recognised roots and file/package formats.
- Metadata parsing and validation.
- Creation templates for supported local artefacts.
- Provenance/editability classification.
- Directory-context assessment and its uncertainty.
- Save/reload guidance supported by that provider's documented behaviour.

Store unknown metadata and unsupported formats without rewriting them. The initial editor changes raw source, then validates it; it does not round-trip an entire document through a simplified form model.

Inventory and applicability are separate queries. Selecting a repository alone cannot determine what an already-running session loaded. Runtime flags, trust, hosted policy, and plugin activation may be unknown. Return those unknowns rather than fabricating an "effective configuration".

## Shared service boundary

Registration and repository scanning are implemented by `ConfigStore` and `scan`. M2 adds `artifacts::inventory`, `Inventory::inspect`, and structured diagnostics. M4 adds `changes::ChangeStore` for prepare/preview/apply/restore:

| Operation | Result |
| --- | --- |
| Register/relocate a root | Validated local registration or an actionable error |
| Scan roots | Inventory snapshot plus partial-failure diagnostics |
| Inspect an artifact | Content snapshot, metadata, related files, scope assessment |
| Diagnose a selection | Structured findings with file locations and severity |
| Prepare a change | Validated change set and preview, without mutation |
| Apply a change | Applied, rejected as stale, or recoverable partial failure |
| Restore a change | Reviewed inverse operation with the same conflict checks |

CLI and desktop call these operations directly. The desktop does not spawn the CLI, and the frontend does not implement its own filesystem writer.

## File mutations and recovery

The M4 content-write service owns supported guidance files and packages. Complete provider settings and enable/disable toggles remain future work. Rooster's own registration settings use the smaller `ConfigStore` transaction described under Local state; they do not modify guidance files or require the content recovery journal.

1. Resolve the registered owner, canonical target, and source snapshot. Reject operations on managed/cache/synced content and unsupported link layouts.
2. Validate proposed content, new names, destination containment, package membership, and known references. An invalid existing file can be repaired by a valid edit. Do not overwrite an existing destination.
3. Show a diff and complete affected-file list. Rename updates only supported, explicitly selected local Markdown links; ambiguous prose paths and cross-root references are diagnostics.
4. Acquire a cooperative lock for Rooster writers, then re-read source bytes and checkout identity. Reject a stale source, changed branch/HEAD, or moved target; retain the draft.
5. Record recovery metadata and original bytes in local application data before replacement. Failure to establish recovery stops the mutation.
6. For a normal file edit, stage on the same filesystem and use a verified replacement primitive that avoids exposing partially written content. Preserve relevant permissions, newline style, BOM, and unedited bytes.
7. Read back the result, update the inventory, and mark the operation complete.

Package create/copy/delete and link-repair renames may touch several files. Track them with a small local operation journal: prepared, applying, completed, or recovery-required. They are not advertised as filesystem-wide atomic transactions. On failure or restart, inspect recorded original/new hashes and offer to finish or restore. Never roll back over an unrelated external edit.

Delete keeps a recoverable package/file copy before removing originals. Restore checks that destinations and current revisions still match expectations. A journal or backup must survive a process crash; disk-full and unavailable-volume failures remain actionable.

Originals are retained separately from editor drafts. M5 provides explicit cleanup with a default minimum age of 30 days for completed/restored recovery entries. Prepared/unresolved records and drafts are never candidates, and no automatic expiry runs.

Atomic replacement is not a universal compare-and-swap against every editor. Cooperative locks protect Rooster instances; a non-cooperating writer can still race after a revision check. Minimise that window, detect observed conflicts, preserve snapshots, and disclose this limitation in developer documentation. Do not claim that a watcher or content hash eliminates every concurrent-write race.

Initially treat symlinked/hard-linked documents and packages containing unsupported links as read-only for mutations, with an explanation. Supporting those writes requires explicit semantics and targeted filesystem verification, not silently replacing a link with a regular file.

M4 saves immutable before/proposed content as private hash-addressed blobs with a versioned change record. The preparation holds the complete selected configuration, source dependencies, complete package trees, parent/owner identities, and Git branch/HEAD/metadata paths. Apply acquires a per-recovery-store cooperative lock and revalidates before writing. Each step records verified output identity. Interrupted writes are reconciled against before/after states; new external content is preserved. A recoverable partial result is never reported as completion.

Destination classification reuses the inventory’s native rules. Exact UTF-8 byte-range patches preserve unedited bytes; full replacements preserve BOM and uniform CRLF conventions. Whole-package operations include binary resources, scripts, hidden files, and empty directories. Explicit inline Markdown repairs share the same preview and journal. See [mutation and recovery behaviour](mutations.md) for CLI contracts, supported formats, Unix identity limits, storage permissions, retention, and ambiguous recovery states.

## Local state

Store schema-versioned settings in the platform's per-user configuration directory and drafts/recovery data in its application-data directory. Support a CLI/config override for tests and portable installations. Resolve paths in Rust and expose a "Show application data" action.

M1 persists schema version 1 as JSON, with the concrete format and overrides in the [CLI guide](cli.md). Reads create no files. Registration/relocation acquires a cooperative file lock, reloads and validates settings, stages a same-directory temporary file, syncs and replaces it, then reads it back. Unknown schemas/fields and malformed settings are rejected without overwriting the file. Non-cooperating editors retain a small check/replace race. New settings files use private permissions on Unix; symlinked settings/lock files are rejected. M3 adds watcher invalidation; M4 adds a schema-versioned recovery store. M5 adds schema-versioned private drafts with revision checks, original source/owner/config/checkout guards, atomic saves, explicit comparison tokens, and reviewed preparation through the existing write path. The desktop displays its data directory; opening it in a file manager remains future work.

M2 adds an optional `codex` settings object for independent home/skill roots, default-root discovery, and additional source classifications. M1 files still load, and registration mutations retain those settings. CLI overrides affect the current inventory only. Artifact inspection/template commands remain read-only. The separate `changes` commands use the M4 mutation protocol; source templates can be supplied through its structured requests.

Persist registered roots, workspace labels, provider root overrides, exclusions, UI preferences, drafts, and recovery records. The derived inventory can be discarded and rebuilt. No database or full-text search service is needed initially.

Use per-user access permissions where supported. Keep original backups out of project directories and provider caches. Logs and exported diagnostics contain paths/status only by default, not document bodies, credentials, or remote URLs with embedded authentication.

Artifact list summaries additionally include names, descriptions, reference paths, and unknown-field names. Full provider metadata/source bytes require explicit inspection. Remote URL user information and queries are redacted in summary references.

## Desktop boundary

M3 provides workspace/repository navigation, content search, read-only CodeMirror source, isolated Markdown preview, and scope/reference details. M5 adds source editing and isolated draft preview, complete per-path before/after comparisons, explicit owner selection, saved drafts, external conflict comparison, recovery/retention controls, and read-only Git changes. Heavy file work runs on Rust blocking workers; frontend state does not duplicate validation logic.

The desktop Session wraps shared-core inventory and scope assessment in cancellable background scans. An opaque generation identifies each immutable snapshot; commands reject stale generations and changed registration settings. Native folder selection is handled in Rust, so frontend commands never accept an arbitrary filesystem path. Source bodies are returned only for explicit inspection; search returns matching IDs.

Native watchers invalidate snapshots; manual refresh and a throttled refresh on window focus rebuild them. Watchers are best-effort: the first scan establishes subscriptions after discovery, so events during that initial scan can be missed. Unwatchable roots remain browsable with a visible warning. The [desktop guide](desktop.md) records commands and verified limits.

Keep the UI responsive while scanning and validating. Empty workspaces, missing roots, malformed files, installed read-only content, conflicts, and recovery-required operations have explicit states.

Expose narrow Tauri commands that accept registered IDs and validated edit payloads. Every operation rechecks ownership/path boundaries in Rust. Do not expose an unrestricted shell or generic read/write-any-path command to rendered documents. Use bundled UI assets, scoped capabilities, and a restrictive CSP. Markdown preview sanitises/disables raw HTML, scripts, unsafe URL schemes, and automatic remote resource loads. See [Tauri capabilities](https://v2.tauri.app/security/capabilities/) and [CSP](https://v2.tauri.app/security/csp/).

## Dependencies and follow-on decisions

Prefer established libraries for CLI parsing, serde-based data exchange, source-preserving TOML edits, Markdown parsing, directory walking, watching, hashes, and temporary files. Select current versions when their milestone starts; avoid installing unused dependencies during planning.

Git CLI inspection is sufficient initially. A Git library, persistent search index, background daemon, shared server, automatic provider conversion, and live-agent integration each need an observed requirement before being added.

## M6 provider integration

One explicit provider selects native roots, classification, parsing/templates, and scope interpretation while retaining the shared traversal, snapshots, mutation, draft, and recovery services. Codex remains the default. A `claude` settings object adds configurable home/managed/additional roots; old schema-1 settings and journals deserialize with defaults. Conventional foreign-provider namespaces and relocated provider homes cannot bypass their native validation through the other provider's ordinary-Markdown view.

Desktop scan requests and inventories carry the selected provider. Switching providers publishes a new immutable generation and clears inspection. Draft records retain their provider independently of the selected view; old records default to Codex. Preparing a saved draft rescans its provider with the full saved source settings. Recovery continues to apply recorded, validated bytes without depending on the current UI provider. No parallel writer or automatic conversion/sync is introduced.
