use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind, MouseEventKind};
use ratatui::layout::Position;
use tui_input::Input;
use tui_input::backend::crossterm::EventHandler;

use super::app::{self, App, EditState, Panel};
use crate::ops::{self, Edits};
use crate::task::TaskId;

pub enum Action {
    Continue,
    Quit,
}

pub fn handle_events(app: &mut App) -> std::io::Result<Action> {
    if !event::poll(std::time::Duration::from_millis(16))? {
        return Ok(Action::Continue);
    }
    let ev = event::read()?;
    if app.edit_state.is_some() {
        return Ok(handle_edit_event(app, &ev));
    }
    match ev {
        Event::Key(key) if key.kind == KeyEventKind::Press => Ok(handle_key(app, key.code)),
        Event::Mouse(mouse) => {
            handle_mouse(app, mouse);
            Ok(Action::Continue)
        }
        _ => Ok(Action::Continue),
    }
}

fn handle_key(app: &mut App, code: KeyCode) -> Action {
    match code {
        KeyCode::Char('q') => return Action::Quit,
        KeyCode::Tab => {
            app.focused_panel = match app.focused_panel {
                Panel::Tree => Panel::Detail,
                Panel::Detail => Panel::Tree,
            };
        }
        _ => match app.focused_panel {
            Panel::Tree => return handle_tree_key(app, code),
            Panel::Detail => handle_detail_key(app, code),
        },
    }
    Action::Continue
}

fn handle_edit_event(app: &mut App, ev: &Event) -> Action {
    let Some(mut edit) = app.edit_state.take() else {
        return Action::Continue;
    };
    let Event::Key(key) = ev else {
        app.edit_state = Some(edit);
        return Action::Continue;
    };
    if key.kind != KeyEventKind::Press {
        app.edit_state = Some(edit);
        return Action::Continue;
    }
    match key.code {
        KeyCode::Enter => {
            let title = edit.input.value().trim().to_string();
            if title.is_empty() {
                edit.error = Some("Title cannot be empty".into());
                app.edit_state = Some(edit);
                return Action::Continue;
            }
            let edits = Edits {
                title: Some(title),
                ..Edits::default()
            };
            if let Err(e) = ops::apply_edits(&app.store, edit.task_id, &edits) {
                edit.error = Some(e.to_string());
                app.edit_state = Some(edit);
                return Action::Continue;
            }
            let _ = app.reload();
        }
        KeyCode::Esc => {}
        _ => {
            edit.error = None;
            edit.input.handle_event(ev);
            app.edit_state = Some(edit);
        }
    }
    Action::Continue
}

fn handle_tree_key(app: &mut App, code: KeyCode) -> Action {
    let prev = app.tree_state.selected().to_vec();
    match code {
        KeyCode::Char('j') | KeyCode::Down => {
            app.tree_state.key_down();
        }
        KeyCode::Char('k') | KeyCode::Up => {
            app.tree_state.key_up();
        }
        KeyCode::Char('h') | KeyCode::Left => {
            app.tree_state.key_left();
        }
        KeyCode::Char('l') | KeyCode::Right => {
            app.tree_state.key_right();
        }
        KeyCode::Char(' ') => {
            app.tree_state.toggle_selected();
        }
        KeyCode::Char('g') | KeyCode::Home => {
            app.tree_state.select_first();
        }
        KeyCode::Char('G') | KeyCode::End => {
            app.tree_state.select_last();
        }
        KeyCode::Char('e') => {
            if let Some(task) = app.selected_task() {
                app.edit_state = Some(EditState {
                    task_id: task.id,
                    input: Input::new(task.title.clone()),
                    error: None,
                });
            }
        }
        _ => {}
    }
    if app.tree_state.selected() != prev {
        app.detail_scroll = 0;
    }
    Action::Continue
}

fn handle_detail_key(app: &mut App, code: KeyCode) {
    match code {
        KeyCode::Char('j') | KeyCode::Down => {
            app.detail_scroll = app.detail_scroll.saturating_add(1);
        }
        KeyCode::Char('k') | KeyCode::Up => {
            app.detail_scroll = app.detail_scroll.saturating_sub(1);
        }
        KeyCode::Char('g') | KeyCode::Home => {
            app.detail_scroll = 0;
        }
        KeyCode::Char('G') | KeyCode::End => {
            app.detail_scroll = u16::MAX; // clamped in draw
        }
        _ => {}
    }
}

fn hit_panel(app: &App, col: u16, row: u16) -> Option<Panel> {
    let pos = Position::new(col, row);
    if app.tree_area.contains(pos) {
        Some(Panel::Tree)
    } else if app.detail_area.contains(pos) {
        Some(Panel::Detail)
    } else {
        None
    }
}

fn tree_content_fits(app: &App) -> bool {
    let visible = app::visible_item_count(&app.children_map, &app.tree_state, &app.tasks);
    // Inner height = area height minus 2 for borders
    let inner_height = app.tree_area.height.saturating_sub(2) as usize;
    visible <= inner_height
}

/// Check if a click in the detail pane hits a dep line, returning the TaskId.
fn detail_dep_hit(app: &App, row: u16) -> Option<TaskId> {
    // inner area starts 1 row below detail_area top (border)
    let inner_top = app.detail_area.y + 1;
    let content_line = (row.checked_sub(inner_top)?) as usize + app.detail_scroll as usize;
    app.detail_dep_lines
        .iter()
        .find(|(line, _)| *line == content_line)
        .map(|(_, id)| *id)
}

fn select_task(app: &mut App, id: TaskId) {
    let path = app::identifier_path(id, &app.parent_map);
    app.tree_state.select(path);
    app.detail_scroll = 0;
}

fn handle_mouse(app: &mut App, mouse: event::MouseEvent) {
    match mouse.kind {
        MouseEventKind::Down(_) => {
            // Check for dep link click in detail pane first.
            if let Some(Panel::Detail) = hit_panel(app, mouse.column, mouse.row)
                && let Some(dep_id) = detail_dep_hit(app, mouse.row)
            {
                select_task(app, dep_id);
                return;
            }
            let prev_selected = app.tree_state.selected().to_vec();
            app.tree_state
                .click_at(Position::new(mouse.column, mouse.row));
            if app.tree_state.selected() != prev_selected {
                app.detail_scroll = 0;
            }
        }
        MouseEventKind::ScrollDown => match hit_panel(app, mouse.column, mouse.row) {
            Some(Panel::Tree) => {
                if !tree_content_fits(app) {
                    app.tree_state.scroll_down(1);
                }
            }
            Some(Panel::Detail) => {
                app.detail_scroll = app.detail_scroll.saturating_add(1);
            }
            None => {}
        },
        MouseEventKind::ScrollUp => match hit_panel(app, mouse.column, mouse.row) {
            Some(Panel::Tree) => {
                if !tree_content_fits(app) {
                    app.tree_state.scroll_up(1);
                }
            }
            Some(Panel::Detail) => {
                app.detail_scroll = app.detail_scroll.saturating_sub(1);
            }
            None => {}
        },
        _ => {}
    }
}
