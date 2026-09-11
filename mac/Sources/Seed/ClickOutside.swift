import AppKit

extension NSEvent {
    /// Clicking empty space moves no responder on its own, and a scroll view
    /// takes the click before anything behind it could, so an editor ends its
    /// own edit when a click lands outside `region` while `editing`. The event
    /// passes through untouched. Remove the returned monitor in `deinit`.
    @MainActor
    static func endEditingOnClickOutside(
        _ region: NSView, while editing: @escaping @MainActor () -> Bool
    ) -> Any? {
        addLocalMonitorForEvents(matching: [.leftMouseDown]) { [weak region] event in
            guard let region, editing(), event.window === region.window else { return event }
            if !region.bounds.contains(region.convert(event.locationInWindow, from: nil)) {
                region.window?.makeFirstResponder(nil)
            }
            return event
        }
    }
}
