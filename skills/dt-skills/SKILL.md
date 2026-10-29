---
name: dt-skills
description: List, install, enable and disable Claude skills with the `dt skills` CLI. Use when the user asks which skills they have, where a skill lives, or wants to turn a global skill on/off or install a new one.
disable-model-invocation: true
argument-hint: "[list | enable <name> | disable <name> | install <source> [--on|--off]]"
---

# dt-skills

Manage the user's Claude skills through the `dt` CLI. Run the command, then report the result.

- `dt skills` lists every skill with its state (on/off), source and path.
- `dt skills disable <name>` / `dt skills enable <name>` toggle global and synced skills.
- `dt skills install <path|git-url|owner/repo> [--path SUBDIR] [--on|--off] [--force]` installs a skill.

Skills marked `[locked]` belong to a repo or plugin and cannot be toggled with `dt`; tell the user that rather than editing their files.

Always pass `--on` or `--off` to `install` (ask the user if they haven't said), since without them `dt` prompts interactively. Use `--dry-run` first if the user is unsure. Changes take effect in new Claude Code sessions.

User request: $ARGUMENTS
