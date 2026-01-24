# Agents Config

This file documents the JSON schema for scripts/launch_agents.py.

## Script options

- --config PATH: Path to the JSON config file.
- --no-window: Skip opening terminal windows.
- --dry-run: Preview actions without creating worktrees or report files.
- --confirm: Require interactive confirmation before creating worktrees or windows.
- --yes: Skip confirmation prompts when creating multiple agents.
- --skip-command-check: Skip executable checks for command[0].

## Top-level fields

- root (string, optional): Repo root. Default is the current working directory.
- worktrees_dir (string, optional): Relative to root. Default is ".worktrees".
- reports_dir (string, optional): Relative to root. Default is "reports".
- inboxes_dir (string, optional): Relative to root. Default is the same as reports_dir.
- protocol_path (string, optional): Path to the agent protocol file. Defaults to the skill's references/agent-protocol.md.
- base_ref (string, optional): Git ref used when creating new worktrees. Default is "HEAD".
- terminal (string, optional): Terminal launcher preference.
- window_mode (string, optional): Windows Terminal only. "window" (default), "tab", or "pane".
  - Windows: "auto" (default), "wt", or "cmd".
  - macOS: "auto" (default) or "terminal".
  - Linux: "auto" (default), "gnome-terminal", "konsole", "xfce4-terminal",
    "mate-terminal", "tilix", "alacritty", "kitty", "xterm", "x-terminal-emulator".
- codex_args (array of strings, optional): Default args appended to Codex commands.
- agents (array, required): List of agent definitions.

## Agent fields

- name (string, required): Display name.
- task (string, optional): Short task description.
- tool (string, optional): Set to "codex" to build a default command when command is missing.
- command (array of strings, optional): Explicit command to run. Use placeholders.
- codex_args (array of strings, optional): Extra args appended to Codex for this agent.
- worktree (string, optional): Worktree path (relative to root unless absolute).
- branch (string, optional): Branch name for the worktree. Default is "agent/<slug>".
- report (string, optional): Report path (relative to root unless absolute).
- inbox (string, optional): Inbox path (relative to root unless absolute).

## Placeholders

Replace in command strings:

- {ROOT}
- {WORKTREE}
- {REPORT}
- {INBOX}
- {PROTOCOL_PATH}
- {PROTOCOL_TEXT}
- {TASK}
- {NAME}

## Example: all Codex

```json
{
  "worktrees_dir": ".worktrees",
  "reports_dir": "reports",
  "agents": [
    {
      "name": "agent-a",
      "tool": "codex",
      "task": "Audit the new network-proxy crate and summarize risks",
      "codex_args": ["-a", "auto-edit"]
    },
    {
      "name": "agent-b",
      "tool": "codex",
      "task": "Scan for updated docs that need refresh",
      "codex_args": ["-a", "auto-edit"]
    }
  ]
}
```

## Prepare helper

Create a config with an interactive loop:

```bash
python scripts/prepare_agents.py --output agents.json
```

Generate a quick example:

```bash
python scripts/prepare_agents.py --output agents.json --example --count 3 --overwrite
```

## Example: mixed tools

```json
{
  "terminal": "wt",
  "agents": [
    {
      "name": "codex-impl",
      "command": [
        "codex",
        "-a",
        "auto-edit",
        "Task: {TASK}\nWrite progress to {REPORT}.\nStop when done."
      ],
      "task": "Implement logging updates",
      "report": "reports/agent-codex.md"
    },
    {
      "name": "other-ai",
      "command": [
        "other-ai-cli",
        "--prompt",
        "Task: {TASK}.\nFollow this protocol:\n{PROTOCOL_TEXT}\nWrite progress to {REPORT}.\nCheck {INBOX}."
      ],
      "task": "Draft release notes",
      "report": "reports/agent-other.md"
    }
  ]
}
```

## Dispatch helper

Append a command to all inboxes:

```bash
python scripts/dispatch_inbox.py --config agents.json --all --message "Review recent changes"
```

Append a command to one agent:

```bash
python scripts/dispatch_inbox.py --config agents.json --agent agent-a --message "Focus on tests"
```
