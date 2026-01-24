# Agent Protocol

This protocol defines how each parallel agent should work and report progress.

## Required behaviors

1. Read the task assigned by the launcher.
2. Write progress to the report file on every meaningful update.
3. Poll the inbox file for new commands and acknowledge each command in the report.
4. Do not edit files outside the assigned scope.
5. Stop when the task is complete and write a final summary.

## Report format

When you complete a command, append a line like:

- Ack: Command 003

When you finish the task, include:

- Status: done
- Summary: <short summary>

## Inbox format

Inbox items arrive as headings like:

## Command 003 (YYYY-MM-DD HH:MM)
- Task details...
