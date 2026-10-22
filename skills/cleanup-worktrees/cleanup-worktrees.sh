#!/usr/bin/env bash
#
# cleanup-worktrees.sh — remove git worktrees whose PR is merged or closed.
#
# Usage:
#   cleanup-worktrees.sh find-repos [DIR]                    # repos one level under DIR (default $PWD)
#   cleanup-worktrees.sh run [--repo PATH] [--dry-run] [--json]
#
# `run` removes every worktree whose newest PR is MERGED or CLOSED: kills
# processes living in it, force-removes it (dirty files and unpushed commits
# are discarded) and deletes its local branch. Everything else is kept and
# listed with the reason. Never touched: the primary clone, the worktree you
# are standing in, worktrees on the default branch, detached HEADs, branches
# with no PR or an open PR, and worktrees pinned in the live compose stack.
set -euo pipefail

JIRA_BASE="${JIRA_BASE_URL:-https://icetana.atlassian.net/browse}"
STACK_DIR="${ICETANA_STACK_DIR:-/opt/icetana/scripts}"

die() { echo "error: $*" >&2; exit 1; }

cmd_find_repos() {
  local dir="${1:-$PWD}"
  dir="$(cd "$dir" && pwd -P)"
  declare -A seen=()
  local d common n
  for d in "$dir" "$dir"/*/; do
    d="${d%/}"
    # A real clone has a .git *directory*; worktrees have a .git file.
    [ -d "$d/.git" ] || continue
    common="$(git -C "$d" rev-parse --path-format=absolute --git-common-dir 2>/dev/null)" || continue
    [ -z "${seen[$common]:-}" ] || continue
    seen[$common]=1
    n="$(git -C "$d" worktree list --porcelain | grep -c '^worktree ')"
    printf '%s\t%s worktrees\n' "$d" "$n"
  done
}

jira_key() {
  local m
  m="$(grep -oE '[A-Za-z]{2,10}-?[0-9]{2,6}' <<<"$1" | head -1 || true)"
  [ -n "$m" ] || return 0
  sed -E 's/^([A-Za-z]+)-?([0-9]+)$/\U\1-\2/' <<<"$m"
}

# Prints pids (excluding this script and its ancestors) whose cwd is inside $1.
pids_in() {
  local wt="$1" skip=" " p c
  p=$$
  while [ "$p" -gt 1 ]; do
    skip+="$p "
    p="$(awk '/^PPid:/{print $2}' "/proc/$p/status" 2>/dev/null || echo 1)"
  done
  for c in /proc/[0-9]*/cwd; do
    p="${c#/proc/}"; p="${p%/cwd}"
    case "$skip" in *" $p "*) continue ;; esac
    c="$(readlink "/proc/$p/cwd" 2>/dev/null || true)"
    case "$c" in "$wt" | "$wt"/*) echo "$p" ;; esac
  done
}

cmd_run() {
  local repo="" dry=0 json=0
  while [ $# -gt 0 ]; do
    case "$1" in
      --repo) repo="${2:?--repo needs a path}"; shift 2 ;;
      --dry-run) dry=1; shift ;;
      --json) json=1; shift ;;
      *) die "unknown option: $1" ;;
    esac
  done
  command -v gh >/dev/null || die "gh not installed"
  command -v jq >/dev/null || die "jq not installed"

  local cwd_top=""
  cwd_top="$(git rev-parse --show-toplevel 2>/dev/null || true)"
  [ -n "$repo" ] || repo="${cwd_top:-}"
  [ -n "$repo" ] || die "not in a git repo; pass --repo PATH (see find-repos)"
  cd "$repo"

  # Worktree list: first entry is always the primary clone.
  local -a W_PATH=() W_BRANCH=() W_HEAD=()
  local line p="" b="" h=""
  while IFS= read -r line || [ -n "$line" ]; do
    case "$line" in
      "worktree "*) p="${line#worktree }"; b=""; h="" ;;
      "HEAD "*) h="${line#HEAD }" ;;
      "branch "*) b="${line#branch refs/heads/}" ;;
      "bare") p="" ;;
      "")
        if [ -n "$p" ]; then W_PATH+=("$p"); W_BRANCH+=("$b"); W_HEAD+=("$h"); fi
        p="" ;;
    esac
  done < <(git worktree list --porcelain; echo)
  [ "${#W_PATH[@]}" -gt 0 ] || die "no worktrees found in $repo"
  local primary="${W_PATH[0]}"

  local default
  default="$(git symbolic-ref --short refs/remotes/origin/HEAD 2>/dev/null | sed 's|^origin/||' || true)"
  if [ -z "$default" ]; then
    for default in main master; do git show-ref -q "refs/heads/$default" && break; default=""; done
  fi

  local compose=""
  [ -f "$STACK_DIR/.env" ] && compose="$(grep -E '^COMPOSE_FILE=' "$STACK_DIR/.env" | tail -1 | cut -d= -f2- || true)"

  local out
  out="$(mktemp)"
  trap "rm -f '$out'" EXIT

  local i path branch head
  for i in "${!W_PATH[@]}"; do
    path="${W_PATH[$i]}"; branch="${W_BRANCH[$i]}"; head="${W_HEAD[$i]}"
    local action="kept" reason="" pr_json="null" dirty="" ahead="" jira="" pr_state=""

    if [ ! -d "$path" ]; then
      action="kept"; reason="directory missing (run: git worktree prune)"
    else
      [ -z "$(git -C "$path" status --porcelain 2>/dev/null | head -1)" ] && dirty=false || dirty=true
      if [ -n "$default" ] && git show-ref -q "refs/remotes/origin/$default"; then
        ahead="$(git -C "$path" rev-list --count "origin/$default..HEAD" 2>/dev/null || echo "")"
      fi
    fi
    [ -z "$branch" ] || jira="$(jira_key "$branch")"

    if [ "$i" -eq 0 ]; then
      reason="primary clone"
    elif [ -n "$cwd_top" ] && [ "$(realpath "$path")" = "$(realpath "$cwd_top")" ]; then
      reason="current worktree"
    elif [ -z "$branch" ]; then
      reason="detached HEAD"
    elif [ "$branch" = "$default" ]; then
      reason="on default branch ($default)"
    fi

    # PR lookup (skipped for protected worktrees only to save API calls; still
    # done for the current worktree so we can say "merged, but you're in it").
    if [ -n "$branch" ] && [ "$reason" != "primary clone" ] && [ "$branch" != "$default" ] && [ -d "$path" ]; then
      if prs="$(gh pr list --head "$branch" --state all --limit 20 \
            --json number,title,state,url,isDraft,createdAt 2>&1)"; then
        pr_json="$(jq 'sort_by(.createdAt) | last // null' <<<"$prs")"
        pr_state="$(jq -r '.state // "NONE"' <<<"$pr_json")"
      else
        pr_state="ERROR"; reason="${reason:-PR lookup failed: ${prs%%$'\n'*}}"
      fi
    fi

    if [ -z "$reason" ]; then
      case "$pr_state" in
        MERGED | CLOSED)
          if [ -n "$compose" ] && [[ "$compose" == *"$path"* ]]; then
            reason="pinned in live stack ($STACK_DIR/.env COMPOSE_FILE); revert pin first (see redeploy-worktree)"
          else
            action="remove"
          fi ;;
        OPEN) reason="open PR" ;;
        NONE) reason="no PR" ;;
        *) reason="unknown PR state: $pr_state" ;;
      esac
    elif [ "$reason" = "current worktree" ] && [[ "$pr_state" == MERGED || "$pr_state" == CLOSED ]]; then
      reason="PR $pr_state, but you're in it; cd elsewhere and rerun"
    fi

    local killed="" tip="" err=""
    if [ "$action" = "remove" ]; then
      tip="$(git -C "$path" rev-parse HEAD 2>/dev/null || echo "$head")"
      if [ "$dry" -eq 1 ]; then
        action="would-remove"
      else
        # Kill anything living in the worktree, then force-remove it. The
        # reset/clean pair the user asked for is subsumed by --force removal,
        # which discards the whole directory in one step.
        killed="$(pids_in "$path" | tr '\n' ' ')"
        if [ -n "$killed" ]; then
          # shellcheck disable=SC2086
          kill $killed 2>/dev/null || true
          sleep 1
          # shellcheck disable=SC2086
          kill -9 $killed 2>/dev/null || true
        fi
        if git worktree remove --force --force "$path" 2>/dev/null; then
          action="removed"
        else
          rm -rf "$path" && git worktree prune && action="removed" || { action="error"; err="could not remove $path"; }
        fi
        if [ "$action" = "removed" ]; then
          git branch -D "$branch" >/dev/null 2>&1 || err="worktree removed but branch '$branch' not deleted"
        fi
      fi
    fi

    jq -cn --arg action "$action" --arg path "$path" --arg branch "$branch" \
      --arg reason "$reason" --arg err "$err" --arg tip "$tip" --arg killed "$killed" \
      --arg dirty "$dirty" --arg ahead "$ahead" --arg jira "$jira" --arg base "$JIRA_BASE" \
      --argjson pr "$pr_json" '
      {action:$action, path:$path, branch:(if $branch=="" then null else $branch end),
       pr:$pr, dirty:(if $dirty=="" then null else ($dirty=="true") end),
       ahead:(if $ahead=="" then null else ($ahead|tonumber) end),
       jira:(if $jira=="" then null else {key:$jira, url:($base+"/"+$jira)} end),
       reason:(if $reason=="" then null else $reason end),
       error:(if $err=="" then null else $err end),
       tip:(if $tip=="" then null else $tip end),
       killed_pids:(if $killed=="" then [] else ($killed|split(" ")|map(select(.!="")|tonumber)) end)}' >>"$out"
  done

  [ "$dry" -eq 1 ] || git worktree prune

  if [ "$json" -eq 1 ]; then
    jq -s --arg repo "$primary" --argjson dry "$dry" '{repo:$repo, dry_run:($dry==1), worktrees:.}' "$out"
    return
  fi

  echo "Repo: $primary"
  [ "$dry" -eq 0 ] || echo "(dry run: nothing was changed)"
  echo
  echo "== Removed =="
  jq -r 'select(.action=="removed" or .action=="would-remove")
    | "\(.action)\t\(.path)\t\(.branch)\tPR #\(.pr.number) \(.pr.state)\ttip \(.tip[0:10])\(if .error then "  ! "+.error else "" end)"' "$out" \
    | column -t -s $'\t' || true
  jq -e -s 'map(select(.action=="removed" or .action=="would-remove"))|length>0' "$out" >/dev/null || echo "(none)"
  echo "  (recover a removed branch: git branch <name> <tip>)"
  echo
  echo "== Errors =="
  jq -r 'select(.action=="error") | "\(.path): \(.error)"' "$out"
  echo
  echo "== Remaining worktrees =="
  jq -r 'select(.action=="kept") |
    [.path, (.branch // "(detached)"),
     (if .pr then "#\(.pr.number) \(.pr.state)\(if .pr.isDraft then " draft" else "" end): \(.pr.title)" else "no PR" end),
     (if .dirty==null then "-" elif .dirty then "dirty" else "clean" end),
     (if .ahead==null then "-" else "+\(.ahead)" end),
     (.jira.url // "-"), (.reason // "")] | @tsv' "$out" \
    | { printf 'PATH\tBRANCH\tPR\tTREE\tAHEAD\tJIRA\tKEPT BECAUSE\n'; cat; } | column -t -s $'\t'
}

case "${1:-}" in
  find-repos) shift; cmd_find_repos "$@" ;;
  run) shift; cmd_run "$@" ;;
  *) sed -n '3,15p' "$0"; exit 2 ;;
esac
