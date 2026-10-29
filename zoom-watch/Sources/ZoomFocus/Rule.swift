import Foundation

/// Runs its actions when the trigger starts (`onStart`) and when it ends (`onEnd`).
struct Rule: Codable, Equatable {
    var name: String
    var trigger: Trigger
    var onStart: [Action]
    var onEnd: [Action]
}

enum Trigger: String, Codable {
    case zoomMeeting
}

struct Action: Codable, Equatable {
    enum Kind: String, Codable {
        case runShortcut
    }

    var kind: Kind
    var name: String
}
