import AppKit
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
    private(set) var missingShortcuts: [String] = []

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
        refreshShortcuts()
        DispatchQueue.main.async { self.offerShortcutSetupIfNeeded() }
    }

    func refreshShortcuts() {
        missingShortcuts = ShortcutSetup.missingNames()
    }

    /// Opens the bundled shortcut files for the missing shortcuts. Shortcuts asks the user to confirm each one.
    func addMissingShortcuts() {
        let files = missingShortcuts.compactMap(ShortcutSetup.bundledFile(for:))
        guard files.count == missingShortcuts.count else {
            showSetupSteps()
            return
        }
        files.forEach { NSWorkspace.shared.open($0) }
    }

    /// Shows the manual steps for the missing shortcuts, with a button to open the Shortcuts app.
    func showSetupSteps() {
        NSApp.activate(ignoringOtherApps: true)
        let alert = NSAlert()
        alert.messageText = "Missing shortcuts: \(missingShortcuts.joined(separator: ", "))"
        alert.informativeText = ShortcutSetup.manualSteps
        alert.addButton(withTitle: "Open Shortcuts")
        alert.addButton(withTitle: "Close")
        if alert.runModal() == .alertFirstButtonReturn {
            NSWorkspace.shared.open(URL(fileURLWithPath: "/System/Applications/Shortcuts.app"))
        }
    }

    /// Offers to add the missing shortcuts at launch. "Check Shortcuts" in the menu re-checks.
    private func offerShortcutSetupIfNeeded() {
        guard !missingShortcuts.isEmpty else { return }
        NSApp.activate(ignoringOtherApps: true)
        let alert = NSAlert()
        alert.messageText = "Add the Zoom DND shortcuts?"
        alert.informativeText = "ZoomFocus runs these shortcuts. Missing: \(missingShortcuts.joined(separator: ", ")). "
            + "Shortcuts will ask you to confirm each one."
        alert.addButton(withTitle: "Add")
        alert.addButton(withTitle: "Not Now")
        if alert.runModal() == .alertFirstButtonReturn {
            addMissingShortcuts()
        }
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
