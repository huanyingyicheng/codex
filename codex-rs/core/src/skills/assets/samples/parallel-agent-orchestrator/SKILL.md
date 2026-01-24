---
name: parallel-agent-orchestrator
description: Orchestrate parallel development by launching multiple Codex or other AI CLI instances in new terminal windows, each with its own git worktree, and collecting progress in reports/agent-*.md for a final merge. Use when the user requests multi-agent parallel coding, research, or refactoring in this repository.
---

# Parallel Agent Orchestrator

## Overview

Launch multiple AI agents in parallel with isolated git worktrees, visible terminal windows, and a shared reporting convention.

## Workflow

1. Confirm tasks, agent count, and tool mix (Codex and/or other CLIs).
2. Create an agents config JSON (use scripts/prepare_agents.py or edit by hand).
3. Run scripts/launch_agents.py --dry-run to review the plan.
4. Run scripts/launch_agents.py to create worktrees, seed report/inbox files, and open new terminal windows per agent.
5. Monitor progress in reports/agent-*.md and inboxes.
6. Merge changes back into the main worktree after review.

## Guidance

- Use one worktree per agent to avoid conflicts.
- In each agent prompt, instruct the agent to write updates to its report file.
- In each agent prompt, instruct the agent to poll its inbox file and acknowledge commands.
- For multiple agents, require confirmation prompts before launching.
- Keep approval and sandbox settings at the user's existing Codex defaults unless asked to override.
- Prefer small, independent tasks so merges stay simple.
- Require each agent to stop after writing a final summary.
- Use terminal preferences per OS; see references/agents-config.md for options.

## Failure handling

- If a worktree already exists, reuse it or remove it with git worktree remove when done.
- If window launch fails, rerun with --no-window and open a terminal manually.
- Use --dry-run to preview planned actions without creating worktrees.

## Resources

- scripts/prepare_agents.py: Interactive config generator with confirmation loop.
- scripts/launch_agents.py: Create worktrees and launch new terminal windows.
- scripts/dispatch_inbox.py: Append new commands to agent inbox files.
- references/agents-config.md: Config schema and examples.
