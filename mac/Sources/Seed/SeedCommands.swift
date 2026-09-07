import AppKit
import SeedKit
import SwiftUI

struct SeedCommands: Commands {
    @FocusedValue(\.workspace) private var workspace
    @Environment(\.openWindow) private var openWindow


    private var task: SeedTask? { workspace?.selectedTask }


    var body: some Commands {
        fileCommands
        findCommands
        sidebarCommands
        viewCommands
        taskCommands
    }

    private var fileCommands: some Commands {
        CommandGroup(replacing: .newItem) {
            Button("New Task") { workspace?.compose() }
            .keyboardShortcut("n")
            .disabled(workspace?.showsTasks != true)

            Button("New Subtask") { workspace?.compose(parent: task?.id) }
            .keyboardShortcut("n", modifiers: [.command, .shift])
            .disabled(task == nil)

            Divider()

            Button("Open Repository…", action: chooseRepository)
                .keyboardShortcut("o")

            Menu("Open Recent") {
                ForEach(Recents.shared.urls, id: \.self) { url in
                    Button(url.lastPathComponent) { open(url) }
                }
                if !Recents.shared.urls.isEmpty {
                    Divider()
                    Button("Clear Menu") { Recents.shared.clear() }
                }
            }
            .disabled(Recents.shared.urls.isEmpty)

            Divider()

            // Each item states what it will move, so the choice is a sentence
            // rather than a duration guessed at blind.
            Menu("Archive") {
                ForEach(workspace?.sweeps ?? [], content: sweepButton)
            }
            .disabled(nothingToArchive)
        }
    }

    private var findCommands: some Commands {
        CommandGroup(after: .textEditing) {
            Button("Find") { workspace?.focusSearch() }
                .keyboardShortcut("f")
                .disabled(workspace?.showsTasks != true)
        }
    }

    private var sidebarCommands: some Commands {
        CommandGroup(after: .sidebar) {
            Button(workspace?.sidebarIsHidden == true ? "Show Sidebar" : "Hide Sidebar") {
                workspace?.toggleSidebar()
            }
            .keyboardShortcut("b")
            .disabled(workspace == nil)

            Divider()

            ForEach(Array(Workspace.Scope.standard.enumerated()), id: \.element) { index, scope in
                scopeButton(scope, index: index)
            }
        }
    }

    private var viewCommands: some Commands {
        CommandGroup(after: .toolbar) {
            Toggle("Show Archived Tasks", isOn: archivedBinding)
                .disabled(workspace?.showsTasks != true)
            Button("Refresh") { workspace?.reload() }
                .keyboardShortcut("r")
                .disabled(workspace == nil)
            Divider()
        }
    }

    private var taskCommands: some Commands {
        CommandMenu("Task") {
            TaskActions(workspace: workspace, task: task)
        }
    }

    private var nothingToArchive: Bool {
        workspace?.sweeps.allSatisfy { $0.count == 0 } ?? true
    }

    private func scopeButton(_ scope: Workspace.Scope, index: Int) -> some View {
        let key = KeyEquivalent(Character("\(index + 1)"))
        return Button(scope.name) { workspace?.scope = scope }
            .keyboardShortcut(key)
            .disabled(workspace?.showsTasks != true)
    }

    private func sweepButton(_ sweep: Workspace.Sweep) -> some View {
        Button("\(sweep.name) (\(sweep.count))") { workspace?.sweeping = sweep }
            .disabled(sweep.count == 0)
    }

    private func chooseRepository() {
        guard let url = Workspace.chooseRepository() else { return }
        open(url)
    }

    /// An empty window takes the repository itself; anything else gets a window of
    /// its own, so opening a second repository never closes the first.
    private func open(_ url: URL) {
        if let workspace, workspace.repository == nil {
            workspace.open(url)
        } else {
            openWindow(value: url)
        }
    }

    private var archivedBinding: Binding<Bool> {
        Binding(get: { workspace?.includeArchived ?? false }) { workspace?.includeArchived = $0 }
    }
}

/// Menu commands act on the window that has focus, which is how each window's
/// repository stays its own.
extension FocusedValues {
    var workspace: Workspace? {
        get { self[WorkspaceKey.self] }
        set { self[WorkspaceKey.self] = newValue }
    }

    private struct WorkspaceKey: FocusedValueKey {
        typealias Value = Workspace
    }
}
