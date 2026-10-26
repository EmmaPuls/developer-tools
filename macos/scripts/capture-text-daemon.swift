import AppKit
import Carbon
import Foundation

// Resident hotkey helper: runs capture-text when ⌘⇧2 is pressed. Staying
// running avoids the Services and Automator launch delay on every press.
let hotKeyID = EventHotKeyID(signature: OSType(0x43544b59), id: 1) // 'CTKY'
var hotKeyRef: EventHotKeyRef?
var capture: Process?

var eventType = EventTypeSpec(
    eventClass: OSType(kEventClassKeyboard),
    eventKind: UInt32(kEventHotKeyPressed)
)

let handler: EventHandlerUPP = { _, _, _ in
    // Ignore presses while a capture is still running, so two selections don't overlap.
    if let running = capture, running.isRunning {
        return noErr
    }
    let task = Process()
    task.executableURL = URL(fileURLWithPath: "/bin/bash")
    task.arguments = [FileManager.default.homeDirectoryForCurrentUser
        .appendingPathComponent(".local/bin/capture-text").path]
    do {
        try task.run()
        capture = task
    } catch {
        FileHandle.standardError.write("could not start capture-text: \(error)\n".data(using: .utf8)!)
    }
    return noErr
}

let installStatus = InstallEventHandler(
    GetApplicationEventTarget(), handler, 1, &eventType, nil, nil
)
guard installStatus == noErr else {
    FileHandle.standardError.write("InstallEventHandler failed: \(installStatus)\n".data(using: .utf8)!)
    exit(1)
}

let registerStatus = RegisterEventHotKey(
    UInt32(kVK_ANSI_2), UInt32(cmdKey | shiftKey), hotKeyID,
    GetApplicationEventTarget(), 0, &hotKeyRef
)
guard registerStatus == noErr else {
    FileHandle.standardError.write("RegisterEventHotKey failed: \(registerStatus)\n".data(using: .utf8)!)
    exit(1)
}

// Accessory apps have no Dock icon or menu bar; the run loop delivers hotkey events.
NSApplication.shared.setActivationPolicy(.accessory)
NSApplication.shared.run()
