---
name: cleanup-worktrees
model: sonnet
effort: low
description: Remove every git worktree whose PR is merged or closed, then list the remaining worktrees and what each is for. Use when the user wants to clean up, prune, or tidy worktrees. Triggers "clean up worktrees", "remove merged worktrees", "prune old worktrees", "what worktrees do I have".
---

# Clean Up Worktrees

A script does all the work. Your job is to pick the repo, run it, and relay the
result.

```bash
~/.claude/skills/cleanup-worktrees/cleanup-worktrees.sh find-repos [DIR]
~/.claude/skills/cleanup-worktrees/cleanup-worktrees.sh run [--repo PATH] [--dry-run] [--json]
```

## Steps

1. **Pick the repo.**
   - If `git rev-parse --show-toplevel` succeeds, you are in a repo (or one of
     its worktrees): skip to step 2 with no `--repo`.
   - Otherwise run `find-repos` (scans the cwd one level deep) and ask the user
     which one with `AskUserQuestion`, even if there is only one candidate.
     Pass the answer as `--repo PATH`. If there are no candidates, tell the
     user and stop.
2. **Run it.** `run --json` (add `--repo PATH` when needed). It deletes
   straight away, with no confirmation step. Use `--dry-run` only if the user
   asks for a preview.
3. **Report** from the JSON:
   - **Removed**: path, branch, PR, and the tip SHA. Say once that removal was
     forced, so uncommitted files and unpushed commits are gone, and a branch
     can be recovered with `git branch <name> <tip>`.
   - **Errors**: anything with `action: "error"` or a non-null `error`.
   - **Remaining worktrees**: a table of path, branch, PR (number, title,
     state), clean/dirty, commits ahead, Jira link, and why it was kept. Add
     one line per worktree saying what it appears to be for, from the PR title
     and branch name. End by asking whether the user wants any of these removed.
   - If a kept worktree says it is pinned in the live stack, print the revert
     step from `redeploy-worktree` (remove the path from `COMPOSE_FILE` in
     `/opt/icetana/scripts/.env`, then `docker compose up -d --force-recreate
     <services>`). Do not run it.
   - If the kept reason is "PR MERGED, but you're in it", tell the user to
     `cd` out of that worktree and rerun.

## What the script does

A worktree is removed only when its newest PR (`gh pr list --head <branch>
--state all`) is MERGED or CLOSED. For each one it kills processes whose cwd
is inside the worktree (such as vite dev servers), runs
`git worktree remove --force --force` (this discards dirty files and unpushed
commits, subsuming `git reset --hard && git clean -fd`), then `git branch -D`.
Remote branches are left alone.

Always kept, with the reason listed: the primary clone, the worktree you are in,
worktrees on the default branch, detached HEADs, open or draft PRs, branches
with no PR, PR lookup failures, and worktrees pinned in
`/opt/icetana/scripts/.env` `COMPOSE_FILE`.

Environment overrides: `JIRA_BASE_URL` (default
`https://icetana.atlassian.net/browse`), `ICETANA_STACK_DIR` (default
`/opt/icetana/scripts`).
