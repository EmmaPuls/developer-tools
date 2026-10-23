import Foundation

/// Checks that the Shortcuts the default rule runs are in the Shortcuts library.
enum ShortcutSetup {
    /// Shortcut names the default rule runs. Must match `RuleStore.defaultFile`.
    static let names = ["Zoom DND On", "Zoom DND Off"]

    static let manualSteps = """
    Create each shortcut in the Shortcuts app:

    1. Click + to make a new shortcut.
    2. Click the name at the top and rename it "Zoom DND On".
    3. Click Add Action, search for "Set Focus", and add it.
    4. Set the Focus to "Do Not Disturb" and "Turn" to "On".
    5. Repeat for "Zoom DND Off", with "Turn" set to "Off".

    The wording in Shortcuts may differ slightly between macOS versions.
    """

    /// The exported shortcut bundled into the app. Opening it makes Shortcuts ask the user to add it.
    static func bundledFile(for name: String) -> URL? {
        Bundle.main.url(forResource: name, withExtension: "shortcut")
    }

    static func missingNames() -> [String] {
        let installed = installedNames()
        return names.filter { !installed.contains($0) }
    }

    private static func installedNames() -> Set<String> {
        let process = Process()
        let pipe = Pipe()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/shortcuts")
        process.arguments = ["list"]
        process.standardOutput = pipe
        do {
            try process.run()
        } catch {
            return []
        }
        let data = pipe.fileHandleForReading.readDataToEndOfFile()
        process.waitUntilExit()
        let output = String(data: data, encoding: .utf8) ?? ""
        return Set(output.split(separator: "\n").map { $0.trimmingCharacters(in: .whitespaces) })
    }
}
