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

    /// What a window with no repository of its own opens, so it has to still be
    /// one — a moved or deleted head otherwise greets the user with a first-run
    /// pane offering to initialize a directory that isn't there. Checked here
    /// rather than filtered at load: an entry on an unmounted volume would put
    /// an autofs mount attempt on the launch path for a repository nobody asked
    /// to open.
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

/// What `Recents` needs of `UserDefaults`, so a test can hand it something that
/// is not the user's real preferences. A scratch suite cannot be cleaned up:
/// cfprefsd writes the domain back out after the process exits, so removing the
/// domain and unlinking the file still leaves an empty plist behind — seconds
/// later, which is late enough to look clean if you check straight away.
public protocol RecentsStore: AnyObject {
    func stringArray(forKey key: String) -> [String]?
    func set(_ value: Any?, forKey key: String)
}

extension UserDefaults: RecentsStore {}
