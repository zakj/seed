import SeedKit
import SwiftUI

struct TaskListView: View {
    @Environment(Workspace.self) private var workspace
    @FocusState private var searching: Bool

    var body: some View {
        @Bindable var workspace = workspace
        let rows = workspace.visibleRows

        ScrollViewReader { list in
            List(rows, selection: $workspace.selection) { row in
                TaskRow(row: row)
            }
            // The list's own double-click and right-click, rather than gestures
            // in the row: a tap gesture there competes with the list for the
            // same click and selection stops working intermittently.
            .contextMenu(forSelectionType: Int.self) { ids in
                if let task = ids.first.flatMap({ workspace.graph[$0] }) {
                    TaskActions(workspace: workspace, task: task)
                    Divider()
                    Button("New Subtask") { workspace.compose(parent: task.id) }
                }
            } primaryAction: { ids in
                guard workspace.showsTree else { return }
                for id in ids { workspace.toggle(id) }
            }
            // Not animated: the transaction would also catch the detail pane
            // swapping from the composer to the created task, and cross-fade it.
            .onChange(of: workspace.revealed) { _, reveal in
                guard let reveal else { return }
                list.scrollTo(reveal.id)
            }
            // A row that survives a filter change can still land at a different
            // offset, which moves it out of view without changing the selection.
            // Keyed off the selection, not `revealed`: that one is sticky for
            // the window's life, so reading it here would drag the list back to
            // the last revealed task on every filter change forever.
            .onChange(of: rows.map(\.id)) { _, _ in
                guard let id = workspace.selection else { return }
                list.scrollTo(id)
            }
        }
        // ← and → are what every macOS outline uses, and unlike a tap gesture in
        // the row they do not compete with the list for a click.
        .onKeyPress(.leftArrow) { workspace.collapseSelection() ? .handled : .ignored }
        .onKeyPress(.rightArrow) { workspace.expandSelection() ? .handled : .ignored }
        .searchable(text: $workspace.search, prompt: "Search Tasks")
        .searchFocused($searching)
        .onChange(of: workspace.focusSearchRequests) { _, _ in searching = true }
        .overlay {
            if rows.isEmpty { emptyState }
        }
        .toolbar {
            ToolbarItem(placement: .navigation) {
                Button {
                    workspace.compose()
                } label: {
                    Label("New Task", systemImage: "plus")
                }
                .help("New task (⌘N)")
            }
        }
    }

    @ViewBuilder
    private var emptyState: some View {
        if !workspace.search.isEmpty {
            ContentUnavailableView.search(text: workspace.search)
        } else {
            ContentUnavailableView {
                Label("Nothing Here", systemImage: "checkmark.circle")
            } description: {
                Text(workspace.scope == .next
                     ? "No task is unblocked and ready to start."
                     : "No tasks match this list.")
            }
        }
    }
}

struct TaskRow: View {
    @Environment(Workspace.self) private var workspace
    let row: OutlineRow

    private var task: SeedTask { row.task }

    var body: some View {
        HStack(spacing: 7) {
            disclosure

            StatusIcon(task: task)

            Text(task.title)
                .lineLimit(1)
                .truncationMode(.tail)
                .foregroundStyle(titleStyle)

            PriorityMark(priority: task.priority)

            Spacer(minLength: 6)

            Text(String(task.id))
                .monospacedDigit()
                .foregroundStyle(.tertiary)
        }
        .padding(.vertical, 1)
    }

    /// A row carried along to show where a search match sits reads as context,
    /// not as an answer.
    private var titleStyle: HierarchicalShapeStyle {
        if !row.matches { return .tertiary }
        return task.status.isResolved ? .secondary : .primary
    }

    /// A childless row draws the triangle too, invisibly: omitting it makes the
    /// row measure differently, and a level's titles stop lining up.
    private var disclosure: some View {
        let open = workspace.expanded.contains(task.id)

        return Button { workspace.toggle(task.id) } label: {
            Image(systemName: "chevron.right")
                .imageScale(.small)
                .foregroundStyle(.secondary)
                .rotationEffect(.degrees(open ? 90 : 0))
                .frame(width: 11, height: 11)
                .contentShape(.rect)
        }
        .buttonStyle(.plain)
        .opacity(row.hasChildren ? 1 : 0)
        .disabled(!row.hasChildren)
        .accessibilityLabel(open ? "Collapse" : "Expand")
        // The invisible placeholder is layout, not a control anyone can use.
        .accessibilityHidden(!row.hasChildren)
        .padding(.leading, CGFloat(row.depth) * 16)
        .animation(.easeOut(duration: 0.12), value: open)
    }
}

struct StatusIcon: View {
    let task: SeedTask

    var body: some View {
        Image(systemName: symbol)
            .foregroundStyle(tint)
            .imageScale(.medium)
            .help(task.isBlocked ? "Blocked" : task.status.name)
    }

    private var symbol: String {
        task.isBlocked ? "circle.dotted" : task.status.symbol
    }

    private var tint: HierarchicalShapeStyle {
        switch task.status {
        case .inProgress: .primary
        case .todo: task.isBlocked ? .tertiary : .secondary
        case .done, .dropped: .tertiary
        }
    }
}

/// The same symbols the priority menu shows, so a row and the control that
/// changes it are never drawing the same priority two different ways.
struct PriorityMark: View {
    let priority: Priority

    var body: some View {
        switch priority {
        case .critical, .high:
            Image(systemName: priority.symbol)
                .imageScale(.small)
                .fontWeight(.bold)
                .foregroundStyle(.tint)
                .help("\(priority.name) priority")
        case .normal:
            EmptyView()
        case .low:
            Image(systemName: priority.symbol)
                .imageScale(.small)
                .foregroundStyle(.tertiary)
                .help("Low priority")
        }
    }
}

extension Status {
    /// Menu order, which is the order the work goes in rather than the order the
    /// cases are declared: start it, finish it, and only then the two ways back
    /// out. `allCases` reads as "Move Back to To Do" first, which is nobody's
    /// first thought about a task.
    static let menuOrder: [Status] = [.inProgress, .done, .todo, .dropped]

    /// The key that sets this status. `todo` has none — it is the state a task
    /// starts in, reached by moving back rather than by aiming at it.
    var shortcut: KeyEquivalent? {
        switch self {
        case .inProgress: "s"
        case .done: "d"
        case .todo: nil
        case .dropped: .delete
        }
    }
}

/// Everything the Task menu offers, rendered by both the menu bar and a row's
/// context menu — which act on the same task, since `contextMenu(forSelectionType:)`
/// selects the row before building. Written once because the two had already
/// drifted apart about which actions exist.
///
/// The buttons carry their key equivalents here rather than only in the menu
/// bar: a shortcut inside a context menu is displayed but never registered,
/// checked by giving one a chord nothing else used and finding it inert.
///
/// Both inputs are optional because the menu bar exists with no window focused
/// and no task selected, where every item has to appear and disable rather than
/// vanish.
struct TaskActions: View {
    let workspace: Workspace?
    let task: SeedTask?

    private var editing: Bool { workspace?.editing != nil }

    var body: some View {
        ForEach(Status.menuOrder) { status in
            Button(status.verb) { apply(.status(status)) }
                // Disabled mid-edit: a menu key equivalent beats the field
                // editor, so ⌘S in a description would set a status and log it,
                // and `sd` has no undo.
                .keyboardShortcut(status.shortcut.map { KeyboardShortcut($0) })
                .disabled(task == nil || task?.status == status || editing)
        }

        Divider()

        Menu("Priority") {
            ForEach(Priority.allCases) { priority in
                Button(priority.name) { apply(.priority(priority)) }
                    .disabled(task == nil || task?.priority == priority)
            }
        }

        Divider()

        // Toggles, so the key that starts an edit also ends it. Both halves are
        // the workspace's, which is why this needs no view to reach into and no
        // opinion about which window is key.
        Button(editing ? "Save Description" : "Edit Description") {
            if editing { workspace?.commitEditing() } else { workspace?.beginEditing() }
        }
        .keyboardShortcut("e")
        .disabled(task == nil)

        Button("Edit Labels…") {
            if let task { workspace?.labelling = task.id }
        }
        .disabled(task == nil)

        Button("Relations…") {
            if let task { workspace?.relating = .init(task: task.id, opening: .blockedBy) }
        }
        .keyboardShortcut("r", modifiers: [.command, .shift])
        .disabled(task == nil)

        Divider()

        Button("Copy Task ID") {
            if let task { workspace?.copyID(task) }
        }
            .keyboardShortcut("c", modifiers: [.command, .shift])
            .disabled(task == nil)
    }

    private func apply(_ edit: Edit) {
        guard let task, let workspace else { return }
        workspace.edit(task.id, edit)
    }
}
