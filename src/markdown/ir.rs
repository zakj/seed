use pulldown_cmark::{Alignment, Event, Options, Parser, Tag, TagEnd};

/// A row of table cells, where each cell is a list of inline elements.
pub type TableRow = Vec<Vec<Inline>>;

#[derive(Debug, PartialEq)]
pub enum Block {
    Heading(pulldown_cmark::HeadingLevel, Vec<Inline>),
    Paragraph(Vec<Inline>),
    Code(Vec<String>),
    Quote(Vec<Block>),
    List(ListKind, Vec<Vec<Block>>),
    Table {
        alignments: Vec<Alignment>,
        header: Vec<TableRow>,
        body: Vec<TableRow>,
    },
    Rule,
}

#[derive(Debug, PartialEq)]
pub enum ListKind {
    Unordered,
    Ordered(u64),
}

impl ListKind {
    pub fn item_prefix(&self, item_idx: usize, depth: usize) -> String {
        let indent = depth * 4;
        match self {
            ListKind::Unordered => format!("{}- ", " ".repeat(indent)),
            ListKind::Ordered(start) => {
                let num = start.saturating_add(item_idx as u64);
                format!("{}{}. ", " ".repeat(indent), num)
            }
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum Inline {
    Text(String),
    Code(String),
    Emphasis(Vec<Inline>),
    Strong(Vec<Inline>),
    Link { url: String, content: Vec<Inline> },
    SoftBreak,
    HardBreak,
    Html(String),
}

pub fn parse(text: &str) -> Vec<Block> {
    let parser = Parser::new_ext(text, Options::ENABLE_TABLES);
    let events: Vec<Event> = parser.collect();
    let mut pos = 0;
    parse_blocks(&events, &mut pos, None)
}

fn parse_blocks(events: &[Event], pos: &mut usize, stop: Option<TagEnd>) -> Vec<Block> {
    let mut blocks = Vec::new();
    while *pos < events.len() {
        if let Some(ref end) = stop
            && matches!(&events[*pos], Event::End(e) if e == end)
        {
            *pos += 1;
            return blocks;
        }
        if let Some(block) = parse_one_block(events, pos) {
            blocks.push(block);
        } else {
            *pos += 1;
        }
    }
    blocks
}

/// Parse a single block-level element at the current position.
/// Returns None (without advancing pos) if the event isn't a block start.
fn parse_one_block(events: &[Event], pos: &mut usize) -> Option<Block> {
    match &events[*pos] {
        Event::Start(Tag::Heading { level, .. }) => {
            let level = *level;
            *pos += 1;
            let inlines = parse_inlines(events, pos, &TagEnd::Heading(level));
            Some(Block::Heading(level, inlines))
        }
        Event::Start(Tag::Paragraph) => {
            *pos += 1;
            let inlines = parse_inlines(events, pos, &TagEnd::Paragraph);
            Some(Block::Paragraph(inlines))
        }
        Event::Start(Tag::CodeBlock(_)) => {
            *pos += 1;
            let mut lines = Vec::new();
            while *pos < events.len() {
                match &events[*pos] {
                    Event::Text(text) => {
                        lines.extend(text.lines().map(String::from));
                        *pos += 1;
                    }
                    Event::End(TagEnd::CodeBlock) => {
                        *pos += 1;
                        break;
                    }
                    _ => {
                        *pos += 1;
                    }
                }
            }
            Some(Block::Code(lines))
        }
        Event::Start(Tag::BlockQuote(_)) => {
            *pos += 1;
            let inner = parse_blocks(events, pos, Some(TagEnd::BlockQuote(None)));
            Some(Block::Quote(inner))
        }
        Event::Start(Tag::List(start)) => {
            let kind = match start {
                Some(n) => ListKind::Ordered(*n),
                None => ListKind::Unordered,
            };
            *pos += 1;
            let mut items = Vec::new();
            while *pos < events.len() {
                match &events[*pos] {
                    Event::End(TagEnd::List(_)) => {
                        *pos += 1;
                        break;
                    }
                    Event::Start(Tag::Item) => {
                        *pos += 1;
                        let item_blocks = parse_item(events, pos);
                        items.push(item_blocks);
                    }
                    _ => {
                        *pos += 1;
                    }
                }
            }
            Some(Block::List(kind, items))
        }
        Event::Start(Tag::Table(alignments)) => {
            let alignments = alignments.clone();
            *pos += 1;
            let (header, body) = parse_table(events, pos);
            Some(Block::Table {
                alignments,
                header,
                body,
            })
        }
        Event::Rule => {
            *pos += 1;
            Some(Block::Rule)
        }
        _ => None,
    }
}

/// Parse a list item: collects blocks until TagEnd::Item.
/// Tight lists emit bare inlines (no Paragraph wrapper) — we wrap them.
fn parse_item(events: &[Event], pos: &mut usize) -> Vec<Block> {
    let mut blocks = Vec::new();
    let mut bare_inlines = Vec::new();

    while *pos < events.len() {
        if matches!(&events[*pos], Event::End(TagEnd::Item)) {
            *pos += 1;
            break;
        }

        // Block-level elements: delegate to shared parser
        if let Some(block) = parse_one_block(events, pos) {
            flush_bare_inlines(&mut bare_inlines, &mut blocks);
            blocks.push(block);
            continue;
        }

        // Bare inline content (tight lists without Paragraph wrappers)
        if let Some(inline) = parse_one_inline(events, pos) {
            bare_inlines.push(inline);
        } else {
            *pos += 1;
        }
    }

    flush_bare_inlines(&mut bare_inlines, &mut blocks);
    blocks
}

fn flush_bare_inlines(inlines: &mut Vec<Inline>, blocks: &mut Vec<Block>) {
    if !inlines.is_empty() {
        blocks.push(Block::Paragraph(std::mem::take(inlines)));
    }
}

fn parse_inlines(events: &[Event], pos: &mut usize, stop: &TagEnd) -> Vec<Inline> {
    let mut inlines = Vec::new();
    while *pos < events.len() {
        if matches!(&events[*pos], Event::End(e) if e == stop) {
            *pos += 1;
            return inlines;
        }
        if let Some(inline) = parse_one_inline(events, pos) {
            inlines.push(inline);
        } else {
            *pos += 1;
        }
    }
    inlines
}

/// Parse a single inline element at the current position.
/// Returns None (without advancing pos) if the event isn't an inline.
fn parse_one_inline(events: &[Event], pos: &mut usize) -> Option<Inline> {
    match &events[*pos] {
        Event::Text(t) => {
            *pos += 1;
            Some(Inline::Text(t.to_string()))
        }
        Event::Code(c) => {
            *pos += 1;
            Some(Inline::Code(c.to_string()))
        }
        Event::SoftBreak => {
            *pos += 1;
            Some(Inline::SoftBreak)
        }
        Event::HardBreak => {
            *pos += 1;
            Some(Inline::HardBreak)
        }
        Event::Start(Tag::Emphasis) => {
            *pos += 1;
            Some(Inline::Emphasis(parse_inlines(
                events,
                pos,
                &TagEnd::Emphasis,
            )))
        }
        Event::Start(Tag::Strong) => {
            *pos += 1;
            Some(Inline::Strong(parse_inlines(events, pos, &TagEnd::Strong)))
        }
        Event::Start(Tag::Link { dest_url, .. }) => {
            let url = dest_url.to_string();
            *pos += 1;
            Some(Inline::Link {
                url,
                content: parse_inlines(events, pos, &TagEnd::Link),
            })
        }
        Event::Html(html) | Event::InlineHtml(html) => {
            *pos += 1;
            Some(Inline::Html(html.to_string()))
        }
        _ => None,
    }
}

fn parse_table(events: &[Event], pos: &mut usize) -> (Vec<TableRow>, Vec<TableRow>) {
    let mut header = Vec::new();
    let mut body = Vec::new();
    let mut current_row: Vec<Vec<Inline>> = Vec::new();

    while *pos < events.len() {
        match &events[*pos] {
            Event::End(TagEnd::Table) => {
                *pos += 1;
                return (header, body);
            }
            Event::Start(Tag::TableHead) => {
                current_row = Vec::new();
                *pos += 1;
            }
            Event::End(TagEnd::TableHead) => {
                header.push(std::mem::take(&mut current_row));
                *pos += 1;
            }
            Event::Start(Tag::TableRow) => {
                current_row = Vec::new();
                *pos += 1;
            }
            Event::End(TagEnd::TableRow) => {
                body.push(std::mem::take(&mut current_row));
                *pos += 1;
            }
            Event::Start(Tag::TableCell) => {
                *pos += 1;
                let cell = parse_inlines(events, pos, &TagEnd::TableCell);
                current_row.push(cell);
            }
            _ => {
                *pos += 1;
            }
        }
    }
    (header, body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pulldown_cmark::HeadingLevel;

    #[test]
    fn paragraph() {
        let blocks = parse("Hello world.");
        assert_eq!(
            blocks,
            vec![Block::Paragraph(vec![Inline::Text("Hello world.".into())])]
        );
    }

    #[test]
    fn heading() {
        let blocks = parse("# Title");
        assert_eq!(
            blocks,
            vec![Block::Heading(
                HeadingLevel::H1,
                vec![Inline::Text("Title".into())]
            )]
        );
    }

    #[test]
    fn code_block() {
        let blocks = parse("```\nlet x = 1;\nlet y = 2;\n```");
        assert_eq!(
            blocks,
            vec![Block::Code(vec!["let x = 1;".into(), "let y = 2;".into()])]
        );
    }

    #[test]
    fn unordered_list() {
        let blocks = parse("- one\n- two\n- three");
        assert_eq!(
            blocks,
            vec![Block::List(
                ListKind::Unordered,
                vec![
                    vec![Block::Paragraph(vec![Inline::Text("one".into())])],
                    vec![Block::Paragraph(vec![Inline::Text("two".into())])],
                    vec![Block::Paragraph(vec![Inline::Text("three".into())])],
                ]
            )]
        );
    }

    #[test]
    fn ordered_list() {
        let blocks = parse("1. first\n2. second\n3. third");
        assert_eq!(
            blocks,
            vec![Block::List(
                ListKind::Ordered(1),
                vec![
                    vec![Block::Paragraph(vec![Inline::Text("first".into())])],
                    vec![Block::Paragraph(vec![Inline::Text("second".into())])],
                    vec![Block::Paragraph(vec![Inline::Text("third".into())])],
                ]
            )]
        );
    }

    #[test]
    fn ordered_list_custom_start() {
        let blocks = parse("5. fifth\n6. sixth");
        assert_eq!(
            blocks,
            vec![Block::List(
                ListKind::Ordered(5),
                vec![
                    vec![Block::Paragraph(vec![Inline::Text("fifth".into())])],
                    vec![Block::Paragraph(vec![Inline::Text("sixth".into())])],
                ]
            )]
        );
    }

    #[test]
    fn nested_lists() {
        let blocks = parse("- outer\n  - inner");
        assert_eq!(
            blocks,
            vec![Block::List(
                ListKind::Unordered,
                vec![vec![
                    Block::Paragraph(vec![Inline::Text("outer".into())]),
                    Block::List(
                        ListKind::Unordered,
                        vec![vec![Block::Paragraph(vec![Inline::Text("inner".into())])]]
                    ),
                ]]
            )]
        );
    }

    #[test]
    fn blockquote() {
        let blocks = parse("> quoted text");
        assert_eq!(
            blocks,
            vec![Block::Quote(vec![Block::Paragraph(vec![Inline::Text(
                "quoted text".into()
            )])])]
        );
    }

    #[test]
    fn table_with_alignments() {
        let blocks = parse("| A | B | C |\n|---|---:|:---:|\n| 1 | 2 | 3 |");
        match &blocks[0] {
            Block::Table {
                alignments,
                header,
                body,
            } => {
                assert_eq!(
                    alignments,
                    &vec![Alignment::None, Alignment::Right, Alignment::Center]
                );
                assert_eq!(header.len(), 1);
                assert_eq!(header[0].len(), 3);
                assert_eq!(body.len(), 1);
                assert_eq!(body[0].len(), 3);
            }
            other => panic!("expected Table, got {other:?}"),
        }
    }

    #[test]
    fn rule() {
        let blocks = parse("---");
        assert_eq!(blocks, vec![Block::Rule]);
    }

    #[test]
    fn tight_vs_loose_list() {
        // Tight list: no blank lines between items
        let tight = parse("- a\n- b");
        match &tight[0] {
            Block::List(ListKind::Unordered, items) => {
                assert_eq!(items.len(), 2);
                // Each item has exactly one Paragraph
                for item in items {
                    assert_eq!(item.len(), 1);
                    assert!(matches!(&item[0], Block::Paragraph(_)));
                }
            }
            _ => panic!("expected list"),
        }

        // Loose list: blank line between items
        let loose = parse("- a\n\n- b");
        match &loose[0] {
            Block::List(ListKind::Unordered, items) => {
                assert_eq!(items.len(), 2);
                for item in items {
                    assert!(matches!(&item[0], Block::Paragraph(_)));
                }
            }
            _ => panic!("expected list"),
        }
    }

    #[test]
    fn hard_break_vs_soft_break() {
        // Two trailing spaces = HardBreak
        let blocks = parse("line one  \nline two");
        match &blocks[0] {
            Block::Paragraph(inlines) => {
                assert!(inlines.contains(&Inline::HardBreak));
            }
            _ => panic!("expected paragraph"),
        }

        // Single newline = SoftBreak
        let blocks = parse("line one\nline two");
        match &blocks[0] {
            Block::Paragraph(inlines) => {
                assert!(inlines.contains(&Inline::SoftBreak));
            }
            _ => panic!("expected paragraph"),
        }
    }

    #[test]
    fn inline_nesting() {
        // Bold inside italic inside link
        let blocks = parse("[*__deep__*](http://example.com)");
        match &blocks[0] {
            Block::Paragraph(inlines) => {
                assert_eq!(inlines.len(), 1);
                match &inlines[0] {
                    Inline::Link { url, content } => {
                        assert_eq!(url, "http://example.com");
                        assert_eq!(content.len(), 1);
                        match &content[0] {
                            Inline::Emphasis(inner) => {
                                assert_eq!(inner.len(), 1);
                                assert!(matches!(&inner[0], Inline::Strong(_)));
                            }
                            other => panic!("expected Emphasis, got {other:?}"),
                        }
                    }
                    other => panic!("expected Link, got {other:?}"),
                }
            }
            _ => panic!("expected paragraph"),
        }
    }

    #[test]
    fn inline_code() {
        let blocks = parse("Use `foo` here.");
        match &blocks[0] {
            Block::Paragraph(inlines) => {
                assert_eq!(
                    inlines,
                    &vec![
                        Inline::Text("Use ".into()),
                        Inline::Code("foo".into()),
                        Inline::Text(" here.".into()),
                    ]
                );
            }
            _ => panic!("expected paragraph"),
        }
    }
}
