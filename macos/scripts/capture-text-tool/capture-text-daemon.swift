import AppKit
import Carbon
import Foundation
import Vision

// Resident helper for ⌘⇧2: select an area with screencapture, read its text
// with Vision, and copy the text. Staying running avoids a launch delay on
// every press.

let logURL = FileManager.default.homeDirectoryForCurrentUser
    .appendingPathComponent("Library/Logs/capture-text.log")
let maxLogLines = 500
// Captures and the launch warm-up both log, so writes go through one queue.
let logQueue = DispatchQueue(label: "capture-text.log")

// Log lines are epoch seconds with milliseconds, one per event.
func logLine(_ message: String) {
    let stamp = String(format: "%.3f", Date().timeIntervalSince1970)
    let line = Data("\(stamp) \(message)\n".utf8)
    logQueue.sync {
        if !FileManager.default.fileExists(atPath: logURL.path) {
            FileManager.default.createFile(atPath: logURL.path, contents: nil)
        }
        guard let handle = try? FileHandle(forWritingTo: logURL) else { return }
        defer { try? handle.close() }
        handle.seekToEndOfFile()
        handle.write(line)
    }
}

// Keeps only the last maxLogLines lines so the log doesn't grow without limit.
func trimLog() {
    logQueue.sync {
        guard let text = try? String(contentsOf: logURL, encoding: .utf8) else { return }
        let lines = text.split(separator: "\n")
        guard lines.count > maxLogLines else { return }
        let kept = lines.suffix(maxLogLines).joined(separator: "\n") + "\n"
        try? kept.write(to: logURL, atomically: true, encoding: .utf8)
    }
}

// Vision doesn't return observations in reading order, so sort by height.
func recognizeLines(_ handler: VNImageRequestHandler) throws -> [String] {
    let request = VNRecognizeTextRequest()
    request.recognitionLevel = .accurate
    request.usesLanguageCorrection = true
    try handler.perform([request])
    let observations = (request.results ?? []).sorted { $0.boundingBox.minY > $1.boundingBox.minY }
    return observations.compactMap { $0.topCandidates(1).first?.string }
}

// The first Vision request in a new binary can take tens of seconds to load its
// text model. Running one at launch moves that wait out of the first capture.
// The text is discarded and nothing is copied.
func warmUpVision() {
    let size = CGSize(width: 320, height: 60)
    guard let context = CGContext(
        data: nil, width: Int(size.width), height: Int(size.height),
        bitsPerComponent: 8, bytesPerRow: 0, space: CGColorSpaceCreateDeviceRGB(),
        bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
    ) else { return }
    context.setFillColor(CGColor(gray: 1, alpha: 1))
    context.fill(CGRect(origin: .zero, size: size))
    NSGraphicsContext.saveGraphicsState()
    NSGraphicsContext.current = NSGraphicsContext(cgContext: context, flipped: false)
    ("warm up" as NSString).draw(at: NSPoint(x: 10, y: 14), withAttributes: [
        .font: NSFont.systemFont(ofSize: 32),
        .foregroundColor: NSColor.black,
    ])
    NSGraphicsContext.restoreGraphicsState()
    guard let image = context.makeImage() else { return }

    let started = Date()
    do {
        _ = try recognizeLines(VNImageRequestHandler(cgImage: image))
        logLine("warmup ms=\(Int(Date().timeIntervalSince(started) * 1000))")
    } catch {
        logLine("warmup failed: \(error)")
    }
}

// Runs one selection and returns its text, or nil if the selection was cancelled or empty.
func captureText() -> String? {
    trimLog()
    logLine("start")

    let image = FileManager.default.temporaryDirectory
        .appendingPathComponent("capture-text-\(UUID().uuidString).png")
    defer { try? FileManager.default.removeItem(at: image) }

    let screencapture = Process()
    screencapture.executableURL = URL(fileURLWithPath: "/usr/sbin/screencapture")
    screencapture.arguments = ["-i", "-x", image.path]
    do {
        try screencapture.run()
    } catch {
        logLine("could not start screencapture: \(error)")
        return nil
    }
    screencapture.waitUntilExit()

    // Esc cancels screencapture without writing a file, so the file is the signal.
    let bytes = (try? FileManager.default.attributesOfItem(atPath: image.path))?[.size] as? Int ?? 0
    guard bytes > 0 else {
        logLine("cancelled")
        return nil
    }
    logLine("selected size=\(bytes) bytes")

    let started = Date()
    do {
        let lines = try recognizeLines(VNImageRequestHandler(url: image))
        let ms = Int(Date().timeIntervalSince(started) * 1000)
        logLine("ocr done lines=\(lines.count) ms=\(ms)")
        return lines.joined(separator: "\n")
    } catch {
        logLine("ocr failed: \(error)")
        return nil
    }
}

let hotKeyID = EventHotKeyID(signature: OSType(0x43544b59), id: 1) // 'CTKY'
var hotKeyRef: EventHotKeyRef?

// Only touched on the main thread: the hotkey handler sets it, and the
// capture queue clears it by hopping back to main.
var capturing = false
let captureQueue = DispatchQueue(label: "capture-text.capture")

var eventType = EventTypeSpec(
    eventClass: OSType(kEventClassKeyboard),
    eventKind: UInt32(kEventHotKeyPressed)
)

let handler: EventHandlerUPP = { _, _, _ in
    // Ignore presses while a capture is still running, so two selections don't overlap.
    if capturing { return noErr }
    capturing = true
    captureQueue.async {
        let text = captureText()
        DispatchQueue.main.async {
            if let text, !text.isEmpty {
                let pasteboard = NSPasteboard.general
                pasteboard.clearContents()
                pasteboard.setString(text, forType: .string)
                logLine("copied")
            }
            capturing = false
        }
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

// Off the capture queue, so a press during warm-up isn't stuck behind it.
DispatchQueue.global(qos: .utility).async { warmUpVision() }

// Accessory apps have no Dock icon or menu bar; the run loop delivers hotkey events.
NSApplication.shared.setActivationPolicy(.accessory)
NSApplication.shared.run()
