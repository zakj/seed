import Foundation
import Markdown

/// A flat block model for rendering: cmark-gfm does the parsing, this reduces its
/// tree to the handful of shapes the detail pane draws natively.
public enum MarkdownBlock: Hashable, Sendable {
    case heading(level: Int, text: AttributedString)
    case paragraph(AttributedString)
    case listItem(indent: Int, marker: ListMarker, checked: Bool?, text: AttributedString)
    case code(language: String?, text: String)
    case quote([MarkdownBlock])
    case table(header: [AttributedString], rows: [[AttributedString]])
    case rule

    public enum ListMarker: Hashable, Sendable {
        case bullet
        case ordered(Int)
    }
}

public enum Markdown {
    public static func parse(_ source: String) -> [MarkdownBlock] {
        blocks(of: Document(parsing: source), indent: 0)
    }

    private static func blocks(of markup: any Markup, indent: Int) -> [MarkdownBlock] {
        switch markup {
        case let heading as Heading:
            [.heading(level: heading.level, text: inline(of: heading))]

        case let paragraph as Paragraph:
            [.paragraph(inline(of: paragraph))]

        case let code as CodeBlock:
            [.code(
                language: code.language,
                text: code.code.trimmingCharacters(in: .newlines)
            )]

        case let quote as BlockQuote:
            [.quote(children(of: quote, indent: indent))]

        case is ThematicBreak:
            [.rule]

        case let list as UnorderedList:
            list.listItems.flatMap { item(item: $0, marker: .bullet, indent: indent) }

        case let list as OrderedList:
            list.listItems.enumerated().flatMap { offset, listItem in
                item(
                    item: listItem,
                    marker: .ordered(Int(list.startIndex) + offset),
                    indent: indent
                )
            }

        case let table as Table:
            [.table(
                header: table.head.cells.map { inline(of: $0) },
                rows: table.body.rows.map { row in row.cells.map { inline(of: $0) } }
            )]

        // Raw HTML has no native rendering here; show it rather than drop it.
        case let html as HTMLBlock:
            [.code(language: "html", text: html.rawHTML.trimmingCharacters(in: .newlines))]

        default:
            children(of: markup, indent: indent)
        }
    }

    private static func children(of markup: any Markup, indent: Int) -> [MarkdownBlock] {
        markup.children.flatMap { blocks(of: $0, indent: indent) }
    }

    /// A list item's first paragraph is the item's own text; anything after it —
    /// a nested list, a code block — becomes its own block one level in.
    private static func item(
        item: ListItem, marker: MarkdownBlock.ListMarker, indent: Int
    ) -> [MarkdownBlock] {
        var text: AttributedString?
        var rest: [MarkdownBlock] = []

        for child in item.children {
            if text == nil, let paragraph = child as? Paragraph {
                text = inline(of: paragraph)
            } else {
                rest += blocks(of: child, indent: indent + 1)
            }
        }

        let checked = item.checkbox.map { $0 == .checked }
        return [.listItem(indent: indent, marker: marker, checked: checked, text: text ?? AttributedString())] + rest
    }

    /// Inline spans are built straight from cmark's tree. Re-emitting them as
    /// markdown source and parsing that again would reinterpret whatever the
    /// first pass had already resolved — `\*literal\*` came back as emphasis.
    private static func inline(of markup: any Markup) -> AttributedString {
        markup.children.map(fragment).reduce(into: AttributedString()) { $0 += $1 }
    }

    private static func fragment(_ markup: any Markup) -> AttributedString {
        switch markup {
        case let text as Text: AttributedString(text.string)
        case let code as InlineCode: styled(AttributedString(code.code), .code)
        case is SoftBreak: AttributedString(" ")
        case is LineBreak: AttributedString("\n")
        case let emphasis as Emphasis: styled(inline(of: emphasis), .emphasized)
        case let strong as Strong: styled(inline(of: strong), .stronglyEmphasized)
        case let struck as Strikethrough: styled(inline(of: struck), .strikethrough)
        case let link as Link: linked(inline(of: link), to: link.destination)
        case let html as InlineHTML: AttributedString(html.rawHTML)
        default: inline(of: markup)
        }
    }

    /// Unioned rather than assigned so `**bold *and italic***` keeps both.
    private static func styled(
        _ text: AttributedString, _ intent: InlinePresentationIntent
    ) -> AttributedString {
        text.transformingAttributes(\.inlinePresentationIntent) {
            $0.value = ($0.value ?? []).union(intent)
        }
    }

    private static func linked(_ text: AttributedString, to destination: String?) -> AttributedString {
        guard let destination, let url = URL(string: destination) else { return text }
        var result = text
        result.link = url
        return result
    }
}
