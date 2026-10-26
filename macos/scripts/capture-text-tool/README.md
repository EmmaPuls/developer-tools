# capture-text

Press **⌘⇧2**, select an area of the screen, and the text in that area is copied to your clipboard. No image is saved. macOS only.

## Install

Requires the Xcode command line tools, for `swiftc`. If you don't have them, run `xcode-select --install`.

```bash
./macos/scripts/capture-text-tool/install-capture-text.sh
```

The installer:

- compiles `capture-text-daemon` into `~/.local/bin`
- starts it at login with a LaunchAgent (`~/Library/LaunchAgents/local.capture-text.plist`)

It's safe to re-run.

## Grant Screen Recording permission

The first time you press ⌘⇧2 after installing, macOS shows this prompt:

> "capture-text-daemon" would like to record this computer's screen and audio.

Click **Open System Settings**, then turn on **capture-text-daemon** under Privacy & Security → Screen & System Audio Recording. Then press ⌘⇧2 again.

Until this is granted, selections come back blank and nothing is copied.

## Use

1. Press ⌘⇧2. The crosshair appears.
2. Drag over the text you want.
3. Paste. The recognized text is on your clipboard.

Press Esc during the selection to cancel. Pressing ⌘⇧2 again while a capture is in progress does nothing.

When the daemon starts (at login, and after each install), it loads the text recognizer in the background. The first time a new build runs this can take up to about a minute. Nothing is copied during that step. A capture started before it finishes can take as long as the load still needs, so wait a minute after installing before the first capture.

## Troubleshooting

Timings and errors are logged to:

- `~/Library/Logs/capture-text.log`: one entry per capture, with selection size, line count, and OCR time. Each start also logs a `warmup ms=` line for the background load. The file keeps the last 500 lines.
- `~/Library/Logs/capture-text-daemon.log`: startup errors from the daemon.

If ⌘⇧2 does nothing, check that the daemon is running:

```bash
launchctl print gui/$(id -u)/local.capture-text | grep -E 'state|pid'
```

If it isn't, re-run the installer.

## Uninstall

```bash
launchctl bootout gui/$(id -u)/local.capture-text
rm ~/Library/LaunchAgents/local.capture-text.plist
rm ~/.local/bin/capture-text-daemon
```

If you installed an earlier version, also remove its leftover files:

```bash
rm -f ~/.local/bin/capture-text ~/.local/bin/capture-text-ocr
rm -rf "$HOME/Library/Services/Capture Text.workflow"
```

Then remove **capture-text-daemon** from Screen & System Audio Recording in System Settings.

## Files

- `capture-text-daemon.swift`: registers ⌘⇧2, takes the selection with `screencapture`, reads the text with Apple Vision, copies it, and writes the log. Compiled to `capture-text-daemon`.
- `install-capture-text.sh`: builds, installs, and starts the daemon.
