#!/usr/bin/env bash
set -euo pipefail

AUTHOR_NAME=${AUTHOR_NAME:-"Emma Puls"}
STACK_PATTERN=${STACK_PATTERN:-'^stack/'}
PULL_REQUEST_KEYWORD=${PULL_REQUEST_KEYWORD:-"Pull Request"}
ACLI_MESSAGE=${ACLI_MESSAGE:-"Look at the staged changes in git and provide a commit message that would be appropriate for these changes. The message should be concise but descriptive, ideally starting with a verb in the present tense."}
ACLI_OUTPUT_FILE=${ACLI_OUTPUT_FILE:-"acli_rovodev_output.json"}
ACLI_OUTPUT_SCHEMA=${ACLI_OUTPUT_SCHEMA:-"/Users/epuls/.local/bin/scripts/newScript/acli_schema.json"}
ACLI_CONFIG_FILE=${ACLI_CONFIG_FILE:-"/Users/epuls/.local/bin/scripts/newScript/config.ag-stack.yml"}
STACK_LOG_LIMIT=${STACK_LOG_LIMIT:-10}
VERBOSE=${VERBOSE:-false}
DEFAULT_MESSAGE=${DEFAULT_MESSAGE:-"Update stack commit"}

usage() {
  cat <<'EOF'
Usage: ag-stack-update.sh [-a AUTHOR] [-p BRANCH_PATTERN]

Environment variables:
  AUTHOR_NAME           Override the expected Git author (default: Emma Puls)
  STACK_PATTERN         Regex that matches valid stack branches (default: ^stack/)
  PULL_REQUEST_KEYWORD  Keyword that marks stack commits (default: Pull Request)
  ACLI_MESSAGE          Optional default message passed to acli rovodev
  ACLI_OUTPUT_FILE      Optional default --output-file path for acli rovodev
  ACLI_OUTPUT_SCHEMA    Optional default --output-schema path for acli rovodev
  ACLI_CONFIG_FILE      Optional default --config-file path for acli rovodev
  STACK_LOG_LIMIT       Limit for recent commits scanned when locating stack commits (default: 10)
  VERBOSE               Enable verbose stack detection logs (default: false)
  DEFAULT_MESSAGE       Optional default commit message if acli rovodev is not used
EOF
}

log_info() {
  echo "[INFO] $*"
}

log_verbose() {
  case "${VERBOSE,,}" in
    true|1|yes|on)
      echo "[VERBOSE] $*"
      ;;
  esac
}

prompt_yes_no() {
  local prompt=$1
  local default=${2:-N}
  local reply
  read -rp "$prompt [$default] " reply
  reply=${reply:-$default}
  [[ $reply =~ ^[Yy]$ ]]
}

require_clean_stack_branch() {
  local branch
  branch=$(git rev-parse --abbrev-ref HEAD)
  if [[ ! $branch =~ $STACK_PATTERN ]]; then
    echo "Current branch '$branch' does not look like a stack branch." >&2
    exit 1
  fi
}

ensure_changes_present() {
  if git diff --quiet && git diff --cached --quiet; then
    echo "No staged or unstaged changes detected. Nothing to commit." >&2
    exit 1
  fi
  if ! git diff --quiet; then
    if ! prompt_yes_no "Unstaged changes detected. Continue with these changes?" "y"; then
      echo "Aborting at user request." >&2
      exit 1
    fi
  fi
}

collect_stack_commits() {
  if ! [[ $STACK_LOG_LIMIT =~ ^[0-9]+$ ]] || (( STACK_LOG_LIMIT <= 0 )); then
    echo "STACK_LOG_LIMIT must be a positive integer (got '$STACK_LOG_LIMIT')." >&2
    exit 1
  fi
  local sha author subject
  mapfile -t STACK_LINES < <(git log -n "$STACK_LOG_LIMIT" --pretty=format:'%H%x1f%an%x1f%s' HEAD)
  STACK_SHAS=()
  STACK_SUBJECTS=()
  for line in "${STACK_LINES[@]}"; do
    IFS=$'\x1f' read -r sha author subject <<<"$line"
    log_verbose "Inspecting commit ${sha:0:7} (author=$author) subject=$subject"
    local body
    body=$(git show -s --format=%B "$sha")
    local has_keyword="false"
    local match_source=""
    if [[ $subject == *"$PULL_REQUEST_KEYWORD"* ]]; then
      has_keyword="true"
      match_source="subject"
    elif [[ $body == *"$PULL_REQUEST_KEYWORD"* ]]; then
      has_keyword="true"
      match_source="body"
    fi
    if [[ $author == "$AUTHOR_NAME" && $has_keyword == "true" ]]; then
      log_verbose "→ matches keyword in $match_source"
      STACK_SHAS+=("$sha")
      STACK_SUBJECTS+=("$subject")
    else
      if [[ $author != "$AUTHOR_NAME" ]]; then
        log_verbose "→ stopping: author '$author' does not match '$AUTHOR_NAME'"
      elif [[ $has_keyword != "true" ]]; then
        log_verbose "→ stopping: keyword '$PULL_REQUEST_KEYWORD' missing from subject/body"
      fi
      break
    fi
  done
  if [[ ${#STACK_SHAS[@]} -eq 0 ]]; then
    echo "No contiguous stack commits found for $AUTHOR_NAME with keyword '$PULL_REQUEST_KEYWORD'." >&2
    exit 1
  fi
}

select_stack_commit() {
  echo "Select the PR commit to amend:"
  local idx=1
  for subject in "${STACK_SUBJECTS[@]}"; do
    printf '  %2d) %s %s\n' "$idx" "${STACK_SHAS[$((idx-1))]:0:7}" "$subject"
    idx=$((idx+1))
  done
  local choice
  while true; do
    read -rp "Enter number (1-${#STACK_SHAS[@]}): " choice
    if [[ $choice =~ ^[0-9]+$ ]] && (( choice >= 1 && choice <= ${#STACK_SHAS[@]} )); then
      SELECTED_SHA=${STACK_SHAS[$((choice-1))]}
      SELECTED_SUBJECT=${STACK_SUBJECTS[$((choice-1))]}
      break
    fi
    echo "Invalid selection." >&2
  done
}

extract_commit_message() {
  local file=$1
  if [[ ! -f $file ]]; then
    echo ""
    return
  fi
  python3 - "$file" <<'PY'
import json, sys
path = sys.argv[1]
try:
    with open(path, "r", encoding="utf-8") as fh:
        data = json.load(fh)
    msg = data.get("commit_message", "")
except Exception:
    msg = ""
print(msg.strip())
PY
}

cleanup() {
  local exit_code=$?
  if [[ $exit_code -ne 0 ]]; then
    echo "Script exited with code $exit_code. Resolve any in-progress rebase manually." >&2
  fi
}
trap cleanup EXIT

primary_flow() {
  log_info "Validating current branch matches stack pattern"
  require_clean_stack_branch
  log_info "Checking for staged or unstaged changes"
  ensure_changes_present
  log_info "Collecting candidate stack commits (last ${STACK_LOG_LIMIT} commits)"
  collect_stack_commits
  log_info "Prompting for target commit selection"
  select_stack_commit
  log_info "Selected commit ${SELECTED_SHA:0:7} - ${SELECTED_SUBJECT:-unknown}"

  log_info "Staging working tree changes"
  git add -A

  local acli_message=${ACLI_MESSAGE:-}
  local acli_output_file=${ACLI_OUTPUT_FILE:-}
  local acli_output_schema=${ACLI_OUTPUT_SCHEMA:-}
  local acli_config_file=${ACLI_CONFIG_FILE:-}

  log_info "Gathering acli rovodev inputs"
  while [[ -z $acli_message ]]; do
    read -rp "Enter message for acli rovodev: " acli_message
  done
  if [[ -z $acli_output_file ]]; then
    echo "ACLI_OUTPUT_FILE must point to the file where acli rovodev writes the commit message." >&2
    exit 1
  fi
  if [[ -z $acli_output_schema ]]; then
    echo "ACLI_OUTPUT_SCHEMA must be provided (path to the schema json)." >&2
    exit 1
  fi
  if [[ -z $acli_config_file ]]; then
    echo "ACLI_CONFIG_FILE must be provided (path to the config file)." >&2
    exit 1
  fi

  log_info "Requesting commit message suggestion from acli rovodev"
  acli rovodev "$acli_message" --output-file="$acli_output_file" --output-schema="$acli_output_schema" --config-file="$acli_config_file"

  log_info "Extracting suggested commit message"
  local suggested_message
  suggested_message=$(extract_commit_message "$acli_output_file")
  rm -f "$acli_output_file" 2>/dev/null || true
  if [[ -z $suggested_message ]]; then
    suggested_message="$DEFAULT_MESSAGE"
  fi

  log_info "Showing git status summary"
  git status -sb
  if ! prompt_yes_no "Review the diff now if needed. Continue?" "y"; then
    echo "Aborting at user request. Resolve staged changes manually if desired." >&2
    exit 1
  fi

  log_info "Creating fixup commit targeting ${SELECTED_SHA:0:7}"
  if ! git commit --fixup="$SELECTED_SHA"; then
    echo "git commit --fixup failed. Resolve the issue (conflicts, empty changes, etc.) and rerun." >&2
    exit 1
  fi

  log_info "Running autosquash rebase"
  if ! GIT_SEQUENCE_EDITOR=: git rebase -i --autosquash "${SELECTED_SHA}^"; then
    echo "Rebase failed. Resolve conflicts and run 'git rebase --continue' manually." >&2
    exit 1
  fi

  echo "Suggested commit message:" >&2
  echo "-------------------------" >&2
  echo "$suggested_message" >&2
  echo "-------------------------" >&2
  read -rp "Press Enter to keep, or type a new commit message: " maybe_new
  local final_message
  if [[ -n ${maybe_new:-} ]]; then
    final_message="$maybe_new"
  else
    final_message="$suggested_message"
  fi

  log_info "Submitting stack via ag"
  ag submit -m "$final_message" --all

  echo "Stack commit updated and submitted." >&2
}

if [[ ${1:-} == "-h" || ${1:-} == "--help" ]]; then
  usage
  exit 0
fi

primary_flow
