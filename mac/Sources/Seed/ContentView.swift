import AppKit
import SeedKit
import SwiftUI

struct ContentView: View {
    @Environment(Workspace.self) private var workspace

    var body: some View {
        @Bindable var workspace = workspace

        Group {
            switch workspace.store.presentation {
            case .loading:
                ProgressView()
                    .controlSize(.large)
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            case .noRepository:
                ContentUnavailableView {
                    Label("No Repository Open", systemImage: "folder.badge.questionmark")
                } description: {
                } actions: {
                    Button("Open Repository…") {
                        if let url = Workspace.chooseRepository() { workspace.store.open(url) }
                    }
                }
            case .uninitialized(let repository):
                BootstrapView(repository: repository)
                    .id(repository)
            case .missingBinary:
                ContentUnavailableView {
                    Label("Can't Find sd", systemImage: "terminal")
                } description: {
                    Text("The copy of sd bundled with Seed is missing. Reinstall the app.")
                }
            case .loadFailed(let message):
                ContentUnavailableView {
                    Label("Couldn't Load Tasks", systemImage: "exclamationmark.triangle")
                } description: {
                    Text(message)
                } actions: {
                    Button("Try Again") { workspace.store.reload() }
                }
            case .tasks:
                split
            }
        }
        // Over the whole window: an id can be copied from a row's context menu too.
        .overlay(alignment: .bottom) {
            // The ZStack scopes the animation to the badge; on the Group it
            // would catch every change landing in the same transaction.
            ZStack {
                if let confirmation = workspace.confirmation {
                    ConfirmationBadge(text: confirmation.value)
                }
            }
            .padding(.bottom, 34)
            // Opacity only: a fade needs no Reduce Motion exception.
            .animation(.easeOut(duration: 0.16), value: workspace.confirmation)
        }
        // Announced: nothing moves the VoiceOver cursor to a badge that merely appears.
        .onChange(of: workspace.confirmation) { _, new in
            if let new { AccessibilityNotification.Announcement(new.value).post() }
        }
        // One at a time; dismissing takes the head off the queue.
        .alert(
            workspace.store.failures.first?.title ?? "",
            isPresented: Binding(
                get: { !workspace.store.failures.isEmpty },
                set: {
                    if !$0, !workspace.store.failures.isEmpty {
                        workspace.store.failures.removeFirst()
                    }
                }
            ),
            presenting: workspace.store.failures.first
        ) { failure in
            if let arguments = failure.force {
                Button("Mark as Done Anyway") { workspace.store.retry(arguments) }
            }
            Button("OK", role: .cancel) {}
        } message: { failure in
            Text(failure.message)
        }
        .sheet(item: $workspace.relating) { relating in
            TaskPicker(relating: relating)
        }
        // Nothing in the app unarchives, so this asks first.
        .confirmationDialog(
            sweepTitle,
            isPresented: Binding(
                get: { workspace.sweeping != nil },
                set: { if !$0 { workspace.sweeping = nil } }
            ),
            titleVisibility: .visible,
            presenting: workspace.sweeping
        ) { sweep in
            Button("Archive", role: .destructive) { workspace.store.archive(sweep) }
        } message: { _ in
            Text("They move to .seed/archive. Only the Finder brings them back.")
        }
        // An occluded app gets App Napped, which delays the watcher by seconds.
        .task {
            let activations = NotificationCenter.default.notifications(
                named: NSApplication.didBecomeActiveNotification
            )
            for await _ in activations { workspace.store.reload() }
        }
    }

    private var split: some View {
        @Bindable var workspace = workspace

        return NavigationSplitView(columnVisibility: $workspace.columns) {
            SidebarView()
                .navigationSplitViewColumnWidth(min: 180, ideal: 215, max: 300)
        } content: {
            TaskListView()
                .navigationSplitViewColumnWidth(min: 260, ideal: 340)
        } detail: {
            DetailView()
                .navigationSplitViewColumnWidth(min: 340, ideal: 460)
        }
        .navigationTitle(workspace.store.repository?.lastPathComponent ?? "Seed")
        .navigationSubtitle(subtitle)
        // ⌘. does the same as ⌘B without earning a second menu item.
        .background {
            Button("Show or Hide Sidebar") { workspace.toggleSidebar() }
                .keyboardShortcut(".", modifiers: .command)
                .opacity(0)
        }
    }

    /// Inflection markup needs a String Catalog, which this app has none of.
    private var sweepTitle: String {
        let count = workspace.sweeping?.count ?? 0
        return count == 1 ? "Archive 1 task?" : "Archive \(count) tasks?"
    }

    private var subtitle: String {
        if workspace.store.presentation == .tasks(stale: true) {
            return "Showing the last loaded tasks"
        }
        let open = workspace.openCount
        return open == 1 ? "1 open task" : "\(open) open tasks"
    }
}

/// Built from `.regularMaterial` and `.tint`, which follow appearance, Reduce
/// Transparency and the accent colour on their own.
private struct ConfirmationBadge: View {
    let text: String

    var body: some View {
        HStack(spacing: 6) {
            Image(systemName: "checkmark.circle.fill")
                .foregroundStyle(.tint)
            Text(text)
        }
        .font(.callout)
        .padding(.horizontal, 14)
        .padding(.vertical, 9)
        .background(.regularMaterial, in: .capsule)
        // Keeps the edge legible over content of nearly the same tone.
        .overlay(Capsule().strokeBorder(.separator, lineWidth: 0.5))
        .shadow(color: .black.opacity(0.16), radius: 10, y: 4)
        // Must not swallow a click meant for the row beneath.
        .allowsHitTesting(false)
        .accessibilityHidden(true)
    }
}

/// Runs the two commands the CLI would.
struct BootstrapView: View {
    @Environment(Workspace.self) private var workspace
    let repository: URL

    @State private var primeClaude: Bool

    init(repository: URL) {
        self.repository = repository
        _primeClaude = State(initialValue: SeedCLI.hasClaudeConfiguration(repository))
    }

    var body: some View {
        ContentUnavailableView {
            Label("No Tasks Here Yet", systemImage: "shippingbox")
        } description: {
            Text(
                "\(repository.lastPathComponent) isn't a Seed repository. Creating one adds a .seed folder to it."
            )
        } actions: {
            VStack(spacing: 14) {
                Toggle("Also prime Claude Code in this folder", isOn: $primeClaude)
                    .toggleStyle(.checkbox)

                Button("Create Repository") {
                    workspace.store.bootstrap(primeClaude: primeClaude)
                }
                .keyboardShortcut(.defaultAction)
            }
            .padding(.top, 4)
        }
    }
}

struct SidebarView: View {
    @Environment(Workspace.self) private var workspace

    var body: some View {
        @Bindable var workspace = workspace

        List(selection: $workspace.scope) {
            Section {
                ForEach(Workspace.Scope.standard, id: \.self) { scope in
                    link(scope)
                }
            }

            if !workspace.labels.isEmpty {
                Section("Labels") {
                    ForEach(workspace.labels, id: \.self) { label in
                        link(.label(label))
                    }
                }
            }
        }
        .listStyle(.sidebar)
    }

    private func link(_ scope: Workspace.Scope) -> some View {
        Label(scope.name, systemImage: scope.symbol)
            .badge(workspace.count(scope))
            .tag(scope)
    }
}
