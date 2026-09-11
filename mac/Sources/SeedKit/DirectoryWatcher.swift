import CoreServices
import Foundation

/// `sd` writes through a temp file and a rename, so a directory watch sees every
/// change. FSEvents watches a tree by path, so `tasks/`, a later `archive/`, and
/// a replaced directory all arrive on one stream.
final class DirectoryWatcher {
    private let stream: FSEventStreamRef

    /// The C callback cannot capture. `retain` and `release` tie the box's
    /// lifetime to the stream, so no callback outlives it.
    private final class Handler: Sendable {
        let onChange: @Sendable () -> Void

        init(_ onChange: @escaping @Sendable () -> Void) {
            self.onChange = onChange
        }
    }

    /// Nil when the path cannot be watched; the caller asks again once it exists.
    init?(url: URL, onChange: @escaping @Sendable () -> Void) {
        let handler = Handler(onChange)
        var context = FSEventStreamContext(
            version: 0,
            info: Unmanaged.passUnretained(handler).toOpaque(),
            retain: { UnsafeRawPointer(Unmanaged<Handler>.fromOpaque($0!).retain().toOpaque()) },
            release: { Unmanaged<Handler>.fromOpaque($0!).release() },
            copyDescription: nil
        )
        // The latency folds one `sd` command's several files into one reload.
        guard
            let stream = FSEventStreamCreate(
                nil,
                { _, info, _, _, _, _ in
                    guard let info else { return }
                    Unmanaged<Handler>.fromOpaque(info).takeUnretainedValue().onChange()
                },
                &context,
                [url.path] as CFArray,
                FSEventStreamEventId(kFSEventStreamEventIdSinceNow),
                0.15,
                FSEventStreamCreateFlags(kFSEventStreamCreateFlagWatchRoot)
            )
        else {
            return nil
        }

        FSEventStreamSetDispatchQueue(stream, DispatchQueue(label: "app.zakj.seed.watcher"))
        // A stream that fails to start reports nothing, and only a nil watcher
        // is re-armed.
        guard FSEventStreamStart(stream) else {
            FSEventStreamInvalidate(stream)
            FSEventStreamRelease(stream)
            return nil
        }
        self.stream = stream
    }

    deinit {
        FSEventStreamStop(stream)
        FSEventStreamInvalidate(stream)
        FSEventStreamRelease(stream)
    }
}
