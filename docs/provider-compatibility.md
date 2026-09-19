# Provider compatibility

Status: Codex and Claude Code local-file adapters implemented. Claude sources rechecked on 18 September 2026; Codex sources were checked during M2 on 17 September 2026.

This document separates provider-documented behaviour, local observations, and Rooster's handling. Native configuration evolves: confirm the relevant installed version and current documentation when implementing an adapter.

The local Codex CLI was rechecked as 0.153.4 during M2. The installed Claude Code binary reported **2.1.276 (Claude Code)** during M6. `claude --version` and `claude --help` were inspected; no suitable standalone non-executing agent-file validator was identified. Claude sessions, hooks, skill commands, and AI evaluations were not run. Provider format claims below come from fetched official documentation. Rooster's tests establish local structural handling, not that a provider runtime accepted or loaded the fixtures.

## Common vocabulary

- An **instruction** provides standing guidance, potentially scoped to a directory.
- A **skill** packages a workflow in SKILL.md with optional supporting content.
- A **custom agent** defines a role and its provider-specific settings.
- **Reference documentation** supports another artifact or is ordinary Markdown.
- **Provider settings** can affect discovery and visibility without being an agent or skill.

Provider identity, ownership, scope, and editability are distinct properties. A personal file can be disabled; a plugin file can be present but inactive; a skill can reference another repository while belonging to its original repository.

## Codex

### Documented locations and formats

| Artifact | Conventional location / representation |
| --- | --- |
| Personal guidance | Codex home: AGENTS.override.md or AGENTS.md |
| Repository guidance | AGENTS.override.md, AGENTS.md, or configured fallback names along the project-root-to-working-directory path |
| Personal skills | ~/.agents/skills/<name>/SKILL.md |
| Repository skills | .agents/skills beneath the working directory and its ancestors up to the repository root |
| Personal/project agents | ~/.codex/agents/*.toml and .codex/agents/*.toml |
| Main settings | ~/.codex/config.toml and trusted project .codex/config.toml layers |

Codex home defaults to ~/.codex and can be changed with CODEX_HOME. Resolve user skill roots separately rather than assuming all skills are underneath Codex home.

Guidance discovery selects at most one non-empty instruction file per directory, with override/fallback rules and a configured size limit. A folder selected as a Rooster workspace is not automatically an instruction ancestor that Codex loads. [Codex instruction discovery](https://learn.chatgpt.com/docs/agent-configuration/agents-md)

Skill discovery is directory-sensitive; two skills with the same name are not merged. SKILL.md requires name and description metadata. A skill's agents/openai.yaml supplies metadata and policies, not a standalone custom-agent definition. [Codex skills](https://developers.openai.com/codex/build-skills)

Standalone custom agents are TOML files containing name, description, and developer_instructions, with optional session settings. Agent name comes from metadata, not necessarily the filename. [Custom agents](https://learn.chatgpt.com/docs/agent-configuration/subagents#custom-agents)

The configuration reference also documents agents.<name>.config_file role declarations. Detect that representation without silently migrating it or assuming it is identical to standalone files. Project configuration, profiles, flags, and managed sources can affect the result. [Configuration reference](https://learn.chatgpt.com/docs/config-file/config-reference), [precedence](https://learn.chatgpt.com/docs/config-file/config-basic#configuration-precedence)

### Local observations and handling

The user's setup also includes ~/.codex/skills/.system and plugin content beneath ~/.codex/plugins, alongside personal skills in ~/.agents/skills. Those observed locations do not establish a universal stable cache layout.

Rooster supports explicitly configured compatibility roots, with uncertain provider availability. Installed content exposes package metadata where discovered; active installation references are not currently resolved. A hashed cache directory alone establishes neither the active version nor enablement.

System, managed, installed, and cached sources are classified read-only in Rooster. Ordinary supported local definitions are classified as authoring sources; supported edits use the shared M4 mutation/recovery protocol. Referenced legacy role files can be inspected; enable their native edits only after their schema and ownership have been characterised.

### M2 implementation evidence

The adapter implements the documented local root families, standalone TOML agent fields, skill YAML/frontmatter, and instruction override/fallback selection. It treats agents/openai.yaml as package metadata and reads config-file role declarations separately. Source snapshots preserve comments, unrecognized keys, BOM/newline style, and binary supporting assets.

Tests cover the synthetic nested App layout, separate personal/config roots, a relocated Codex home inside a checkout, sibling isolation, duplicate names, malformed agents, shared legacy role layers, invocation policy, configured disabling, system/compatibility roots, multiple cached plugin versions, missing/local/remote references, symlink boundaries, and source-byte preservation. The CLI exercises list/show/check/template operations. All writes are confined to disposable fixtures.

Plugin manifests can be inspected as package evidence. Installation enablement and the active cached version remain unknown. Legacy role layers retain their declared names/config references and are not interpreted as standalone-agent activation. Profiles, runtime flags, project trust, managed policy, custom root markers, and prose conflicts remain outside the scope estimate.

The verified parser resolution is TOML 1.1.6, serde-saphyr 1.2.0, and pulldown-cmark 0.13.4, fixed in Cargo.lock. No provider task, skill script, hook, or network reference is run to validate a fixture. See the [artifact guide](artifact-inventory.md) for limits, outputs, and the distinction between a complete inventory and provider compatibility.

## Claude Code

### Documented locations and formats

| Artifact | Conventional location / representation |
| --- | --- |
| Personal skills | ~/.claude/skills/<name>/SKILL.md |
| Project skills | .claude/skills/<name>/SKILL.md, including documented nested discovery |
| Personal/project agents | ~/.claude/agents/*.md and .claude/agents/*.md |
| Guidance and rules | CLAUDE.md, CLAUDE.local.md, .claude/CLAUDE.md, and .claude/rules/ |
| Settings | ~/.claude/settings.json, .claude/settings.json, .claude/settings.local.json |

Claude Code's settings are strict JSON, and CLAUDE_CONFIG_DIR can relocate its configuration directory. Keep project settings and personal project overrides distinct. [Settings files](https://code.claude.com/docs/en/settings)

Custom agents use YAML frontmatter followed by a Markdown prompt. Fields and tool/model conventions differ from Codex TOML. [Claude Code subagents](https://code.claude.com/docs/en/sub-agents)

Skills follow the shared Agent Skills format but add Claude-specific behaviour. Legacy .claude/commands Markdown is still recognised. Skills downloaded under skills/synced are account-managed; deleting their local files is not equivalent to disabling them at the source. [Claude Code skills](https://code.claude.com/docs/en/skills)

Guidance can load at startup or when files in a nested directory are accessed. Rules may use path conditions and instructions may import other files. Report unsupported conditions/import resolution as unknown rather than flattening everything into a universal precedence model. [Claude Code memory and rules](https://code.claude.com/docs/en/memory)

### Rooster handling

The M6 adapter supports local Markdown guidance, rules, skills, agent definitions, and their related files. Recognise installed, synced, and managed sources as read-only. Preserve unknown frontmatter keys and distinguish syntax errors from unsupported provider features.

Inspect settings only as required for supported provenance and visibility. A provider's local caches do not authorise editing its account-level configuration. Hosted Claude chat/Cowork configuration and account synchronisation are not managed by the first release.

### Native interpretation and limits

Claude skill metadata is optional; its folder identifies invocation while `name` can supply a display label. Agents require `name` and `description`; Rooster reports incomplete agent-directory candidates as malformed even when Claude would silently skip them as documentation. Unknown keys are retained without asserting their semantics. [Skills](https://code.claude.com/docs/en/skills), [subagent files](https://code.claude.com/docs/en/sub-agents)

Local same-name estimates prefer skills over legacy commands, personal skills over project skills, and project agents over personal agents. Nested skills remain separate candidates. Installed/synced namespaces, enterprise precedence, settings overrides, and live activation remain unknown. [Native name handling](https://code.claude.com/docs/en/skills)

Rooster combines directory instruction candidates without Codex override or byte-budget rules. Conditional `paths` remain uncertain. Literal `@file` references outside code are followed only inside registered boundaries, with cycle/read limits. Imported documents are references, not automatically added to the estimated instruction chain. Runtime trust, approval, exclusions, and import depth are not simulated. Quoted paths and dynamic expressions are not expanded; `@imports` require manual rename repair. [Memory and rules](https://code.claude.com/docs/en/memory)

Personal-home `plugins` and `skills/synced`, managed roots, and explicitly classified installed/synced roots are read-only. `.trash` is omitted. Default macOS managed discovery uses `/Library/Application Support/ClaudeCode`; other operating systems are unverified. Plugin enablement, cache version selection, custom manifest paths, hosted accounts, and auto-memory are outside this adapter's scope. Explicit copies retain their native bytes and are independent; no provider conversion is performed.

The structural parser uses the existing bounded YAML parser, strict JSON for settings/manifests, and raw-source edits. Local checks cover required fields and selected field types; they do not establish tool/model availability or complete provider schema compatibility. Provider settings and plugin manifests remain inspection-only.

## Compatibility contract

| Concern | Rooster behaviour |
| --- | --- |
| Shared skill format | Reuse parsing concepts; validate against each provider's supported interpretation. |
| Provider-specific instructions/tools | Display faithfully; do not promise that copying makes them work elsewhere. |
| New or unknown metadata | Preserve source and report compatibility uncertainty. |
| Same-name artifacts | Retain distinct owners and physical sources; explain only precedence that is established. |
| Plugin/system/synced definitions | Inspect read-only; never edit caches as though they were authoring sources. |
| Copy for customisation | Produce an independent file/package in an explicit destination; no implicit synchronisation or replacement of the original. |
| Native enable/disable controls | Defer until each provider's setting, scope, version, and reload effect has dedicated verification. |
| Configured versus loaded | Report disk evidence and a context estimate separately from live session state. |

## Implementation evidence to capture

For each adapter, record the version tested, source links, synthetic fixtures, supported formats/actions, and remaining unknowns. Verify reads first, then writes on disposable fixture roots with the actual provider parser or non-executing validation facility when available.

Never spawn an AI task, execute skill scripts, or run repository hooks just to scan content. If the provider offers no suitable non-executing validation, state that limitation and verify syntax/structure locally.

Refresh this document when discovery or edit behaviour changes. Avoid copying the same native rules into the product specification and implementation plan.

### M6 independent-review validation follow-up

The installed Claude Code 2.1.276 advertises `claude plugin validate` for agent directories. An isolated `CLAUDE_CONFIG_DIR` and `claude plugin validate <fixture>/.claude/agents --json` returned exit 0 with `contents: []` for both valid agents and deliberately malformed YAML; the parent `.claude` directory behaved the same. No agent checks were reported, so these results establish neither native acceptance nor rejection. Rooster's verified metadata support remains local structural validation; no assistant session, script, or hook ran. The [upstream subagent documentation](https://code.claude.com/docs/en/sub-agents) describes the facility and recursive agent folders.
