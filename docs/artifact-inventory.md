# Artifact inventory

M2 adds read-only discovery and inspection to the shared Rust core. The CLI calls `artifacts::inventory`; the M3 desktop uses the same inventory, progress/cancellation events, and `Inventory::inspect` operation.

## Codex commands

~~~sh
cargo run -- artifacts list --workspace Personal
cargo run -- artifacts list --workspace Personal --context /absolute/path/to/checkout/App --json
cargo run -- artifacts list --all-markdown --no-default-roots --json
cargo run -- artifacts show ARTIFACT_ID --json
cargo run -- check --workspace Personal --json
cargo run -- artifacts template skill --name review-changes --description "Use when reviewing local changes."
~~~

`list`, `show`, and `check` accept the same selection flags: `--workspace`, `--context`, `--all-markdown`, `--exclude`, `--no-default-roots`, `--codex-home`, `--user-skills`, `--admin-skills`, repeated `--compat-root`, repeated `--installed-root`, and `--json`. Keep selection flags consistent when using an ID from a previous listing. Each command rescans; missing IDs fail rather than showing stale cached content.

`template` prints a native instruction, skill, or standalone-agent template. It creates no file. Source mutations use the separate [M4 mutation and recovery protocol](mutations.md).

Replace `ARTIFACT_ID` with an ID returned by `artifacts list`.

## Roots and ownership

By default, personal guidance/settings/agents are resolved from Codex home, honoring `CODEX_HOME`. User skills are resolved separately from `~/.agents/skills`. Admin skills use `/etc/codex/skills`. Observed compatibility/system and installed-plugin locations beneath Codex home are inventoried with uncertain activation.

Only guidance, config, agent, skill, and plugin locations are inspected under Codex home. The inventory does not recursively browse its authentication files, sessions, or logs. Missing conventional roots are optional; an unavailable explicit root reports a partial inventory.

`--no-default-roots` disables ambient defaults. Explicit roots from flags or saved settings still apply. These overrides select Rooster's inventory locations; they do not configure Codex itself.

Settings remain schema version 1, with an optional `codex` object. M1 settings load with defaults, and subsequent registration edits preserve this object:

~~~json
{
  "codex": {
    "include_default_roots": false,
    "home": "/absolute/path/to/codex-home",
    "user_skills": "/absolute/path/to/personal-skills",
    "admin_skills": null,
    "extra_roots": [
      { "path": "/absolute/path/to/compatibility", "provenance": "compatibility" },
      { "path": "/absolute/path/to/plugin-cache", "provenance": "installed_plugin" }
    ]
  }
}
~~~

Add this object alongside the existing workspace fields. Saved override paths must be absolute. CLI path arguments also accept relative paths and a leading `~`. Older Rooster builds may reject the new settings field; backward reading is supported by M2, not promised in the other direction.

Physical ownership and provider scope are distinct. A relocated Codex home inside a Git checkout retains personal scope, while its source snapshot records the actual checkout owner. Nested repositories own their files independently. A reference to another checkout does not import its instructions into the current context.

Automatically derived Codex-home folders (`agents`, `skills`, and `plugins`) cannot authorize an external symlink target. Register that target independently to inspect it. Allowed links retain discovery and physical paths and are classified read-only; blocked links are reported without reading their contents. Explicitly selected root paths can still be canonicalized.

## Inventory and inspection

An artifact records its ID, discovery/physical paths, owner and checkout revision, source classification, directory scope, package ID, validation, unknown-field names, ignore status, references, and snapshot metadata. IDs identify the native discovery path. Linked skill aliases show both paths and remain read-only.

Skills retain their complete discovered package: SKILL.md, reference documents, agents/openai.yaml, scripts, and assets. The YAML file is skill metadata, not an agent. In the desktop library it appears beneath its owning skill with the other package files, rather than in Settings. Scripts and binary assets are inspectable supporting files and are never executed. Dependencies/build directories remain excluded even inside packages.

List output omits full metadata values and source bodies. Explicit `show --json` includes parsed metadata and exact snapshot bytes, plus decoded text when UTF-8. Snapshots retain hashes, file identity, BOM, and newline information. Malformed metadata can still be inspected. Human output escapes non-text control characters.

Standalone TOML agents use their declared name. Referenced legacy role layers are labeled separately, kept read-only, and carry their declared role names. Multiple declarations sharing one file retain their config references and names; combined role scope remains unknown.

`role_declaration_count` counts declarations independently of the distinct `declared_role_names`. Two repositories declaring the same role name against one shared file therefore still produce unknown combined scope.

Ordinary Markdown is opt-in and uses Git's tracked/unignored-file list. Recognized guidance and package files remain discoverable when ignored, with the ignore status shown. Local CommonMark links and images, including reference-style links, add related files. Links inside code blocks are not interpreted.

Reference targets must remain inside registered boundaries and outside exclusions. Ordinary links do not follow filesystem symlinks. Supported skill-folder links can be followed when their targets are within a registered boundary; cyclic or external targets are recorded without reading their content. References never trigger network fetches. Remote URL user information and queries are redacted in the summary; the exact original remains in explicit source inspection.

Referenced files keep their own source classification, including installed-content read-only status. Path components are checked before collapsing `..`, so a symlink cannot silently redirect an inspected reference. Ordinary parent-relative links between registered roots remain supported.

## Scope and diagnostics

`--context` estimates instruction selection by directory, override/fallback names, and byte limit. It also reports skill/agent candidates, out-of-scope definitions, configured disabling, and known invocation policy. Malformed policy is unknown, rather than silently treated as enabled.

Fallback filenames use applicable configuration layers and their overrides. A project setting cannot classify files in another checkout or an unrelated directory branch. Inventory can retain ancestor guidance that a descendant configuration would select; the context estimate determines whether it belongs in the selected instruction chain.

The estimate assumes applicable project config layers are trusted. It explicitly leaves profiles, runtime flags, managed policy, custom project-root markers, catalog budgets, plugin enablement, compatibility-cache activation, and live-session loading unknown. Legacy role activation is not equated with standalone agents. Contexts outside scanned checkouts receive personal information and an explicit gap in repository assessment.

Diagnostics distinguish malformed native metadata, unknown fields/features, missing references, potentially duplicate names, unsupported links, and incomplete reads. Duplicate definitions remain separate. Settings are read only for supported inventory/scope facts; Rooster does not evaluate arbitrary provider settings or infer conflicts in prose.

`list`/`show` return 0 for complete inventories, even when validation errors are present. `check` returns 3 for validation errors; warnings alone return 0. Incomplete inventories return 2, cancellation returns 130, and fatal settings/ID errors return 1. Zero does not certify provider loading or model compatibility.

## Limits and verification

The initial defaults are 20,000 candidate files, 1 MiB per source snapshot, and 64 MiB total snapshot bytes. YAML parsing also has explicit depth/event/alias budgets and no include or property-expansion facility. Reaching an inventory/read limit is reported. File changes during a snapshot are rejected; an entire directory/checkout scan is still a best-effort observation, not an atomic transaction.

Markdown heading fragments, arbitrary prose/backtick path mentions, profile evaluation, custom project-root markers, non-repository project lookup, and active plugin-version resolution are not implemented. Those uncertainties stay visible. Very large tree responsiveness remains M7 work.

Tests use independent temporary personal/config roots and a synthetic nested App layout, plus sibling/nested checkouts, overrides/fallbacks, duplicate names, malformed agents, legacy declarations, installed versions, linked roots, broken references, exact source preservation, root relocation, cancellation, and limits. The CLI tests exercise actual list/show/check/template commands. See the [progress tracker](implementation-plan.md#progress) for accepted milestone evidence.

## Claude Code inventories (M6)

Use `--provider claude` on `artifacts list`, `artifacts show`, `artifacts template`, and `check`. Codex remains the default. Both providers use the same repository registrations, scan exclusions, snapshots, references, and read limits.

~~~sh
rooster artifacts list --provider claude --context /absolute/checkout/App --json
rooster artifacts show ARTIFACT_ID --provider claude --json
rooster check --provider claude --no-default-roots
rooster artifacts template agent --provider claude --name reviewer
rooster artifacts template rule --provider claude
~~~

`--claude-home`, `--managed-root`, repeated `--installed-root`, and repeated `--synced-root` override/add roots for this inventory. `--no-default-roots` disables ambient discovery but retains explicit overrides. Provider-specific flags reject the wrong provider. Persistent schema-1 settings accept this optional object (paths must be absolute):

~~~json
"claude": {
  "include_default_roots": false,
  "home": "/absolute/personal-claude",
  "managed": null,
  "extra_roots": [
    {"path": "/absolute/plugin-cache", "provenance": "installed_plugin"},
    {"path": "/absolute/account-skills", "provenance": "synced"}
  ]
}
~~~

With ambient discovery enabled, home resolves from `CLAUDE_CONFIG_DIR`, then `~/.claude`. Known home files and derived skills/agents/rules/commands/plugins are scanned; unrelated credential/session files are not included by ordinary home traversal. See [provider compatibility](provider-compatibility.md#claude-code) for native layouts and limits.

JSON inventory/desktop results identify their provider. New artifact kinds are `rule` and `legacy_command`, and provenance adds `synced`. Scope estimates identify the provider; `instruction_byte_limit` is null for Claude because the Codex budget is inapplicable. Existing Codex values stay numeric. Same-name diagnostics and directory estimates retain source differences; conditional paths, imports, installed/synced namespaces, and runtime loading are not flattened into a claimed effective configuration.
