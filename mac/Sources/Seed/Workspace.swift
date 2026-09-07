import AppKit
import Observation
import SeedKit
import SwiftUI

@MainActor
@Observable
final class Workspace {
    enum Scope: Hashable {
        case all, next, inProgress, completed
        case label(String)

        /// The four the sidebar lists, in its order — what ⌘1 through ⌘4 pick.
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

    private enum State: Equatable {
        case loading
        case noRepository
        /// A folder that opened but holds no `.seed` — the app can make one.
        case uninitialized
        case missingBinary
        case ready
        case failed(String)
    }

    struct Failure {
        let title: String
        let message: String
        /// Set when re-running the same command with `--force` would succeed.
        let force: [String]?
    }

    struct Composition: Identifiable {
        let id = UUID()
        var parent: Int?
    }

    /// Which task the picker was opened on, and which of the three it opens
    /// showing — the picker switches between them itself from there.
    struct Relating: Identifiable {
        let id = UUID()
        let task: Int
        let opening: Relation
    }

    private(set) var repository: URL?
    private(set) var graph = TaskGraph([])
    private var state = State.loading

    var columns = NavigationSplitViewVisibility.automatic
    private(set) var expanded: Set<Int> = []
    /// Set when the app moves the selection rather than the user, so the list
    /// knows to scroll to it. The token is what makes the same task revealed
    /// twice running read as two requests: `onChange` fires on a change, and
    /// nothing clears this in between.
    struct Reveal: Equatable {
        let id: Int
        private let token = UUID()
    }
    private(set) var revealed: Reveal?
    var scope: Scope? = .all
    var selection: Int? {
        didSet {
            // Every pane swap flushes: the draft belongs to the task it was
            // typed against, not to whatever the pane shows next.
            commitEditing()
            labelling = nil
        }
    }
    var search = ""
    var includeArchived = false { didSet { reload() } }
    /// A queue, not a slot. The alert is window-modal and `reload()` reverts the
    /// edit underneath it, so a failure dropped on the floor is an edit that
    /// undoes itself with nothing on screen saying why.
    var failures: [Failure] = []
    var composition: Composition?
    var relating: Relating?
    var sweeping: Sweep?
    /// The description being edited and the text so far. One value rather than
    /// a flag beside a draft in the pane's own state: whatever is about to
    /// replace the pane has to be able to ask which task has unsaved text, and
    /// a flag that `selection` clears on its way past cannot answer that. The
    /// id travels with the draft, so a commit always compares against the task
    /// the text was typed for.
    struct Editing: Equatable {
        let id: Int
        var draft: String
    }
    var editing: Editing?
    /// Raised by ⌘F. A count rather than a flag: a flag has to be written back
    /// by the view that serves it, and a second ⌘F with no intervening reset is
    /// then no change at all. See `Reveal` for the same problem with a payload.
    private(set) var focusSearchRequests = 0
    /// The task whose labels popover is open, so the menu bar can open it too.
    var labelling: Int?

    /// A task written but not yet on screen. The composer stays up across both
    /// steps — `run` starts the reload it does not await, so the id comes back
    /// well before the graph holds it — and Return stays live the whole time,
    /// so this is what keeps a second press from writing a second task.
    private enum Creation {
        case writing
        case revealing(Int)
    }
    private var creating: Creation?

    private var watcher: DirectoryWatcher?
    /// Commands run one at a time: `sd` rejects a write whose task file changed
    /// since it read it, so two overlapping edits fail the second one.
    private var pending = Task<Data?, Never> { nil }
    private var reloading: Task<Void, Never>?

    /// Seeds a window that opened without one of its own: the last repository
    /// opened in any window.
    static var remembered: URL? { Recents.shared.firstWorkspace }

    var selectedTask: SeedTask? { selection.flatMap { graph[$0] } }

    /// Naming happens in the detail pane, so the selection steps aside — the
    /// pane cannot show a task and a task being named at the same time.
    func compose(parent: Int? = nil) {
        composition = Composition(parent: parent)
        selection = nil
    }

    func toggle(_ id: Int) {
        setExpanded(id, !expanded.contains(id))
    }

    func setExpanded(_ id: Int, _ open: Bool) {
        guard open else {
            expanded.remove(id)
            return
        }
        // A task with no children has nothing to open, and an id stored for one
        // would unfold the row by itself on the day it gets a child. Reachable
        // by double-clicking a leaf, which looks like it did nothing.
        guard graph[id]?.children.isEmpty == false else { return }
        expanded.insert(id)
    }

    /// ← on an open row closes it, and on a closed one selects its parent, so
    /// walking back out of a deep branch never needs the mouse. Returns whether
    /// there was anything to do, so an unhandled key falls through.
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

    /// → opens a closed row, and steps into the first child of one already
    /// open — the same pair of meanings ← has on the way back out.
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

    /// Selecting on the user's behalf — a new subtask, a link in the detail pane.
    /// Whatever hides the task has to open, or the selection lands out of sight.
    /// The scope and the search hide tasks as surely as a closed parent does:
    /// ⌘N under a label scope makes a task carrying no labels, and a link can
    /// point outside the current filter.
    func reveal(_ id: Int) {
        expanded.formUnion(graph.ancestors(of: id))
        if !visibleRows.contains(where: { $0.id == id }) {
            scope = .all
            search = ""
        }
        selection = id
        revealed = Reveal(id: id)
    }

    func focusSearch() { focusSearchRequests += 1 }

    /// A short confirmation the window shows and then forgets — the thing that
    /// tells you ⌘⇧C did anything at all. Held here rather than in the pane
    /// because both routes to it have to look identical, and a menu item has no
    /// view to reach into.
    /// Tokened for the reason `Reveal` and `focusSearchRequests` are: copying
    /// the same id twice sets the same text, and a bare `String?` would make
    /// that no change at all — leaving the announcement unspoken for exactly
    /// the people who have no badge to look at.
    struct Confirmation: Equatable {
        let text: String
        private let token = UUID()
    }
    private(set) var confirmation: Confirmation?
    private var confirmationDismissal: Task<Void, Never>?

    func copyID(_ task: SeedTask) {
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(String(task.id), forType: .string)
        confirm("Copied #\(task.id)")
    }

    private func confirm(_ text: String) {
        confirmation = Confirmation(text: text)
        // Cancelled and restarted, so copying twice extends the confirmation
        // rather than having the first one's timer cut the second one short.
        confirmationDismissal?.cancel()
        confirmationDismissal = Task {
            try? await Task.sleep(for: .seconds(1.6))
            guard !Task.isCancelled else { return }
            confirmation = nil
        }
    }

    func beginEditing() {
        guard let id = selection, let task = graph[id] else { return }
        editing = Editing(id: id, draft: task.description ?? "")
    }

    /// Idempotent, and that is the point: ⌘E, Escape, a click on empty space,
    /// ⌘N, a new selection and the pane being torn down all arrive here, some
    /// of them twice and in an order AppKit decides. Clearing first means a
    /// second call has nothing left to write.
    func commitEditing() {
        guard let editing else { return }
        self.editing = nil
        guard editing.draft != (graph[editing.id]?.description ?? "") else { return }
        let write = edit(editing.id, .description(editing.draft))
        Task {
            // A failed write is where the typed text would otherwise be lost —
            // the alert carries `sd`'s stderr, not the description — so put the
            // editor back. Only onto an idle pane still showing that task: a
            // second edit started while this was in flight is newer than this.
            guard await write.value == nil, selection == editing.id, self.editing == nil
            else { return }
            self.editing = editing
        }
    }

    /// What the window is showing, in the cases the view actually draws. `state`
    /// is the loader's business; this is the one value both the view and the
    /// menu bar read, so a pane and the commands aimed at it cannot disagree.
    /// A failed reload with tasks already on screen keeps showing them.
    enum Presentation: Equatable {
        case loading
        case noRepository
        case uninitialized(URL)
        case missingBinary
        case loadFailed(String)
        case tasks(stale: Bool)
    }

    var presentation: Presentation {
        switch state {
        case .loading: .loading
        case .noRepository: .noRepository
        case .uninitialized: repository.map(Presentation.uninitialized) ?? .noRepository
        case .missingBinary: .missingBinary
        case .ready: .tasks(stale: false)
        case .failed(let message):
            graph.tasks.isEmpty ? .loadFailed(message) : .tasks(stale: true)
        }
    }

    /// Whether the sidebar, the search field and the composer exist in this
    /// window — the menu bar disables against it.
    var showsTasks: Bool {
        if case .tasks = presentation { return true }
        return false
    }

    /// The outline only exists unfiltered; every other scope is a flat list, so
    /// ← and → have nothing to act on. Derived from the same normalized values
    /// `visibleRows` branches on, so the two cannot disagree about which one the
    /// list is showing.
    var showsTree: Bool { showsTasks && activeScope == .all && query.isEmpty }

    /// `scope` is the sidebar's selection binding and goes nil on a deselect.
    private var activeScope: Scope { scope ?? .all }
    private var query: String { search.trimmingCharacters(in: .whitespaces) }

    var sidebarIsHidden: Bool { columns == .doubleColumn || columns == .detailOnly }

    /// Animated to match the toolbar's own sidebar button, which is the same
    /// action arriving by a different route.
    func toggleSidebar() {
        withAnimation { columns = sidebarIsHidden ? .all : .doubleColumn }
    }

    /// Set in Settings; empty means the copy bundled beside the app.
    static let binaryPathKey = "seedBinaryPath"

    private var binary: URL? {
        SeedCLI.locate(override: UserDefaults.standard.string(forKey: Self.binaryPathKey) ?? "")
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

    /// A window with nothing to show lands on the empty state. A window adopts
    /// one repository and keeps it: every caller is guarded to a workspace that
    /// has never loaded one, which is what makes a reset unnecessary here rather
    /// than merely absent. A second repository gets a second window.
    func open(_ url: URL?) {
        assert(repository == nil, "a window adopts one repository")
        guard let url = url.map(SeedCLI.directory) else { return state = .noRepository }
        repository = url
        // A folder with no `.seed` is a mistake at the Open panel, not somewhere
        // to reopen: the head of this list is what a fresh window opens.
        if SeedCLI.isWorkspace(url) { Recents.shared.add(url) }
        state = .loading
        reload()
    }

    /// `.seed` rather than the repository: the watch covers a whole tree, and the
    /// project around it changes for reasons that are none of ours.
    private func watch() {
        guard let repository else { return }
        watcher = DirectoryWatcher(url: repository.appending(path: ".seed")) { [weak self] in
            Task { @MainActor in self?.reload() }
        }
    }

    func reload() {
        guard let repository else { return state = .noRepository }
        guard let binary else { return state = .missingBinary }
        guard SeedCLI.isWorkspace(repository) else { return state = .uninitialized }
        // A stream over a path that does not exist is inert rather than refused,
        // so the store has to be there before there is anything to watch. Arming
        // here is what picks up a repository that became one after the window
        // opened — `sd init` from the first-run pane, or from a terminal.
        if watcher == nil { watch() }
        // Three things ask for a reload — a command finishing, the watcher, and
        // the app coming forward — so an older read can outlive a newer one.
        reloading?.cancel()
        reloading = Task {
            do {
                let tasks = try await SeedCLI.list(
                    binary: binary, repository: repository, includeArchived: includeArchived
                )
                guard !Task.isCancelled else { return }
                graph = TaskGraph(tasks)
                // An id whose task lost its last child would otherwise sit here
                // and silently unfold the row on the day it gets another.
                expanded.formIntersection(
                    graph.tasks.lazy.filter { !$0.children.isEmpty }.map(\.id)
                )
                state = .ready
                settleReveal()
            } catch {
                guard !Task.isCancelled else { return }
                state = .failed(error.localizedDescription)
            }
        }
    }

    /// `sd init` first — priming needs a store to write its hook beside.
    func bootstrap(primeClaude: Bool) {
        guard let repository else { return }
        state = .loading

        let created = run(["init"], title: "Couldn't Create the Repository")
        Task {
            guard await created.value != nil else { return }
            Recents.shared.add(repository)
            guard primeClaude else { return }
            run(["prime", "--install", "claude"], title: "Couldn't Prime Claude Code")
        }
    }

    // MARK: - Editing

    @discardableResult
    func edit(_ id: Int, _ edit: Edit) -> Task<Data?, Never> {
        run(["edit", String(id)] + edit.arguments, forceable: edit.forceable)
    }

    func create(title: String, parent: Int?) {
        let title = title.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !title.isEmpty else { return composition = nil }
        guard creating == nil else { return }
        creating = .writing
        let add = run(SeedCLI.addArguments(title: title, parent: parent))
        Task {
            // The composer stays up until the task is on screen, so the pane is
            // never empty in between and a failed write keeps the typed title.
            guard let output = await add.value else { return creating = nil }
            let text = String(decoding: output, as: UTF8.self)
                .trimmingCharacters(in: .whitespacesAndNewlines)
            guard let id = Int(text) else {
                creating = nil
                return self.composition = nil
            }
            creating = .revealing(id)
            settleReveal()
        }
    }

    func retry(_ arguments: [String]) {
        run(arguments + ["--force"])
    }

    /// The returned task carries the command's stdout, or nil if it failed —
    /// awaiting it is how an editor knows whether it can close.
    @discardableResult
    private func run(
        _ arguments: [String], forceable: Bool = false,
        title: String = "Couldn't Update Task"
    ) -> Task<Data?, Never> {
        guard let repository, let binary else {
            // Returning a bare nil is indistinguishable from a failed command,
            // and leaves whatever set `.loading` spinning on it forever.
            reload()
            return Task { nil }
        }
        let previous = pending
        let work = Task { () -> Data? in
            _ = await previous.value
            var output: Data?
            do {
                output = try await SeedCLI.run(arguments, binary: binary, repository: repository)
            } catch {
                failures.append(
                    Failure(
                        title: title,
                        message: error.localizedDescription,
                        force: forceable ? arguments : nil
                    )
                )
            }
            reload()
            return output
        }
        pending = work
        return work
    }

    /// Called by every reload that lands: whichever one brings the task in gets
    /// to show it, and the composer steps aside in the same breath.
    private func settleReveal() {
        guard case .revealing(let id) = creating, graph[id] != nil else { return }
        creating = nil
        reveal(id)
        composition = nil
    }

    // MARK: - Archiving

    /// What `sd archive` moves: resolved tasks it has not moved already. The
    /// cutoff reads a task's last change rather than when it was resolved, so
    /// the menu says "untouched" rather than "finished".
    struct Sweep: Identifiable {
        let name: String
        /// A humantime duration, or nil for everything.
        let cutoff: String?
        let count: Int

        var id: String { cutoff ?? "all" }
    }

    var sweeps: [Sweep] {
        let resolved = graph.tasks.filter { $0.status.isResolved && !$0.archived }
        // The cutoff `sd` is given and the number counted here are derived from
        // one value: as two literals a menu item could count 31 days and
        // archive 30, with nothing to catch it and no undo.
        let sweep = { (name: String, days: Int) in
            let before = Date().addingTimeInterval(-Double(days) * 86_400)
            return Sweep(
                name: name,
                cutoff: "\(days)d",
                count: resolved.count { $0.modified <= before }
            )
        }
        return [
            Sweep(name: "All Completed Tasks", cutoff: nil, count: resolved.count),
            sweep("Untouched for a Day", 1),
            sweep("Untouched for a Week", 7),
        ]
    }

    func archive(_ sweep: Sweep) {
        run(["archive"] + (sweep.cutoff.map { [$0] } ?? []), title: "Couldn't Archive")
    }

    // MARK: - Presented tasks

    var labels: [String] { graph.labels }

    /// What the window subtitle carries: what is left to do. Not `count(.all)`,
    /// which is every task including the finished ones.
    var openCount: Int { graph.tasks.count { !$0.status.isResolved } }

    func count(_ scope: Scope) -> Int {
        graph.tasks.filter { matches($0, scope) }.count
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
        let scope = activeScope
        let wanted = { (task: SeedTask) in
            self.matches(task, scope) && (query.isEmpty || task.matches(query))
        }
        guard scope == .all else { return graph.flat(matching: wanted) }
        return query.isEmpty ? graph.outline(expanded: expanded) : graph.outline(matching: wanted)
    }

    // MARK: - Relations

    /// The picker refuses a barred candidate here rather than in the view: the
    /// mouse path is disabled but the keyboard path is not, and one refusal
    /// should not be enforced in two places.
    func relate(_ candidate: Int, to id: Int, by relation: Relation, on: Bool) {
        guard graph.candidates(for: id, by: relation)
            .first(where: { $0.id == candidate })?.barred == nil
        else { return }
        let write = relation.edit(candidate, to: id, on: on)
        edit(write.task, write.edit)
    }
}
