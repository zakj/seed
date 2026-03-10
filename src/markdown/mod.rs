pub(crate) mod ir;

pub use ir::parse;

use anstyle::{Ansi256Color, AnsiColor, Color, Reset, Style};
use pulldown_cmark::Alignment;

use crate::term::{visible_width, wrap_words};

use ir::{Block, Inline, ListKind};

const MAX_WIDTH: usize = 80;
const BOLD: Style = Style::new().bold();
const CODE_BG: Style = Style::new().bg_color(Some(Color::Ansi256(Ansi256Color(235))));
const DIM: Style = Style::new().dimmed();
const LINK_COLOR: Style = Style::new().fg_color(Some(Color::Ansi(AnsiColor::BrightBlue)));

// anstyle only supports full SGR reset, so inline toggles that must nest
// (e.g. bold inside italic) use raw targeted escape sequences.
const BOLD_ON: &str = "\x1b[1m";
const BOLD_OFF: &str = "\x1b[22m";
const ITALIC_ON: &str = "\x1b[3m";
const ITALIC_OFF: &str = "\x1b[23m";

// OSC 8 hyperlink framing (outside SGR, not supported by anstyle).
const LINK_START: &str = "\x1b]8;;";
const LINK_END: &str = "\x1b\\";

pub fn render(text: &str, terminal_width: Option<usize>) -> String {
    let term_width = terminal_width.unwrap_or(MAX_WIDTH);
    let width = term_width.min(MAX_WIDTH);
    let blocks = ir::parse(text);
    let mut out = String::new();
    render_blocks(&blocks, &mut out, width, term_width, 0);

    let trimmed = out.trim_end();
    if trimmed.is_empty() {
        String::new()
    } else {
        format!("{trimmed}\n")
    }
}

fn render_blocks(
    blocks: &[Block],
    out: &mut String,
    width: usize,
    term_width: usize,
    depth: usize,
) {
    for block in blocks {
        match block {
            Block::Heading(level, inlines) => {
                let hashes = "#".repeat(*level as usize);
                let initial = format!("{DIM}{hashes}{Reset}{BOLD} ");
                let subsequent = format!("{BOLD}{}", " ".repeat(hashes.len() + 1));
                let text = render_inlines(inlines);
                wrap_words(&text, out, width, &initial, &subsequent);
                out.push_str(&format!("{Reset}\n"));
            }
            Block::Paragraph(inlines) => {
                render_paragraph(inlines, out, width, "", "");
                out.push('\n');
            }
            Block::Code(lines) => {
                let max_w = lines.iter().map(|l| visible_width(l)).max().unwrap_or(0);
                for line in lines {
                    let padding = max_w - visible_width(line);
                    out.push_str(&format!(
                        "{CODE_BG}  {line}{}  {Reset}\n",
                        " ".repeat(padding)
                    ));
                }
                out.push('\n');
            }
            Block::Quote(inner) => {
                let bar = format!(" {DIM}▎{Reset} ");
                let bar_width = visible_width(&bar);
                let inner_width = width.saturating_sub(bar_width);
                let mut buf = String::new();
                render_blocks(inner, &mut buf, inner_width, term_width, depth + 1);
                for line in buf.lines() {
                    out.push_str(&bar);
                    out.push_str(line);
                    out.push('\n');
                }
                out.push('\n');
            }
            Block::List(kind, items) => {
                for (item_idx, item_blocks) in items.iter().enumerate() {
                    let prefix = match kind {
                        ListKind::Unordered => {
                            let indent = depth * 4;
                            format!("{}- ", " ".repeat(indent))
                        }
                        ListKind::Ordered(start) => {
                            let num = start.saturating_add(item_idx as u64);
                            let indent = depth * 4;
                            format!("{}{}. ", " ".repeat(indent), num)
                        }
                    };
                    let subsequent = " ".repeat(visible_width(&prefix));

                    for (block_idx, item_block) in item_blocks.iter().enumerate() {
                        match item_block {
                            Block::Paragraph(inlines) => {
                                let text = render_inlines(inlines);
                                if !text.is_empty() {
                                    if block_idx == 0 {
                                        wrap_words(&text, out, width, &prefix, &subsequent);
                                    } else {
                                        wrap_words(&text, out, width, &subsequent, &subsequent);
                                    }
                                }
                            }
                            _ => {
                                render_blocks(
                                    std::slice::from_ref(item_block),
                                    out,
                                    width,
                                    term_width,
                                    depth + 1,
                                );
                            }
                        }
                    }
                }
                if depth == 0 {
                    out.push('\n');
                }
            }
            Block::Table {
                alignments,
                header,
                body,
            } => {
                // Convert inline cells to rendered strings for the table renderer
                let mut rows: Vec<Vec<String>> = Vec::new();
                for row in header {
                    rows.push(row.iter().map(|cell| render_inlines(cell)).collect());
                }
                let header_rows = rows.len();
                for row in body {
                    rows.push(row.iter().map(|cell| render_inlines(cell)).collect());
                }
                flush_table(&rows, alignments, header_rows, term_width, out);
                out.push('\n');
            }
            Block::Rule => {
                // Collapse the preceding blank line so the rule sits tight
                if out.ends_with("\n\n") {
                    out.pop();
                }
                let rule_width = width / 3;
                let pad = (width - rule_width) / 2;
                out.push_str(&format!(
                    "{}{DIM}{}{Reset}\n\n",
                    " ".repeat(pad),
                    "▁".repeat(rule_width)
                ));
            }
        }
    }
}

fn render_inlines(inlines: &[Inline]) -> String {
    let mut out = String::new();
    for inline in inlines {
        render_inline_into(inline, &mut out);
    }
    out
}

/// Render a paragraph, splitting at HardBreaks and wrapping each segment.
fn render_paragraph(
    inlines: &[Inline],
    out: &mut String,
    width: usize,
    initial_indent: &str,
    subsequent_indent: &str,
) {
    let has_hard_break = inlines.iter().any(|i| matches!(i, Inline::HardBreak));
    if !has_hard_break {
        let text = render_inlines(inlines);
        if !text.is_empty() {
            wrap_words(&text, out, width, initial_indent, subsequent_indent);
        }
        return;
    }
    // Split at HardBreaks and wrap each segment separately
    let mut seg_start = 0;
    let mut first = true;
    for (i, inline) in inlines.iter().enumerate() {
        if matches!(inline, Inline::HardBreak) {
            if seg_start < i {
                let text = render_inlines(&inlines[seg_start..i]);
                if !text.is_empty() {
                    let indent = if first {
                        initial_indent
                    } else {
                        subsequent_indent
                    };
                    wrap_words(&text, out, width, indent, subsequent_indent);
                    first = false;
                }
            }
            seg_start = i + 1;
        }
    }
    if seg_start < inlines.len() {
        let text = render_inlines(&inlines[seg_start..]);
        if !text.is_empty() {
            let indent = if first {
                initial_indent
            } else {
                subsequent_indent
            };
            wrap_words(&text, out, width, indent, subsequent_indent);
        }
    }
}

fn render_inline_into(inline: &Inline, out: &mut String) {
    match inline {
        Inline::Text(t) => out.push_str(t),
        Inline::Code(c) => {
            out.push_str(&format!("{CODE_BG}\u{00a0}{c}\u{00a0}{Reset}"));
        }
        Inline::Emphasis(inner) => {
            out.push_str(ITALIC_ON);
            out.push_str(&render_inlines(inner));
            out.push_str(ITALIC_OFF);
        }
        Inline::Strong(inner) => {
            out.push_str(BOLD_ON);
            out.push_str(&render_inlines(inner));
            out.push_str(BOLD_OFF);
        }
        Inline::Link { url, content } => {
            out.push_str(&format!(
                "{LINK_START}{url}{LINK_END}{}",
                LINK_COLOR.render()
            ));
            out.push_str(&render_inlines(content));
            out.push_str(&format!(
                "{}{LINK_START}{LINK_END}",
                LINK_COLOR.render_reset()
            ));
        }
        Inline::SoftBreak | Inline::HardBreak => out.push(' '),
        Inline::Html(html) => out.push_str(html),
    }
}

fn flush_table(
    rows: &[Vec<String>],
    alignments: &[Alignment],
    header_rows: usize,
    width: usize,
    out: &mut String,
) {
    if rows.is_empty() {
        return;
    }
    let num_cols = alignments.len();
    if num_cols == 0 {
        return;
    }

    let col_widths = compute_col_widths(rows, alignments, width);

    let border_row = |left: &str, mid: &str, right: &str| {
        let mut s = format!("{DIM}{left}");
        for (i, &w) in col_widths.iter().enumerate() {
            if i > 0 {
                s.push_str(mid);
            }
            s.push_str(&"─".repeat(w + 2));
        }
        s.push_str(&format!("{right}{Reset}\n"));
        s
    };

    out.push_str(&border_row("╭", "┬", "╮"));

    for (row_idx, row) in rows.iter().enumerate() {
        let is_header = row_idx < header_rows;

        // Wrap each cell to its column width
        let wrapped: Vec<Vec<String>> = row
            .iter()
            .enumerate()
            .map(|(i, cell)| {
                let w = col_widths.get(i).copied().unwrap_or(1);
                wrap_to_lines(cell, w)
            })
            .collect();
        let max_lines = wrapped.iter().map(|w| w.len()).max().unwrap_or(1);

        for line_idx in 0..max_lines {
            let mut line = String::new();
            for (i, wrapped_cell) in wrapped.iter().enumerate() {
                line.push_str(&format!("{DIM}│{Reset} "));
                let cell_text = wrapped_cell.get(line_idx).map(|s| s.as_str()).unwrap_or("");
                let w = visible_width(cell_text);
                let col_w = col_widths.get(i).copied().unwrap_or(0);
                let pad = col_w.saturating_sub(w);
                let push_content = |line: &mut String, content: &str| {
                    if is_header {
                        line.push_str(BOLD_ON);
                        line.push_str(content);
                        line.push_str(BOLD_OFF);
                    } else {
                        line.push_str(content);
                    }
                };
                match alignments.get(i) {
                    Some(Alignment::Center) => {
                        let left = pad / 2;
                        let right = pad - left;
                        line.push_str(&" ".repeat(left));
                        push_content(&mut line, cell_text);
                        line.push_str(&" ".repeat(right));
                    }
                    Some(Alignment::Right) => {
                        line.push_str(&" ".repeat(pad));
                        push_content(&mut line, cell_text);
                    }
                    _ => {
                        push_content(&mut line, cell_text);
                        line.push_str(&" ".repeat(pad));
                    }
                }
                line.push(' ');
            }
            line.push_str(&format!("{DIM}│{Reset}\n"));
            out.push_str(&line);
        }

        if row_idx == header_rows.saturating_sub(1) && header_rows > 0 {
            out.push_str(&border_row("├", "┼", "┤"));
        }
    }

    out.push_str(&border_row("╰", "┴", "╯"));
}

/// Compute column widths for a table, fitting within the given width.
pub(crate) fn compute_col_widths(
    rows: &[Vec<String>],
    alignments: &[Alignment],
    width: usize,
) -> Vec<usize> {
    let num_cols = alignments.len();
    let mut col_widths = vec![0usize; num_cols];
    for row in rows {
        for (i, cell) in row.iter().enumerate() {
            if i < num_cols {
                col_widths[i] = col_widths[i].max(visible_width(cell));
            }
        }
    }
    for w in col_widths.iter_mut() {
        *w = (*w).min(MAX_WIDTH);
    }

    // Shrink columns to fit terminal width.
    let overhead = 3 * num_cols + 1;
    let budget = width.saturating_sub(overhead).max(num_cols);
    let total: usize = col_widths.iter().sum();
    if total > budget {
        let mut frozen = vec![false; num_cols];
        let mut frozen_total = 0usize;
        loop {
            let unfrozen: usize = frozen.iter().filter(|&&f| !f).count();
            if unfrozen == 0 {
                break;
            }
            let remaining = budget.saturating_sub(frozen_total);
            let fair = remaining / unfrozen;
            let mut changed = false;
            for (i, w) in col_widths.iter_mut().enumerate() {
                if !frozen[i] && *w <= fair {
                    frozen[i] = true;
                    frozen_total += *w;
                    changed = true;
                }
            }
            if !changed {
                let remaining = budget.saturating_sub(frozen_total);
                let unfrozen_indices: Vec<usize> = (0..num_cols).filter(|&i| !frozen[i]).collect();
                let per_col = remaining / unfrozen_indices.len();
                let mut extra = remaining % unfrozen_indices.len();
                for &i in &unfrozen_indices {
                    col_widths[i] = per_col
                        + if extra > 0 {
                            extra -= 1;
                            1
                        } else {
                            0
                        };
                }
                break;
            }
        }
    }
    col_widths
}

fn wrap_to_lines(text: &str, width: usize) -> Vec<String> {
    if text.is_empty() {
        return vec![String::new()];
    }
    let mut out = String::new();
    wrap_words(text, &mut out, width, "", "");
    if out.is_empty() {
        return vec![String::new()];
    }
    out.trim_end_matches('\n')
        .split('\n')
        .map(String::from)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_paragraph_wraps() {
        let text = "This is a fairly long paragraph that should be wrapped at the specified width for readability in the terminal.";
        let result = render(text, Some(40));
        assert!(result.lines().all(|l| visible_width(l) <= 40));
        assert!(result.lines().count() > 1);
    }

    #[test]
    fn caps_at_max_width() {
        let long = "x ".repeat(50);
        let wide = render(&long, Some(200));
        let capped = render(&long, Some(MAX_WIDTH));
        assert_eq!(wide, capped);
    }

    #[test]
    fn renders_bold() {
        let result = render("Some **bold** text.", Some(80));
        assert!(result.contains("\x1b[1mbold\x1b[22m"));
    }

    #[test]
    fn renders_heading_bold() {
        let result = render("# Title\n\nBody.", Some(80));
        assert!(result.contains(&format!("{DIM}#{Reset}{BOLD} Title")));
        assert!(result.contains("Body."));
    }

    #[test]
    fn renders_code_block_with_background() {
        let result = render("```\nlet x = 1;\n```", Some(80));
        assert!(result.contains(&format!("{CODE_BG}")));
        assert!(result.contains("let x = 1;"));
    }

    #[test]
    fn blank_line_after_code_block() {
        let result = render("```\ncode\n```\n# Heading", Some(80));
        assert!(result.contains(&format!("{Reset}\n\n{DIM}#{Reset}{BOLD} Heading")));
    }

    #[test]
    fn renders_list_items() {
        let result = render("- one\n- two\n- three", Some(80));
        assert!(result.contains("- one"));
        assert!(result.contains("- two"));
        assert!(result.contains("- three"));
    }

    #[test]
    fn renders_ordered_list() {
        let result = render("1. first\n2. second\n3. third", Some(80));
        assert!(result.contains("1. first"));
        assert!(result.contains("2. second"));
        assert!(result.contains("3. third"));
    }

    #[test]
    fn renders_ordered_list_custom_start() {
        let result = render("5. fifth\n6. sixth", Some(80));
        assert!(result.contains("5. fifth"));
        assert!(result.contains("6. sixth"));
    }

    #[test]
    fn renders_blockquote_with_bar() {
        let result = render("> quoted text", Some(80));
        assert!(
            result
                .lines()
                .any(|l| l.contains("▎") && l.contains("quoted text"))
        );
    }

    #[test]
    fn renders_table() {
        let md =
            "| Name | Age | City |\n|------|----:|:----:|\n| Alice | 30 | NYC |\n| Bob | 25 | LA |";
        let result = render(md, Some(80));
        let lines: Vec<&str> = result.lines().collect();
        // Top border, header, separator, two body rows, bottom border
        assert_eq!(lines.len(), 6);
        assert!(lines[0].contains("╭") && lines[0].contains("╮"));
        assert!(lines[1].contains(BOLD_ON));
        assert!(lines[2].contains("├") && lines[2].contains("┼") && lines[2].contains("┤"));
        assert!(lines[3].contains(" 30"));
        assert!(lines[5].contains("╰") && lines[5].contains("╯"));
    }

    #[test]
    fn table_wraps_within_width() {
        let md = "| Column One | Column Two |\n|---|---|\n| This cell has quite a bit of text | And so does this one here |";
        let result = render(md, Some(40));
        for line in result.lines() {
            assert!(
                visible_width(line) <= 40,
                "line too wide ({}): {:?}",
                visible_width(line),
                line,
            );
        }
        // Content should wrap to multiple lines within the row
        let content_lines: Vec<&str> = result
            .lines()
            .filter(|l| l.contains("text") || l.contains("this"))
            .collect();
        assert!(
            content_lines.len() > 1,
            "expected wrapped cell content across multiple lines"
        );
    }

    #[test]
    fn table_stretches_beyond_max_width() {
        // Tables can use full terminal width, not just MAX_WIDTH
        let md = "| A | B |\n|---|---|\n| short | this cell has lots of words that would need wrapping at eighty columns but should fit fine at a wider terminal width |";
        let result = render(md, Some(140));
        // No line should need wrapping since 140 is wide enough
        let data_lines: Vec<&str> = result.lines().filter(|l| l.contains("wrapping")).collect();
        assert_eq!(data_lines.len(), 1, "cell content should fit on one line");
        // But prose in the same render is still capped at MAX_WIDTH
        let prose = "| A |\n|---|\n| x |\n\nThis is a long paragraph that should still wrap at MAX_WIDTH even though the terminal is wider than that value.";
        let result = render(prose, Some(140));
        assert!(
            result
                .lines()
                .filter(|l| !l.contains('│') && !l.contains('─') && visible_width(l) > 0)
                .all(|l| visible_width(l) <= MAX_WIDTH),
            "prose should wrap at MAX_WIDTH"
        );
    }

    #[test]
    fn inline_code_style_restored_across_wrap() {
        // When inline code wraps, each line must have matching bg and reset
        let text = "Start `code that is long enough to wrap across lines` end.";
        let result = render(text, Some(30));
        let code_bg = format!("{CODE_BG}");
        let reset = format!("{Reset}");
        let styled_lines: Vec<&str> = result
            .lines()
            .filter(|l| l.contains(&code_bg) || l.contains(&reset))
            .collect();
        assert!(!styled_lines.is_empty());
        for line in &styled_lines {
            assert!(
                line.contains(&code_bg) && line.contains(&reset),
                "each styled line needs both bg and reset: {line:?}",
            );
        }
    }

    #[test]
    fn styled_text_wraps_at_visible_width() {
        // ANSI escapes should not count toward line width
        let text = "Some **bold** and *italic* words in a sentence that should wrap correctly.";
        let result = render(text, Some(40));
        for line in result.lines() {
            assert!(
                visible_width(line) <= 40,
                "line too wide ({}): {:?}",
                visible_width(line),
                line,
            );
        }
    }

    #[test]
    fn renders_horizontal_rule() {
        let result = render("Above\n\n---\n\nBelow", Some(90));
        assert!(result.contains("Above"));
        assert!(result.contains("Below"));
        let rule_line = result.lines().find(|l| l.contains("▁")).unwrap();
        // Centered at 1/3 of MAX_WIDTH (prose cap)
        let w = MAX_WIDTH / 3;
        let pad = (MAX_WIDTH - w) / 2;
        assert_eq!(visible_width(rule_line), w + pad);
    }
}
