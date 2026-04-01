#!/usr/bin/env bash
# dt-notify.sh — Play a macOS notification sound when dt is about to require user input.
#
# Usage:
#   dt-notify.sh                     # plays the default sound (Glass)
#   dt-notify.sh <sound-name>        # plays a specific /System/Library/Sounds/<name>.aiff
#
# Sourced or called by dt before any interactive prompt so the user knows
# attention is needed even if they've switched focus to another window.
#
# Supported sound names (macOS built-ins):
#   Basso, Blow, Bottle, Frog, Funk, Glass, Hero, Morse, Ping, Pop,
#   Purr, Sosumi, Submarine, Tink
#   Default: Glass

SOUND="${1:-Glass}"
SOUND_FILE="/System/Library/Sounds/${SOUND}.aiff"

if [[ "$(uname)" != "Darwin" ]]; then
    # Not on macOS — silently exit so this is safe to call cross-platform
    exit 0
fi

if [[ ! -f "$SOUND_FILE" ]]; then
    echo "dt-notify: sound file not found: $SOUND_FILE" >&2
    echo "dt-notify: available sounds: $(ls /System/Library/Sounds/*.aiff 2>/dev/null | xargs -I{} basename {} .aiff | tr '\n' ' ')" >&2
    exit 1
fi

# afplay is available on all modern macOS versions
afplay "$SOUND_FILE"
