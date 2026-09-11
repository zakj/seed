import AppKit
import SwiftUI

/// The description editor: an `NSTextView` in its own scroller, the one
/// arrangement AppKit keeps the caret on screen for. It fills the height it is
/// given, so the caller has to give it one.
struct ScrollingTextView: NSViewRepresentable {
    /// Monospaced because a description is markdown. Stated once so the
    /// placeholder drawn over it cannot drift.
    static let body = NSFont.monospacedSystemFont(
        ofSize: NSFont.preferredFont(forTextStyle: .body).pointSize, weight: .regular)
    /// Room between the text and the editor's edge, inside the scroll view so a
    /// scrolled line clips at the edge rather than short of it.
    static let inset: CGFloat = 8

    @Binding var text: String
    var font: NSFont
    var onCommit: () -> Void = {}
    /// Set to give the editor Escape; without one the key keeps its meaning.
    var onCancel: (() -> Void)?

    func makeNSView(context: Context) -> NSScrollView {
        let scroll = NSTextView.scrollableTextView()
        scroll.autohidesScrollers = true

        guard let view = scroll.documentView as? NSTextView else { return scroll }
        view.string = text
        view.delegate = context.coordinator
        view.font = font
        view.isRichText = false
        // Curly quotes and en dashes would silently corrupt markdown that
        // agents read back. Each is its own default, and all start on.
        view.isAutomaticQuoteSubstitutionEnabled = false
        view.isAutomaticDashSubstitutionEnabled = false
        view.isAutomaticTextReplacementEnabled = false
        view.isAutomaticSpellingCorrectionEnabled = false
        view.allowsUndo = true
        view.drawsBackground = false
        view.textContainer?.lineFragmentPadding = 0
        view.textContainerInset = NSSize(width: Self.inset, height: Self.inset)
        view.setAccessibilityLabel("Description")
        context.coordinator.watchClicks(around: view)

        // A description is added to far more often than replaced.
        DispatchQueue.main.async {
            view.window?.makeFirstResponder(view)
            view.moveToEndOfDocument(nil)
        }
        return scroll
    }

    func updateNSView(_ scroll: NSScrollView, context: Context) {
        context.coordinator.parent = self
        guard let view = scroll.documentView as? NSTextView else { return }
        // Assigning the font re-lays out the whole document.
        if view.font != font { view.font = font }
        // Never while focused: the text came from this view, and writing it
        // back mid-edit fights the typing.
        if view.window?.firstResponder !== view, view.string != text {
            view.string = text
        }
    }

    func makeCoordinator() -> Coordinator { Coordinator(self) }

    final class Coordinator: NSObject, NSTextViewDelegate {
        var parent: ScrollingTextView
        private var clicks: Any?

        init(_ parent: ScrollingTextView) {
            self.parent = parent
        }

        deinit {
            if let clicks { NSEvent.removeMonitor(clicks) }
        }

        @MainActor
        func watchClicks(around view: NSTextView) {
            guard clicks == nil else { return }
            // The scroller is outside the text but belongs to the editor.
            clicks = NSEvent.endEditingOnClickOutside(view.enclosingScrollView ?? view) {
                [weak view] in
                view.map { $0.window?.firstResponder === $0 } ?? false
            }
        }

        func textDidChange(_ notification: Notification) {
            guard let view = notification.object as? NSTextView else { return }
            parent.text = view.string
        }

        func textDidEndEditing(_ notification: Notification) {
            parent.onCommit()
        }

        /// Return inserts a newline here, so only Escape needs saying.
        func textView(_ view: NSTextView, doCommandBy selector: Selector) -> Bool {
            guard selector == #selector(NSResponder.cancelOperation(_:)),
                let cancel = parent.onCancel
            else { return false }
            cancel()
            return true
        }
    }
}
