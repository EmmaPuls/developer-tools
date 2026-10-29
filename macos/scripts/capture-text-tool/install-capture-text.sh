#!/bin/bash
# Installs capture-text: select a screen area with ⌘⇧2 and copy its text.
# A small background helper (a LaunchAgent) listens for the shortcut.
# macOS only. Needs the Xcode command line tools for swiftc.
set -euo pipefail

if [ "$(uname -s)" != "Darwin" ]; then
  echo "install-capture-text.sh only supports macOS" >&2
  exit 1
fi

here=$(cd "$(dirname "$0")" && pwd)
bin="$HOME/.local/bin"
label="local.capture-text"
agent="$HOME/Library/LaunchAgents/$label.plist"
uid=$(id -u)

mkdir -p "$bin" "$HOME/Library/LaunchAgents"
swiftc -O "$here/capture-text-daemon.swift" -o "$bin/capture-text-daemon"

# Start the daemon at login and keep it running. Restart it if already loaded.
launchctl bootout "gui/$uid/$label" 2>/dev/null || true
cat > "$agent" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>Label</key>
	<string>$label</string>
	<key>ProgramArguments</key>
	<array>
		<string>$bin/capture-text-daemon</string>
	</array>
	<key>RunAtLoad</key>
	<true/>
	<key>KeepAlive</key>
	<true/>
	<key>StandardErrorPath</key>
	<string>$HOME/Library/Logs/capture-text-daemon.log</string>
</dict>
</plist>
EOF
launchctl bootstrap "gui/$uid" "$agent"

echo "Installed. Select text on screen with ⌘⇧2 to copy it."
echo "The first capture asks for Screen Recording permission for capture-text-daemon."
echo "Grant it under System Settings → Privacy & Security → Screen & System Audio Recording,"
echo "then press ⌘⇧2 again."
