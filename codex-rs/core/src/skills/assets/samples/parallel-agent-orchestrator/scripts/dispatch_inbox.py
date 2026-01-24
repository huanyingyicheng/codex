#!/usr/bin/env python3
"""
Append commands to agent inbox files.

Usage:
  dispatch_inbox.py --config path/to/agents.json --all --message "..."
  dispatch_inbox.py --config path/to/agents.json --agent name --file note.md
"""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
from datetime import datetime
from pathlib import Path


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


def resolve_inbox_path(
    root: Path,
    inboxes_root: Path,
    agent: dict,
    slug: str,
) -> Path:
    inbox = resolve_path(root, agent.get("inbox"))
    if inbox is None:
        inbox = inboxes_root / f"agent-{slug}.inbox.md"
    return inbox


def read_message(message: str | None, file_path: str | None) -> str:
    if message and file_path:
        die("Use either --message or --file, not both")
    if message:
        return message
    if file_path:
        return Path(file_path).read_text(encoding="utf-8")
    die("Either --message or --file is required")
    raise AssertionError("unreachable")


def append_command(
    inbox_path: Path,
    inbox_name: str,
    message: str,
    command_id: str | None,
    dry_run: bool,
) -> None:
    timestamp = datetime.now().strftime("%Y-%m-%d %H:%M")
    header = f"## Command {command_id} ({timestamp})" if command_id else f"## Command ({timestamp})"
    payload = "\n".join([header, "", message.rstrip("\n"), ""])

    if dry_run:
        return

    inbox_path.parent.mkdir(parents=True, exist_ok=True)
    if not inbox_path.exists():
        inbox_path.write_text(f"# Inbox: {inbox_name}\n\n", encoding="utf-8")
    with inbox_path.open("a", encoding="utf-8") as handle:
        handle.write(payload)


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Append commands to agent inbox files.",
    )
    parser.add_argument("--config", required=True, help="Path to agents.json")
    parser.add_argument("--agent", action="append", default=[], help="Agent name (repeatable)")
    parser.add_argument("--all", action="store_true", help="Target all agents")
    parser.add_argument("--message", help="Command text to append")
    parser.add_argument("--file", help="Path to a file containing the command text")
    parser.add_argument("--id", dest="command_id", help="Command identifier")
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="Preview actions without writing to inbox files",
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

    if not args.all and not args.agent:
        die("Use --all or at least one --agent")

    message = read_message(args.message, args.file)
    reports_dir = config.get("reports_dir", "reports")
    inboxes_dir = config.get("inboxes_dir", reports_dir)
    inboxes_root = resolve_path(root, inboxes_dir) or (root / inboxes_dir)

    targets = []
    for agent in agents:
        if not isinstance(agent, dict):
            die("Each agent must be an object")
        name = agent.get("name")
        if not name:
            die("Each agent must include a name")
        slug = slugify(name)
        targets.append((name, slug, agent))

    if args.all:
        selected = targets
    else:
        wanted = {slugify(name) for name in args.agent}
        selected = [item for item in targets if item[1] in wanted or item[0] in args.agent]
        if not selected:
            die("No matching agents found")

    for name, slug, agent in selected:
        inbox_path = resolve_inbox_path(root, inboxes_root, agent, slug)
        print(f"- {name}")
        print(f"  inbox: {inbox_path}")
        if args.dry_run:
            print("  dry-run: command not written")
        append_command(inbox_path, name, message, args.command_id, args.dry_run)

    if args.dry_run:
        print("\nDry run: no inbox files were modified.")


if __name__ == "__main__":
    main()
