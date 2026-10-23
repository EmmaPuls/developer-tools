# ZoomFocus

A menu bar app for macOS. When Zoom has your camera or microphone open, it runs
the "Zoom DND On" Shortcut. When the call ends, it always runs "Zoom DND Off".

## Install

There are two ways to install ZoomFocus. Both end with the app in your menu bar and
two Shortcuts, "Zoom DND On" and "Zoom DND Off", in your Shortcuts library.

### Option 1: Disk image (no terminal)

1. Open `ZoomFocus.dmg` and drag **ZoomFocus** into **Applications**.
2. Open ZoomFocus from Applications. macOS will block it, because the app isn't
   signed with an Apple Developer ID. To allow it, open **System Settings → Privacy &
   Security**, find the message about ZoomFocus, click **Open Anyway**, and confirm.
   You only need to do this once.
3. When ZoomFocus opens, it offers to add the two shortcuts. Click **Add**. Shortcuts
   opens each one, and you confirm each import.
4. Turn on **Launch at Login** from the menu bar item.

To build the disk image yourself, run `./scripts/build-dmg.sh`. The image is written to
`dist/ZoomFocus.dmg`.

### Option 2: Build from source (terminal)

1. Build the app. This needs the Swift toolchain, from Xcode or the Command Line Tools:

   ```sh
   ./scripts/build-app.sh
   open ZoomFocus.app
   ```

2. When ZoomFocus opens, it offers to add the two shortcuts. Click **Add**, then confirm
   each import in Shortcuts.
3. Turn on **Launch at Login** from the menu bar item.

### Adding the shortcuts later

If you deleted the shortcuts or skipped the prompt, click the ZoomFocus menu bar icon,
choose **Check Shortcuts**, then **Add Missing Shortcuts**. Run **Check Shortcuts**
first. The list of missing shortcuts is refreshed at launch and by that menu item.
If something goes wrong, **Show Setup Steps** lists how to create the shortcuts by hand.

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
