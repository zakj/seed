import SwiftUI
import Textual

/// One document rather than a view per block, so a selection can span paragraphs.
struct MarkdownView: View {
    /// A notch above `.body`, which reads cramped for long descriptions.
    fileprivate static let bodySize: CGFloat = 14

    let source: String

    var body: some View {
        StructuredText(markdown: Self.withoutMathFences(source))
            .textual.headingStyle(SeedHeadingStyle())
            // Selection is Textual's own and off by default.
            .textual.textSelection(.enabled)
            .font(.system(size: Self.bodySize))
            .frame(maxWidth: .infinity, alignment: .leading)
    }

    /// A ```` ```math ```` fence would render through SwiftUIMath, whose bundle
    /// accessor traps inside a signed `.app`, where its fonts are missing.
    /// Relabelled, the LaTeX stays legible. Goes away with
    /// https://github.com/gonzalezreal/textual/pull/82.
    private static func withoutMathFences(_ source: String) -> String {
        source.replacingOccurrences(
            of: "```math", with: "```latex", options: .caseInsensitive
        )
    }
}

/// Textual's own scale puts an h1 at 33pt. Replacing a style replaces its whole
/// body, so the block spacing restates `DefaultHeadingStyle`'s numbers by hand.
private struct SeedHeadingStyle: StructuredText.HeadingStyle {
    func makeBody(configuration: Configuration) -> some View {
        let level = configuration.headingLevel
        let size: CGFloat =
            switch level {
            case 1: MarkdownView.bodySize + 6
            case 2: MarkdownView.bodySize + 3
            default: MarkdownView.bodySize + 1
            }

        configuration.label
            .textual.fontScale(size / MarkdownView.bodySize)
            // Against the ambient size, matching a paragraph.
            .textual.lineSpacing(.fontScaled(0.23))
            .textual.blockSpacing(.fontScaled(top: 1.6, bottom: 0.8))
            .fontWeight(level == 1 ? .bold : .semibold)
    }
}
