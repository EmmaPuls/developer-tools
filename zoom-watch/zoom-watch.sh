#!/bin/zsh
# Watches the macOS unified log for Zoom starting or stopping camera and
# microphone capture, and runs a hook when the state changes.
#
# Usage:
#   ON_START='echo on' ON_STOP='echo off' ./zoom-watch.sh
#   DEBUG=1 ./zoom-watch.sh    # print every Zoom-related log line, to tune the patterns
#
# Hooks run with ZOOM_DEVICE (camera|microphone) and ZOOM_STATE (on|off) set.

set -u

ON_START=${ON_START:-}
ON_STOP=${ON_STOP:-}
DEBUG=${DEBUG:-0}
PROCESS_NAME=${PROCESS_NAME:-zoom.us}

# Substrings that mark a start or stop line in the log. Adjust these after
# running with DEBUG=1 during a real call.
START_PATTERN=${START_PATTERN:-'started|start capture|Starting|began'}
STOP_PATTERN=${STOP_PATTERN:-'stopped|stop capture|Stopping|ended'}

typeset -A state
state[camera]=off
state[microphone]=off

fire() {
  local device=$1 new=$2 hook
  [[ ${state[$device]} == $new ]] && return
  state[$device]=$new
  hook=$([[ $new == on ]] && print -r -- $ON_START || print -r -- $ON_STOP)
  print "$(date '+%H:%M:%S') zoom $device $new"
  [[ -n $hook ]] && ZOOM_DEVICE=$device ZOOM_STATE=$new eval "$hook"
}

classify() {
  # Stdin: one NDJSON log entry per line. Prints "<device> <on|off>" for matches.
  jq -r --arg proc "$PROCESS_NAME" --arg start "$START_PATTERN" --arg stop "$STOP_PATTERN" '
    select((.processImagePath // "") | contains($proc))
    | (.eventMessage // "") as $msg
    | (if (.subsystem // "") | test("cmio") then "camera"
       elif (.subsystem // "") | test("coreaudio|audio") then "microphone"
       else empty end) as $dev
    | if ($msg | test($start; "i")) then "\($dev) on"
      elif ($msg | test($stop; "i")) then "\($dev) off"
      else empty end
  ' 2>/dev/null
}

trap 'print "stopping"; kill 0 2>/dev/null; exit 0' INT TERM

log stream --style ndjson --level info \
  --predicate '(subsystem CONTAINS "cmio") OR (subsystem CONTAINS "coreaudio") OR (subsystem CONTAINS "audio")' |
while IFS= read -r line; do
  if [[ $DEBUG == 1 ]]; then
    print -r -- "$line" | jq -c '{p: .processImagePath, s: .subsystem, m: .eventMessage}' 2>/dev/null
  fi
  # Not piped into a loop: that would run fire() in a subshell and lose state.
  events=$(print -r -- "$line" | classify)
  while read -r device new; do
    [[ -n $device ]] && fire "$device" "$new"
  done <<< "$events"
done
