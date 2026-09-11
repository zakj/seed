import SwiftUI
import Textual

/// Descriptions and log entries render as one document rather than a view per
/// block, so a selection spans paragraphs and lists rather than stopping at the
/// block it started in.
struct MarkdownView: View {
    /// A notch above `.body`, which sits at 13pt and reads cramped for long
    /// descriptions.
    fileprivate static let bodySize: CGFloat = 14

    let source: String

    var body: some View {
        StructuredText(markdown: Self.withoutMathFences(source))
            .textual.headingStyle(SeedHeadingStyle())
            // Selection runs through Textual's own interaction layer and is off
            // by default; SwiftUI's `.textSelection` does not reach it.
            .textual.textSelection(.enabled)
            .font(.system(size: Self.bodySize))
            .frame(maxWidth: .infinity, alignment: .leading)
    }

    /// A ```` ```math ```` fence renders through SwiftUIMath, whose generated
    /// bundle accessor traps rather than degrades when it cannot find its
    /// fonts — and it cannot from inside the app, because it looks beside
    /// `Contents` rather than in `Resources`, where a bundle is unsealed
    /// content that will not codesign. It falls back to a path compiled in
    /// from the machine that built it, so the crash reaches whoever downloads
    /// a release and never the person who built it. Relabelled, the LaTeX is
    /// still legible and nothing loads a font. Goes away with
    /// https://github.com/gonzalezreal/textual/pull/82, which drops SwiftUIMath
    /// from the graph entirely unless a `Math` trait is turned on.
    private static func withoutMathFences(_ source: String) -> String {
        source.replacingOccurrences(
            of: "```math", with: "```latex", options: .caseInsensitive
        )
    }
}

/// Textual's own scale puts an h1 at 33pt, a poster headline in a pane this
/// narrow. Only the size and weight are ours. Replacing a style replaces its
/// whole body, so the block spacing below is `DefaultHeadingStyle`'s own
/// numbers restated — the scale every other block is calibrated against must
/// not move, and nothing keeps this copy in step with upstream but hand.
private struct SeedHeadingStyle: StructuredText.HeadingStyle {
    func makeBody(configuration: Configuration) -> some View {
        let level = configuration.headingLevel
        let size: CGFloat = switch level {
        case 1: MarkdownView.bodySize + 6
        case 2: MarkdownView.bodySize + 3
        default: MarkdownView.bodySize + 1
        }

        configuration.label
            .textual.fontScale(size / MarkdownView.bodySize)
            // Against the ambient size rather than the heading's, matching a
            // paragraph: a heading only wraps in a narrow pane, and when it
            // does it should not open up more than the prose under it.
            .textual.lineSpacing(.fontScaled(0.23))
            .textual.blockSpacing(.fontScaled(top: 1.6, bottom: 0.8))
            .fontWeight(level == 1 ? .bold : .semibold)
    }
}
