/// One task's field, read either way round: "blocks" is the same edge as
/// "blocked by", written on the other task.
public enum Relation: CaseIterable, Identifiable, Hashable, Sendable {
    case blockedBy, blocks, parent

    public var id: Self { self }

    /// Reads as the rest of a sentence about the task the picker was opened on.
    public var name: String {
        switch self {
        case .blockedBy: "Blocked by"
        case .blocks: "Blocks"
        case .parent: "Subtask of"
        }
    }

    /// The edit that turns this relation on or off, which side of the edge it
    /// is stored on included.
    public func edit(_ candidate: Int, to id: Int, on: Bool) -> (task: Int, edit: Edit) {
        switch self {
        case .blockedBy: (id, on ? .addDependency(candidate) : .removeDependency(candidate))
        case .blocks: (candidate, on ? .addDependency(id) : .removeDependency(id))
        case .parent: (id, .parent(on ? candidate : nil))
        }
    }
}

public struct Candidate: Identifiable, Sendable {
    public let task: SeedTask
    /// Why the picker will not offer it, or nil when it will.
    public let barred: String?

    public var id: Int { task.id }
}
