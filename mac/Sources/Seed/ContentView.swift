import AppKit
import SeedKit
import SwiftUI

struct ContentView: View {
    @Environment(Workspace.self) private var workspace
    @AppStorage(Workspace.binaryPathKey) private var binaryPath = ""

    var body: some View {
        @Bindable var workspace = workspace

        Group {
            switch workspace.presentation {
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
                        if let url = Workspace.chooseRepository() { workspace.open(url) }
                    }
                }
            case .uninitialized(let repository):
                BootstrapView(repository: repository)
                    .id(repository)
            case .missingBinary:
                ContentUnavailableView {
                    Label("Can't Find sd", systemImage: "terminal")
                } description: {
                    Text("Seed runs the sd command line tool. Set its location in Settings.")
                } actions: {
                    SettingsLink { Text("Open Settings…") }
                }
            case .loadFailed(let message):
                ContentUnavailableView {
                    Label("Couldn't Load Tasks", systemImage: "exclamationmark.triangle")
                } description: {
                    Text(message)
                } actions: {
                    Button("Try Again") { workspace.reload() }
                    SettingsLink { Text("Settings…") }
                }
            case .tasks:
                split
            }
        }
        // Over the whole window, not the detail pane: the id can be copied from
        // a row's context menu while you are reading the list, and an overlay
        // takes no part in layout, so nothing it appears over can shift.
        .overlay(alignment: .bottom) {
            // The ZStack is the stable ancestor the transition needs, and it
            // keeps the animation to the badge: on the Group it would catch
            // every other change landing in the same transaction, so a reload
            // arriving with a copy would slide the task list too.
            ZStack {
                if let confirmation = workspace.confirmation {
                    ConfirmationBadge(text: confirmation.text)
                }
            }
            .padding(.bottom, 34)
            // Opacity only. A fade needs no exception under Reduce Motion,
            // where a rise or a scale would.
            .animation(.easeOut(duration: 0.16), value: workspace.confirmation)
        }
        // Spoken rather than left in the tree: the VoiceOver cursor could reach
        // a badge, but nothing moves it there, so one that merely appeared would
        // confirm nothing to the people who cannot see it appear.
        .onChange(of: workspace.confirmation) { _, new in
            if let new { AccessibilityNotification.Announcement(new.text).post() }
        }
        // One at a time: dismissing takes the head off, and the next failure
        // presents on its own rather than being overwritten by it.
        .alert(
            workspace.failures.first?.title ?? "",
            isPresented: Binding(
                get: { !workspace.failures.isEmpty },
                set: { if !$0, !workspace.failures.isEmpty { workspace.failures.removeFirst() } }
            ),
            presenting: workspace.failures.first
        ) { failure in
            if let arguments = failure.force {
                Button("Mark as Done Anyway") { workspace.retry(arguments) }
            }
            Button("OK", role: .cancel) {}
        } message: { failure in
            Text(failure.message)
        }
        .sheet(item: $workspace.relating) { relating in
            TaskPicker(relating: relating)
        }
        // Nothing in the app brings an archived task back, so a one-way move
        // gets a sentence before it runs.
        .confirmationDialog(
            sweepTitle,
            isPresented: Binding(
                get: { workspace.sweeping != nil },
                set: { if !$0 { workspace.sweeping = nil } }
            ),
            titleVisibility: .visible,
            presenting: workspace.sweeping
        ) { sweep in
            Button("Archive", role: .destructive) { workspace.archive(sweep) }
        } message: { _ in
            Text("They move to .seed/archive. Only the Finder brings them back.")
        }
        // An occluded app gets App Napped, which coalesces the watcher's timer by
        // seconds. Let it nap, and catch up whenever it comes back to the front.
        .task {
            let activations = NotificationCenter.default.notifications(
                named: NSApplication.didBecomeActiveNotification
            )
            for await _ in activations { workspace.reload() }
        }
        // Settings has no window to reload; every open one answers for itself.
        .onChange(of: binaryPath) { workspace.reload() }
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
        .navigationTitle(workspace.repository?.lastPathComponent ?? "Seed")
        .navigationSubtitle(subtitle)
        // ⌘. does the same as ⌘B without earning a second menu item.
        .background {
            Button("Show or Hide Sidebar") { workspace.toggleSidebar() }
                .keyboardShortcut(".", modifiers: .command)
                .opacity(0)
        }
    }

    /// Inflection markup (`^[\(n) task](inflect: true)`) resolves only through a
    /// String Catalog, which this app has none of — it renders literally.
    private var sweepTitle: String {
        let count = workspace.sweeping?.count ?? 0
        return count == 1 ? "Archive 1 task?" : "Archive \(count) tasks?"
    }

    /// A failure here means a reload failed while earlier tasks are still on
    /// screen; the list stays as it was rather than emptying out.
    private var subtitle: String {
        if workspace.presentation == .tasks(stale: true) { return "Showing the last loaded tasks" }
        let open = workspace.openCount
        return open == 1 ? "1 open task" : "\(open) open tasks"
    }
}

/// macOS ships no toast, but it does ship the parts: `.regularMaterial` follows
/// light and dark on its own, turns opaque under Reduce Transparency, and takes
/// vibrancy from whatever it floats over — all of which a hand-mixed background
/// colour would have to reimplement and would still get wrong in dark mode. The
/// tick is `.tint`, so it follows the accent colour chosen in System Settings
/// rather than a blue of ours.
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
        // The hairline keeps the capsule's edge legible against content of
        // almost the same tone, which the material alone does not guarantee.
        .overlay(Capsule().strokeBorder(.separator, lineWidth: 0.5))
        .shadow(color: .black.opacity(0.16), radius: 10, y: 4)
        // It confirms something that already happened; there is nothing here to
        // click, and it must not swallow a click meant for the row beneath it.
        .allowsHitTesting(false)
        .accessibilityHidden(true)
    }
}

/// A folder with no `.seed` used to be a load failure offering Try Again, which
/// could never work. The app runs the same two commands the CLI would.
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
            Text("\(repository.lastPathComponent) isn't a Seed repository. Creating one adds a .seed folder to it.")
        } actions: {
            VStack(spacing: 14) {
                Toggle("Also prime Claude Code in this folder", isOn: $primeClaude)
                    .toggleStyle(.checkbox)

                Button("Create Repository") {
                    workspace.bootstrap(primeClaude: primeClaude)
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

struct SettingsView: View {
    @AppStorage(Workspace.binaryPathKey) private var binaryPath = ""

    var body: some View {
        Form {
            Section {
                LabeledContent("sd command") {
                    HStack {
                        Text(binaryPath.isEmpty ? "Bundled with Seed" : binaryPath)
                            .lineLimit(1)
                            .truncationMode(.head)
                            .foregroundStyle(binaryPath.isEmpty ? .secondary : .primary)
                        Spacer()
                        if !binaryPath.isEmpty {
                            Button("Use Bundled") { binaryPath = "" }
                        }
                        Button("Choose…", action: choose)
                    }
                }
            } footer: {
                Text("Seed ships with its own copy of sd. Point it at a different build only if you need to.")
                    .font(.callout)
                    .foregroundStyle(.secondary)
            }
        }
        .formStyle(.grouped)
        .frame(width: 480)
    }

    private func choose() {
        let panel = NSOpenPanel()
        panel.canChooseFiles = true
        panel.canChooseDirectories = false
        panel.allowsOtherFileTypes = true
        panel.prompt = "Use"
        panel.message = "Choose the sd executable."
        guard panel.runModal() == .OK, let url = panel.url else { return }
        binaryPath = url.path
    }
}
