import SeedKit
import SwiftUI

struct MarkdownView: View {
    let source: String

    var body: some View {
        MarkdownBlocks(blocks: Markdown.parse(source))
            .frame(maxWidth: .infinity, alignment: .leading)
    }
}

struct MarkdownBlocks: View {
    /// A notch above `.body`, which sits at 13pt and reads cramped for long
    /// descriptions. Code spans have to match or the line height jumps.
    static let bodySize: CGFloat = 14

    let blocks: [MarkdownBlock]

    var body: some View {
        VStack(alignment: .leading, spacing: 9) {
            ForEach(blocks.indices, id: \.self) { index in
                MarkdownBlockView(block: blocks[index])
            }
        }
        .font(.system(size: Self.bodySize))
    }
}

struct MarkdownBlockView: View {
    let block: MarkdownBlock

    var body: some View {
        switch block {
        case .heading(let level, let text):
            Text(styled(text))
                .font(.system(size: headingSize(level), weight: .semibold))
                .padding(.top, 4)

        case .paragraph(let text):
            Text(styled(text))
                .fixedSize(horizontal: false, vertical: true)

        case .listItem(let indent, let marker, let checked, let text):
            HStack(alignment: .firstTextBaseline, spacing: 7) {
                bullet(marker, checked: checked)
                Text(styled(text))
                    .fixedSize(horizontal: false, vertical: true)
            }
            .padding(.leading, CGFloat(indent) * 18)

        case .code(let language, let text):
            ScrollView(.horizontal) {
                Text(text)
                    .font(.system(size: MarkdownBlocks.bodySize - 1, design: .monospaced))
                    .padding(9)
            }
            .background(.fill.quaternary, in: .rect(cornerRadius: 6))
            .overlay(alignment: .topTrailing) {
                if let language {
                    Text(language)
                        .font(.caption2)
                        .foregroundStyle(.tertiary)
                        .padding(.horizontal, 7)
                        .padding(.vertical, 4)
                }
            }

        case .quote(let nested):
            HStack(spacing: 9) {
                Capsule().fill(.tertiary).frame(width: 3)
                MarkdownBlocks(blocks: nested)
                    .foregroundStyle(.secondary)
            }
            .fixedSize(horizontal: false, vertical: true)

        case .table(let header, let rows):
            ScrollView(.horizontal) {
                Grid(alignment: .leading, horizontalSpacing: 18, verticalSpacing: 5) {
                    GridRow {
                        ForEach(header.indices, id: \.self) { column in
                            Text(styled(header[column])).fontWeight(.semibold)
                        }
                    }
                    Divider()
                    ForEach(rows.indices, id: \.self) { row in
                        GridRow {
                            ForEach(rows[row].indices, id: \.self) { column in
                                Text(styled(rows[row][column]))
                            }
                        }
                    }
                }
                .padding(.vertical, 2)
            }

        case .rule:
            Divider().padding(.vertical, 2)
        }
    }

    private func headingSize(_ level: Int) -> CGFloat {
        switch level {
        case 1: MarkdownBlocks.bodySize + 5
        case 2: MarkdownBlocks.bodySize + 3
        default: MarkdownBlocks.bodySize + 1
        }
    }

    @ViewBuilder
    private func bullet(_ marker: MarkdownBlock.ListMarker, checked: Bool?) -> some View {
        if let checked {
            Image(systemName: checked ? "checkmark.square.fill" : "square")
                .foregroundStyle(checked ? AnyShapeStyle(.tint) : AnyShapeStyle(.secondary))
        } else {
            switch marker {
            case .bullet:
                Text("•").foregroundStyle(.secondary)
            case .ordered(let number):
                Text("\(number).").monospacedDigit().foregroundStyle(.secondary)
            }
        }
    }

    /// SwiftUI renders emphasis from an AttributedString on its own, but leaves
    /// code spans looking like body text.
    private func styled(_ text: AttributedString) -> AttributedString {
        var result = text
        for run in text.runs where run.inlinePresentationIntent?.contains(.code) == true {
            result[run.range].font = .system(size: MarkdownBlocks.bodySize, design: .monospaced)
            result[run.range].backgroundColor = .secondary.opacity(0.12)
        }
        return result
    }
}
