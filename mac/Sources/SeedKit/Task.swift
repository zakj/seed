import Foundation

public enum Status: String, Codable, CaseIterable, Identifiable, Sendable {
    case todo
    case inProgress = "in-progress"
    case done
    case dropped

    public var id: Self { self }

    public var name: String {
        switch self {
        case .todo: "To Do"
        case .inProgress: "In Progress"
        case .done: "Done"
        case .dropped: "Dropped"
        }
    }

    /// How the action that moves a task *into* this status reads in a menu.
    public var verb: String {
        switch self {
        case .todo: "Move Back to To Do"
        case .inProgress: "Start"
        case .done: "Mark as Done"
        case .dropped: "Drop"
        }
    }

    public var isResolved: Bool { self == .done || self == .dropped }

    public var symbol: String {
        switch self {
        case .todo: "circle"
        case .inProgress: "circle.lefthalf.filled"
        case .done: "checkmark.circle.fill"
        case .dropped: "xmark.circle"
        }
    }

    /// Mirrors `Status::sort_rank` in src/task.rs.
    func rank(blocked: Bool) -> Int {
        switch (self, blocked) {
        case (.inProgress, _): 0
        case (.todo, false): 1
        case (.todo, true): 2
        case (.done, _), (.dropped, _): 3
        }
    }
}

public enum Priority: String, Codable, CaseIterable, Identifiable, Sendable {
    case critical, high, normal, low

    public var id: Self { self }
    public var name: String { rawValue.capitalized }

    /// An ordered scale: high and low mirror each other, and critical stacks
    /// above high.
    public var symbol: String {
        switch self {
        case .critical: "chevron.up.2"
        case .high: "arrow.up"
        case .normal: "minus"
        case .low: "arrow.down"
        }
    }

    var order: Int {
        switch self {
        case .critical: 0
        case .high: 1
        case .normal: 2
        case .low: 3
        }
    }

}

/// Not `Identifiable`: an id would have to be built from the message, and a
/// `ForEach` asks for it per entry per body pass — on a task whose log runs to
/// kilobytes that is the whole log re-allocated to redraw a hover. `Hashable`
/// lets `ForEach` key on the value itself.
public struct LogEntry: Decodable, Hashable, Sendable {
    public let timestamp: Date
    public let message: String
    public let agent: String?

    private enum CodingKeys: String, CodingKey { case timestamp, message, agent }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        timestamp = try ISO8601.parse(c.decode(String.self, forKey: .timestamp))
        message = try c.decode(String.self, forKey: .message)
        agent = try c.decodeIfPresent(String.self, forKey: .agent)
    }
}

public struct SeedTask: Decodable, Hashable, Identifiable, Sendable {
    public let id: Int
    public let title: String
    public let status: Status
    public let priority: Priority
    public let description: String?
    public let labels: [String]
    public let parent: Int?
    /// Unmet dependencies only — `sd` strips resolved ones from its JSON.
    public let depends: [Int]
    public let children: [Int]
    public let created: Date
    public let modified: Date
    public let log: [LogEntry]
    /// Already moved to `archive/`, and only returned when asked for.
    public let archived: Bool

    private enum CodingKeys: String, CodingKey {
        case id, title, status, priority, description, labels, parent
        case depends, children, created, modified, log, archived
    }

    /// `sd` omits fields at their default, so every optional key needs a fallback.
    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decode(Int.self, forKey: .id)
        title = try c.decode(String.self, forKey: .title)
        status = try c.decode(Status.self, forKey: .status)
        priority = try c.decodeIfPresent(Priority.self, forKey: .priority) ?? .normal
        description = try c.decodeIfPresent(String.self, forKey: .description)
        labels = try c.decodeIfPresent([String].self, forKey: .labels) ?? []
        parent = try c.decodeIfPresent(Int.self, forKey: .parent)
        depends = try c.decodeIfPresent([Int].self, forKey: .depends) ?? []
        children = try c.decodeIfPresent([Int].self, forKey: .children) ?? []
        created = try ISO8601.parse(c.decode(String.self, forKey: .created))
        modified = try ISO8601.parse(c.decode(String.self, forKey: .modified))
        log = try c.decodeIfPresent([LogEntry].self, forKey: .log) ?? []
        archived = try c.decodeIfPresent(Bool.self, forKey: .archived) ?? false
    }

    public var isBlocked: Bool { status == .todo && !depends.isEmpty }

    /// Mirrors `Task::sort_key` in src/task.rs.
    var sortKey: (Int, Int, Int) { (status.rank(blocked: isBlocked), priority.order, id) }
}

enum ISO8601 {
    private static let fractional = Date.ISO8601FormatStyle(includingFractionalSeconds: true)
    private static let whole = Date.ISO8601FormatStyle()

    /// `sd` writes microsecond precision on timestamps it generates, but whole
    /// seconds survive a hand-edited task file.
    static func parse(_ text: String) throws -> Date {
        if let date = try? fractional.parse(text) { return date }
        return try whole.parse(text)
    }
}

extension SeedTask {
    /// The search a person types: case- and diacritic-insensitive, matching an
    /// id, a title, or a label. The `#` an id may carry is stripped here rather
    /// than by every caller, which is what had it written out twice.
    public func matches(_ query: String) -> Bool {
        let id = query.trimmingCharacters(in: CharacterSet(charactersIn: "#"))
        return String(self.id) == id
            || title.localizedStandardContains(query)
            || labels.contains { $0.localizedStandardContains(query) }
    }
}
