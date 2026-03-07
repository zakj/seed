use anstyle::{Ansi256Color, AnsiColor, Color, Reset, Style};
use pulldown_cmark::{Alignment, Event, Options, Parser, Tag, TagEnd};

use crate::term::{visible_width, wrap_words};

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
    let parser = Parser::new_ext(text, Options::ENABLE_TABLES);
    let mut out = String::new();
    let mut buf = String::new();
    let mut code_lines: Vec<String> = Vec::new();
    let mut in_code_block = false;
    let mut in_link = false;
    let mut in_list_item = false;
    let mut list_depth: usize = 0;
    let mut in_blockquote = false;
    let mut table_alignments: Vec<Alignment> = Vec::new();
    let mut table_rows: Vec<Vec<String>> = Vec::new();
    let mut table_header_rows: usize = 0;

    for event in parser {
        match event {
            Event::Start(Tag::Heading { .. }) => {
                flush_paragraph(&mut buf, &mut out, width, 0);
            }
            Event::End(TagEnd::Heading(_)) => {
                let line = std::mem::take(&mut buf);
                out.push_str(&format!("{BOLD}{}{Reset}\n\n", line.trim()));
            }
            Event::Start(Tag::Paragraph) => {}
            Event::End(TagEnd::Paragraph) => {
                if in_list_item {
                    flush_list_item(&mut buf, &mut out, width, list_depth);
                } else if in_blockquote {
                    flush_blockquote(&mut buf, &mut out, width);
                    out.push('\n');
                } else {
                    flush_paragraph(&mut buf, &mut out, width, 0);
                    out.push('\n');
                }
            }
            Event::Start(Tag::CodeBlock(_)) => {
                flush_paragraph(&mut buf, &mut out, width, 0);
                in_code_block = true;
            }
            Event::End(TagEnd::CodeBlock) => {
                in_code_block = false;
                let max_w = code_lines
                    .iter()
                    .map(|l| visible_width(l))
                    .max()
                    .unwrap_or(0);
                for line in code_lines.drain(..) {
                    let padding = max_w - visible_width(&line);
                    out.push_str(&format!(
                        "{CODE_BG}  {line}{}  {Reset}\n",
                        " ".repeat(padding)
                    ));
                }
                out.push('\n');
            }
            Event::Start(Tag::BlockQuote(_)) => {
                flush_paragraph(&mut buf, &mut out, width, 0);
                in_blockquote = true;
            }
            Event::End(TagEnd::BlockQuote(_)) => {
                in_blockquote = false;
            }
            Event::Start(Tag::List(_)) => {
                flush_paragraph(&mut buf, &mut out, width, 0);
                list_depth += 1;
            }
            Event::End(TagEnd::List(_)) => {
                list_depth = list_depth.saturating_sub(1);
                if list_depth == 0 {
                    out.push('\n');
                }
            }
            Event::Start(Tag::Item) => in_list_item = true,
            Event::End(TagEnd::Item) => {
                // Tight lists don't wrap items in paragraphs
                flush_list_item(&mut buf, &mut out, width, list_depth);
                in_list_item = false;
            }
            Event::Start(Tag::Table(alignments)) => {
                flush_paragraph(&mut buf, &mut out, width, 0);
                table_alignments = alignments;
            }
            Event::End(TagEnd::Table) => {
                flush_table(
                    &table_rows,
                    &table_alignments,
                    table_header_rows,
                    term_width,
                    &mut out,
                );
                table_rows.clear();
                table_alignments.clear();
                table_header_rows = 0;
            }
            Event::Start(Tag::TableHead) => {
                table_rows.push(Vec::new());
            }
            Event::End(TagEnd::TableHead) => {
                table_header_rows = table_rows.len();
            }
            Event::Start(Tag::TableRow) => {
                table_rows.push(Vec::new());
            }
            Event::End(TagEnd::TableRow) => {}
            Event::Start(Tag::TableCell) => {
                buf.clear();
            }
            Event::End(TagEnd::TableCell) => {
                if let Some(row) = table_rows.last_mut() {
                    row.push(std::mem::take(&mut buf));
                }
            }
            Event::Start(Tag::Emphasis) => buf.push_str(ITALIC_ON),
            Event::End(TagEnd::Emphasis) => buf.push_str(ITALIC_OFF),
            Event::Start(Tag::Strong) => buf.push_str(BOLD_ON),
            Event::End(TagEnd::Strong) => buf.push_str(BOLD_OFF),
            Event::Start(Tag::Link { dest_url, .. }) => {
                in_link = true;
                buf.push_str(&format!(
                    "{LINK_START}{dest_url}{LINK_END}{}",
                    LINK_COLOR.render()
                ));
            }
            Event::End(TagEnd::Link) => {
                in_link = false;
                buf.push_str(&format!(
                    "{}{LINK_START}{LINK_END}",
                    LINK_COLOR.render_reset()
                ));
            }
            Event::Text(text) => {
                if in_code_block {
                    code_lines.extend(text.lines().map(String::from));
                } else if in_link {
                    // Non-breaking spaces keep link text as one word for wrapping
                    buf.push_str(&text.replace(' ', "\u{00a0}"));
                } else {
                    buf.push_str(&text);
                }
            }
            Event::Code(code) => {
                buf.push_str(&format!("{CODE_BG} {code} {Reset}"));
            }
            Event::Rule => {
                flush_paragraph(&mut buf, &mut out, width, 0);
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
            Event::SoftBreak | Event::HardBreak => buf.push(' '),
            Event::Html(html) | Event::InlineHtml(html) => buf.push_str(&html),
            _ => {}
        }
    }
    flush_paragraph(&mut buf, &mut out, width, 0);

    let trimmed = out.trim_end();
    if trimmed.is_empty() {
        String::new()
    } else {
        format!("{trimmed}\n")
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

    // Natural column widths from content, each capped at MAX_WIDTH for readability
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
    // Freeze columns that fit within a fair share; only shrink wider ones.
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
                // All unfrozen columns exceed fair share; distribute remaining evenly
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
    out.push('\n');
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

fn flush_paragraph(buf: &mut String, out: &mut String, width: usize, indent: usize) {
    let indent_str = " ".repeat(indent);
    flush_wrap(buf, out, width, &indent_str, &indent_str);
}

fn flush_blockquote(buf: &mut String, out: &mut String, width: usize) {
    let bar = format!("{DIM}▎{Reset} ");
    flush_wrap(buf, out, width, &bar, &bar);
}

fn flush_list_item(buf: &mut String, out: &mut String, width: usize, depth: usize) {
    let indent = depth.saturating_sub(1) * 4;
    let prefix = format!("{}- ", " ".repeat(indent));
    let subsequent = format!("{}  ", " ".repeat(indent));
    flush_wrap(buf, out, width, &prefix, &subsequent);
}

fn flush_wrap(
    buf: &mut String,
    out: &mut String,
    width: usize,
    initial_indent: &str,
    subsequent_indent: &str,
) {
    if buf.is_empty() {
        return;
    }
    let text = std::mem::take(buf);
    wrap_words(&text, out, width, initial_indent, subsequent_indent);
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
        assert!(result.contains("\x1b[1mTitle\x1b[0m"));
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
        assert!(result.contains(&format!("{Reset}\n\n{BOLD}Heading")));
    }

    #[test]
    fn renders_list_items() {
        let result = render("- one\n- two\n- three", Some(80));
        assert!(result.contains("- one"));
        assert!(result.contains("- two"));
        assert!(result.contains("- three"));
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
