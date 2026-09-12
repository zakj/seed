import AppKit
import SwiftUI

/// The description editor is a text view that owns its own scroller, because
/// keeping a caret on screen is something AppKit does completely for that
/// arrangement and cannot do for any other. It only works where the editor is
/// given a height: a scroll view proposes none along its scroll axis, so a
/// view inside one has to invent a height from its content, and a caret past
/// the bottom of that content has nowhere to be scrolled to.
///
/// So the height comes from the proposal — grow to the text, stop at the space
/// offered — and the caller is responsible for offering a bounded one.
struct ScrollingTextView: NSViewRepresentable {
    /// The editor face. Monospaced because a description is markdown, and
    /// stated once so the placeholder drawn over it cannot drift.
    static let body = NSFont.monospacedSystemFont(
        ofSize: NSFont.preferredFont(forTextStyle: .body).pointSize,
        weight: .regular
    )

    @Binding var text: String
    var font: NSFont
    var onCommit: () -> Void = {}
    /// Set to give the editor Escape; without one the key keeps its meaning.
    var onCancel: (() -> Void)?

    func makeNSView(context: Context) -> NSScrollView {
        // Only what differs from what `scrollableTextView()` already sets up,
        // so every line here is a deviation rather than a restatement.
        let scroll = NSTextView.scrollableTextView()
        scroll.autohidesScrollers = true

        guard let view = scroll.documentView as? NSTextView else { return scroll }
        view.string = text
        view.delegate = context.coordinator
        view.font = font
        view.isRichText = false
        // A description is plain text that agents read back: curly quotes for
        // straight ones, and an en dash for the `--` of a flag, are silent
        // corruption rather than typography. Rich text being off does not
        // cover these; they are separate defaults and all start on.
        view.isAutomaticQuoteSubstitutionEnabled = false
        view.isAutomaticDashSubstitutionEnabled = false
        view.isAutomaticTextReplacementEnabled = false
        view.isAutomaticSpellingCorrectionEnabled = false
        view.allowsUndo = true
        view.drawsBackground = false
        // Zero, so the words keep the same left margin as the rendered
        // description they replace.
        view.textContainer?.lineFragmentPadding = 0
        view.setAccessibilityLabel("Description")
        context.coordinator.watchClicks(around: view)
        context.coordinator.fadeClippedEdges(of: scroll, document: view)

        // A description is added to far more often than replaced, so the caret
        // starts where the writing does.
        DispatchQueue.main.async {
            view.window?.makeFirstResponder(view)
            view.moveToEndOfDocument(nil)
        }
        return scroll
    }

    func updateNSView(_ scroll: NSScrollView, context: Context) {
        context.coordinator.parent = self
        guard let view = scroll.documentView as? NSTextView else { return }
        // Assigning this sets the font over the whole document, so a
        // description re-lays out on every keystroke if it is not guarded.
        if view.font != font { view.font = font }
        // Never while it holds focus: the text came from this view in the
        // first place, and writing it back mid-edit fights the typing.
        // Identity first: while typing this view is first responder, and the
        // compare walks the whole description to be thrown away.
        if view.window?.firstResponder !== view, view.string != text {
            view.string = text
        }
    }

    func sizeThatFits(
        _ proposal: ProposedViewSize, nsView scroll: NSScrollView, context: Context
    ) -> CGSize? {
        // Falling back to the width it already has rather than declining to
        // answer: SwiftUI asks for an ideal size with nothing specified, and a
        // scroll view has no intrinsic size to fall back on, so declining
        // leaves a stack allocating against a number that is not the text's.
        let proposed = proposal.width ?? 0
        let width = proposed > 0 && proposed < .infinity ? proposed : scroll.frame.width
        guard width > 0, width < .infinity else { return nil }

        // A legacy scroller is inside the clip view, not over it, so the text
        // wraps in less width than the scroll view has. Always subtracted
        // rather than only when the scroller will show: whether it shows
        // depends on the height being computed, and over-measuring costs
        // slack at the bottom where under-measuring clips a line.
        let gutter = scroll.scrollerStyle == .legacy && scroll.hasVerticalScroller
            ? NSScroller.scrollerWidth(for: .regular, scrollerStyle: .legacy) : 0
        let line = context.coordinator.lineHeight(in: font)
        let measured = context.coordinator.height(of: text, at: width - gutter, in: font)
        // Two lines when empty, so it reads as somewhere to write rather than
        // as a single stranded row.
        let wanted = max(measured, line * 2)

        // Nothing proposed asks what this would like to be, and that is a
        // different question from how much room it has — answering them alike
        // is what once made a stack believe this could never exceed its ideal.
        guard let offered = proposal.height else {
            // A window sizes itself to the ideal, so it cannot be the whole
            // text or a long description would open by resizing the window
            // around itself. A dozen lines is a text box.
            return CGSize(width: width, height: min(wanted, line * 12))
        }

        // Infinity asks how tall this could usefully be, and falls out as the
        // whole text — which is what lets a stack hand over its surplus. Zero
        // asks how small it could be made, and falls out as a line; answering
        // the text there would make the text the window's minimum height.
        return CGSize(width: width, height: min(wanted, max(offered, line)))
    }

    func makeCoordinator() -> Coordinator { Coordinator(self) }

    final class Coordinator: NSObject, NSTextViewDelegate {
        var parent: ScrollingTextView
        /// Clicking empty space moves no responder on its own, so the editor
        /// would keep focus and never commit — and a scroll view takes the
        /// click before anything drawn behind it could.
        private var clicks: Any?
        private weak var scroll: NSScrollView?
        private weak var fade: CAGradientLayer?
        private var edges: (top: CGFloat, bottom: CGFloat)?

        private var measured: (text: String, width: CGFloat, font: NSFont, height: CGFloat)?

        init(_ parent: ScrollingTextView) {
            self.parent = parent
        }

        /// Measured off the string rather than the live layout: the text
        /// container tracks the view's width, so asking the layout how tall
        /// the text is feeds the answer back into the question.
        ///
        /// Remembered because the answer is asked for far more often than it
        /// changes. Scrolling a long description re-runs the layout dozens of
        /// times a second — TextKit revises the document's height as it goes —
        /// and measuring ten thousand characters each time is most of a frame.
        @MainActor
        func height(of text: String, at width: CGFloat, in font: NSFont) -> CGFloat {
            if let measured, measured.text == text, measured.width == width,
                measured.font == font
            {
                return measured.height
            }
            let height = Self.measure(text, at: width, in: font)
            measured = (text, width, font, height)
            return height
        }

        /// Not remembered: one glyph is microseconds, where the description
        /// beside it is most of a frame, and a second slot to keep them from
        /// missing on each other costs more than it saves.
        @MainActor
        func lineHeight(in font: NSFont) -> CGFloat {
            Self.measure("X", at: .greatestFiniteMagnitude, in: font)
        }

        private static func measure(
            _ text: String, at width: CGFloat, in font: NSFont
        ) -> CGFloat {
            (text as NSString).boundingRect(
                with: CGSize(width: width, height: CGFloat.greatestFiniteMagnitude),
                options: [.usesLineFragmentOrigin, .usesFontLeading],
                attributes: [.font: font]
            ).height
        }

        deinit {
            if let clicks { NSEvent.removeMonitor(clicks) }
        }

        /// Installed for the editor's whole life rather than per edit, the way
        /// the title field's is: there is no notification for taking focus, so
        /// an editor focused and never typed into would have no monitor at all
        /// and would keep focus when you clicked empty space.
        @MainActor
        func watchClicks(around view: NSTextView) {
            guard clicks == nil else { return }
            // The event is returned untouched, so whatever the click was for
            // still happens: this only ends the edit on its way past.
            clicks = NSEvent.addLocalMonitorForEvents(matching: [.leftMouseDown]) { [weak view] event in
                guard let view, view.window?.firstResponder === view,
                    event.window === view.window
                else { return event }
                // The scroller belongs to the editor though it is outside the
                // text, so dragging it must not end the edit.
                let editor = view.enclosingScrollView ?? view
                let point = editor.convert(event.locationInWindow, from: nil)
                if !editor.bounds.contains(point) {
                    view.window?.makeFirstResponder(nil)
                }
                return event
            }
        }

        /// A line that dissolves reads as one that continues, where a line cut
        /// in half reads as one that broke — so the edge the text runs past is
        /// faded, and only that edge.
        ///
        /// Drawn as a layer mask on the clip view rather than as a mask in
        /// SwiftUI. A SwiftUI mask puts the whole editor through an offscreen
        /// pass on every frame of a scroll, and reporting the edges back out
        /// so SwiftUI could draw them re-rendered the pane around it each time
        /// one changed. Core Animation composites this with the scroll it is
        /// part of, and nothing outside the editor hears about it.
        @MainActor
        func fadeClippedEdges(of scroll: NSScrollView, document: NSView) {
            guard self.scroll == nil else { return }
            self.scroll = scroll

            let clip = scroll.contentView
            clip.wantsLayer = true
            let fade = CAGradientLayer()
            fade.colors = [
                NSColor.clear.cgColor, NSColor.black.cgColor,
                NSColor.black.cgColor, NSColor.clear.cgColor,
            ]
            fade.locations = [0, 0, 1, 1]
            fade.startPoint = CGPoint(x: 0.5, y: 0)
            fade.endPoint = CGPoint(x: 0.5, y: 1)
            clip.layer?.mask = fade
            self.fade = fade

            // Frame changes are posted by default; bounds changes are not.
            clip.postsBoundsChangedNotifications = true
            // Three ways for the mask to fall out of date: the clip view moves
            // over the document, the document grows under it, and the window
            // resizes the clip view itself. A resize posts no bounds change,
            // and a mask left at the old size hides what is past it.
            for (name, object) in [
                (NSView.boundsDidChangeNotification, clip),
                (NSView.frameDidChangeNotification, clip),
                (NSView.frameDidChangeNotification, document),
            ] {
                NotificationCenter.default.addObserver(
                    self, selector: #selector(edgesMoved), name: name, object: object
                )
            }
            edgesMoved()
        }

        @MainActor
        @objc private func edgesMoved() {
            guard let scroll, let fade else { return }
            let clip = scroll.contentView
            let visible = scroll.documentVisibleRect
            let height = max(scroll.documentView?.frame.height ?? 0, 1)
            let span = max(clip.bounds.height, 1)
            let depth = min(18 / span, 0.5)

            // A point of slack, so a scroll landing a hair short of an edge
            // does not claim there is more text past it.
            let top = visible.minY > 1 ? depth : 0
            let bottom = visible.maxY < height - 1 ? depth : 0

            // The mask rides in the clip view's own coordinates, which move as
            // it scrolls, and an implicit animation on either would trail the
            // scroll by a few frames.
            CATransaction.begin()
            CATransaction.setDisableActions(true)
            // The frame has to be rewritten every frame — a clip view scrolls
            // by moving the bounds origin the mask lives in — but the stops
            // move twice in a scroll, at the two ends.
            fade.frame = clip.bounds
            if edges?.top != top || edges?.bottom != bottom {
                fade.locations = [0, top, 1 - bottom, 1] as [NSNumber]
                edges = (top, bottom)
            }
            CATransaction.commit()
        }

        func textDidChange(_ notification: Notification) {
            guard let view = notification.object as? NSTextView else { return }
            parent.text = view.string
        }

        func textDidEndEditing(_ notification: Notification) {
            parent.onCommit()
        }

        /// Return inserts a newline on its own here — a text view is not a
        /// field — so only Escape needs saying.
        func textView(_ view: NSTextView, doCommandBy selector: Selector) -> Bool {
            guard selector == #selector(NSResponder.cancelOperation(_:)),
                let cancel = parent.onCancel
            else { return false }
            cancel()
            return true
        }
    }
}
