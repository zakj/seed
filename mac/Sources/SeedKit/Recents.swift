import Foundation
import Observation

/// The File menu's recent repositories, shared by every window. Its head is also
/// what a window with no repository of its own opens.
@MainActor
@Observable
public final class Recents {
    public static let shared = Recents()

    private static let key = "recentRepositories"
    private static let limit = 10

    private let defaults: RecentsStore
    public private(set) var urls: [URL]

    public init(defaults: RecentsStore = UserDefaults.standard) {
        self.defaults = defaults
        urls = (defaults.stringArray(forKey: Self.key) ?? [])
            .map { SeedCLI.directory(URL(filePath: $0)) }
    }

    /// Has to still be a repository, or a fresh window opens onto a first-run
    /// pane for a folder that is gone. Checked on demand: filtering at load
    /// would put an autofs mount on the launch path.
    public var firstWorkspace: URL? { urls.first(where: SeedCLI.isWorkspace) }

    public func add(_ url: URL) {
        urls.removeAll { $0 == url }
        urls.insert(url, at: 0)
        urls = Array(urls.prefix(Self.limit))
        save()
    }

    public func clear() {
        urls = []
        save()
    }

    private func save() {
        defaults.set(urls.map(\.path), forKey: Self.key)
    }
}

/// What `Recents` needs of `UserDefaults`, so a test can pass memory: a scratch
/// suite is written back by cfprefsd after the process exits.
public protocol RecentsStore: AnyObject {
    func stringArray(forKey key: String) -> [String]?
    func set(_ value: Any?, forKey key: String)
}

extension UserDefaults: RecentsStore {}
