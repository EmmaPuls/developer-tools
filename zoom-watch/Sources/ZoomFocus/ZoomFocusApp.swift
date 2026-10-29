import AppKit
import SwiftUI

@main
struct ZoomFocusApp: App {
    private let model = AppModel()

    var body: some Scene {
        MenuBarExtra {
            MenuContent(model: model)
        } label: {
            Image(systemName: model.inMeeting ? "video.fill" : "video")
        }
        .menuBarExtraStyle(.menu)
    }
}

private struct MenuContent: View {
    let model: AppModel

    var body: some View {
        Text(model.inMeeting ? "In a Zoom meeting" : "Not in a Zoom meeting")

        if let loadError = model.loadError {
            Text("Rules failed to load: \(loadError)")
        }

        Divider()

        Toggle("Enabled", isOn: Binding(
            get: { model.enabled },
            set: { model.setEnabled($0) }
        ))

        Toggle("Launch at Login", isOn: Binding(
            get: { model.launchAtLogin },
            set: { model.setLaunchAtLogin($0) }
        ))

        Divider()

        if model.missingShortcuts.isEmpty {
            Text("Zoom DND shortcuts: installed")
        } else {
            Text("Missing shortcuts: \(model.missingShortcuts.joined(separator: ", "))")
            Button("Add Missing Shortcuts") {
                model.addMissingShortcuts()
            }
            Button("Show Setup Steps") {
                model.showSetupSteps()
            }
        }
        Button("Check Shortcuts") {
            model.refreshShortcuts()
        }

        Divider()

        Button("Open Rules File") {
            NSWorkspace.shared.open(RuleStore.url)
        }

        Divider()

        Button("Quit ZoomFocus") {
            NSApplication.shared.terminate(nil)
        }
        .keyboardShortcut("q")
    }
}
