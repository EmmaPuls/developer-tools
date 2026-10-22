import Foundation
import OSLog
import Observation
import ServiceManagement

@Observable
final class AppModel {
    private(set) var inMeeting = false
    private(set) var enabled = true
    private(set) var rules: [Rule] = []
    private(set) var loadError: String?
    private(set) var launchAtLogin = SMAppService.mainApp.status == .enabled

    private let monitor = MeetingMonitor()
    private let actionQueue = DispatchQueue(label: "ZoomFocus.actions")

    init() {
        do {
            rules = try RuleStore.load()
        } catch {
            loadError = error.localizedDescription
        }
        monitor.onChange = { [weak self] meeting in
            DispatchQueue.main.async { self?.meetingChanged(meeting) }
        }
        monitor.start()
    }

    func setEnabled(_ on: Bool) {
        enabled = on
        if on {
            monitor.start()
        } else {
            monitor.stop()
        }
    }

    func setLaunchAtLogin(_ on: Bool) {
        do {
            if on {
                try SMAppService.mainApp.register()
            } else {
                try SMAppService.mainApp.unregister()
            }
        } catch {
            logger.notice("could not change launch at login: \(error.localizedDescription, privacy: .public)")
        }
        launchAtLogin = SMAppService.mainApp.status == .enabled
    }

    private func meetingChanged(_ meeting: Bool) {
        inMeeting = meeting
        for rule in rules where rule.trigger == .zoomMeeting {
            run(meeting ? rule.onStart : rule.onEnd)
        }
    }

    private func run(_ actions: [Action]) {
        actionQueue.async {
            actions.forEach(ActionRunner.run)
        }
    }
}
