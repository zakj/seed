use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};

const CODE_BG: Color = Color::Rgb(48, 48, 48);

pub fn render(text: &str, width: usize) -> Text<'static> {
    let parser = Parser::new_ext(text, Options::ENABLE_TABLES);
    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut style_stack: Vec<Style> = vec![Style::default()];
    let mut code_lines: Vec<String> = Vec::new();
    let mut in_code_block = false;
    let mut in_list_item = false;
    let mut list_depth: usize = 0;
    let mut in_blockquote = false;

    let current_style = |stack: &[Style]| *stack.last().unwrap_or(&Style::default());

    let push_style = |stack: &mut Vec<Style>, modifier: Modifier| {
        let base = *stack.last().unwrap_or(&Style::default());
        stack.push(base.add_modifier(modifier));
    };

    for event in parser {
        match event {
            Event::Start(Tag::Heading { .. }) => {
                flush_line(&mut spans, &mut lines);
            }
            Event::End(TagEnd::Heading(_)) => {
                // Make the heading line bold
                let heading_spans: Vec<Span<'static>> = spans
                    .drain(..)
                    .map(|s| s.style(Style::new().add_modifier(Modifier::BOLD)))
                    .collect();
                lines.push(Line::from(heading_spans));
                lines.push(Line::default());
            }
            Event::Start(Tag::Paragraph) => {}
            Event::End(TagEnd::Paragraph) => {
                if in_list_item {
                    flush_list_item(&mut spans, &mut lines, list_depth);
                } else if in_blockquote {
                    flush_blockquote(&mut spans, &mut lines);
                } else {
                    flush_line(&mut spans, &mut lines);
                    lines.push(Line::default());
                }
            }
            Event::Start(Tag::CodeBlock(_)) => {
                flush_line(&mut spans, &mut lines);
                in_code_block = true;
            }
            Event::End(TagEnd::CodeBlock) => {
                in_code_block = false;
                let style = Style::new().bg(CODE_BG);
                for code_line in code_lines.drain(..) {
                    lines.push(Line::from(vec![
                        Span::styled("  ", style),
                        Span::styled(code_line, style),
                        Span::styled("  ", style),
                    ]));
                }
                lines.push(Line::default());
            }
            Event::Start(Tag::BlockQuote(_)) => {
                flush_line(&mut spans, &mut lines);
                in_blockquote = true;
            }
            Event::End(TagEnd::BlockQuote(_)) => {
                in_blockquote = false;
            }
            Event::Start(Tag::List(_)) => {
                flush_line(&mut spans, &mut lines);
                list_depth += 1;
            }
            Event::End(TagEnd::List(_)) => {
                list_depth = list_depth.saturating_sub(1);
                if list_depth == 0 {
                    lines.push(Line::default());
                }
            }
            Event::Start(Tag::Item) => in_list_item = true,
            Event::End(TagEnd::Item) => {
                flush_list_item(&mut spans, &mut lines, list_depth);
                in_list_item = false;
            }
            Event::Start(Tag::Emphasis) => push_style(&mut style_stack, Modifier::ITALIC),
            Event::End(TagEnd::Emphasis) => {
                style_stack.pop();
            }
            Event::Start(Tag::Strong) => push_style(&mut style_stack, Modifier::BOLD),
            Event::End(TagEnd::Strong) => {
                style_stack.pop();
            }
            Event::Start(Tag::Link { .. }) => {
                let base = current_style(&style_stack);
                style_stack.push(base.fg(Color::Blue));
            }
            Event::End(TagEnd::Link) => {
                style_stack.pop();
            }
            Event::Text(text) => {
                if in_code_block {
                    code_lines.extend(text.lines().map(String::from));
                } else {
                    spans.push(Span::styled(text.to_string(), current_style(&style_stack)));
                }
            }
            Event::Code(code) => {
                let style = Style::new().bg(CODE_BG);
                spans.push(Span::styled(format!("\u{00a0}{code}\u{00a0}"), style));
            }
            Event::Rule => {
                flush_line(&mut spans, &mut lines);
                // Collapse preceding blank line so the rule sits tight.
                if lines.last().is_some_and(|l| l.spans.is_empty()) {
                    lines.pop();
                }
                let rule_width = width / 3;
                let pad = (width.saturating_sub(rule_width)) / 2;
                lines.push(Line::from(Span::styled(
                    format!("{}{}", " ".repeat(pad), "▁".repeat(rule_width)),
                    Style::new().add_modifier(Modifier::DIM),
                )));
            }
            Event::SoftBreak => {
                spans.push(Span::raw(" "));
            }
            Event::HardBreak => {
                flush_line(&mut spans, &mut lines);
            }
            Event::Html(html) | Event::InlineHtml(html) => {
                spans.push(Span::styled(html.to_string(), current_style(&style_stack)));
            }
            _ => {}
        }
    }
    flush_line(&mut spans, &mut lines);

    // Trim trailing empty lines
    while lines.last().is_some_and(|l| l.spans.is_empty()) {
        lines.pop();
    }

    Text::from(lines)
}

fn flush_line(spans: &mut Vec<Span<'static>>, lines: &mut Vec<Line<'static>>) {
    if !spans.is_empty() {
        lines.push(Line::from(std::mem::take(spans)));
    }
}

fn flush_list_item(spans: &mut Vec<Span<'static>>, lines: &mut Vec<Line<'static>>, depth: usize) {
    if spans.is_empty() {
        return;
    }
    let indent = depth.saturating_sub(1) * 4;
    let prefix = format!("{}- ", " ".repeat(indent));
    let mut item_spans = vec![Span::raw(prefix)];
    item_spans.append(spans);
    lines.push(Line::from(item_spans));
}

fn flush_blockquote(spans: &mut Vec<Span<'static>>, lines: &mut Vec<Line<'static>>) {
    if spans.is_empty() {
        return;
    }
    let mut quote_spans = vec![Span::styled("▎ ", Style::new().add_modifier(Modifier::DIM))];
    quote_spans.append(spans);
    lines.push(Line::from(quote_spans));
}
