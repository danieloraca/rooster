# Desktop browsing, editing, and recovery

M3–M5 provide a Tauri 2 application over the same Rust inventory and mutation services used by the CLI. It registers workspaces, inspects Codex artifacts, edits native source, and previews recoverable file/package changes.

## Launch

Requires Rust 1.90+, Git on PATH, Node 22.12+, npm, and Tauri platform prerequisites. macOS needs Xcode Command Line Tools. Dependencies are fixed by Cargo.lock and package-lock.json.

To build, install, and open the macOS app, run `./scripts/install.sh` from the repository root. An existing bundle can be installed with `./scripts/install.sh --app /absolute/path/to/Rooster.app`. See [installer options](release.md#install-and-open-with-one-command).

For the development server with live reload:

~~~sh
cd apps/desktop
npm ci
npm run desktop
~~~

`npm run dev` alone serves the web interface but cannot access local inventory. Use the Tauri launcher for native dialogs and Rust commands. `npm run desktop:build` builds an executable with embedded frontend assets; bundling is disabled by default. `npm run desktop:bundle` creates an ad-hoc signed local pilot app; `npm run desktop:bundle:debug` creates its development variant. See [release preparation](release.md) for output locations, installation, and public-distribution prerequisites.

The desktop shares the CLI settings location and honors `ROOSTER_CONFIG`. For an isolated trial, set that variable to an absolute disposable JSON path and `ROOSTER_DATA_DIR` to a private disposable recovery directory before launching. Existing settings can disable default provider roots and select explicit fixture roots; see [inventory configuration](artifact-inventory.md). In the interface, **Personal & installed sources** controls all configured personal/installed roots, including explicit roots.

## Browse

1. Choose **Add workspace**, enter a local label, and select a repository or parent folder in the native folder picker. Only Rooster registration settings are written. Under **Locations → Registered folders**, each registered folder has a **× Remove location** button. Confirm removal to stop scanning that registration; source files, saved drafts, and recovery history stay intact. Offline folders can also be removed. Removing a workspace’s last folder removes the empty workspace and returns its selection to All workspaces. Overlapping registrations can keep the same repository visible.
2. Choose **Codex** or **Claude Code** in the Provider selector, then select a workspace and repository. Filter instructions/rules, skills/legacy commands, agents, or settings. Supporting files are available through their skill packages and search in **All artifacts**, without a separate References category. In **All artifacts** and **Skills & commands**, expand a skill’s supporting files to inspect its metadata (including `agents/openai.yaml`), references, scripts, and assets. **Settings** contains provider configuration and plugin manifests; skill metadata stays with its package. Search and **Needs attention** show matching child files beneath their owning skill, even if the skill itself does not match.
3. Search names, paths, descriptions, and source text. Enable ordinary Markdown to include repository documentation.
4. Select a file. **Preview** renders Markdown, **Source** shows its unchanged text, and **Details** explains ownership, provenance, validation, directory applicability, package members, and references.
5. Select a directory context to assess nested guidance. Contexts currently include repository roots and discovered guidance directories, not every arbitrary subdirectory. Unknown runtime/trust/activation inputs remain visible.
6. Follow local references through Details. Remote links and images are inert in preview.

CodeMirror 6 provides selection, syntax highlighting, line numbers, wrapping, and find for Markdown, TOML, JSON, and YAML. The inspector is read-only; **Edit** opens a separate private draft with undo/redo. Binary/unavailable files show metadata or diagnostics instead of pretending to contain editable text.

Scan progress reports observed counts because the tree size is unknown. Cancel retains explicitly incomplete results. Discovery failures, unavailable sources, malformed metadata, and watcher failures have visible outcomes. Installed-source restrictions have explicit reasons; copying a skill to an authoring owner creates an independent source.

## Edit and recover

1. Select a supported authoring file and choose **Edit**, or choose **New** with an explicit destination owner, relative path, and native file type. New skills start with a native SKILL.md template; add metadata separately.
2. Edit source, import a local text file, or inspect the isolated Markdown preview. Draft changes save after a short idle period. Wait for **Draft saved locally** or use **Save draft**; a crash before that acknowledgment can lose the newest unsaved keystrokes. **Keep draft & close** flushes pending text. **Drafts** reopens saved text after navigation or restart.
3. Choose **Review changes** for native validation and a complete before/after preview. Every affected path is listed; binary files show sizes and hashes, and empty directories remain visible. Source files change only after **Apply reviewed changes**.
4. **Duplicate**, **Rename**, and **Delete** use the same shared protocol. A selected skill means its entire package. Rename stays within its owner; duplication asks for a destination owner. Incoming references are shown, and selected supported same-owner links can be repaired within the reviewed change.
5. If another process changes the source, review is rejected while retaining your draft. **Compare current file** shows both texts. For a source-only change, explicitly keep your draft on the displayed current base, or replace it with that current text. The exact displayed revision is rechecked. Changed ownership, settings, or checkout require opening a fresh draft; the saved draft remains available to copy.
6. **Recovery** reopens saved proposals, finishes interrupted changes, and previews restoration of originals. Conflicting newer revisions are preserved. **Preview cleanup** defaults to completed/restored records at least 30 days old. Deleting the explicitly listed records removes backups and restore capability, never source files; prepared/unresolved operations and drafts are excluded. No automatic expiry runs.
7. **Git changes** shows staged and working-tree status for a selected checkout. Rooster does not commit, push, switch branches, or claim that a running assistant loaded the edits. Follow the displayed reload guidance for your assistant.

Drafts are versioned private records alongside recovery data, outside source owners. Concurrent draft updates reject stale versions. Local source edits preserve unedited mixed newline sequences and BOM; imported replacements intentionally use the selected text, with the shared replacement rules described in the [mutation guide](mutations.md).

## Refresh and boundaries

A scan publishes an immutable generation. Inspection returns that snapshot; a changed file does not silently replace content mid-view. Native filesystem notifications mark it stale, and **Refresh now** rescans. Returning focus also refreshes after a two-second throttle. Settings changes and replaced generations reject stale commands. A refresh requested during an active scan is queued, including registration during a focus-triggered scan.

Watchers are best-effort. Initial subscriptions are installed after discovery, and OS events can be delayed/coalesced or unavailable. Missing roots can be noticed through an existing parent; deep missing ancestry may require manual refresh. Broad registered roots can generate unrelated invalidations. Manual refresh remains authoritative. A complete scan is not proof that a live AI session loaded its files.

The frontend passes registered workspace IDs, artifact IDs, directory-context IDs, and generations. Editing adds typed requests with explicit registered owner IDs and relative destination paths; the Rust service validates containment and native layouts. The native text picker imports at most 16 MiB of UTF-8 into a private draft. It cannot submit an arbitrary path for inspection or registration; the latter comes from a Rust-owned native picker. Rust retains the shared core’s containment, provenance, snapshot, and scope rules.

Only eleven application commands are granted to the main local window. No generic shell, filesystem, remote-window, or frontend dialog capabilities are granted. The production CSP permits bundled assets and local Tauri IPC. Markdown is rendered with raw HTML disabled, links as text, and images as placeholders, inside a sandboxed iframe with no script or same-origin permissions and its own deny-by-default CSP. Preview documents cannot read the parent or invoke its privileged bridge. This follows [Tauri capabilities](https://v2.tauri.app/security/capabilities/) and [CSP guidance](https://v2.tauri.app/security/csp/).

## Verification and limits

Verified on macOS Apple Silicon using a built app with embedded assets: native add-root dialog → discover two repositories → choose web repository → inspect nested skill → follow its reference. The native hostile-Markdown fixture rendered text without executing its script, opening a privileged dialog, changing the title, or requesting its loopback tracking URLs. A native source view also rendered the original script text inertly.

Rust integration tests cover ID-only inspection, stale generations/settings, selected directory scope, cancellation, snapshot preservation, and external-change invalidation followed by refresh. Frontend tests cover hostile rendering, iframe restrictions, overlapping scan starts, queued registration refresh, retained conflict drafts, and mixed-newline source edits. Browser checks use a disposable inventory exported through the real Rust Session; this browser-only fixture is a UI test aid and is excluded from production builds.

Use `cargo test --workspace --locked`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, and `cargo +1.90.0 test --workspace --locked` from the root. In `apps/desktop`, run `npm test` and `npm run build`. macOS watcher tests require normal native filesystem event access; a restricted runner may suppress events.

The source viewer is loaded lazily and currently exceeds Vite’s 500 kB chunk warning threshold. [Discovery measurements](performance.md) document the tested workloads. Other operating systems and public signed/notarized distribution remain unverified.

The M3 registration/focus overlap finding is resolved by queued refreshes and a regression that rejects the old inventory before publishing the follow-up scan.

M5 native verification used disposable repositories and a built macOS app: instruction edit with undo/redo and before-apply byte checks; complete skill copy including metadata, references, binary data, and an empty directory; package delete and restore with exact tree comparison; rename with explicit incoming-link repair while preserving a misleading link title; native text import into a new-file draft; quit/relaunch and reopen that draft; reviewed creation; and an external process changing an open draft’s source, which rejected preparation and displayed both retained texts. Git status and recovery retention preview were also inspected. Supplemental browser checks confirmed the library and draft dialog at 1280×850 and 900×650 without page overflow or application console errors (the dev fixture has an optional favicon 404). Browser checks do not prove native IPC.

## Claude editing (M6)

Provider selection uses the same library and editing controls. Claude **New** offers native `.md` agents, rules, and legacy commands; instruction templates use CLAUDE.md. Details shows conditional-rule/import diagnostics, unknown metadata, and read-only installed/synced provenance. Saved drafts display their provider and keep that validation after view changes or restart. Copies are explicitly independent. Root overrides remain in Rooster's local settings; an in-app provider-settings editor is not included.

M6 native verification used a separate fixture home and two repositories: Codex → Claude → Codex selection; retained Claude draft with unknown YAML metadata; reviewed apply while viewing Codex and exact restore; read-only synced skill → complete independent personal copy → restore. Readbacks verified all bytes, binary assets, and independent file identities. Browser QA used a read-only export through the real Session at 1280×850 and 900×650, checking provider switching/cleared selection, conditional rules, read-only controls, and reachable repository navigation. The Browser plugin was unavailable, so installed Chrome/Playwright provided supplemental rendering evidence. A cramped compact sidebar found in screenshots was corrected and rechecked. No application console errors or framework overlays were observed; the optional dev favicon 404 is unrelated. Native console/network instrumentation was not collected; browser evidence does not replace native IPC checks.
