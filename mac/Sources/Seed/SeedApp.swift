import AppKit
import SeedKit
import SwiftUI

@main
struct SeedApp: App {
    @NSApplicationDelegateAdaptor(AppDelegate.self) private var delegate

    var body: some Scene {
        // Keyed by repository, so opening one already showing in a window
        // focuses that window. A window restores without its value, so the
        // repository is also kept in scene storage below.
        WindowGroup(for: URL.self) { $repository in
            RootView(repository: $repository)
        }
        .defaultSize(width: 1080, height: 700)
        .commands { SeedCommands() }

        Settings {
            SettingsView()
        }
    }
}

/// A window owns its repository: the workspace, its watcher, and its reloads all
/// live and die with the window, so two repositories can be open side by side.
struct RootView: View {
    /// A binding, not a value: a window that adopts a repository any other way
    /// — at launch, on restore, from the Finder — has to write it back, or
    /// `openWindow(value:)` cannot tell that this window is already showing it
    /// and opens a second one onto the same store.
    @Binding var repository: URL?
    /// Restoration hands a window back without its value, so the window keeps
    /// its own record of what it was showing.
    @SceneStorage("repository") private var restored: String?
    @State private var workspace = Workspace()
    @Environment(\.openWindow) private var openWindow

    var body: some View {
        ContentView()
            .environment(workspace)
            .focusedSceneValue(\.workspace, workspace)
            // A folder opened from the Finder or `open` arrives here, in the
            // window SwiftUI raised for it — which is empty, so it takes the
            // repository. A window already showing one keeps it.
            .onOpenURL { url in
                guard url.hasDirectoryPath else { return }
                if workspace.repository == nil {
                    workspace.open(url)
                } else {
                    openWindow(value: SeedCLI.directory(url))
                }
            }
            .onChange(of: workspace.repository) { _, opened in
                restored = opened?.path
                repository = opened
            }
            // SwiftUI builds this view several times per window, so the
            // repository opens once the window exists rather than in an
            // initializer that also runs for views it throws away.
            .task {
                // A folder opened at launch reaches `onOpenURL` first; it wins.
                guard workspace.repository == nil else { return }
                workspace.open(
                    repository ?? restored.map { URL(filePath: $0) } ?? Workspace.remembered
                )
            }
    }
}

final class AppDelegate: NSObject, NSApplicationDelegate {
    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool { true }
}
