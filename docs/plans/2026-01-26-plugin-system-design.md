# Plugin System Design (B-first, C-extensible)

## Goals
- Install content plugins that ship commands, skills, rules, contexts, hooks, and MCP configs.
- Support user-level and project-level installs with clear precedence.
- Validate plugins on install (structure + traversal/symlink safety + hook/script scan).
- Expose CLI and TUI management with enable/disable and policy prompts.
- Leave an explicit extension point for runtime code plugins (C) without executing code now.

## Non-goals (for B phase)
- Execute plugin hooks or runtime code.
- Provide sandboxed plugin runtime.
- Auto-enable MCP servers or hooks without user policy.

## Plugin Format
- Prefer `.claude-plugin/plugin.json` and `.claude-plugin/marketplace.json`.
- Also accept `plugin.json` at repo root for compatibility.
- Components are optional and defined as relative paths:
  - `commands`, `skills`, `rules`, `contexts`, `hooks`, `agents`, `mcp-configs`.

## Storage Layout
- User scope: `~/.codex/plugins/<plugin_name>/...`
- Project scope: `<repo>/.codex/plugins/<plugin_name>/...`
- Registry: `installed_plugins.json` stored under each scope root.

## Architecture
- `PluginManager` (codex-core) reads registries, loads manifests, and resolves component roots.
- `PluginInstaller` installs from local path, GitHub repo, or URL zip into a temp dir, validates, then moves to target.
- `PluginValidator` enforces:
  - Manifest presence and component path validity.
  - Path traversal and symlink escape checks.
  - Hook/script static scan with warnings and policy gating.
- `PluginRegistry` stores install metadata, enabled state, compliance results, and policy settings.
- `RuntimePlugin` trait reserved for C phase.

## Data Flow
- Install:
  1) Fetch/unpack → 2) Validate → 3) Write plugin dir → 4) Update registry → 5) Report compliance.
- Load:
  - Merge registries (project > user), filter enabled.
  - Provide component roots to skills, rules, prompts, contexts, and MCP config loaders.

## Component Integration
- Commands: expose as custom prompts under `/prompts:`.
- Skills: add plugin skill roots to skills loader.
- Rules: add plugin rules dirs to execpolicy loader (lower precedence than local rules).
- Contexts: append plugin context files to user instructions.
- MCP configs: load configs as disabled-by-default servers; user must enable explicitly.
- Hooks/Agents: recorded only; runtime execution deferred to C.

## Policy and Compliance
- Mandatory checks: manifest + safety + hook/script scan.
- Optional checks: license/source/hash (warn only).
- Hooks/scripts default to disabled. User must set policy to allow.

## CLI UX
- `codex plugin install <source> [--project|--user]`.
- `codex plugin list` with status + warnings.
- `codex plugin enable|disable <name>`.
- `codex plugin policy set <name> ...`.
- `codex plugin marketplace list|add|remove`.

## TUI UX
- `/plugins` opens a management view.
- Show installed plugins, scope, enabled state, and compliance status.
- Toggle enable/disable and open policy hints.

## Testing
- Core unit tests for manifest parsing, path checks, registry merge.
- CLI tests for install/list/enable/disable.
- TUI snapshot tests for `/plugins` view.
