# Skill Frontmatter Fix

## Decisions
- Add YAML frontmatter with name/description to missing skills.
- Use a PowerShell script with try/catch and path pre-checks.

## Changes
- Added YAML frontmatter to:
  - C:\Users\Asus\.codex\skills\everything-claude-code\project-guidelines-example\SKILL.md
  - C:\Users\Asus\.codex\skills\everything-claude-code\verification-loop\SKILL.md

## Verification
- Ran:
  - Get-Content -TotalCount 5 "C:\Users\Asus\.codex\skills\everything-claude-code\project-guidelines-example\SKILL.md"
  - Get-Content -TotalCount 5 "C:\Users\Asus\.codex\skills\everything-claude-code\verification-loop\SKILL.md"
- Result: frontmatter headers present at file start.
