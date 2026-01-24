#!/usr/bin/env python3
"""
Interactive helper to create an agents config file.

Usage:
  prepare_agents.py --output path/to/agents.json [--overwrite]
  prepare_agents.py --output path/to/agents.json --example [--count 2]
"""

from __future__ import annotations

import argparse
import json
import shlex
import sys
from pathlib import Path

WINDOWS = sys.platform.startswith("win")


def die(message: str) -> None:
    raise SystemExit(f"[ERROR] {message}")


def prompt_line(label: str) -> str:
    return input(label).strip()


def prompt_non_empty(label: str) -> str:
    while True:
        value = prompt_line(label)
        if value:
            return value
        print("Value is required.")


def prompt_yes_no(label: str, default: bool = True) -> bool:
    suffix = " [Y/n] " if default else " [y/N] "
    while True:
        value = prompt_line(label + suffix).lower()
        if not value:
            return default
        if value in ("y", "yes"):
            return True
        if value in ("n", "no"):
            return False
        print("Please enter yes or no.")


def prompt_optional_list(label: str) -> list[str]:
    value = prompt_line(label + " (optional): ")
    if not value:
        return []
    return shlex.split(value, posix=not WINDOWS)


def parse_command(label: str) -> list[str]:
    while True:
        value = prompt_line(label)
        if not value:
            print("Command is required.")
            continue
        return shlex.split(value, posix=not WINDOWS)


def build_agent(index: int) -> dict:
    print(f"\nAgent {index}")
    name = prompt_non_empty("Name: ")
    task = prompt_line("Task (optional): ")
    agent: dict = {"name": name}

    use_custom = prompt_yes_no("Use custom command?", default=False)
    if use_custom:
        agent["command"] = parse_command("Command line: ")
        if task:
            agent["task"] = task
        return agent

    agent["tool"] = "codex"
    if task:
        agent["task"] = task
    codex_args = prompt_optional_list("Extra codex args")
    if codex_args:
        agent["codex_args"] = codex_args
    return agent


def collect_config() -> dict:
    agent_count = prompt_non_empty("Number of agents: ")
    if not agent_count.isdigit() or int(agent_count) < 1:
        die("Number of agents must be a positive integer.")

    agents = [build_agent(i + 1) for i in range(int(agent_count))]
    config = {"agents": agents}

    terminal = prompt_line("Terminal preference (blank for auto): ")
    if terminal:
        config["terminal"] = terminal

    return config


def write_config(path: Path, config: dict, overwrite: bool) -> None:
    if path.exists() and not overwrite:
        if not prompt_yes_no(f"{path} exists. Overwrite?", default=False):
            die("Aborted by user.")

    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(config, indent=2), encoding="utf-8")
    print(f"Written {path}")


def example_config(count: int) -> dict:
    agents = []
    for index in range(count):
        name = f"agent-{index + 1}"
        agents.append(
            {
                "name": name,
                "tool": "codex",
                "task": f"Task for {name}",
            }
        )
    return {"agents": agents}


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Interactive helper to create an agents config file.",
    )
    parser.add_argument("--output", required=True, help="Path to agents.json")
    parser.add_argument("--overwrite", action="store_true", help="Overwrite if file exists")
    parser.add_argument(
        "--example",
        action="store_true",
        help="Write an example config instead of prompting",
    )
    parser.add_argument("--count", type=int, default=2, help="Example agent count")
    args = parser.parse_args()

    output_path = Path(args.output).resolve()

    if args.example:
        config = example_config(max(1, args.count))
        write_config(output_path, config, args.overwrite)
        return

    while True:
        config = collect_config()
        print("\nConfig preview:\n")
        print(json.dumps(config, indent=2))
        choice = prompt_line("\nType 'confirm' to write, 'redo' to edit, or 'quit' to exit: ")
        if choice == "confirm":
            write_config(output_path, config, args.overwrite)
            return
        if choice == "redo":
            continue
        if choice == "quit":
            die("Aborted by user.")
        print("Unknown option.")


if __name__ == "__main__":
    main()
