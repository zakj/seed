use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::symbols::border;
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{
    Block, Borders, Clear, Padding, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState,
    Wrap,
};
use tui_tree_widget::Tree;
use unicode_width::UnicodeWidthStr;

use super::app::{self, App, Panel};
use super::markdown;
use crate::format::{format_date, format_datetime};
use crate::task::{Task, TaskId};

pub fn draw(frame: &mut Frame, app: &mut App) {
    let [left, right] =
        Layout::horizontal([Constraint::Percentage(40), Constraint::Percentage(60)])
            .areas(frame.area());

    // Reserve bottom row for footer.
    let [tree_area, footer_area] =
        Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).areas(left);
    let [detail_area, _] =
        Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).areas(right);

    // Store areas for mouse hit-testing in event handler.
    app.tree_area = tree_area;
    app.detail_area = detail_area;

    draw_tree(frame, app, tree_area);
    draw_detail(frame, app, detail_area);
    draw_footer(frame, app, footer_area);

    if app.edit_state.is_some() {
        draw_edit_popup(frame, app);
    }
}

fn focused_border_style(panel: Panel, focused: Panel) -> Style {
    if panel == focused {
        Style::new().fg(Color::White)
    } else {
        Style::new().fg(Color::DarkGray)
    }
}

fn draw_tree(frame: &mut Frame, app: &mut App, area: Rect) {
    let items = app::build_tree_items(&app.tasks, &app.done_ids, &app.children_map);
    let tree = Tree::new(&items)
        .expect("task IDs are unique")
        .block(
            Block::default()
                .title(" Tasks ")
                .borders(Borders::ALL)
                .border_set(border::ROUNDED)
                .border_style(focused_border_style(Panel::Tree, app.focused_panel)),
        )
        .highlight_style(
            Style::new()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        )
        .node_closed_symbol("▶ ")
        .node_open_symbol("▼ ")
        .node_no_children_symbol("  ");
    frame.render_stateful_widget(tree, area, &mut app.tree_state);
}

fn draw_detail(frame: &mut Frame, app: &mut App, area: Rect) {
    let selected = app.selected_task();

    let date_title = selected.map(|task| {
        Line::from(Span::styled(
            format!(" {} ", format_date(&task.modified)),
            focused_border_style(Panel::Detail, app.focused_panel),
        ))
        .right_aligned()
    });

    let mut block = Block::default()
        .borders(Borders::ALL)
        .border_set(border::ROUNDED)
        .border_style(focused_border_style(Panel::Detail, app.focused_panel))
        .padding(Padding::right(1));
    if let Some(title) = date_title {
        block = block.title(title);
    }
    let inner = block.inner(area);

    let content = match selected {
        Some(task) => {
            let (text, dep_lines) =
                build_detail_content(task, &app.tasks, &app.done_ids, inner.width as usize);
            app.detail_dep_lines = dep_lines;
            text
        }
        None => {
            app.detail_dep_lines.clear();
            Text::raw("No task selected")
        }
    };

    let content_height = wrapped_line_count(&content, inner.width) as u16;
    let viewport_height = inner.height;
    let max_scroll = content_height.saturating_sub(viewport_height);
    app.detail_scroll = app.detail_scroll.min(max_scroll);

    let paragraph = Paragraph::new(content)
        .block(block)
        .wrap(Wrap { trim: false })
        .scroll((app.detail_scroll, 0));
    frame.render_widget(paragraph, area);

    // Render scrollbar on top of the right border, between corners.
    if content_height > viewport_height {
        let scrollbar_area = Rect {
            x: area.x + area.width - 1,
            y: area.y + 1,
            width: 1,
            height: area.height.saturating_sub(2),
        };
        let mut scrollbar_state =
            ScrollbarState::new(max_scroll as usize).position(app.detail_scroll as usize);
        frame.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .begin_symbol(None)
                .end_symbol(None)
                .track_symbol(Some("│")),
            scrollbar_area,
            &mut scrollbar_state,
        );
    }
}

fn build_detail_content(
    task: &Task,
    all_tasks: &[Task],
    done_ids: &std::collections::HashSet<crate::task::TaskId>,
    width: usize,
) -> (Text<'static>, Vec<(usize, TaskId)>) {
    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut dep_lines: Vec<(usize, TaskId)> = Vec::new();
    let dim = Style::new().add_modifier(Modifier::DIM);

    // Labels
    if !task.labels.is_empty() {
        let label_str = task.labels.iter().cloned().collect::<Vec<_>>().join(", ");
        lines.push(Line::from(vec![
            Span::styled("Labels: ", Style::new().add_modifier(Modifier::BOLD)),
            Span::raw(label_str),
        ]));
    }

    // Dependencies
    let deps: Vec<&Task> = task
        .depends
        .iter()
        .filter_map(|id| all_tasks.iter().find(|t| &t.id == id))
        .collect();
    if !deps.is_empty() {
        lines.push(Line::from(Span::styled(
            "Depends on:",
            Style::new().add_modifier(Modifier::BOLD),
        )));
        for dep in &deps {
            let mut spans = vec![Span::raw("  ")];
            spans.extend(format_related_spans(dep, done_ids));
            dep_lines.push((lines.len(), dep.id));
            lines.push(Line::from(spans));
        }
    }

    // Description
    if let Some(desc) = &task.description {
        if !lines.is_empty() {
            lines.push(Line::default());
        }
        lines.extend(markdown::render(desc, width).lines);
    }

    // Log entries
    for entry in &task.log {
        let ts = format_datetime(&entry.timestamp);
        let agent_str = match &entry.agent {
            Some(a) => format!(" ({a}) "),
            None => " ".to_string(),
        };
        let label = format!("── {ts}{agent_str}");
        let fill = width.saturating_sub(label.width());
        lines.push(Line::default());
        lines.push(Line::from(Span::styled(
            format!("{label}{}", "─".repeat(fill)),
            dim,
        )));
        lines.extend(markdown::render(&entry.message, width).lines);
    }

    (Text::from(lines), dep_lines)
}

fn format_related_spans(
    task: &Task,
    done_ids: &std::collections::HashSet<crate::task::TaskId>,
) -> Vec<Span<'static>> {
    let blocked = task.is_blocked(done_ids);
    let indicator = task.indicator(blocked);
    let style = app::anstyle_to_ratatui(indicator.color);
    if indicator.symbol.trim().is_empty() {
        vec![Span::raw(format!("#{} {}", task.id, task.title))]
    } else {
        vec![
            Span::styled(format!("{} ", indicator.symbol), style),
            Span::raw(format!("#{} {}", task.id, task.title)),
        ]
    }
}

/// Approximate visual line count after wrapping. Uses character-width ceiling
/// division, which can undercount vs ratatui's word-boundary wrapping.
fn wrapped_line_count(text: &Text, width: u16) -> usize {
    let w = width as usize;
    if w == 0 {
        return text.lines.len();
    }
    text.lines
        .iter()
        .map(|line| {
            let line_width = line.width();
            if line_width == 0 {
                1
            } else {
                line_width.div_ceil(w)
            }
        })
        .sum()
}

fn draw_edit_popup(frame: &mut Frame, app: &App) {
    let edit = app.edit_state.as_ref().unwrap();
    let area = frame.area();
    if area.height < 5 || area.width < 10 {
        return;
    }

    let width = (area.width * 3 / 5).clamp(30, 80).min(area.width);
    let height = if edit.error.is_some() { 4 } else { 3 };
    let x = (area.width.saturating_sub(width)) / 2;
    let y = area.height / 3;

    let popup_area = Rect::new(x, y, width, height.min(area.height.saturating_sub(y)));
    frame.render_widget(Clear, popup_area);

    let block = Block::default()
        .title(" Edit title ")
        .borders(Borders::ALL)
        .border_set(border::ROUNDED)
        .border_style(Style::new().fg(Color::White))
        .padding(Padding::horizontal(1));
    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    let inner_width = inner.width as usize;
    let scroll = edit.input.visual_scroll(inner_width);
    let paragraph = Paragraph::new(edit.input.value()).scroll((0, scroll as u16));
    frame.render_widget(paragraph, Rect::new(inner.x, inner.y, inner.width, 1));

    if let Some(ref err) = edit.error {
        frame.render_widget(
            Paragraph::new(Span::styled(err.as_str(), Style::new().fg(Color::Red))),
            Rect::new(inner.x, inner.y + 1, inner.width, 1),
        );
    }

    let cursor_x = inner.x + (edit.input.visual_cursor().max(scroll) - scroll) as u16;
    frame.set_cursor_position(ratatui::layout::Position::new(cursor_x, inner.y));
}

fn draw_footer(frame: &mut Frame, app: &App, area: Rect) {
    if let Some(error) = &app.error {
        let line = Line::from(vec![
            Span::raw(" "),
            Span::styled(error.as_str(), Style::new().fg(Color::Red)),
        ]);
        frame.render_widget(Paragraph::new(line), area);
        return;
    }
    let keys: &[(&str, &str)] = match app.focused_panel {
        Panel::Tree => &[
            ("j/k", "navigate"),
            ("h/l", "collapse/expand"),
            ("Space", "toggle"),
            ("e", "edit"),
            ("E", "describe"),
            ("a/A", "add/add child"),
            ("g/G", "top/bottom"),
            ("Tab", "detail"),
            ("q", "quit"),
        ],
        Panel::Detail => &[
            ("j/k", "scroll"),
            ("g/G", "top/bottom"),
            ("Tab", "tasks"),
            ("q", "quit"),
        ],
    };
    let spans: Vec<Span> = keys
        .iter()
        .enumerate()
        .flat_map(|(i, (key, desc))| {
            let mut s = vec![
                Span::styled(
                    format!(" {key} "),
                    Style::new().fg(Color::Black).bg(Color::DarkGray),
                ),
                Span::styled(format!(" {desc} "), Style::new().fg(Color::DarkGray)),
            ];
            if i < keys.len() - 1 {
                s.push(Span::raw(" "));
            }
            s
        })
        .collect();
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}
