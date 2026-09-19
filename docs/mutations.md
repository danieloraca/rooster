# Reviewed file changes and recovery

M4–M5 implement one Rust mutation service used by the CLI and desktop, with private drafts and explicit recovery retention. These operations do not run guidance, scripts, AI sessions, Git hooks, commits, or pushes.

## Protocol

1. Inspect a fresh inventory and identify the artifact or destination owner.
2. Prepare a proposal. Rooster validates supported formats and ownership, snapshots complete package contents, and saves original/proposed bytes in private recovery storage. Source files remain untouched.
3. Review its saved preview: every affected file and directory, before/after text or binary hashes and sizes, permissions, and warnings.
4. Apply that change ID. Rooster locks its recovery store, checks configuration, owner/parent identity, checkout branch/HEAD/Git directories, source dependencies, package membership, file identity/content, and destination absence.
5. Each file change uses a temporary file in its destination directory, sync and atomic replacement (or no-clobber creation), followed by readback and a durable journal update. Package operations are ordered individual changes, not a filesystem-wide transaction.
6. On interruption, finish the same change or restore it. Any conflicting external revision is preserved and reported.

Prepared changes remain available for later review/application. There is no direct arbitrary-path write command or automatic force-overwrite mode. An inventory prepared with different source settings is rejected; mutation discovery uses the complete saved Rooster configuration. The original snapshots and reviewed proposal survive rejected or failed apply attempts.

## CLI

All `changes` commands emit JSON. Replace IDs and paths with values from your installation:

~~~sh
rooster artifacts list --all-markdown --json
rooster changes owners
rooster changes edit ARTIFACT_ID --source /absolute/path/to/draft.md
rooster changes preview CHANGE_ID
rooster changes apply CHANGE_ID
rooster changes list
rooster changes restore CHANGE_ID
~~~

`edit` prepares a full UTF-8 replacement. A uniform CRLF convention and UTF-8 BOM are retained; unknown metadata/comments are never parsed and serialized back into the source. For exact edits to mixed-newline files, use byte-range edits below. Full replacements otherwise use the supplied text; Rooster cannot infer which portions of a supplied full replacement the author intended to leave unchanged.

Create an ordinary Markdown file:

~~~sh
rooster changes create OWNER_ID docs/new-guide.md --source /absolute/path/to/source.md
~~~

For other operations, save a JSON request and run:

~~~sh
rooster changes prepare --request /absolute/path/to/request.json
~~~

Example requests:

~~~json
{"operation":"edit","artifact_id":"ARTIFACT_ID","edits":[{"start":20,"end":25,"replacement":"new text"}]}
~~~

Ranges are non-overlapping UTF-8 byte offsets in the inspected original text. All bytes outside the selected ranges remain unchanged, including comments, unknown fields, BOM, and mixed line endings.

~~~json
{"operation":"create","owner_id":"OWNER_ID","path":".codex/agents/reviewer.toml","kind":"agent","text":"name = \"reviewer\"\ndescription = \"Review code\"\ndeveloper_instructions = \"Inspect changes carefully\"\n"}
~~~

File kinds are `markdown`, `instruction`, `skill`, `skill_metadata`, and `agent`. Destinations must fit the native format and supported discovery rules. Skill metadata uses `agents/openai.yaml` in an existing skill package. For a new complete package, use `create_package` with a required `SKILL.md`:

~~~json
{"operation":"create_package","owner_id":"OWNER_ID","path":".agents/skills/example","files":[{"path":"SKILL.md","bytes":[...]}]}
~~~

The illustrative `...` must be replaced with the actual byte array. File bytes can be binary for package resources; the skill entry point and supported metadata are validated as native text. A skill source can also be created from a `create` request with kind `skill` and a full native SKILL.md text.

~~~json
{"operation":"duplicate","artifact_id":"SKILL_ARTIFACT_ID","owner_id":"DESTINATION_OWNER_ID","path":".agents/skills/independent-copy"}
{"operation":"rename","artifact_id":"SKILL_ARTIFACT_ID","path":".agents/skills/renamed","repair_links":["REFERRING_MARKDOWN_ARTIFACT_ID"]}
{"operation":"delete","artifact_id":"ARTIFACT_ID"}
~~~

Each line above is a separate request file. A skill artifact selects its complete package directory for duplicate, rename, and delete, including scripts, binary assets, hidden files, and empty directories. These complete operations do not execute or edit script bodies. Direct script editing is unsupported. Oversized or linked packages are rejected as a unit rather than partially copied/deleted. Rename stays within the original owner. Duplicate can target another authoring owner; it is an independent copy and does not rewrite provider names or establish activation.

Known incoming references are reported. Only explicitly selected, same-owner inline Markdown links/images with unambiguous literal destinations are rewritten, including rebasing outgoing links in selected moved documents. Reference-definition syntax, code/HTML labels, nested links/images in labels, escaped/ambiguous destinations, cross-owner repairs, fragments, and prose need review; unselected links remain unchanged. Fragments are retained but not validated. The resulting bytes are part of the same preview/journal.

Exit codes: `0` means the requested operation completed; `1` is validation, conflict, or I/O rejection; `2` is an operation that started but needs recovery. Read the structured status and error. `changed` contains verified applied paths; `pending` contains steps without a recorded successful apply and can include the step interrupted between its filesystem change and journal update. Do not interpret `pending` as proof that a path was untouched.

## Recovery storage and restart

The default directory is `<platform local data directory>/rooster/recovery`. Set `ROOSTER_DATA_DIR` or `changes --data-dir /absolute/private/path` for an isolated trial. `ROOSTER_CONFIG`/`--config` selects registration settings. Recovery storage must stay outside repository owners and provider sources. Every development test uses disposable settings, repositories, and recovery storage.

The recovery directory and change directories use mode 0700 on Unix; JSON records and content-addressed blobs use 0600. Blobs contain original and proposed bytes and are hash-checked before writes. Records have schema version 1 and explicit `prepared`, `applying`, `completed`, `recovery_required`, `restoring`, and `restored` states. Prepared records are published only after their blobs are durable. Abandoned temporary preparations with no record touched no source files.

On restart, use `changes list` and inspect the affected ID. `apply ID` finishes an interrupted forward operation. `restore ID` reverses completed/partially applied steps after validating expected current bytes and identities. An incomplete restore is resumed using `restore ID`. A different unresolved operation blocks new writes in the same recovery store. Do not delete or edit its record to bypass that guard.

A crash between an atomic write and its journal update is reconciled by comparing the recorded before/after content, type, and mode. Once an applied identity is durably recorded, a replacement file is rejected even if its bytes match. Ambiguous states, missing/corrupt backups, new children in directories being removed, changed settings, unavailable/moved roots, and changed checkouts are left untouched for explicit resolution. No automatic rollback replaces newer content.

Recovery records have no automatic expiry. Explicit cleanup defaults to completed/restored records at least 30 days old, measured from the last journal update. Prepared and unresolved operations are never eligible. Draft records are separate and never included. Cleanup rechecks the explicit ID list under the store lock and reports partial removal if I/O fails. Keep the same private recovery directory across CLI invocations so cooperative locking and unresolved-operation detection apply to all writers.

## Drafts, cleanup, and Git inspection

The desktop uses `ChangeStore` draft operations to open, save, compare, explicitly rebase, prepare, and discard versioned private UTF-8 drafts. A draft captures its original content and ownership/configuration/checkout identities. Saving a draft never mutates source; preparation rejects a changed base. Comparison supplies a token for the exact current revision. Only an explicit, still-current source comparison can become a new base, and ownership/checkout/settings changes cannot be silently rebased. Atomic private records survive reopening, and concurrent stale draft versions are rejected.

~~~sh
rooster changes git OWNER_ID
rooster changes cleanup-preview --days 30
rooster changes cleanup --days 30 --id CHANGE_ID --id ANOTHER_CHANGE_ID
~~~

Git output contains the local checkout, branch/HEAD, and staged/worktree status with rename origins. It does not stage, commit, push, or run hooks. Cleanup removes only selected eligible recovery directories, including their backups; it never edits original source files. `--days 0` allows explicit removal of recently completed/restored entries. Inspect the preview before selecting IDs. A partial cleanup returns exit code 2 and lists removed and pending IDs.

## Preservation and limits

Verified platform: macOS Apple Silicon. Mutations currently require Unix device/inode identity checks; other platforms remain read-only until their identity and replacement semantics are implemented and tested. Regular-file bytes and Unix rwx modes are preserved; metadata such as ACLs, extended attributes, ownership changes, and timestamps is not claimed. Symlinked ancestors, symbolic/hard-linked documents, special files, and unsupported package links are rejected.

Installed, managed, system, compatibility, and linked sources cannot be mutation destinations. An intact installed skill can be explicitly duplicated into a separate authoring owner. Main provider settings, legacy role configurations, plugin manifests, and general-purpose scripts are read-only outside complete package operations. Supported native metadata is structurally checked using the same Codex parser as inventory; unknown fields are retained and surfaced, not discarded. This is not a claim that a running Codex session accepted or loaded the change.

Limits are 16 MiB per mutation file, 64 MiB of distinct backup/proposal bytes per change, and 20,000 package entries. Inventory limits can be stricter and unavailable source snapshots prevent preparation. Partial inventories cannot be used to prepare writes.

Cooperative locks coordinate Rooster instances using one recovery directory. They do not lock out other editors, filesystem changes, or writers using another recovery directory. A non-cooperating writer can race after the final check and before replace/unlink; a matching post-crash content state cannot prove who wrote identical bytes. Backups, narrow rechecks, readback, and conservative conflict handling reduce this risk but do not provide universal filesystem compare-and-swap or multi-file atomicity.

## Claude local files (M6)

Select `--provider claude` on `changes owners`, `changes edit`, `changes create`, or `changes prepare`. Inventory IDs must come from a Claude inventory. Preparation uses the saved `claude` source settings; one-off inventory overrides must be persisted before preparing changes. Preview/apply/restore/history use the saved change ID and do not require repeating provider selection.

Supported Claude files are Markdown instructions, YAML/Markdown agents, skill packages, rules, legacy commands, and ordinary/supporting Markdown. Structured creation accepts `instruction`, `agent`, `skill`, `rule`, and `legacy_command` kinds and validates their destination classification. Native agents use `.md`; Codex TOML is not converted. Metadata/source bytes retain the existing unknown-field, BOM/newline, snapshot, and recovery protections.

Installed, managed, and synced sources are inspection/copy-only. Account-managed `skills/synced` (including case variants) and `.trash` cannot become authoring destinations. A copied package remains independent. Conventional foreign-provider namespaces and configured provider homes are excluded from the other provider's editor. Literal Claude `@imports` are reported as incoming references but are not rewritten by Markdown-link repair; review them manually when renaming.

Private drafts carry a provider; records written before M6 default to Codex. Reopening a Claude draft after switching the desktop to Codex retains Claude validation. Source settings/ownership/checkout changes still require the normal conflict workflow.
