import Foundation

struct RuleFile: Codable {
    var rules: [Rule]
}

/// Reads rules from `~/Library/Application Support/ZoomFocus/rules.json`,
/// writing the default file on first launch.
enum RuleStore {
    static var url: URL {
        let support = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
        return support.appendingPathComponent("ZoomFocus/rules.json")
    }

    static let defaultFile = RuleFile(rules: [
        Rule(
            name: "Zoom meetings",
            trigger: .zoomMeeting,
            onStart: [Action(kind: .runShortcut, name: "Zoom DND On")],
            onEnd: [Action(kind: .runShortcut, name: "Zoom DND Off")]
        )
    ])

    static func load() throws -> [Rule] {
        if !FileManager.default.fileExists(atPath: url.path) {
            try writeDefault()
        }
        let data = try Data(contentsOf: url)
        return try JSONDecoder().decode(RuleFile.self, from: data).rules
    }

    private static func writeDefault() throws {
        try FileManager.default.createDirectory(
            at: url.deletingLastPathComponent(),
            withIntermediateDirectories: true
        )
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
        try encoder.encode(defaultFile).write(to: url)
    }
}
