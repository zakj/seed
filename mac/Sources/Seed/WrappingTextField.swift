import AppKit
import SwiftUI

/// SwiftUI's multi-line `TextField` wraps only while focused; a wrapping
/// `NSTextField` wraps in both states.
struct WrappingTextField: NSViewRepresentable {
    @Binding var text: String
    var placeholder: String
    var font: NSFont
    var onEditingChanged: (Bool) -> Void = { _ in }
    var onCommit: () -> Void = {}
    /// Set to give the field Escape; without one the key keeps its usual meaning.
    var onCancel: (() -> Void)?
    var takesFocus = false

    /// The title face, shared by the composer and the detail pane.
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
            DispatchQueue.main.async { field.window?.makeFirstResponder(field) }
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

    /// `intrinsicContentSize` reports a single line, so measure the cell directly.
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
        private var clicks: Any?

        init(_ parent: WrappingTextField) {
            self.parent = parent
        }

        deinit {
            if let clicks { NSEvent.removeMonitor(clicks) }
        }

        /// For the field's whole life: `controlTextDidBeginEditing` announces
        /// the first change, not focus, so a per-edit monitor would miss a field
        /// focused and never typed into.
        @MainActor
        func watchClicks(around field: NSTextField) {
            guard clicks == nil else { return }
            clicks = NSEvent.endEditingOnClickOutside(field) { [weak field] in
                field?.currentEditor() != nil
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

        /// A wrapping field editor treats Return as a newline; here it ends the edit.
        func control(
            _ control: NSControl, textView: NSTextView, doCommandBy selector: Selector
        ) -> Bool {
            if selector == #selector(NSResponder.cancelOperation(_:)), let cancel = parent.onCancel
            {
                cancel()
                return true
            }
            guard selector == #selector(NSResponder.insertNewline(_:)) else { return false }
            control.window?.makeFirstResponder(nil)
            return true
        }
    }
}
