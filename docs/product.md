# Product specification

Status: agreed product direction, with proposed delivery boundaries. Last updated: 17 September 2026.

## Outcome

Rooster helps developers understand and maintain their AI coding setup across personal configuration and multiple Git repositories. A user can find an agent, skill, instruction, or reference document; understand its owner and scope; edit it; and review the resulting local file change.

The primary problem is fragmented configuration: useful instructions live in different home directories, repositories, nested applications, and installed packages. Rooster makes those locations visible without changing how coding assistants consume them.

## Users and value

- An individual developer manages their agents, skills, and project guidance.
- A teammate adopts repository guidance using their own local directory structure.
- A maintainer updates shared workflows through normal Git review.

A shared setup communicates the team's intended process. It does not guarantee identical model behaviour or enforce organisational policy.

## Confirmed direction

- Product name: Rooster.
- Rust backend and CLI; Tauri desktop interface.
- Codex first, with Claude Code local-file support through the shared core.
- Existing native files remain authoritative.
- Multiple user-selected workspace folders and individual repositories.
- Repository-specific Markdown, including nested instructions and skill references.
- Create, read, update, and delete user-owned content.
- Personal configuration remains separate from shared repository guidance.

## Proposed release boundary

The first development target is macOS. Keep path handling, the core, and the desktop structure portable; claim Linux or Windows support only after those builds and filesystem behaviours are verified.

The first usable desktop milestone supports Codex. The complete initial release includes Claude Code local-file support. A source editor with syntax highlighting and a Markdown preview is sufficient; a WYSIWYG editor is not required.

The CLI exposes the same discovery and edit operations for scripts and coding assistants. It is delivered incrementally alongside the core, rather than delaying the desktop interface until a large standalone CLI is finished.

## Main workflows

### Add and browse a workspace

1. Select a parent folder or an individual repository.
2. Review the repositories discovered there, including branch/worktree context.
3. Browse Instructions, Skills, Agents, and References. An "All Markdown" view includes ordinary documentation without calling it AI guidance.
4. Filter by repository, provider, scope, file type, name, content, or diagnostic.
5. Refresh the inventory after external changes or relocate a moved checkout.

Multiple roots may overlap. Show the same checkout once, while keeping separate checkouts and linked worktrees distinct.

### Understand an item

Show its source path, provider, repository or personal ownership, containing directory, file format, editability, and relationships to other documents. Scope labels describe what the provider can establish; uncertain activation is labelled as uncertain.

Selecting a skill shows its SKILL.md alongside its references, scripts, assets, and metadata. Script bodies may be inspected as supporting files, but previewing a skill never runs them. General-purpose script editing is outside the initial Markdown/configuration editor.

### Edit and recover

1. Open a supported file or create one in an explicit scope.
2. Edit the source while previewing Markdown and seeing relevant validation.
3. Review the changed files and save to the original checkout.
4. If another process changed the file, keep the draft and offer comparison/reload.
5. For rename or delete, show affected files and known incoming references.
6. Retain a local recovery copy and offer restore without replacing newer edits silently.

A skill is a package. Duplicating or deleting it covers its complete directory, including references and assets, and previews that scope. Ordinary Markdown CRUD operates on the selected file.

Renames stay within the same registered owner/root in the first release. Moving a package between repositories is deferred. A cross-repository copy may be created explicitly; it remains an independent copy, not a synchronised installation.

### Work with a team

Team definitions live in Git. Rooster presents the user's current checkout and its uncommitted changes. Personal root selections, drafts, recovery history, and machine paths stay in the user's local Rooster data.

The initial release neither commits nor pushes edits. Team members use their normal Git workflow for review and distribution. Shared catalogues, installation presets, and detecting divergence between intentional copies can follow later.

### Example: a nested web application

The user supplied [Gecko-Admin-Web-App PR #1180](https://github.com/geckolabs/Gecko-Admin-Web-App/pull/1180), titled "EVE-825 AGENTS.md and Skills Setup (RELEASE BRANCH)". The inspected changes include:

~~~text
App/
  AGENTS.md
  .agents/skills/
    gecko-admin-development/
      SKILL.md
      references/
        commands.md
        pages.md
        routing.md
        structure.md
        translations.md
    gecko-angularjs-to-react/
      SKILL.md
~~~

The local checkout inspected during discovery was on production, while the PR was open. This is a requirements example, not a fixture dependency or an assumption that those files exist in every checkout.

Rooster must discover that nested layout, group the first skill's references, and explain directory-specific guidance. A skill can discuss multiple repositories while being owned by one. Mentioning another repository does not change ownership or make the skill globally available.

Tests use synthetic repositories with this shape; they do not require access to Gecko's private source or copy its instruction bodies.

## Acceptance criteria

| ID | Required behaviour |
| --- | --- |
| R1 | Register arbitrary parent folders and direct repositories; rescan and relocate them without hard-coded usernames or company paths. |
| R2 | Distinguish clones, linked worktrees, submodules, detached HEAD, and repositories with no commits; display partial discovery failures. |
| R3 | Index nested instructions, agents, skill packages, references, and optional ordinary Markdown without traversing dependency/build trees by default. |
| R4 | Present Codex and Claude Code native files through provider-specific rules, with clear scope, provenance, and unknown states. |
| R5 | Support create, duplicate, read, edit, rename, and recoverable delete for supported user-owned files and skill packages. |
| R6 | Preserve unedited content/metadata, detect stale drafts and branch changes, and make partial operations recoverable. |
| R7 | Present installed, synced, system, and managed content as read-only; offer a clearly independent personal copy where appropriate. |
| R8 | Report malformed metadata, duplicate identities, and broken local references with actionable file locations. |
| R9 | Keep shared content in Git and local application state outside repositories; core workflows work offline without an AI account. |
| R10 | Provide a usable Tauri desktop journey and a CLI using the same Rust discovery, validation, and write services. |

## Deliberate limits

- Running agents and task monitoring are outside the initial release.
- Native skill formats overlap, but provider-specific tools, metadata, permissions, and invocation behaviour are not universally interchangeable.
- Runtime activation, cloud policy, plugin enablement, and arbitrary prose conflicts cannot always be determined from local files.
- A local scan sees the current checkout. Remote branches and PRs require an explicit future integration.
- Main provider settings are initially inspected only as needed to explain supported artefacts. A complete settings editor and enable/disable controls are follow-on work, with provider-specific semantics.
- Do not edit installed caches or infer that every cached version is active.
- Imported Markdown is content to display. It cannot issue application commands or cause automatic network fetches.

## Success signals

Measure these during a small team pilot, without uploading document content:

- A new teammate can locate repository guidance and make a reviewed local edit without path-specific coaching.
- Users can distinguish personal configuration from shared and installed content.
- External edits and failed writes leave recoverable drafts and clear state.
- The nested-app fixture is completely discoverable, and its support files remain grouped.

Set performance targets from representative fixtures during discovery implementation, rather than inventing them before measuring repository sizes.

## Related documents

- [Architecture](architecture.md)
- [Provider compatibility](provider-compatibility.md)
- [Implementation plan](implementation-plan.md)
