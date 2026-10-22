import Foundation
import OSLog

enum ActionRunner {
    /// Blocks until the action finishes, so call it off the main thread.
    static func run(_ action: Action) {
        switch action.kind {
        case .runShortcut:
            runShortcut(named: action.name)
        }
    }

    private static func runShortcut(named name: String) {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/shortcuts")
        process.arguments = ["run", name]
        do {
            try process.run()
            process.waitUntilExit()
            if process.terminationStatus != 0 {
                logger.notice("shortcut '\(name, privacy: .public)' exited with status \(process.terminationStatus, privacy: .public)")
            }
        } catch {
            logger.notice("could not run shortcut '\(name, privacy: .public)': \(error.localizedDescription, privacy: .public)")
        }
    }
}
