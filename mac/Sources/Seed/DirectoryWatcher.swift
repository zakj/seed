import CoreServices
import Foundation

/// `sd` writes each task through a temporary file and a rename, so watching the
/// directory catches every change an agent makes. FSEvents reports a whole tree
/// against a path rather than an inode, so `tasks/`, an `archive/` that does not
/// exist yet, and a directory replaced wholesale all arrive without re-arming
/// anything.
final class DirectoryWatcher {
    private let stream: FSEventStreamRef

    /// The callback is a C function pointer and cannot capture, so what it needs
    /// travels as the stream's `info` pointer. `retain` and `release` below tie
    /// that pointer's lifetime to the stream, so CoreServices drops it at the
    /// point it knows no callback is still running — releasing it by hand from
    /// `deinit` would race a callback already executing on the watcher queue.
    private final class Handler: Sendable {
        let onChange: @Sendable () -> Void

        init(_ onChange: @escaping @Sendable () -> Void) {
            self.onChange = onChange
        }
    }

    /// Fails when the path cannot be watched at all; a repository whose `.seed`
    /// appears later is watched by asking again once it has.
    init?(url: URL, onChange: @escaping @Sendable () -> Void) {
        let handler = Handler(onChange)
        var context = FSEventStreamContext(
            version: 0,
            info: Unmanaged.passUnretained(handler).toOpaque(),
            retain: { UnsafeRawPointer(Unmanaged<Handler>.fromOpaque($0!).retain().toOpaque()) },
            release: { Unmanaged<Handler>.fromOpaque($0!).release() },
            copyDescription: nil
        )
        // A single `sd` command touches several files, and the latency is what
        // coalesces them into one reload.
        guard let stream = FSEventStreamCreate(
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
        ) else {
            return nil
        }

        FSEventStreamSetDispatchQueue(stream, DispatchQueue(label: "app.zakj.seed.watcher"))
        // A stream that fails to start reports nothing afterwards, and the
        // caller only re-arms a watcher that is nil — so a non-nil dead one
        // means the window stops seeing agent writes for the rest of its life.
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
