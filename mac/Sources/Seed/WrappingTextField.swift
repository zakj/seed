import AppKit
import SwiftUI

/// SwiftUI's multi-line `TextField` only wraps while it holds focus; unfocused it
/// falls back to a single-line cell and clips. A plain wrapping `NSTextField`
/// behaves the same in both states.
struct WrappingTextField: NSViewRepresentable {
    @Binding var text: String
    var placeholder: String
    var font: NSFont
    var onEditingChanged: (Bool) -> Void = { _ in }
    var onCommit: () -> Void = {}
    /// Set to give the field Escape; without one the key keeps its usual meaning.
    var onCancel: (() -> Void)?
    var takesFocus = false
    /// Return inserts a line rather than ending the edit.
    var insertsNewlines = false
    /// Taking focus selects the text, so the first keystroke replaces it. Fine
    /// for a title being renamed, ruinous for a description being appended to.
    var selectsOnFocus = true

    /// The title face, shared by the composer and the detail pane — they are
    /// meant to be the same field, which two identical literals only imply.
    static let title = NSFont.systemFont(
        ofSize: NSFont.preferredFont(forTextStyle: .title2).pointSize,
        weight: .semibold
    )

    func makeNSView(context: Context) -> NSTextField {
        let field = NSTextField(wrappingLabelWithString: text)
        field.isEditable = true
        field.placeholderString = placeholder
        field.font = font
        field.delegate = context.coordinator
        field.focusRingType = .none
        context.coordinator.watchClicks(around: field)
        if takesFocus {
            let selectsOnFocus = selectsOnFocus
            DispatchQueue.main.async {
                field.window?.makeFirstResponder(field)
                guard !selectsOnFocus else { return }
                field.currentEditor()?.moveToEndOfDocument(nil)
            }
        }
        return field
    }

    func updateNSView(_ field: NSTextField, context: Context) {
        context.coordinator.parent = self
        field.font = font
        if field.stringValue != text, field.currentEditor() == nil {
            field.stringValue = text
        }
    }

    /// `intrinsicContentSize` reports a single line no matter what
    /// `preferredMaxLayoutWidth` is set to, so measure the cell directly.
    func sizeThatFits(
        _ proposal: ProposedViewSize, nsView field: NSTextField, context: Context
    ) -> CGSize? {
        guard let width = proposal.width, width > 0, width < .infinity, let cell = field.cell else {
            return nil
        }
        let bounds = NSRect(x: 0, y: 0, width: width, height: .greatestFiniteMagnitude)
        return CGSize(width: width, height: cell.cellSize(forBounds: bounds).height)
    }

    func makeCoordinator() -> Coordinator { Coordinator(self) }

    final class Coordinator: NSObject, NSTextFieldDelegate {
        var parent: WrappingTextField
        /// Watches for a click outside the field while it is being edited.
        /// Clicking empty space moves no responder on its own, so the field
        /// would keep focus and never commit — and a scroll view takes the click
        /// before anything drawn behind it could.
        private var clicks: Any?

        init(_ parent: WrappingTextField) {
            self.parent = parent
        }

        deinit {
            if let clicks { NSEvent.removeMonitor(clicks) }
        }

        /// Installed for the field's whole life rather than per edit:
        /// `controlTextDidBeginEditing` announces the first *change*, not focus,
        /// so a field focused and never typed into had no monitor at all and
        /// kept focus when you clicked empty space.
        @MainActor
        func watchClicks(around field: NSTextField) {
            guard clicks == nil else { return }
            // The event is returned untouched, so whatever the click was for
            // still happens: this only ends the edit on its way past.
            clicks = NSEvent.addLocalMonitorForEvents(matching: [.leftMouseDown]) { [weak field] event in
                guard let field, field.currentEditor() != nil, event.window === field.window else {
                    return event
                }
                let point = field.convert(event.locationInWindow, from: nil)
                if !field.bounds.contains(point) {
                    field.window?.makeFirstResponder(nil)
                }
                return event
            }
        }

        func controlTextDidBeginEditing(_ notification: Notification) {
            parent.onEditingChanged(true)
        }

        func controlTextDidChange(_ notification: Notification) {
            guard let field = notification.object as? NSTextField else { return }
            parent.text = field.stringValue
        }

        func controlTextDidEndEditing(_ notification: Notification) {
            parent.onEditingChanged(false)
            parent.onCommit()
        }

        /// A wrapping field editor treats Return as a newline; titles are one line.
        func control(
            _ control: NSControl, textView: NSTextView, doCommandBy selector: Selector
        ) -> Bool {
            if selector == #selector(NSResponder.cancelOperation(_:)), let cancel = parent.onCancel {
                cancel()
                return true
            }
            guard selector == #selector(NSResponder.insertNewline(_:)) else { return false }
            if parent.insertsNewlines {
                // The field editor ends editing on Return unless told otherwise.
                textView.insertNewlineIgnoringFieldEditor(nil)
            } else {
                control.window?.makeFirstResponder(nil)
            }
            return true
        }
    }
}
