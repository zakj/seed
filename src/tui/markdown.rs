use pulldown_cmark::Alignment;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use unicode_width::UnicodeWidthStr;

use crate::markdown::ir::{Block, Inline};
use crate::markdown::{compute_col_widths, parse};

const CODE_BG: Color = Color::Rgb(48, 48, 48);
const DIM: Style = Style::new().add_modifier(Modifier::DIM);

pub fn render(text: &str, width: usize) -> Text<'static> {
    let blocks = parse(text);
    let mut lines: Vec<Line<'static>> = Vec::new();
    render_blocks(&blocks, &mut lines, width, 0);

    // Trim trailing empty lines
    while lines.last().is_some_and(|l| l.spans.is_empty()) {
        lines.pop();
    }

    Text::from(lines)
}

fn render_blocks(blocks: &[Block], lines: &mut Vec<Line<'static>>, width: usize, depth: usize) {
    for block in blocks {
        match block {
            Block::Heading(level, inlines) => {
                let hashes = "#".repeat(*level as usize);
                let indent_w = hashes.len() + 1;
                let bold = Style::new().add_modifier(Modifier::BOLD);
                let spans: Vec<Span<'static>> = render_inlines(inlines, Style::default())
                    .into_iter()
                    .map(|s| s.style(bold))
                    .collect();
                let words = split_spans_into_words(&spans);
                let initial = Span::styled(format!("{hashes} "), DIM);
                let subsequent = Span::raw(" ".repeat(indent_w));
                let wrapped = wrap_spans(&words, initial, subsequent, width);
                lines.extend(wrapped.into_iter().map(Line::from));
                lines.push(Line::default());
            }
            Block::Paragraph(inlines) => {
                let spans = render_inlines(inlines, Style::default());
                wrap_spans_into(lines, &spans, width, "", "");
                lines.push(Line::default());
            }
            Block::Code(code_lines) => {
                let style = Style::new().bg(CODE_BG);
                let max_w = code_lines.iter().map(|l| l.width()).max().unwrap_or(0);
                for code_line in code_lines {
                    let padding = max_w - code_line.width();
                    lines.push(Line::from(vec![
                        Span::styled("  ", style),
                        Span::styled(code_line.clone(), style),
                        Span::styled(" ".repeat(padding + 2), style),
                    ]));
                }
                lines.push(Line::default());
            }
            Block::Quote(inner) => {
                let bar_width = 3; // visible width of " ▎ "
                let inner_width = width.saturating_sub(bar_width);
                let mut inner_lines: Vec<Line<'static>> = Vec::new();
                render_blocks(inner, &mut inner_lines, inner_width, depth + 1);
                for line in inner_lines {
                    let mut spans = vec![Span::raw(" "), Span::styled("▎ ", DIM)];
                    spans.extend(line.spans);
                    lines.push(Line::from(spans));
                }
                lines.push(Line::default());
            }
            Block::List(kind, items) => {
                for (item_idx, item_blocks) in items.iter().enumerate() {
                    let prefix = kind.item_prefix(item_idx, depth);
                    let indent_str = " ".repeat(prefix.width());

                    for (block_idx, item_block) in item_blocks.iter().enumerate() {
                        match item_block {
                            Block::Paragraph(inlines) => {
                                let spans = render_inlines(inlines, Style::default());
                                if block_idx == 0 {
                                    wrap_spans_into(lines, &spans, width, &prefix, &indent_str);
                                } else {
                                    wrap_spans_into(lines, &spans, width, &indent_str, &indent_str);
                                }
                            }
                            _ => {
                                render_blocks(
                                    std::slice::from_ref(item_block),
                                    lines,
                                    width,
                                    depth + 1,
                                );
                            }
                        }
                    }
                }
                if depth == 0 {
                    lines.push(Line::default());
                }
            }
            Block::Table {
                alignments,
                header,
                body,
            } => {
                render_table(lines, alignments, header, body, width);
                lines.push(Line::default());
            }
            Block::Rule => {
                // Collapse preceding blank line so the rule sits tight.
                if lines.last().is_some_and(|l| l.spans.is_empty()) {
                    lines.pop();
                }
                let rule_width = width / 3;
                let pad = (width.saturating_sub(rule_width)) / 2;
                lines.push(Line::from(Span::styled(
                    format!("{}{}", " ".repeat(pad), "▁".repeat(rule_width)),
                    DIM,
                )));
                lines.push(Line::default());
            }
        }
    }
}

fn render_inlines(inlines: &[Inline], style: Style) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    for inline in inlines {
        match inline {
            Inline::Text(t) => {
                spans.push(Span::styled(t.clone(), style));
            }
            Inline::Code(c) => {
                let code_style = Style::new().bg(CODE_BG);
                spans.push(Span::styled(" ", code_style));
                spans.push(Span::styled(c.clone(), code_style));
                spans.push(Span::styled(" ", code_style));
            }
            Inline::Emphasis(inner) => {
                spans.extend(render_inlines(inner, style.add_modifier(Modifier::ITALIC)));
            }
            Inline::Strong(inner) => {
                spans.extend(render_inlines(inner, style.add_modifier(Modifier::BOLD)));
            }
            Inline::Link { content, .. } => {
                spans.extend(render_inlines(content, style.fg(Color::LightBlue)));
            }
            Inline::SoftBreak => {
                spans.push(Span::raw(" "));
            }
            Inline::HardBreak => {
                spans.push(Span::raw("\n"));
            }
            Inline::Html(html) => {
                spans.push(Span::styled(html.clone(), style));
            }
        }
    }
    spans
}

/// Measure the visible width of a slice of spans.
fn spans_width(spans: &[Span]) -> usize {
    spans.iter().map(|s| s.content.width()).sum()
}

/// Word-wrap spans into lines with styled prefix spans.
fn wrap_spans(
    words: &[Vec<Span<'static>>],
    initial_prefix: Span<'static>,
    subsequent_prefix: Span<'static>,
    width: usize,
) -> Vec<Vec<Span<'static>>> {
    if words.is_empty() {
        return Vec::new();
    }

    let initial_width = initial_prefix.content.width();
    let subsequent_width = subsequent_prefix.content.width();

    let mut result: Vec<Vec<Span<'static>>> = Vec::new();
    let mut current_line: Vec<Span<'static>> = vec![initial_prefix];
    let mut line_width = initial_width;
    let mut first_on_line = true;

    for word in words {
        // A \n word means HardBreak — flush current line
        if word.len() == 1 && word[0].content == "\n" {
            if !first_on_line || current_line.len() > 1 {
                result.push(std::mem::take(&mut current_line));
            }
            current_line = vec![subsequent_prefix.clone()];
            line_width = subsequent_width;
            first_on_line = true;
            continue;
        }

        let word_width = spans_width(word);

        if !first_on_line && line_width + 1 + word_width > width {
            result.push(std::mem::take(&mut current_line));
            current_line = vec![subsequent_prefix.clone()];
            line_width = subsequent_width;
            first_on_line = true;
        }

        if !first_on_line {
            // If both sides of the space have a background color, carry it
            // through so inline code spans stay visually connected.
            let space = match (
                current_line.last().and_then(|s| s.style.bg),
                word.first().and_then(|s| s.style.bg),
            ) {
                (Some(bg), Some(bg2)) if bg == bg2 => Span::styled(" ", Style::new().bg(bg)),
                _ => Span::raw(" "),
            };
            current_line.push(space);
            line_width += 1;
        }

        current_line.extend(word.iter().cloned());
        line_width += word_width;
        first_on_line = false;
    }

    if !current_line.is_empty() && (current_line.len() > 1 || !first_on_line) {
        result.push(current_line);
    }

    result
}

/// Word-wrap spans into lines with string indentation.
fn wrap_spans_into(
    lines: &mut Vec<Line<'static>>,
    spans: &[Span<'static>],
    width: usize,
    initial_prefix: &str,
    subsequent_prefix: &str,
) {
    let words = split_spans_into_words(spans);
    let wrapped = wrap_spans(
        &words,
        Span::raw(initial_prefix.to_string()),
        Span::raw(subsequent_prefix.to_string()),
        width,
    );
    lines.extend(wrapped.into_iter().map(Line::from));
}

/// Split a list of spans into "words" — groups of spans separated by whitespace.
/// Each word is a Vec<Span> preserving the original styles.
fn split_spans_into_words(spans: &[Span<'static>]) -> Vec<Vec<Span<'static>>> {
    let mut words: Vec<Vec<Span<'static>>> = Vec::new();
    let mut current_word: Vec<Span<'static>> = Vec::new();

    for span in spans {
        // Handle HardBreak markers
        if span.content.as_ref() == "\n" {
            if !current_word.is_empty() {
                words.push(std::mem::take(&mut current_word));
            }
            words.push(vec![span.clone()]);
            continue;
        }

        let text = span.content.as_ref();
        let style = span.style;

        // Whitespace-only spans with a background (e.g. inline code padding)
        // stay attached to the current word rather than acting as a boundary.
        if style.bg.is_some() && text.chars().all(|c| c.is_whitespace()) {
            current_word.push(span.clone());
            continue;
        }

        // Split span text on whitespace, creating word boundaries.
        let mut chars = text.char_indices().peekable();
        let mut segment_start = 0;
        let mut in_whitespace = false;

        while let Some(&(i, ch)) = chars.peek() {
            if ch.is_whitespace() {
                if !in_whitespace {
                    let segment = &text[segment_start..i];
                    if !segment.is_empty() {
                        current_word.push(Span::styled(segment.to_string(), style));
                    }
                    if !current_word.is_empty() {
                        words.push(std::mem::take(&mut current_word));
                    }
                    in_whitespace = true;
                }
                chars.next();
                segment_start = i + ch.len_utf8();
            } else {
                if in_whitespace {
                    in_whitespace = false;
                    segment_start = i;
                }
                chars.next();
            }
        }

        let remaining = &text[segment_start..];
        if !remaining.is_empty() {
            current_word.push(Span::styled(remaining.to_string(), style));
        }
    }

    if !current_word.is_empty() {
        words.push(current_word);
    }

    words
}

fn render_table(
    lines: &mut Vec<Line<'static>>,
    alignments: &[Alignment],
    header: &[Vec<Vec<Inline>>],
    body: &[Vec<Vec<Inline>>],
    width: usize,
) {
    // Render all cells to spans once; derive strings for width calculation.
    let all_rows: Vec<(bool, Vec<Vec<Span<'static>>>)> = header
        .iter()
        .map(|row| {
            (
                true,
                row.iter()
                    .map(|cell| render_inlines(cell, Style::default()))
                    .collect(),
            )
        })
        .chain(body.iter().map(|row| {
            (
                false,
                row.iter()
                    .map(|cell| render_inlines(cell, Style::default()))
                    .collect(),
            )
        }))
        .collect();

    let string_rows: Vec<Vec<String>> = all_rows
        .iter()
        .map(|(_, cells)| {
            cells
                .iter()
                .map(|spans| spans.iter().map(|s| s.content.as_ref()).collect())
                .collect()
        })
        .collect();

    let col_widths = compute_col_widths(&string_rows, alignments, width);
    let border_line = |left: &str, mid: &str, right: &str| -> Line<'static> {
        let mut spans = vec![Span::styled(left.to_string(), DIM)];
        for (i, &w) in col_widths.iter().enumerate() {
            if i > 0 {
                spans.push(Span::styled(mid.to_string(), DIM));
            }
            spans.push(Span::styled("─".repeat(w + 2), DIM));
        }
        spans.push(Span::styled(right.to_string(), DIM));
        Line::from(spans)
    };

    lines.push(border_line("╭", "┬", "╮"));

    let header_count = header.len();
    for (row_idx, (is_header, cell_spans)) in all_rows.iter().enumerate() {
        let wrapped: Vec<Vec<Vec<Span<'static>>>> = cell_spans
            .iter()
            .enumerate()
            .map(|(i, spans)| {
                let col_w = col_widths.get(i).copied().unwrap_or(1);
                let words = split_spans_into_words(spans);
                let result = wrap_spans(&words, Span::raw(""), Span::raw(""), col_w);
                if result.is_empty() {
                    vec![vec![]]
                } else {
                    result
                }
            })
            .collect();
        let max_lines = wrapped.iter().map(|w| w.len()).max().unwrap_or(1);

        for line_idx in 0..max_lines {
            let mut row_spans: Vec<Span<'static>> = Vec::new();
            for (i, wrapped_cell) in wrapped.iter().enumerate() {
                row_spans.push(Span::styled("│", DIM));
                row_spans.push(Span::raw(" "));

                let col_w = col_widths.get(i).copied().unwrap_or(0);
                let cell_line = wrapped_cell.get(line_idx).cloned().unwrap_or_default();
                let cell_width: usize = cell_line.iter().map(|s| s.content.width()).sum();
                let pad = col_w.saturating_sub(cell_width);

                let styled: Vec<Span<'static>> = if *is_header {
                    cell_line
                        .into_iter()
                        .map(|s| {
                            let style = s.style.add_modifier(Modifier::BOLD);
                            s.style(style)
                        })
                        .collect()
                } else {
                    cell_line
                };

                match alignments.get(i) {
                    Some(Alignment::Center) => {
                        let left = pad / 2;
                        let right = pad - left;
                        row_spans.push(Span::raw(" ".repeat(left)));
                        row_spans.extend(styled);
                        row_spans.push(Span::raw(" ".repeat(right)));
                    }
                    Some(Alignment::Right) => {
                        row_spans.push(Span::raw(" ".repeat(pad)));
                        row_spans.extend(styled);
                    }
                    _ => {
                        row_spans.extend(styled);
                        row_spans.push(Span::raw(" ".repeat(pad)));
                    }
                }
                row_spans.push(Span::raw(" "));
            }
            row_spans.push(Span::styled("│", DIM));
            lines.push(Line::from(row_spans));
        }

        if row_idx == header_count.saturating_sub(1) && header_count > 0 {
            lines.push(border_line("├", "┼", "┤"));
        }
    }

    lines.push(border_line("╰", "┴", "╯"));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Extract the plain text from a rendered Text, joining lines with newlines.
    fn plain(text: &Text) -> String {
        text.lines
            .iter()
            .map(|line| {
                line.spans
                    .iter()
                    .map(|s| s.content.as_ref())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn line_width(line: &Line) -> usize {
        spans_width(&line.spans)
    }

    // -- Code blocks --

    #[test]
    fn code_block_lines_have_uniform_width() {
        let md = "```\nshort\na longer line of code\n```";
        let text = render(md, 80);
        let code_lines: Vec<_> = text
            .lines
            .iter()
            .filter(|l| l.spans.iter().any(|s| s.style.bg == Some(CODE_BG)))
            .collect();
        assert!(code_lines.len() >= 2);
        let widths: Vec<usize> = code_lines.iter().map(|l| line_width(l)).collect();
        assert!(
            widths.iter().all(|&w| w == widths[0]),
            "code block lines should have uniform width, got {widths:?}"
        );
    }

    #[test]
    fn code_block_wide_lines_not_capped() {
        // Wide code lines extend past render width (clipped by ratatui at render time)
        let long_line = "x".repeat(200);
        let md = format!("```\n{long_line}\nshort\n```");
        let text = render(&md, 60);
        let code_lines: Vec<_> = text
            .lines
            .iter()
            .filter(|l| l.spans.iter().any(|s| s.style.bg == Some(CODE_BG)))
            .collect();
        // Short line is padded to match the long line
        let widths: Vec<usize> = code_lines.iter().map(|l| line_width(l)).collect();
        assert!(widths[0] > 60, "long line should exceed render width");
        assert!(
            widths.iter().all(|&w| w == widths[0]),
            "code block lines should have uniform width, got {widths:?}"
        );
    }

    #[test]
    fn code_block_has_background_style() {
        let md = "```\nlet x = 1;\n```";
        let text = render(md, 80);
        let has_code_bg = text
            .lines
            .iter()
            .any(|l| l.spans.iter().any(|s| s.style.bg == Some(CODE_BG)));
        assert!(has_code_bg, "code block should have background color");
    }

    #[test]
    fn code_block_followed_by_blank_line() {
        let md = "```\ncode\n```\n\nAfter.";
        let text = render(md, 80);
        let p = plain(&text);
        // There should be a blank line between code and "After."
        assert!(p.contains("\n\n"), "expected blank line after code block");
    }

    // -- Paragraphs --

    #[test]
    fn paragraph_wraps_at_width() {
        let md = "This is a fairly long paragraph that should be wrapped at the specified width for readability.";
        let text = render(md, 40);
        assert!(text.lines.len() > 1, "paragraph should wrap");
        for line in &text.lines {
            assert!(
                line_width(line) <= 40,
                "line width {} exceeds 40",
                line_width(line)
            );
        }
    }

    #[test]
    fn hard_break_forces_new_line() {
        // Two trailing spaces = hard break in CommonMark
        let md = "line one  \nline two";
        let text = render(md, 80);
        let p = plain(&text);
        assert!(
            p.contains("line one") && p.contains("line two"),
            "both lines should appear"
        );
        // Should be on separate lines (not joined by a space)
        let content_lines: Vec<_> = text.lines.iter().filter(|l| !l.spans.is_empty()).collect();
        assert!(
            content_lines.len() >= 2,
            "hard break should produce two lines"
        );
    }

    // -- Headings --

    #[test]
    fn heading_has_bold_style() {
        let md = "# Title";
        let text = render(md, 80);
        let bold_title =
            text.lines.iter().flat_map(|l| &l.spans).any(|s| {
                s.content.contains("Title") && s.style.add_modifier.contains(Modifier::BOLD)
            });
        assert!(bold_title, "heading title should have BOLD modifier");
    }

    #[test]
    fn heading_levels_have_correct_prefix() {
        for level in 1..=3 {
            let md = format!("{} Heading", "#".repeat(level));
            let text = render(&md, 80);
            let p = plain(&text);
            let prefix = format!("{} ", "#".repeat(level));
            assert!(
                p.contains(&prefix),
                "level {level} heading should have prefix {prefix:?}, got {p:?}"
            );
        }
    }

    // -- Inline styles --

    #[test]
    fn bold_text_has_bold_modifier() {
        let md = "Some **bold** text.";
        let text = render(md, 80);
        let bold_span =
            text.lines.iter().flat_map(|l| &l.spans).find(|s| {
                s.content.contains("bold") && s.style.add_modifier.contains(Modifier::BOLD)
            });
        assert!(bold_span.is_some(), "bold text should have BOLD modifier");
    }

    #[test]
    fn italic_text_has_italic_modifier() {
        let md = "Some *italic* text.";
        let text = render(md, 80);
        let italic_span = text.lines.iter().flat_map(|l| &l.spans).find(|s| {
            s.content.contains("italic") && s.style.add_modifier.contains(Modifier::ITALIC)
        });
        assert!(
            italic_span.is_some(),
            "italic text should have ITALIC modifier"
        );
    }

    #[test]
    fn inline_code_has_background() {
        let md = "Use `foo` here.";
        let text = render(md, 80);
        let code_span = text
            .lines
            .iter()
            .flat_map(|l| &l.spans)
            .find(|s| s.content.contains("foo") && s.style.bg == Some(CODE_BG));
        assert!(
            code_span.is_some(),
            "inline code should have background color"
        );
    }

    #[test]
    fn link_has_color() {
        let md = "A [link](https://example.com) here.";
        let text = render(md, 80);
        let link_span = text
            .lines
            .iter()
            .flat_map(|l| &l.spans)
            .find(|s| s.content.contains("link") && s.style.fg.is_some());
        assert!(link_span.is_some(), "link should have foreground color");
    }

    // -- Lists --

    #[test]
    fn unordered_list_items() {
        let md = "- one\n- two\n- three";
        let text = render(md, 80);
        let p = plain(&text);
        assert!(p.contains("- one"));
        assert!(p.contains("- two"));
        assert!(p.contains("- three"));
    }

    #[test]
    fn ordered_list_items() {
        let md = "1. first\n2. second\n3. third";
        let text = render(md, 80);
        let p = plain(&text);
        assert!(p.contains("1. first"));
        assert!(p.contains("2. second"));
        assert!(p.contains("3. third"));
    }

    #[test]
    fn nested_list() {
        let md = "- outer\n  - inner";
        let text = render(md, 80);
        let p = plain(&text);
        assert!(p.contains("- outer"));
        assert!(p.contains("    - inner"), "inner item should be indented");
    }

    // -- Blockquotes --

    #[test]
    fn blockquote_has_bar() {
        let md = "> quoted text";
        let text = render(md, 80);
        let p = plain(&text);
        assert!(p.contains("▎"), "blockquote should have bar character");
        assert!(p.contains("quoted text"));
    }

    // -- Tables --

    #[test]
    fn table_has_borders() {
        let md = "| A | B |\n|---|---|\n| 1 | 2 |";
        let text = render(md, 80);
        let p = plain(&text);
        assert!(p.contains("╭") && p.contains("╮"), "should have top border");
        assert!(
            p.contains("╰") && p.contains("╯"),
            "should have bottom border"
        );
        assert!(p.contains("│"), "should have vertical borders");
    }

    #[test]
    fn table_header_is_bold() {
        let md = "| Name |\n|------|\n| val  |";
        let text = render(md, 80);
        let bold_name =
            text.lines.iter().flat_map(|l| &l.spans).find(|s| {
                s.content.contains("Name") && s.style.add_modifier.contains(Modifier::BOLD)
            });
        assert!(bold_name.is_some(), "table header should be bold");
    }

    // -- Horizontal rule --

    #[test]
    fn horizontal_rule_renders() {
        let md = "Above\n\n---\n\nBelow";
        let text = render(md, 80);
        let p = plain(&text);
        assert!(p.contains("▁"), "should contain rule character");
        assert!(p.contains("Above") && p.contains("Below"));
    }

    // -- Trailing whitespace --

    #[test]
    fn trailing_empty_lines_trimmed() {
        let md = "Hello.\n\n";
        let text = render(md, 80);
        let last = text.lines.last().unwrap();
        assert!(
            !last.spans.is_empty(),
            "trailing empty lines should be trimmed"
        );
    }
}
