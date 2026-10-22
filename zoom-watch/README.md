# ZoomFocus

A menu bar app for macOS. When Zoom has your camera or microphone open, it runs
the "Zoom DND On" Shortcut. When the call ends, it always runs "Zoom DND Off".

## Setup

1. Create two Shortcuts in the Shortcuts app, named exactly:
   - `Zoom DND On`: Set Focus to Do Not Disturb, turn on
   - `Zoom DND Off`: Set Focus to Do Not Disturb, turn off
2. Build the app:

   ```sh
   ./scripts/build-app.sh
   open ZoomFocus.app
   ```

3. Turn on **Launch at Login** from the menu bar item.

The first time the app runs Shortcuts, macOS may ask you to allow it.

## Rules

Rules live in `~/Library/Application Support/ZoomFocus/rules.json`. The file is
created on first launch. Use **Open Rules File** from the menu to edit it.
Restart the app after editing.

```json
{
  "rules": [
    {
      "name": "Zoom meetings",
      "trigger": "zoomMeeting",
      "onStart": [{ "kind": "runShortcut", "name": "Zoom DND On" }],
      "onEnd": [{ "kind": "runShortcut", "name": "Zoom DND Off" }]
    }
  ]
}
```

Only the `zoomMeeting` trigger and the `runShortcut` action exist so far.

## How detection works

A meeting is in progress when either of these is true, checked every 2 seconds:

- Zoom (`us.zoom.xos`) is capturing the microphone. CoreAudio reports which process
  is capturing input, so this check is exact.
- A camera is running somewhere while Zoom is open. This can false-positive if another
  app uses the camera during a Zoom session.

The app first tried watching the unified log, as `zoom-watch.sh` does. That didn't work:
Zoom's own log entries have no camera or audio subsystem, so the subsystem filter
never matched them.

## Building

Requires Swift 5.9 or later and macOS 14.4 or later. The SwiftUI `@State` macro
needs the full Xcode toolchain, so the app avoids it and builds with the
Command Line Tools alone.
