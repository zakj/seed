import Foundation

public struct CLIError: LocalizedError, Sendable {
    public let errorDescription: String?
}

/// One field of one task; only SeedKit knows which flag carries it.
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

    /// `--flag=value` rather than two arguments: a value starting with `-` would
    /// read as a flag, and a description often opens with a bullet.
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

    /// `--force` overrides only `sd`'s refusal to close a task with unmet
    /// dependencies or open children.
    public var forceable: Bool {
        if case .status(.done) = self { return true }
        return false
    }
}

public enum SeedCLI {
    /// `--` keeps a title opening with a dash from reading as a flag, so every
    /// option precedes it.
    public static func addArguments(title: String, parent: Int?) -> [String] {
        ["add", "--quiet"] + (parent.map { Edit.parent($0).arguments } ?? []) + ["--", title]
    }

    /// The bundled `sd` unless the override names another build.
    public static func locate(override: String) -> URL? {
        let trimmed = override.trimmingCharacters(in: .whitespaces)
        // A directory passes the executable check and fails at `run`.
        var isDirectory: ObjCBool = false
        if !trimmed.isEmpty,
            FileManager.default.fileExists(atPath: trimmed, isDirectory: &isDirectory),
            !isDirectory.boolValue,
            FileManager.default.isExecutableFile(atPath: trimmed)
        {
            return URL(filePath: trimmed)
        }
        return Bundle.main.url(forAuxiliaryExecutable: "sd")
    }

    /// A trailing slash decides whether two URLs for one directory compare equal.
    public static func directory(_ url: URL) -> URL {
        URL(filePath: url.path, directoryHint: .isDirectory)
    }

    public static func isWorkspace(_ url: URL) -> Bool {
        var isDirectory: ObjCBool = false
        let seed = url.appending(path: ".seed").path
        return FileManager.default.fileExists(atPath: seed, isDirectory: &isDirectory)
            && isDirectory.boolValue
    }

    /// Pre-ticks the priming checkbox for a folder already set up for Claude Code.
    public static func hasClaudeConfiguration(_ url: URL) -> Bool {
        [".claude", "CLAUDE.md"].contains {
            FileManager.default.fileExists(atPath: url.appending(path: $0).path)
        }
    }

    public static func run(_ arguments: [String], binary: URL, repository: URL) async throws -> Data
    {
        let process = Process()
        process.executableURL = binary
        process.arguments = arguments
        process.currentDirectoryURL = repository
        process.standardInput = FileHandle.nullDevice

        let out = Pipe()
        let err = Pipe()
        process.standardOutput = out
        process.standardError = err

        // Before either reader: a reader for a process that never launched
        // waits forever, and it does not answer cancellation.
        try process.run()

        // Both drained at once: reading one to its end first deadlocks when the
        // other fills its buffer.
        async let stdout = collect(out)
        async let stderr = collect(err)
        await withTaskCancellationHandler {
            await withCheckedContinuation { continuation in
                process.terminationHandler = { _ in continuation.resume() }
            }
        } onCancel: {
            // Otherwise a cancelled reload leaves its `sd` running and writes
            // queue behind it.
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

    /// Reads until the far end closes. The handler runs serially per handle.
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

    /// `--json` failures arrive as `{"error":…}`; anything else is plain text.
    static func failureMessage(_ stderr: Data) -> String {
        if let envelope = try? JSONDecoder().decode(ErrorEnvelope.self, from: stderr) {
            return envelope.error
        }
        return String(decoding: stderr, as: UTF8.self)
            .trimmingCharacters(in: .whitespacesAndNewlines)
    }

    public static func list(binary: URL, repository: URL, includeArchived: Bool) async throws
        -> [SeedTask]
    {
        let data = try await run(
            ["list", "--json"] + (includeArchived ? ["--include-archived"] : []),
            binary: binary, repository: repository
        )
        return try JSONDecoder.seed.decode([SeedTask].self, from: data)
    }
}
