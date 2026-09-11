import AppKit
import Observation
import SeedKit
import SwiftUI

/// One window's view of its `Store`: what is selected, open, searched, and
/// being edited. The store owns the tasks and the `sd` that writes them.
@MainActor
@Observable
final class Workspace {
    enum Scope: Hashable {
        case all, next, inProgress, completed
        case label(String)

        /// The sidebar's order, which is what ⌘1 through ⌘4 pick.
        static let standard: [Scope] = [.all, .next, .inProgress, .completed]

        var name: String {
            switch self {
            case .all: "All Tasks"
            case .next: "Next Up"
            case .inProgress: "In Progress"
            case .completed: "Completed"
            case .label(let label): label
            }
        }

        var symbol: String {
            switch self {
            case .all: "list.bullet.indent"
            case .next: "arrow.forward.circle"
            case .inProgress: "circle.lefthalf.filled"
            case .completed: "checkmark.circle"
            case .label: "tag"
            }
        }
    }

    struct Composition: Identifiable {
        let id = UUID()
        var parent: Int?
    }

    /// Which task the relations picker opened on, and which segment it opened showing.
    struct Relating: Identifiable {
        let id = UUID()
        let task: Int
        let opening: Relation
    }

    /// The description being edited and its text so far. The id travels with
    /// the draft so a commit always compares against the task it was typed for.
    struct Editing: Equatable {
        let id: Int
        var draft: String
    }

    /// Equal only to itself, so assigning one is a change `onChange` sees even
    /// when the value repeats: the same task revealed twice, the same id copied twice.
    struct Stamped<Value>: Equatable {
        let value: Value
        private let stamp = UUID()

        init(_ value: Value) {
            self.value = value
        }

        static func == (a: Self, b: Self) -> Bool { a.stamp == b.stamp }
    }

    let store = Store()
    var graph: TaskGraph { store.graph }

    var columns = NavigationSplitViewVisibility.automatic
    private(set) var expanded: Set<Int> = []
    /// A selection the app moved rather than the user, which the list scrolls to.
    private(set) var revealed: Stamped<Int>?
    var scope = Scope.all
    var selection: Int? {
        didSet {
            // The draft belongs to the task it was typed against, not to
            // whatever the pane shows next.
            commitEditing()
            labelling = nil
        }
    }
    var search = ""
    var composition: Composition?
    var relating: Relating?
    var sweeping: Store.Sweep?
    var editing: Editing?
    /// Raised by ⌘F.
    private(set) var searchFocus: Stamped<Void>?
    /// The task whose labels popover is open, so the menu bar can open it too.
    var labelling: Int?
    /// A confirmation the window shows briefly.
    private(set) var confirmation: Stamped<String>?
    private var confirmationDismissal: Task<Void, Never>?
    /// Return stays live while a task is being written; this keeps a second
    /// press from writing a second task.
    private var creating = false

    /// The last repository opened in any window, for a window with none of its own.
    static var remembered: URL? { Recents.shared.firstWorkspace }

    var selectedTask: SeedTask? { selection.flatMap { graph[$0] } }

    /// Whether the sidebar, the search field and the composer exist in this window.
    var showsTasks: Bool {
        if case .tasks = store.presentation { return true }
        return false
    }

    /// The outline exists only unfiltered; every other scope is a flat list.
    var showsTree: Bool { showsTasks && scope == .all && query.isEmpty }

    private var query: String { search.trimmingCharacters(in: .whitespaces) }

    // MARK: - Tree

    func toggle(_ id: Int) {
        setExpanded(id, !expanded.contains(id))
    }

    func setExpanded(_ id: Int, _ open: Bool) {
        guard open else {
            expanded.remove(id)
            return
        }
        // A leaf has nothing to open, and an id stored for one would unfold
        // the row the day it gets a child.
        guard graph[id]?.children.isEmpty == false else { return }
        expanded.insert(id)
    }

    /// ← closes an open row and selects the parent of a closed one. Returns
    /// whether there was anything to do, so an unhandled key falls through.
    func collapseSelection() -> Bool {
        guard showsTree, let id = selection else { return false }
        if expanded.contains(id) {
            setExpanded(id, false)
        } else if let parent = graph[id]?.parent {
            selection = parent
        } else {
            return false
        }
        return true
    }

    /// → opens a closed row and steps into the first child of an open one.
    func expandSelection() -> Bool {
        guard showsTree, let id = selection, let task = graph[id] else { return false }
        if expanded.contains(id) {
            guard let child = task.children.first else { return false }
            selection = child
        } else {
            guard !task.children.isEmpty else { return false }
            setExpanded(id, true)
        }
        return true
    }

    /// Selecting on the user's behalf. Whatever hides the task has to open,
    /// the scope and the search included, or the selection lands out of sight.
    func reveal(_ id: Int) {
        expanded.formUnion(graph.ancestors(of: id))
        if !visibleRows.contains(where: { $0.id == id }) {
            scope = .all
            search = ""
        }
        selection = id
        revealed = Stamped(id)
    }

    func focusSearch() {
        searchFocus = Stamped(())
    }

    var sidebarIsHidden: Bool { columns == .doubleColumn || columns == .detailOnly }

    /// Animated to match the toolbar's own sidebar button.
    func toggleSidebar() {
        withAnimation { columns = sidebarIsHidden ? .all : .doubleColumn }
    }

    // MARK: - Presented tasks

    var labels: [String] { graph.labels }

    /// What is left to do, for the window subtitle.
    var openCount: Int { graph.tasks.count { !$0.status.isResolved } }

    func count(_ scope: Scope) -> Int {
        graph.tasks.count { matches($0, scope) }
    }

    private func matches(_ task: SeedTask, _ scope: Scope) -> Bool {
        switch scope {
        case .all: true
        case .next: graph.isNext(task)
        case .inProgress: task.status == .inProgress
        case .completed: task.status.isResolved
        case .label(let label): task.labels.contains(label)
        }
    }

    var visibleRows: [OutlineRow] {
        let query = query
        let scope = scope
        let wanted = { (task: SeedTask) in
            self.matches(task, scope) && (query.isEmpty || task.matches(query))
        }
        guard scope == .all else { return graph.flat(matching: wanted) }
        return query.isEmpty ? graph.outline(expanded: expanded) : graph.outline(matching: wanted)
    }

    // MARK: - Composing

    /// Naming happens in the detail pane, so the selection steps aside.
    func compose(parent: Int? = nil) {
        composition = Composition(parent: parent)
        selection = nil
    }

    /// Nothing is written until the title is committed: `sd` has no delete.
    func create(title: String, parent: Int?) {
        let title = title.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !title.isEmpty else {
            composition = nil
            return
        }
        guard !creating else { return }
        creating = true
        Task {
            defer { creating = false }
            // The composer stays up until the task is on screen, so a failed
            // write keeps the typed title.
            guard let id = await store.add(title: title, parent: parent).value else { return }
            reveal(id)
            composition = nil
        }
    }

    // MARK: - Editing

    func beginEditing() {
        guard let id = selection, let task = graph[id] else { return }
        editing = Editing(id: id, draft: task.description ?? "")
    }

    /// Idempotent: ⌘E, Escape, a click elsewhere, ⌘N, a new selection and the
    /// pane's teardown all arrive here, some twice, in an order AppKit decides.
    func commitEditing() {
        guard let editing else { return }
        self.editing = nil
        guard editing.draft != (graph[editing.id]?.description ?? "") else { return }
        let write = store.edit(editing.id, .description(editing.draft))
        Task {
            // A failed write puts the draft back, but only onto an idle pane
            // still showing that task.
            guard await write.value == nil, selection == editing.id, self.editing == nil
            else { return }
            self.editing = editing
        }
    }

    func copyID(_ task: SeedTask) {
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(String(task.id), forType: .string)
        confirm("Copied #\(task.id)")
    }

    private func confirm(_ text: String) {
        confirmation = Stamped(text)
        // Restarted, so a second copy extends the confirmation rather than
        // being cut short by the first one's timer.
        confirmationDismissal?.cancel()
        confirmationDismissal = Task {
            try? await Task.sleep(for: .seconds(1.6))
            guard !Task.isCancelled else { return }
            confirmation = nil
        }
    }

    // MARK: - Repository

    /// The caller decides whether the choice belongs in this window or a new one.
    static func chooseRepository() -> URL? {
        let panel = NSOpenPanel()
        panel.canChooseDirectories = true
        panel.canChooseFiles = false
        panel.prompt = "Open"
        panel.message = "Choose a folder that contains a .seed directory."
        guard panel.runModal() == .OK, let url = panel.url else { return nil }
        return SeedCLI.directory(url)
    }
}
