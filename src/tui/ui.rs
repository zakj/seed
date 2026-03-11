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
use super::keys;
use super::markdown;
use crate::format::{format_date, format_datetime};
use crate::task::{Priority, Task, TaskId};

pub const PRIORITIES: [Priority; 4] = [
    Priority::Critical,
    Priority::High,
    Priority::Normal,
    Priority::Low,
];
pub const DEFAULT_PRIORITY_INDEX: usize = 2; // Normal

pub fn draw(frame: &mut Frame, app: &mut App) {
    let [main_area, footer_area] =
        Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).areas(frame.area());

    let [tree_area, detail_area] =
        Layout::horizontal([Constraint::Percentage(40), Constraint::Percentage(60)])
            .areas(main_area);

    // Store areas for mouse hit-testing in event handler.
    app.tree_area = tree_area;
    app.detail_area = detail_area;

    draw_tree(frame, app, tree_area);
    if app.selected_task().is_none() {
        app.tree_state.select_first();
    }
    draw_detail(frame, app, detail_area);
    draw_footer(frame, app, footer_area);

    if app.edit_state.is_some() {
        draw_edit_popup(frame, app);
    }
    if app.priority_selection.is_some() {
        draw_priority_popup(frame, app);
    }
    if app.help_scroll.is_some() {
        draw_help_overlay(frame, app);
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
    let inner_width = area.width.saturating_sub(2);
    let items = app::build_tree_items(&app.tasks, &app.done_ids, &app.children_map, inner_width);
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
                .bg(Color::Indexed(235))
                .add_modifier(Modifier::BOLD),
        )
        .node_closed_symbol("▸ ")
        .node_open_symbol("▾ ")
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

    render_border_scrollbar(frame, area, max_scroll, app.detail_scroll);
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
    vec![
        Span::styled(format!("{} ", indicator.symbol), style),
        Span::raw(format!("#{} {}", task.id, task.title)),
    ]
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

/// Render a scrollbar on the right border of `area`, between the corners.
fn render_border_scrollbar(frame: &mut Frame, area: Rect, max_scroll: u16, position: u16) {
    if max_scroll == 0 {
        return;
    }
    let scrollbar_area = Rect {
        x: area.x + area.width - 1,
        y: area.y + 1,
        width: 1,
        height: area.height.saturating_sub(2),
    };
    let mut state = ScrollbarState::new(max_scroll as usize).position(position as usize);
    frame.render_stateful_widget(
        Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(None)
            .end_symbol(None)
            .track_symbol(Some("│")),
        scrollbar_area,
        &mut state,
    );
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

fn draw_priority_popup(frame: &mut Frame, app: &App) {
    let selected = app.priority_selection.unwrap();
    let area = frame.area();
    if area.height < 8 || area.width < 22 {
        return;
    }

    let width = 22u16;
    let height = (PRIORITIES.len() as u16) + 2; // border top + rows + border bottom
    let x = (area.width.saturating_sub(width)) / 2;
    let y = area.height / 3;

    let popup_area = Rect::new(x, y, width, height.min(area.height.saturating_sub(y)));
    frame.render_widget(Clear, popup_area);

    let block = Block::default()
        .title(" Priority ")
        .borders(Borders::ALL)
        .border_set(border::ROUNDED)
        .border_style(Style::new().fg(Color::White));
    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    for (i, (&priority, hint)) in PRIORITIES.iter().zip(keys::PRIORITY.iter()).enumerate() {
        let style = priority.style();
        let color_style = app::anstyle_to_ratatui(style.color);
        let mut row_style = color_style;
        if i == selected {
            row_style = row_style.bg(Color::Indexed(235));
        }

        let line = Line::from(vec![
            Span::styled(format!("  {}  ", hint.label), row_style),
            Span::styled(format!("{} ", style.symbol), row_style),
            Span::styled(format!("{:<10}", style.label), row_style),
        ]);
        frame.render_widget(
            Paragraph::new(line),
            Rect::new(inner.x, inner.y + i as u16, inner.width, 1),
        );
    }
}

fn draw_footer(frame: &mut Frame, app: &App, area: Rect) {
    if let Some((msg, _)) = &app.status_message {
        let line = Line::from(vec![
            Span::raw(" "),
            Span::styled(msg.as_str(), Style::new().fg(Color::Yellow)),
        ]);
        frame.render_widget(Paragraph::new(line), area);
        return;
    }

    let tables: &[&[keys::Hint]] = match app.focused_panel {
        Panel::Tree => &[keys::TREE, keys::GLOBAL],
        Panel::Detail => &[keys::DETAIL, keys::GLOBAL],
    };
    render_hints(frame, area, tables);
}

fn hint_spans(hint: &keys::Hint) -> Vec<Span<'static>> {
    vec![
        Span::styled(
            format!(" {} ", hint.label),
            Style::new().fg(Color::Black).bg(Color::DarkGray),
        ),
        Span::styled(
            format!(" {} ", hint.description),
            Style::new().fg(Color::DarkGray),
        ),
    ]
}

fn render_hints(frame: &mut Frame, area: Rect, tables: &[&[keys::Hint]]) {
    let visible: Vec<&keys::Hint> = tables
        .iter()
        .flat_map(|t| t.iter())
        .filter(|h| h.footer != keys::Footer::Hidden)
        .collect();

    let (right, left): (Vec<_>, Vec<_>) = visible
        .into_iter()
        .partition(|h| h.footer == keys::Footer::Right);

    let mut left_spans: Vec<Span> = left
        .iter()
        .enumerate()
        .flat_map(|(i, hint)| {
            let mut s = hint_spans(hint);
            if i < left.len() - 1 {
                s.push(Span::raw(" "));
            }
            s
        })
        .collect();

    if !right.is_empty() {
        let right_spans: Vec<Span> = right.iter().flat_map(|h| hint_spans(h)).collect();
        let left_width: usize = left_spans.iter().map(|s| s.width()).sum();
        let right_width: usize = right_spans.iter().map(|s| s.width()).sum();
        let gap = (area.width as usize).saturating_sub(left_width + right_width);
        left_spans.push(Span::raw(" ".repeat(gap)));
        left_spans.extend(right_spans);
    }

    frame.render_widget(Paragraph::new(Line::from(left_spans)), area);
}

fn draw_help_overlay(frame: &mut Frame, app: &mut App) {
    let area = frame.area();
    let scroll = app.help_scroll.as_mut().unwrap();

    fn is_navigation(cmd: &keys::Command) -> bool {
        matches!(
            cmd,
            keys::Command::NavigateDown
                | keys::Command::NavigateUp
                | keys::Command::Collapse
                | keys::Command::Expand
                | keys::Command::Toggle
                | keys::Command::First
                | keys::Command::Last
        )
    }

    type HelpSection = (
        &'static str,
        &'static [keys::Hint],
        fn(&keys::Command) -> bool,
    );
    let sections: &[HelpSection] = &[
        ("Navigation", keys::TREE, |c| is_navigation(c)),
        ("Actions", keys::TREE, |c| !is_navigation(c)),
        ("General", keys::GLOBAL, |_| true),
        ("Detail pane", keys::DETAIL, |_| true),
    ];

    let bold = Style::new().add_modifier(Modifier::BOLD);
    let dim = Style::new().add_modifier(Modifier::DIM);
    let mut lines: Vec<Line> = Vec::new();
    let mut max_line_width: usize = 0;

    for (i, &(header, table, filter)) in sections.iter().enumerate() {
        if i > 0 {
            lines.push(Line::default());
        }
        lines.push(Line::from(Span::styled(header, bold)));
        max_line_width = max_line_width.max(header.len());

        for hint in table.iter().filter(|h| !h.label.is_empty()) {
            let Some((_, cmd)) = hint.keys.first() else {
                continue;
            };
            if !filter(cmd) {
                continue;
            }

            let row_width = 2 + 8 + hint.description.len();
            max_line_width = max_line_width.max(row_width);
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled(format!("{:<8}", hint.label), dim),
                Span::raw(hint.description),
            ]));
        }
    }

    let content_height = lines.len() as u16;
    let width = ((max_line_width as u16) + 4).min(area.width);
    let height = (content_height + 2).min(area.height);

    if area.height < 5 || area.width < width {
        return;
    }

    let x = (area.width.saturating_sub(width)) / 2;
    let y = (area.height.saturating_sub(height)) / 2;

    let popup_area = Rect::new(x, y, width, height);
    frame.render_widget(Clear, popup_area);

    let block = Block::default()
        .title(" Help ")
        .borders(Borders::ALL)
        .border_set(border::ROUNDED)
        .border_style(Style::new().fg(Color::White))
        .padding(Padding::horizontal(1));

    let viewport_height = height.saturating_sub(2);
    let max_scroll = content_height.saturating_sub(viewport_height);
    *scroll = (*scroll).min(max_scroll);

    let paragraph = Paragraph::new(Text::from(lines))
        .block(block)
        .scroll((*scroll, 0));
    frame.render_widget(paragraph, popup_area);

    render_border_scrollbar(frame, popup_area, max_scroll, *scroll);
}
