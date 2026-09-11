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

/// `Hashable` rather than `Identifiable`, so `ForEach` keys on the value
/// without an id built from the message on every body pass.
public struct LogEntry: Decodable, Hashable, Sendable {
    public let timestamp: Date
    public let message: String
    public let agent: String?
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
        created = try c.decode(Date.self, forKey: .created)
        modified = try c.decode(Date.self, forKey: .modified)
        log = try c.decodeIfPresent([LogEntry].self, forKey: .log) ?? []
        archived = try c.decodeIfPresent(Bool.self, forKey: .archived) ?? false
    }

    public var isBlocked: Bool { status == .todo && !depends.isEmpty }

    /// Mirrors `Task::sort_key` in src/task.rs.
    var sortKey: (Int, Int, Int) { (status.rank(blocked: isBlocked), priority.order, id) }
}

extension JSONDecoder {
    /// Decodes `sd`'s JSON. Timestamps `sd` writes carry microseconds; whole
    /// seconds survive a hand-edited task file.
    static var seed: JSONDecoder {
        let fractional = Date.ISO8601FormatStyle(includingFractionalSeconds: true)
        let whole = Date.ISO8601FormatStyle()
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .custom { decoder in
            let text = try decoder.singleValueContainer().decode(String.self)
            if let date = try? fractional.parse(text) { return date }
            return try whole.parse(text)
        }
        return decoder
    }
}

extension SeedTask {
    /// The search a person types: case- and diacritic-insensitive, matching an
    /// id with or without its `#`, a title, or a label.
    public func matches(_ query: String) -> Bool {
        let id = query.trimmingCharacters(in: CharacterSet(charactersIn: "#"))
        return String(self.id) == id
            || title.localizedStandardContains(query)
            || labels.contains { $0.localizedStandardContains(query) }
    }
}
