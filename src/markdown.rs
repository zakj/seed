use anstyle::{Ansi256Color, AnsiColor, Color, Reset, Style};
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};

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
    let width = terminal_width.unwrap_or(MAX_WIDTH).min(MAX_WIDTH);
    let parser = Parser::new_ext(text, Options::empty());
    let mut out = String::new();
    let mut buf = String::new();
    let mut code_lines: Vec<String> = Vec::new();
    let mut in_code_block = false;
    let mut in_link = false;
    let mut in_list_item = false;
    let mut list_depth: usize = 0;
    let mut in_blockquote = false;

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
}
