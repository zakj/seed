import Foundation

/// One drawn line of the tree. The rows are flattened here rather than handed to
/// `List(children:)` because that owns its own expansion state, and nothing can
/// open a row the app needs to show.
public struct OutlineRow: Identifiable, Hashable, Sendable {
    public let task: SeedTask
    public let depth: Int
    public let hasChildren: Bool
    /// False for a row carried along to show where a match sits.
    public let matches: Bool

    public var id: Int { task.id }

    public init(task: SeedTask, depth: Int, hasChildren: Bool, matches: Bool = true) {
        self.task = task
        self.depth = depth
        self.hasChildren = hasChildren
        self.matches = matches
    }
}

public struct TaskGraph: Sendable {
    public let tasks: [SeedTask]
    public let labels: [String]
    /// Every task, in the order `sd list` prints them. Stored rather than
    /// computed: the relations picker asks for it on every keystroke.
    public let ordered: [SeedTask]
    private let byID: [Int: SeedTask]

    public init(_ tasks: [SeedTask]) {
        self.tasks = tasks
        labels = Set(tasks.flatMap(\.labels)).sorted()
        byID = Dictionary(tasks.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a })
        ordered = tasks.sorted { $0.sortKey < $1.sortKey }
    }

    public subscript(id: Int) -> SeedTask? { byID[id] }

    /// Roots first, each level ordered the way `sd list` orders it. A task whose
    /// parent was filtered out (archived, say) surfaces as a root.
    public func outline(expanded: Set<Int>) -> [OutlineRow] {
        var rows: [OutlineRow] = []
        for root in sorted(roots) {
            append(root, depth: 0, expanded: expanded, to: &rows)
        }
        return rows
    }

    private func append(
        _ task: SeedTask, depth: Int, expanded: Set<Int>, to rows: inout [OutlineRow]
    ) {
        let children = children(of: task)
        rows.append(OutlineRow(task: task, depth: depth, hasChildren: !children.isEmpty))
        guard expanded.contains(task.id) else { return }
        for child in children {
            append(child, depth: depth + 1, expanded: expanded, to: &rows)
        }
    }

    /// A search says "find this in my tree", so the tasks between a match and
    /// its root come along dimmed rather than the tree collapsing to a list.
    public func outline(matching predicate: (SeedTask) -> Bool) -> [OutlineRow] {
        let matched = Set(tasks.filter(predicate).map(\.id))
        var shown = matched
        for id in matched { shown.formUnion(ancestors(of: id)) }

        var rows: [OutlineRow] = []
        func visit(_ task: SeedTask, depth: Int) {
            rows.append(
                OutlineRow(
                    task: task, depth: depth, hasChildren: false,
                    matches: matched.contains(task.id)
                )
            )
            for child in children(of: task) where shown.contains(child.id) {
                visit(child, depth: depth + 1)
            }
        }
        for root in sorted(roots) where shown.contains(root.id) { visit(root, depth: 0) }
        return rows
    }

    /// `sd` stores only what a task waits on, so what it holds up is derived.
    public func blocking(_ id: Int) -> [SeedTask] {
        sorted(tasks.filter { $0.depends.contains(id) })
    }

    /// The tasks this one is waiting on. `blocking(_:)` is the same edge read
    /// the other way, and both are sorted the way `sd list` sorts.
    public func blockedBy(_ id: Int) -> [SeedTask] {
        sorted(self[id]?.depends.compactMap { self[$0] } ?? [])
    }

    /// Everything already waiting on this task, however far down the chain —
    /// depending on any of them would close a loop. Complete over live tasks
    /// only: `sd` strips resolved dependencies from its JSON, so an edge that
    /// points at a resolved task is not in the graph the app can see, and a
    /// chain running through one is invisible here. `validate_dag` reads the
    /// unstripped store and still refuses such a link.
    public func dependents(of id: Int) -> Set<Int> {
        var found: Set<Int> = []
        var queue = [id]
        while let next = queue.popLast() {
            for task in tasks where task.depends.contains(next) && found.insert(task.id).inserted {
                queue.append(task.id)
            }
        }
        return found
    }

    /// Everything this task already waits on, however far down the chain — it
    /// cannot come to block any of them without closing a loop.
    public func dependencies(of id: Int) -> Set<Int> {
        var found: Set<Int> = []
        var queue = byID[id]?.depends ?? []
        while let next = queue.popLast() {
            guard found.insert(next).inserted else { continue }
            queue.append(contentsOf: byID[next]?.depends ?? [])
        }
        return found
    }

    /// A task cannot become a child of its own descendant.
    public func descendants(of id: Int) -> Set<Int> {
        var found: Set<Int> = []
        var queue = [id]
        while let next = queue.popLast() {
            for child in byID[next]?.children ?? [] where found.insert(child).inserted {
                queue.append(child)
            }
        }
        return found
    }

    /// Every task between this one and its root, nearest first.
    public func ancestors(of id: Int) -> [Int] {
        var result: [Int] = []
        var seen: Set<Int> = [id]
        var next = byID[id]?.parent
        while let parent = next, seen.insert(parent).inserted {
            result.append(parent)
            next = byID[parent]?.parent
        }
        return result
    }

    private func sorted(_ tasks: [SeedTask]) -> [SeedTask] {
        tasks.sorted { $0.sortKey < $1.sortKey }
    }

    /// A task whose parent is not in the graph — archived, or filtered out —
    /// surfaces as a root rather than disappearing with it.
    private var roots: [SeedTask] {
        tasks.filter { $0.parent.flatMap { byID[$0] } == nil }
    }

    private func children(of task: SeedTask) -> [SeedTask] {
        sorted(task.children.compactMap { byID[$0] })
    }

    /// What the relations picker may offer, and why it may not. `sd` refuses a
    /// relation that closes a loop or nests a task inside itself, and offering a
    /// move it will refuse is worse than not offering it. It also drops a
    /// dependency on a resolved task, which would make the tick vanish.
    public func candidates(for id: Int, by relation: Relation) -> [Candidate] {
        if relation != .parent, self[id]?.status.isResolved != false { return [] }

        let barred: Set<Int> = switch relation {
        case .blockedBy: dependents(of: id)
        case .blocks: dependencies(of: id)
        case .parent: descendants(of: id)
        }
        let reason = switch relation {
        case .blockedBy: "Already waiting on this one"
        case .blocks: "This one is already waiting on it"
        case .parent: "Already inside this one"
        }

        return ordered.compactMap { task in
            if task.id == id { return nil }
            if relation != .parent, task.status.isResolved { return nil }
            return Candidate(task: task, barred: barred.contains(task.id) ? reason : nil)
        }
    }

    /// Why the picker has nothing to show, which is the model's answer rather
    /// than something the view can re-derive from an empty array.
    public func emptiness(for id: Int, by relation: Relation) -> String {
        relation != .parent && self[id]?.status.isResolved != false
            ? "A finished task has no dependencies."
            : "Nothing to pick."
    }

    public func isRelated(_ candidate: Int, to id: Int, by relation: Relation) -> Bool {
        switch relation {
        case .blockedBy: self[id]?.depends.contains(candidate) ?? false
        case .blocks: self[candidate]?.depends.contains(id) ?? false
        case .parent: self[id]?.parent == candidate
        }
    }

    /// Ready to pick up: todo, unblocked, no unresolved children. Matches `sd next`.
    public func isNext(_ task: SeedTask) -> Bool {
        task.status == .todo
            && task.depends.isEmpty
            && task.children.allSatisfy { byID[$0]?.status.isResolved ?? true }
    }

    /// A scope answers "what can I start", and a parent that cannot be started
    /// is noise in that answer — so those lists do not nest.
    public func flat(matching predicate: (SeedTask) -> Bool) -> [OutlineRow] {
        sorted(tasks.filter(predicate)).map {
            OutlineRow(task: $0, depth: 0, hasChildren: false)
        }
    }
}
