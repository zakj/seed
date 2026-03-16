use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::symbols::border;
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{
    Block, Borders, Clear, Padding, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState,
};
use tui_tree_widget::Tree;
use unicode_width::UnicodeWidthStr;

use super::app::{self, App, Mode, Panel};
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

    match &app.mode {
        Mode::Edit(_) => draw_edit_popup(frame, app),
        Mode::Priority(_) => draw_priority_popup(frame, app),
        _ => {}
    }
    if app.help.is_some() {
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

fn build_overlay(app: &App) -> Option<app::TreeOverlay<'_>> {
    match &app.mode {
        Mode::Move(ms) => Some(app::TreeOverlay::Move(ms)),
        Mode::Dep(ds) => Some(app::TreeOverlay::Dep(ds)),
        _ => None,
    }
}

fn draw_tree(frame: &mut Frame, app: &mut App, area: Rect) {
    let inner_width = area.width.saturating_sub(2);
    let overlay = build_overlay(app);
    let search_matches = app.search_match_ids();
    let ctx = app::TreeContext {
        done_ids: &app.done_ids,
        children_map: &app.children_map,
        inner_width,
        overlay: overlay.as_ref(),
        search_matches: &search_matches,
    };
    let items = app::build_tree_items(&app.tasks, &ctx);

    let title: std::borrow::Cow<str> = match &app.mode {
        Mode::Move(ms) => format!(" Move #{} ", ms.task_id).into(),
        Mode::Dep(ds) => format!(" Deps #{} ", ds.task_id).into(),
        _ => " Tasks ".into(),
    };

    let border_style = focused_border_style(Panel::Tree, app.panel());
    let mut block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_set(border::ROUNDED)
        .border_style(border_style);

    if let Some(ref search) = app.search {
        // Available width for query text inside the bottom border.
        let prefix_width = 3; // " / "
        let query_budget = (area.width as usize).saturating_sub(prefix_width + 4); // borders + pad

        let display_query = match search.input {
            Some(ref input) => {
                let scroll = input.visual_scroll(query_budget);
                let value = input.value();
                &value[value
                    .char_indices()
                    .nth(scroll)
                    .map(|(i, _)| i)
                    .unwrap_or(value.len())..]
            }
            None => search.query.as_str(),
        };

        let mut spans = vec![
            Span::styled(" / ", Style::new().fg(Color::Yellow)),
            Span::raw(display_query.to_string()),
            Span::raw("  "),
        ];
        if !search.query.is_empty() {
            let match_count = search.matches.len();
            let count_str = format!(
                "[{match_count} match{}] ",
                if match_count == 1 { "" } else { "es" }
            );
            spans.push(Span::styled(count_str, Style::new().fg(Color::Yellow)));
        }
        block = block.title_bottom(Line::from(spans));
    }

    let tree = Tree::new(&items)
        .expect("task IDs are unique")
        .block(block)
        .highlight_style(
            Style::new()
                .bg(Color::Indexed(235))
                .add_modifier(Modifier::BOLD),
        )
        .node_closed_symbol("▸ ")
        .node_open_symbol("▾ ")
        .node_no_children_symbol("  ");
    frame.render_stateful_widget(tree, area, &mut app.tree_state);

    // Position cursor in the bottom title when search input is active.
    if let Some(ref search) = app.search
        && let Some(ref input) = search.input
    {
        let query_budget = (area.width as usize).saturating_sub(3 + 4);
        let scroll = input.visual_scroll(query_budget);
        let cursor_offset = input.visual_cursor().saturating_sub(scroll) as u16;
        let cursor_x = area.x + 4 + cursor_offset;
        frame.set_cursor_position(ratatui::layout::Position::new(
            cursor_x.min(area.x + area.width - 2),
            area.y + area.height - 1,
        ));
    }
}

fn draw_detail(frame: &mut Frame, app: &mut App, area: Rect) {
    let selected = app.selected_task();

    let date_title = selected.map(|task| {
        Line::from(Span::styled(
            format!(" {} ", format_date(&task.modified)),
            focused_border_style(Panel::Detail, app.panel()),
        ))
        .right_aligned()
    });

    let mut block = Block::default()
        .borders(Borders::ALL)
        .border_set(border::ROUNDED)
        .border_style(focused_border_style(Panel::Detail, app.panel()))
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

    let content_height = content.lines.len() as u16;
    let content_width = content.width() as u16;
    let viewport_height = inner.height;
    let max_scroll = content_height.saturating_sub(viewport_height);
    let max_hscroll = content_width.saturating_sub(inner.width);
    app.detail_scroll = app.detail_scroll.min(max_scroll);
    app.detail_hscroll = app.detail_hscroll.min(max_hscroll);

    let paragraph = Paragraph::new(content)
        .block(block)
        .scroll((app.detail_scroll, app.detail_hscroll));
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
    let Mode::Edit(ref edit) = app.mode else {
        unreachable!()
    };
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
    let Mode::Priority(selected) = app.mode else {
        unreachable!()
    };
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

    render_hints(frame, area, app.mode.key_tables());
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

fn build_help_column(
    sections: &[(&'static str, &[keys::Hint])],
    bold: Style,
    dim: Style,
) -> (Vec<Line<'static>>, usize) {
    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut max_width: usize = 0;

    for (i, &(header, table)) in sections.iter().enumerate() {
        if i > 0 {
            lines.push(Line::default());
        }
        lines.push(Line::from(Span::styled(header, bold)));
        max_width = max_width.max(header.len());

        for hint in table.iter().filter(|h| !h.label.is_empty()) {
            let row_width = 2 + 8 + hint.description.len();
            max_width = max_width.max(row_width);
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled(format!("{:<8}", hint.label), dim),
                Span::raw(hint.description),
            ]));
        }
    }

    (lines, max_width)
}

fn draw_help_overlay(frame: &mut Frame, app: &mut App) {
    let area = frame.area();
    let Some(ref mut help) = app.help else {
        unreachable!()
    };
    let scroll = &mut help.scroll;

    let left_sections: &[(&str, &[keys::Hint])] = &[
        ("Navigation", keys::NAV),
        ("Actions", keys::TREE),
        ("General", keys::GLOBAL),
    ];
    let right_sections: &[(&str, &[keys::Hint])] = &[
        ("Detail pane", keys::DETAIL),
        ("Search", keys::SEARCH),
        ("Move mode", keys::MOVE),
        ("Dep mode", keys::DEP),
    ];

    let bold = Style::new().add_modifier(Modifier::BOLD);
    let dim = Style::new().add_modifier(Modifier::DIM);

    let (left_lines, left_width) = build_help_column(left_sections, bold, dim);
    let (right_lines, right_width) = build_help_column(right_sections, bold, dim);

    let gap = 5; // 2 spaces + dim bar + 2 spaces
    let content_width = left_width + gap + right_width;
    let content_height = left_lines.len().max(right_lines.len()) as u16;
    let width = ((content_width as u16) + 4).min(area.width);
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

    // Merge columns into single lines with padding.
    let row_count = left_lines.len().max(right_lines.len());
    let mut left_iter = left_lines.into_iter();
    let mut right_iter = right_lines.into_iter();
    let mut merged: Vec<Line> = Vec::with_capacity(row_count);
    for _ in 0..row_count {
        let left = left_iter.next();
        let right = right_iter.next();

        let left_rendered_width: usize = left
            .as_ref()
            .map(|l| l.spans.iter().map(|s| s.content.len()).sum())
            .unwrap_or(0);
        let pad = left_width.saturating_sub(left_rendered_width) + 2;

        let mut spans: Vec<Span> = Vec::new();
        if let Some(l) = left {
            spans.extend(l.spans);
        }
        spans.push(Span::raw(" ".repeat(pad)));
        spans.push(Span::styled("│", dim));
        spans.push(Span::raw("  "));
        if let Some(r) = right {
            spans.extend(r.spans);
        }
        merged.push(Line::from(spans));
    }

    let viewport_height = height.saturating_sub(2);
    let max_scroll = content_height.saturating_sub(viewport_height);
    *scroll = (*scroll).min(max_scroll);

    let paragraph = Paragraph::new(Text::from(merged))
        .block(block)
        .scroll((*scroll, 0));
    frame.render_widget(paragraph, popup_area);

    render_border_scrollbar(frame, popup_area, max_scroll, *scroll);
}
