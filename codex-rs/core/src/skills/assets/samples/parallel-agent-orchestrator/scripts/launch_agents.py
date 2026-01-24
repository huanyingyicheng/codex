#!/usr/bin/env python3
"""
Launch parallel agent terminals with isolated git worktrees.

Usage:
  launch_agents.py --config path/to/agents.json [--no-window]
"""

from __future__ import annotations

import argparse
import json
import re
import shlex
import subprocess
import sys
from datetime import datetime
from pathlib import Path
from shutil import which

WINDOWS = sys.platform.startswith("win")
MACOS = sys.platform == "darwin"
LINUX = sys.platform.startswith("linux")


def die(message: str) -> None:
    raise SystemExit(f"[ERROR] {message}")


def load_config(path: Path) -> dict:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except FileNotFoundError:
        die(f"Config not found: {path}")
    except json.JSONDecodeError as exc:
        die(f"Invalid JSON in config: {exc}")


def ensure_git_repo(root: Path) -> None:
    result = subprocess.run(
        ["git", "-C", str(root), "rev-parse", "--git-dir"],
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        die(f"Not a git repo: {root}")


def slugify(value: str) -> str:
    slug = re.sub(r"[^A-Za-z0-9_-]+", "-", value.strip()).strip("-")
    return slug.lower() or "agent"


def resolve_path(root: Path, raw: str | None) -> Path | None:
    if raw is None:
        return None
    path = Path(raw)
    if not path.is_absolute():
        path = root / path
    return path


def branch_exists(root: Path, branch: str) -> bool:
    result = subprocess.run(
        ["git", "-C", str(root), "show-ref", "--verify", f"refs/heads/{branch}"],
        capture_output=True,
        text=True,
    )
    return result.returncode == 0


def run_checked(cmd: list[str], cwd: Path | None = None) -> None:
    result = subprocess.run(cmd, cwd=str(cwd) if cwd else None)
    if result.returncode != 0:
        die(f"Command failed: {' '.join(cmd)}")


def ensure_worktree(root: Path, worktree: Path, branch: str, base_ref: str) -> None:
    if worktree.exists():
        return
    worktree.parent.mkdir(parents=True, exist_ok=True)
    if branch_exists(root, branch):
        run_checked(["git", "-C", str(root), "worktree", "add", str(worktree), branch])
        return
    run_checked(
        [
            "git",
            "-C",
            str(root),
            "worktree",
            "add",
            "-b",
            branch,
            str(worktree),
            base_ref,
        ]
    )


def apply_placeholders(value: str, mapping: dict[str, str]) -> str:
    updated = value
    for key, replacement in mapping.items():
        updated = updated.replace(key, replacement)
    return updated


def build_shell_command(worktree: Path, command: list[str]) -> str:
    quoted_dir = shlex.quote(str(worktree))
    return f"cd {quoted_dir} && {shlex.join(command)}"


def applescript_escape(value: str) -> str:
    return value.replace("\\", "\\\\").replace("\"", "\\\"")


def build_command(
    agent: dict,
    defaults: dict,
    mapping: dict[str, str],
) -> list[str]:
    command = agent.get("command")
    if command is None:
        tool = agent.get("tool")
        task = agent.get("task")
        if tool not in (None, "codex"):
            die(f"Agent '{agent.get('name')}' is missing command")
        if not task:
            die(f"Agent '{agent.get('name')}' is missing task for codex")
        codex_args = list(defaults.get("codex_args", []))
        codex_args.extend(agent.get("codex_args", []))
        prompt = "\n".join(
            [
                f"Task: {task}",
                f"Write progress to {mapping['{REPORT}']}.",
                f"Check {mapping['{INBOX}']} for new commands.",
                "Stop when done.",
            ]
        )
        command = ["codex", *codex_args, prompt]

    if isinstance(command, str):
        die("command must be an array of strings, not a single string")
    if not isinstance(command, list) or not all(isinstance(item, str) for item in command):
        die("command must be an array of strings")

    return [apply_placeholders(item, mapping) for item in command]


def write_report_stub(report_path: Path, name: str, task: str | None) -> None:
    if report_path.exists():
        return
    report_path.parent.mkdir(parents=True, exist_ok=True)
    lines = [f"# Agent: {name}", ""]
    if task:
        lines.extend([f"Task: {task}", ""])
    lines.extend(["## Progress", "", "- "])
    report_path.write_text("\n".join(lines), encoding="utf-8")


def write_inbox_stub(inbox_path: Path, name: str) -> None:
    if inbox_path.exists():
        return
    inbox_path.parent.mkdir(parents=True, exist_ok=True)
    timestamp = datetime.now().strftime("%Y-%m-%d %H:%M")
    lines = [
        f"# Inbox: {name}",
        "",
        f"## Command 001 ({timestamp})",
        "- Replace this line with your instruction.",
        "",
    ]
    inbox_path.write_text("\n".join(lines), encoding="utf-8")


def launch_window(
    command: list[str],
    worktree: Path,
    terminal: str,
) -> bool:
    if not WINDOWS:
        if MACOS:
            return launch_macos_window(command, worktree, terminal)
        if LINUX:
            return launch_linux_window(command, worktree, terminal)
        return False

    use_wt = terminal in ("auto", "wt") and which("wt")
    if terminal == "wt" and not use_wt:
        die("terminal is set to 'wt' but wt was not found on PATH")

    if use_wt:
        subprocess.Popen(["wt", "-d", str(worktree), "--", *command])
        return True

    cmd_line = subprocess.list2cmdline(command)
    subprocess.Popen(
        ["cmd.exe", "/k", cmd_line],
        cwd=str(worktree),
        creationflags=subprocess.CREATE_NEW_CONSOLE,
    )
    return True


def launch_macos_window(command: list[str], worktree: Path, terminal: str) -> bool:
    if terminal not in ("auto", "terminal"):
        die("terminal must be 'auto' or 'terminal' on macOS")
    if not which("osascript"):
        die("osascript not found on PATH")

    shell_command = build_shell_command(worktree, command)
    script = (
        'tell application "Terminal" to do script "'
        + applescript_escape(shell_command)
        + '"'
    )
    subprocess.Popen(["osascript", "-e", script])
    return True


def launch_linux_window(command: list[str], worktree: Path, terminal: str) -> bool:
    terminals: list[tuple[str, list[str], str]] = [
        ("gnome-terminal", ["--working-directory", str(worktree), "--"], "argv"),
        ("konsole", ["--workdir", str(worktree), "-e"], "argv"),
        ("xfce4-terminal", ["--working-directory", str(worktree), "--command"], "shell"),
        ("mate-terminal", ["--working-directory", str(worktree), "--"], "argv"),
        ("tilix", ["--working-directory", str(worktree), "-e"], "argv"),
        ("alacritty", ["--working-directory", str(worktree), "-e"], "argv"),
        ("kitty", ["--directory", str(worktree), "--"], "argv"),
        ("xterm", ["-e"], "shell"),
        ("x-terminal-emulator", ["-e"], "shell"),
    ]

    if terminal != "auto":
        names = [name for name, _, _ in terminals]
        if terminal not in names:
            die("terminal is not supported on Linux")
        terminals = [item for item in terminals if item[0] == terminal]

    for name, base_args, mode in terminals:
        if not which(name):
            continue
        if mode == "argv":
            subprocess.Popen([name, *base_args, *command])
            return True
        shell_command = build_shell_command(worktree, command)
        if mode == "shell":
            if name in ("xterm", "x-terminal-emulator"):
                subprocess.Popen([name, *base_args, "sh", "-lc", shell_command])
                return True
            subprocess.Popen([name, *base_args, f"sh -lc {shlex.quote(shell_command)}"])
            return True

    if terminal != "auto":
        die(f"terminal '{terminal}' not found on PATH")
    return False


def require_confirmation(agent_count: int, force: bool) -> None:
    if agent_count <= 1 and not force:
        return
    response = input(
        f"Confirm {agent_count} agent(s). Type {agent_count} to continue: "
    ).strip()
    if response != str(agent_count):
        die("Confirmation failed")
    response = input("Type 'launch' to proceed: ").strip().lower()
    if response != "launch":
        die("Confirmation failed")


def apply_changes(
    root: Path,
    reports_root: Path,
    inboxes_root: Path,
    base_ref: str,
    prepared: list[tuple[str, Path, Path, Path, str, str | None, list[str]]],
    terminal: str,
    no_window: bool,
) -> None:
    reports_root.mkdir(parents=True, exist_ok=True)
    inboxes_root.mkdir(parents=True, exist_ok=True)

    for name, worktree, report, inbox, branch, task, command in prepared:
        ensure_worktree(root, worktree, branch, base_ref)
        write_report_stub(report, name, task)
        write_inbox_stub(inbox, name)
        if no_window:
            continue
        if not launch_window(command, worktree, terminal):
            print(f"- {name}")
            print("  note: window launch not supported on this OS")


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Launch parallel agent terminals with git worktrees.",
    )
    parser.add_argument("--config", required=True, help="Path to agents.json")
    parser.add_argument(
        "--no-window",
        action="store_true",
        help="Do not open new terminal windows; only prepare worktrees and print commands",
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="Print planned actions without creating worktrees or reports",
    )
    parser.add_argument(
        "--confirm",
        action="store_true",
        help="Require interactive confirmation before creating worktrees or windows",
    )
    parser.add_argument(
        "--yes",
        action="store_true",
        help="Skip confirmation prompts when creating multiple agents",
    )
    args = parser.parse_args()

    config_path = Path(args.config).resolve()
    config = load_config(config_path)

    root = resolve_path(Path.cwd(), config.get("root")) or Path.cwd()
    root = root.resolve()
    ensure_git_repo(root)

    agents = config.get("agents")
    if not isinstance(agents, list) or not agents:
        die("config must include a non-empty agents array")

    worktrees_dir = config.get("worktrees_dir", ".worktrees")
    reports_dir = config.get("reports_dir", "reports")
    inboxes_dir = config.get("inboxes_dir", reports_dir)
    base_ref = config.get("base_ref", "HEAD")
    terminal = config.get("terminal", "auto")

    defaults = {
        "codex_args": config.get("codex_args", []),
    }

    reports_root = resolve_path(root, reports_dir) or (root / "reports")
    inboxes_root = resolve_path(root, inboxes_dir) or (root / inboxes_dir)

    prepared = []

    for agent in agents:
        if not isinstance(agent, dict):
            die("Each agent must be an object")
        name = agent.get("name")
        if not name:
            die("Each agent must include a name")

        slug = slugify(name)
        task = agent.get("task")
        branch = agent.get("branch", f"agent/{slug}")
        worktree = resolve_path(root, agent.get("worktree"))
        if worktree is None:
            worktree = resolve_path(root, f"{worktrees_dir}/{slug}")
        report = resolve_path(root, agent.get("report"))
        if report is None:
            report = reports_root / f"agent-{slug}.md"
        inbox = resolve_path(root, agent.get("inbox"))
        if inbox is None:
            inbox = inboxes_root / f"agent-{slug}.inbox.md"

        mapping = {
            "{ROOT}": str(root),
            "{WORKTREE}": str(worktree),
            "{REPORT}": str(report),
            "{INBOX}": str(inbox),
            "{TASK}": task or "",
            "{NAME}": name,
        }

        command = build_command(agent, defaults, mapping)
        prepared.append((name, worktree, report, inbox, branch, task, command))

    print(f"Prepared {len(prepared)} agent(s).")

    for name, worktree, report, inbox, _branch, _task, command in prepared:
        print(f"- {name}")
        print(f"  worktree: {worktree}")
        print(f"  report:   {report}")
        print(f"  inbox:    {inbox}")
        print(f"  command:  {' '.join(command)}")

    if args.dry_run:
        print("\nDry run: no worktrees or reports were created.")
        return

    if not args.yes:
        require_confirmation(len(prepared), args.confirm)

    apply_changes(
        root,
        reports_root,
        inboxes_root,
        base_ref,
        prepared,
        terminal,
        args.no_window,
    )

    if not WINDOWS and not args.no_window:
        print("\nTip: use --no-window and open terminals manually on this OS.")


if __name__ == "__main__":
    main()
