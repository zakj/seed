import Foundation
import Observation

/// One repository's tasks and the `sd` that reads and writes them. Owns the
/// FSEvents watch, the reload, and the command queue.
@MainActor
@Observable
public final class Store {
    public enum Presentation: Equatable {
        case loading
        case noRepository
        /// A folder that opened but holds no `.seed`; `bootstrap` can make one.
        case uninitialized(URL)
        case missingBinary
        case loadFailed(String)
        /// `stale` when the last reload failed and the tasks shown are the previous ones.
        case tasks(stale: Bool)
    }

    public struct Failure {
        public let title: String
        public let message: String
        /// The command to re-run with `--force`, when that would succeed.
        public let force: [String]?
    }

    /// What `sd archive` would move: resolved tasks last changed before `cutoff`.
    public struct Sweep: Identifiable {
        public let name: String
        /// A humantime duration, or nil for every resolved task.
        public let cutoff: String?
        public let count: Int

        public var id: String { cutoff ?? "all" }
    }

    private enum State: Equatable {
        case loading, noRepository, uninitialized, missingBinary, ready
        case failed(String)
    }

    public private(set) var repository: URL?
    public private(set) var graph = TaskGraph([])
    public var includeArchived = false { didSet { reload() } }
    /// A queue: the alert shows one at a time, and the reload after a failed
    /// edit reverts it on screen, so a dropped failure is an edit that undoes
    /// itself with nothing saying why.
    public var failures: [Failure] = []

    private var state = State.loading
    private let binary: URL?
    private let recents: Recents
    private var watcher: DirectoryWatcher?
    /// Commands run one at a time: `sd` refuses a write whose task file
    /// changed since it read it, so overlapping edits would fail the second.
    private var pending = Task<Data?, Never> { nil }
    private var reloading: Task<Void, Never>?

    /// `defaults write net.zakj.seed seedBinaryPath <path>` points the app at
    /// an `sd` other than the bundled one.
    public init(
        binary: URL? = SeedCLI.locate(
            override: UserDefaults.standard.string(forKey: "seedBinaryPath") ?? ""),
        recents: Recents = .shared
    ) {
        self.binary = binary
        self.recents = recents
    }

    public var presentation: Presentation {
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

    // MARK: - Repository

    /// A store adopts one repository for its life; a second one gets a second window.
    public func open(_ url: URL?) {
        assert(repository == nil, "a store adopts one repository")
        guard let url = url.map(SeedCLI.directory) else {
            state = .noRepository
            return
        }
        repository = url
        // A folder with no `.seed` is a mistake at the Open panel, not somewhere to reopen.
        if SeedCLI.isWorkspace(url) { recents.add(url) }
        state = .loading
        reload()
    }

    public func reload() {
        guard let repository else {
            state = .noRepository
            return
        }
        guard let binary else {
            state = .missingBinary
            return
        }
        guard SeedCLI.isWorkspace(repository) else {
            state = .uninitialized
            return
        }
        // Armed here rather than in `open`: a stream over a path that does not
        // exist yet is inert, and this is the point where `.seed` is known to.
        if watcher == nil { watch(repository) }
        // Newest wins. The watcher, a finished command, and the app coming
        // forward all ask, and an older read must not land after a newer one.
        reloading?.cancel()
        reloading = Task {
            do {
                let tasks = try await SeedCLI.list(
                    binary: binary, repository: repository, includeArchived: includeArchived)
                guard !Task.isCancelled else { return }
                graph = TaskGraph(tasks)
                state = .ready
            } catch {
                guard !Task.isCancelled else { return }
                state = .failed(error.localizedDescription)
            }
        }
    }

    /// `.seed` rather than the repository: the watch is recursive, and the
    /// project around it changes for reasons that are none of ours.
    private func watch(_ repository: URL) {
        watcher = DirectoryWatcher(url: repository.appending(path: ".seed")) { [weak self] in
            Task { @MainActor in self?.reload() }
        }
    }

    /// `sd init`, then the priming hook if asked; priming needs a store to write beside.
    public func bootstrap(primeClaude: Bool) {
        guard let repository else { return }
        state = .loading
        let created = run(["init"], title: "Couldn't Create the Repository")
        Task {
            guard await created.value != nil else { return }
            recents.add(repository)
            guard primeClaude else { return }
            run(["prime", "--install", "claude"], title: "Couldn't Prime Claude Code")
        }
    }

    // MARK: - Writes

    /// Resolves once the write and the reload after it have landed, carrying
    /// the command's stdout, or nil if it failed.
    @discardableResult
    public func edit(_ id: Int, _ edit: Edit) -> Task<Data?, Never> {
        run(["edit", String(id)] + edit.arguments, forceable: edit.forceable)
    }

    /// The new task's id, or nil if `sd` refused.
    public func add(title: String, parent: Int?) -> Task<Int?, Never> {
        let write = run(SeedCLI.addArguments(title: title, parent: parent))
        return Task {
            guard let output = await write.value else { return nil }
            let text = String(decoding: output, as: UTF8.self)
            return Int(text.trimmingCharacters(in: .whitespacesAndNewlines))
        }
    }

    public func retry(_ arguments: [String]) {
        run(arguments + ["--force"])
    }

    /// Refuses what `sd` would refuse, so the keyboard path cannot slip past a disabled row.
    public func relate(_ candidate: Int, to id: Int, by relation: Relation, on: Bool) {
        let candidates = graph.candidates(for: id, by: relation)
        guard candidates.first(where: { $0.id == candidate })?.barred == nil else { return }
        let write = relation.edit(candidate, to: id, on: on)
        edit(write.task, write.edit)
    }

    public func archive(_ sweep: Sweep) {
        run(["archive"] + (sweep.cutoff.map { [$0] } ?? []), title: "Couldn't Archive")
    }

    public var sweeps: [Sweep] {
        let resolved = graph.tasks.filter { $0.status.isResolved && !$0.archived }
        // The cutoff `sd` gets and the count shown derive from one number, so
        // a menu item cannot count 31 days and archive 30.
        let sweep = { (name: String, days: Int) in
            let before = Date().addingTimeInterval(-Double(days) * 86_400)
            return Sweep(
                name: name, cutoff: "\(days)d", count: resolved.count { $0.modified <= before })
        }
        return [
            Sweep(name: "All Completed Tasks", cutoff: nil, count: resolved.count),
            sweep("Untouched for a Day", 1),
            sweep("Untouched for a Week", 7),
        ]
    }

    @discardableResult
    private func run(
        _ arguments: [String], forceable: Bool = false, title: String = "Couldn't Update Task"
    ) -> Task<Data?, Never> {
        guard let repository, let binary else {
            // Surfaces what is missing rather than leaving a spinner on a bare nil.
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
                        title: title, message: error.localizedDescription,
                        force: forceable ? arguments : nil))
            }
            reload()
            await reloading?.value
            return output
        }
        pending = work
        return work
    }
}
