import Foundation

public struct CLIError: LocalizedError, Sendable {
    public let errorDescription: String?
}

/// One field of one task. Views name what they want changed; only SeedKit knows
/// which flag carries it.
public enum Edit: Sendable {
    case status(Status)
    case priority(Priority)
    case title(String)
    case description(String)
    case addLabel(String)
    case removeLabel(String)
    case addDependency(Int)
    case removeDependency(Int)
    /// `nil` unparents.
    case parent(Int?)

    /// `--flag=value` rather than two arguments: a value of its own starting with
    /// `-` reads as a flag, and a description opening with a bullet list is the
    /// most ordinary text there is.
    public var arguments: [String] {
        switch self {
        case .status(let value): ["--status=\(value.rawValue)"]
        case .priority(let value): ["--priority=\(value.rawValue)"]
        case .title(let value): ["--title=\(value)"]
        case .description(let value): ["--description=\(value)"]
        case .addLabel(let value): ["--add-label=\(value)"]
        case .removeLabel(let value): ["--rm-label=\(value)"]
        case .addDependency(let id): ["--add-dep=\(id)"]
        case .removeDependency(let id): ["--rm-dep=\(id)"]
        case .parent(let id): id.map { ["--parent=\($0)"] } ?? ["--no-parent"]
        }
    }

    /// `sd` refuses to close a task with unmet dependencies or open children, and
    /// `--force` overrides only that.
    public var forceable: Bool {
        if case .status(.done) = self { return true }
        return false
    }
}

public enum SeedCLI {
    /// The one command with a positional argument. `--` is what keeps a title
    /// opening with a dash from being read as a flag, so every option has to
    /// precede it.
    /// `Edit.parent` rather than the flag spelled again — but only when there
    /// is one: `Edit.parent(nil)` is `--no-parent`, which `add` does not take.
    public static func addArguments(title: String, parent: Int?) -> [String] {
        ["add", "--quiet"] + (parent.map { Edit.parent($0).arguments } ?? []) + ["--", title]
    }

    /// The app ships the `sd` it talks to, so the CLI and the JSON it emits are
    /// always the same version. The override exists to point at a different build.
    public static func locate(override: String) -> URL? {
        let trimmed = override.trimmingCharacters(in: .whitespaces)
        // A directory is executable too, and one named here would pass the check
        // and then fail at `Process.run` with nothing pointing back at Settings.
        var isDirectory: ObjCBool = false
        if !trimmed.isEmpty,
            FileManager.default.fileExists(atPath: trimmed, isDirectory: &isDirectory),
            !isDirectory.boolValue,
            FileManager.default.isExecutableFile(atPath: trimmed) {
            return URL(filePath: trimmed)
        }
        return Bundle.main.url(forAuxiliaryExecutable: "sd")
    }

    /// Windows and the recent list are both keyed by a repository's URL, and a
    /// trailing slash decides whether two URLs for the same directory compare
    /// equal.
    public static func directory(_ url: URL) -> URL {
        URL(filePath: url.path, directoryHint: .isDirectory)
    }

    public static func isWorkspace(_ url: URL) -> Bool {
        var isDirectory: ObjCBool = false
        let seed = url.appending(path: ".seed").path
        return FileManager.default.fileExists(atPath: seed, isDirectory: &isDirectory)
            && isDirectory.boolValue
    }

    /// A folder already carrying Claude Code configuration is one where the
    /// priming hook is probably wanted.
    public static func hasClaudeConfiguration(_ url: URL) -> Bool {
        [".claude", "CLAUDE.md"].contains {
            FileManager.default.fileExists(atPath: url.appending(path: $0).path)
        }
    }

    public static func run(_ arguments: [String], binary: URL, repository: URL) async throws -> Data {
        let process = Process()
        process.executableURL = binary
        process.arguments = arguments
        process.currentDirectoryURL = repository
        process.standardInput = FileHandle.nullDevice

        let out = Pipe(), err = Pipe()
        process.standardOutput = out
        process.standardError = err

        // Before either reader: a reader waits for the child to close its end,
        // so one started for a process that never launched waits for an end that
        // will never close — and it does not answer cancellation, so the throw
        // out of here would hang rather than propagate.
        try process.run()

        // Both pipes are drained at once: reading one to the end before the
        // other deadlocks whenever the other fills its buffer.
        async let stdout = collect(out)
        async let stderr = collect(err)
        await withTaskCancellationHandler {
            await withCheckedContinuation { continuation in
                process.terminationHandler = { _ in continuation.resume() }
            }
        } onCancel: {
            // A cancelled reload otherwise leaves its `sd` running to completion,
            // and writes queue behind whatever is still in flight.
            process.terminate()
        }

        guard process.terminationStatus == 0 else {
            let message = await failureMessage(stderr)
            throw CLIError(
                errorDescription: message.isEmpty
                    ? "sd exited with status \(process.terminationStatus)"
                    : message
            )
        }
        return await stdout
    }

    /// Reads until the far end closes. The handler runs serially per handle, so
    /// the box is only ever touched by one of them at a time.
    private static func collect(_ pipe: Pipe) async -> Data {
        final class Box: @unchecked Sendable {
            var data = Data()
        }
        let box = Box()

        return await withCheckedContinuation { continuation in
            pipe.fileHandleForReading.readabilityHandler = { handle in
                let chunk = handle.availableData
                guard chunk.isEmpty else { return box.data.append(chunk) }
                handle.readabilityHandler = nil
                continuation.resume(returning: box.data)
            }
        }
    }

    private struct ErrorEnvelope: Decodable {
        let error: String
    }

    /// In `--json` mode `sd` reports failures as `{"error":…}` on stderr. Which
    /// commands do that is the envelope's business, not the caller's — anything
    /// that isn't one falls through as the plain text it already was.
    static func failureMessage(_ stderr: Data) -> String {
        if let envelope = try? JSONDecoder().decode(ErrorEnvelope.self, from: stderr) {
            return envelope.error
        }
        return String(decoding: stderr, as: UTF8.self)
            .trimmingCharacters(in: .whitespacesAndNewlines)
    }

    public static func list(binary: URL, repository: URL, includeArchived: Bool) async throws -> [SeedTask] {
        let data = try await run(
            ["list", "--json"] + (includeArchived ? ["--include-archived"] : []),
            binary: binary, repository: repository
        )
        return try JSONDecoder().decode([SeedTask].self, from: data)
    }
}
