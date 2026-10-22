import AppKit
import CoreAudio
import CoreMediaIO
import Foundation
import OSLog

/// Reports when a Zoom meeting starts and ends.
///
/// A meeting is in progress when Zoom is capturing the microphone, or when a camera
/// is running while Zoom is open. The log stream does not work for this: Zoom's own
/// log entries have no camera or audio subsystem, so filtering on subsystem misses them.
final class MeetingMonitor {
    /// Called on the monitor's queue with `true` when a meeting starts and `false` when it ends.
    var onChange: ((Bool) -> Void)?

    private let queue = DispatchQueue(label: "ZoomFocus.monitor")
    private let zoomBundleID = "us.zoom.xos"
    private var timer: DispatchSourceTimer?
    private var inMeeting = false

    func start() {
        queue.async {
            guard self.timer == nil else { return }
            let timer = DispatchSource.makeTimerSource(queue: self.queue)
            timer.schedule(deadline: .now(), repeating: 2)
            timer.setEventHandler { [weak self] in self?.poll() }
            timer.resume()
            self.timer = timer
        }
    }

    func stop() {
        queue.async {
            self.timer?.cancel()
            self.timer = nil
            self.setMeeting(false)
        }
    }

    private func poll() {
        let zoomPIDs = zoomProcessIDs()
        let meeting = zoomCapturesMicrophone(pids: zoomPIDs) || (!zoomPIDs.isEmpty && cameraRunning())
        setMeeting(meeting)
    }

    private func setMeeting(_ meeting: Bool) {
        guard meeting != inMeeting else { return }
        inMeeting = meeting
        logger.notice("meeting \(meeting ? "started" : "ended", privacy: .public)")
        onChange?(meeting)
    }

    private func zoomProcessIDs() -> [pid_t] {
        NSWorkspace.shared.runningApplications
            .filter { $0.bundleIdentifier == zoomBundleID }
            .map { $0.processIdentifier }
    }

    // MARK: Microphone

    private func zoomCapturesMicrophone(pids: [pid_t]) -> Bool {
        audioProcessObjects().contains { object in
            uint32Property(object, kAudioProcessPropertyIsRunningInput) != 0
                && pids.contains(pid_t(bitPattern: uint32Property(object, kAudioProcessPropertyPID)))
        }
    }

    private func audioProcessObjects() -> [AudioObjectID] {
        var address = audioAddress(kAudioHardwarePropertyProcessObjectList)
        var size: UInt32 = 0
        guard AudioObjectGetPropertyDataSize(AudioObjectID(kAudioObjectSystemObject), &address, 0, nil, &size) == noErr else {
            return []
        }
        var objects = [AudioObjectID](repeating: 0, count: Int(size) / MemoryLayout<AudioObjectID>.size)
        guard AudioObjectGetPropertyData(AudioObjectID(kAudioObjectSystemObject), &address, 0, nil, &size, &objects) == noErr else {
            return []
        }
        return objects
    }

    private func uint32Property(_ object: AudioObjectID, _ selector: AudioObjectPropertySelector) -> UInt32 {
        var address = audioAddress(selector)
        var value: UInt32 = 0
        var size = UInt32(MemoryLayout<UInt32>.size)
        AudioObjectGetPropertyData(object, &address, 0, nil, &size, &value)
        return value
    }

    private func audioAddress(_ selector: AudioObjectPropertySelector) -> AudioObjectPropertyAddress {
        AudioObjectPropertyAddress(mSelector: selector, mScope: kAudioObjectPropertyScopeGlobal, mElement: kAudioObjectPropertyElementMain)
    }

    // MARK: Camera

    private func cameraRunning() -> Bool {
        var address = cameraAddress(CMIOObjectPropertySelector(kCMIOHardwarePropertyDevices))
        var size: UInt32 = 0
        guard CMIOObjectGetPropertyDataSize(CMIOObjectID(kCMIOObjectSystemObject), &address, 0, nil, &size) == noErr else {
            return false
        }
        var devices = [CMIODeviceID](repeating: 0, count: Int(size) / MemoryLayout<CMIODeviceID>.size)
        var dataUsed: UInt32 = 0
        guard CMIOObjectGetPropertyData(CMIOObjectID(kCMIOObjectSystemObject), &address, 0, nil, size, &dataUsed, &devices) == noErr else {
            return false
        }
        return devices.contains { device in
            var runAddress = cameraAddress(CMIOObjectPropertySelector(kCMIODevicePropertyDeviceIsRunningSomewhere))
            var running: UInt32 = 0
            var runUsed: UInt32 = 0
            let runSize = UInt32(MemoryLayout<UInt32>.size)
            return CMIOObjectGetPropertyData(device, &runAddress, 0, nil, runSize, &runUsed, &running) == noErr && running != 0
        }
    }

    private func cameraAddress(_ selector: CMIOObjectPropertySelector) -> CMIOObjectPropertyAddress {
        CMIOObjectPropertyAddress(
            mSelector: selector,
            mScope: CMIOObjectPropertyScope(kCMIOObjectPropertyScopeGlobal),
            mElement: CMIOObjectPropertyElement(kCMIOObjectPropertyElementMain)
        )
    }
}
