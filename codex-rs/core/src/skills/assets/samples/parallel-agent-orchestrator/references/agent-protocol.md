# Agent Protocol

This protocol defines how each parallel agent should work and report progress.

## Required behaviors

1. Read the task assigned by the launcher.
2. Write a short plan (2-5 bullets) in the report before editing.
3. Do not edit files outside the assigned scope.
4. If scope must expand, request approval via inbox and wait.
5. Write progress to the report file on every meaningful update.
6. Poll the inbox file for new commands and acknowledge each command in the report.
7. Keep changes small and localized; avoid unrelated refactors.
8. Track any stated budget (time/tokens/commands). If near the limit, stop and report.
9. Run the minimum required tests, or note why tests were not run.
10. Stop when the task is complete and write a final summary.

## Report format

When you complete a command, append a line like:

- Ack: Command 003

For each update (including the initial plan), include:

- Status: in-progress | blocked | done
- Scope: <paths/modules>
- Budget: <time/tokens/commands; remaining if known>
- Plan: <short bullets or 1-2 lines>
- Changes: <concise list>
- Tests: <commands or "not run: reason">
- Risks: <conflicts/assumptions/unknowns>
- Next: <handoff or question>

When you finish the task, include:

- Status: done
- Summary: <short summary>
- Handoff: <merge notes or follow-ups>

## Inbox format

Inbox items arrive as headings like:

## Command 003 (YYYY-MM-DD HH:MM)
- Task details...
