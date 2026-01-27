# Plugins

Plugins are installable content bundles that extend Codex with commands, skills, rules, contexts,
hooks, agents, and MCP configs.

## Install

- `codex plugin install <path|url|github:owner/repo@ref>`
- `codex plugin install marketplace:<name> --marketplace <path>`
- `codex plugin list`
- `codex plugin enable <name> [--scope user|project]`
- `codex plugin disable <name> [--scope user|project]`
- `codex plugin policy set <name> --allow-hooks true --allow-scripts true`

### Sources

- Local directory path.
- HTTP/HTTPS URL to a zip archive.
- GitHub repo (`github:owner/repo@ref`).
- Optional marketplace index (JSON). If the index is not configured, name-based installs are
  disabled but direct path/URL installs still work.

## Structure

- Manifest: `.claude-plugin/plugin.json` is preferred, otherwise `plugin.json` at the root.
- Component roots: `commands`, `skills`, `rules`, `contexts`, `hooks`, `agents`, `mcp-configs`.
- If a component path is not specified in the manifest, the default directory name is used when it
  exists.

## Compliance and policy

- Install validates manifest paths, blocks path traversal and symlink escapes.
- Hooks and `scripts/` are detected but **disabled by default**. Use `codex plugin policy set` to
  allow them.
- Missing license info is a warning; install still succeeds.
- Plugin MCP configs are discovered but default to `enabled = false` until the user enables them in
  config.

## Registries

- User scope: `~/.codex/plugins/installed_plugins.json`
- Project scope: `.codex/plugins/installed_plugins.json`
