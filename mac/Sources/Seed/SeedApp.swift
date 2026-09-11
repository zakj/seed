import AppKit
import SeedKit
import SwiftUI

@main
struct SeedApp: App {
    @NSApplicationDelegateAdaptor(AppDelegate.self) private var delegate

    var body: some Scene {
        // Keyed by repository, so opening one already showing focuses that window.
        WindowGroup(for: URL.self) { $repository in
            RootView(repository: $repository)
        }
        .defaultSize(width: 1080, height: 700)
        .commands { SeedCommands() }
    }
}

/// A window owns its workspace, so two repositories can be open side by side.
struct RootView: View {
    /// Written back by a window that adopts a repository at launch, on restore
    /// or from the Finder; otherwise `openWindow(value:)` cannot tell this
    /// window is already showing it and opens a second one onto the same store.
    @Binding var repository: URL?
    /// Restoration hands a window back without its value.
    @SceneStorage("repository") private var restored: String?
    @State private var workspace = Workspace()
    @Environment(\.openWindow) private var openWindow

    var body: some View {
        ContentView()
            .environment(workspace)
            .focusedSceneValue(\.workspace, workspace)
            // A folder opened from the Finder lands in the empty window SwiftUI
            // raised for it.
            .onOpenURL { url in
                guard url.hasDirectoryPath else { return }
                if workspace.store.repository == nil {
                    workspace.store.open(url)
                } else {
                    openWindow(value: SeedCLI.directory(url))
                }
            }
            .onChange(of: workspace.store.repository) { _, opened in
                restored = opened?.path
                repository = opened
            }
            // Once the window exists, not in an initializer that also runs for
            // views SwiftUI throws away.
            .task {
                // A folder opened at launch reaches `onOpenURL` first; it wins.
                guard workspace.store.repository == nil else { return }
                workspace.store.open(
                    repository ?? restored.map { URL(filePath: $0) } ?? Workspace.remembered
                )
            }
    }
}

final class AppDelegate: NSObject, NSApplicationDelegate {
    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool { true }
}
