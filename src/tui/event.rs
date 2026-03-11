use std::time::Duration;

use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind, MouseEventKind};
use ratatui::layout::Position;
use tui_input::Input;
use tui_input::backend::crossterm::EventHandler;

use super::app::{self, App, EditState, Panel};
use super::keys::{self, Command};
use crate::error::Error;
use crate::ops::{self, Edits};
use crate::store::Store;
use crate::task::{Priority, Task, TaskId};

pub enum Action {
    Continue,
    Quit,
    EditDescription(TaskId),
}

pub fn handle_events(app: &mut App) -> std::io::Result<Action> {
    // Expire old status messages.
    if let Some((_, t)) = &app.status_message
        && t.elapsed() > Duration::from_secs(3)
    {
        app.status_message = None;
    }

    if !event::poll(Duration::from_millis(16))? {
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
    // Clear status on any keypress.
    app.status_message = None;

    if app.priority_mode {
        return handle_priority_key(app, code);
    }

    let tables: &[&[keys::Hint]] = match app.focused_panel {
        Panel::Tree => &[keys::GLOBAL, keys::TREE],
        Panel::Detail => &[keys::GLOBAL, keys::DETAIL],
    };
    let Some(cmd) = keys::resolve(tables, code) else {
        return Action::Continue;
    };
    execute(app, cmd)
}

fn execute(app: &mut App, cmd: Command) -> Action {
    match cmd {
        Command::Quit => return Action::Quit,
        Command::TogglePanel => {
            app.focused_panel = match app.focused_panel {
                Panel::Tree => Panel::Detail,
                Panel::Detail => Panel::Tree,
            };
        }

        // Tree navigation
        Command::NavigateDown => navigate(app, |ts| {
            ts.key_down();
        }),
        Command::NavigateUp => navigate(app, |ts| {
            ts.key_up();
        }),
        Command::Collapse => {
            app.tree_state.key_left();
        }
        Command::Expand => {
            app.tree_state.key_right();
        }
        Command::Toggle => {
            app.tree_state.toggle_selected();
        }
        Command::First => match app.focused_panel {
            Panel::Tree => {
                app.tree_state.select_first();
                app.detail_scroll = 0;
            }
            Panel::Detail => app.detail_scroll = 0,
        },
        Command::Last => match app.focused_panel {
            Panel::Tree => {
                app.tree_state.select_last();
                app.detail_scroll = 0;
            }
            Panel::Detail => app.detail_scroll = u16::MAX,
        },

        // Detail scrolling
        Command::ScrollDown => {
            app.detail_scroll = app.detail_scroll.saturating_add(1);
        }
        Command::ScrollUp => {
            app.detail_scroll = app.detail_scroll.saturating_sub(1);
        }

        // Editing
        Command::EditTitle => {
            if let Some(task) = app.selected_task() {
                app.edit_state = Some(EditState {
                    task_id: task.id,
                    input: Input::new(task.title.clone()),
                    error: None,
                });
            }
        }
        Command::EditDescription => {
            if let Some(task) = app.selected_task() {
                return Action::EditDescription(task.id);
            }
        }
        Command::AddTask => {
            create_and_edit_task(app, None);
        }
        Command::AddChildTask => {
            let parent = app.selected_task().map(|t| t.id);
            create_and_edit_task(app, parent);
        }

        // Status mutations
        Command::StartTask => {
            mutate_task(app, ops::start_task, "Task started", "Already in progress");
        }
        Command::CompleteTask => {
            mutate_task(
                app,
                |s, id| ops::complete_task(s, id, false),
                "Task completed",
                "Already done",
            );
        }
        Command::DropTask => {
            mutate_task(app, ops::drop_task, "Task dropped", "Already dropped");
        }

        Command::PriorityMode => {
            app.priority_mode = true;
        }

        _ => {}
    }
    Action::Continue
}

fn navigate(app: &mut App, f: impl FnOnce(&mut tui_tree_widget::TreeState<TaskId>)) {
    let prev = app.tree_state.selected().to_vec();
    f(&mut app.tree_state);
    if app.tree_state.selected() != prev {
        app.detail_scroll = 0;
    }
}

fn mutate_task(
    app: &mut App,
    op: impl FnOnce(&Store, TaskId) -> Result<(Task, bool), Error>,
    success: &str,
    noop: &str,
) {
    let Some(task) = app.selected_task() else {
        return;
    };
    let id = task.id;
    match op(&app.store, id) {
        Ok((_, true)) => {
            let _ = app.reload();
            app.set_status(success);
        }
        Ok((_, false)) => app.set_status(noop),
        Err(e) => app.set_status(e.to_string()),
    }
}

fn handle_priority_key(app: &mut App, code: KeyCode) -> Action {
    let Some(cmd) = keys::resolve(&[keys::PRIORITY], code) else {
        app.priority_mode = false;
        return Action::Continue;
    };

    app.priority_mode = false;

    let priority = match cmd {
        Command::SetCritical => Priority::Critical,
        Command::SetHigh => Priority::High,
        Command::SetNormal => Priority::Normal,
        Command::SetLow => Priority::Low,
        Command::Cancel => return Action::Continue,
        _ => return Action::Continue,
    };

    if let Some(task) = app.selected_task() {
        let id = task.id;
        let edits = Edits {
            priority: Some(priority),
            ..Edits::default()
        };
        match ops::apply_edits(&app.store, id, &edits) {
            Ok((_, true)) => {
                let _ = app.reload();
                app.set_status(format!("Priority set to {priority}"));
            }
            Ok((_, false)) => app.set_status("Priority unchanged"),
            Err(e) => app.set_status(e.to_string()),
        }
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
            if let Err(e) = app.reload() {
                edit.error = Some(e.to_string());
                app.edit_state = Some(edit);
                return Action::Continue;
            }
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

fn create_and_edit_task(app: &mut App, parent: Option<TaskId>) {
    let placeholder = "New task".to_string();
    if let Ok(task) = ops::create_task(
        &app.store,
        placeholder,
        None,
        std::iter::empty(),
        parent,
        &[],
        None,
    ) {
        let new_id = task.id;
        if app.reload().is_ok() {
            if let Some(parent_id) = parent {
                let path = app::identifier_path(parent_id, &app.parent_map);
                app.tree_state.open(path);
            }
            select_task(app, new_id);
            app.edit_state = Some(EditState {
                task_id: new_id,
                input: Input::new(String::new()),
                error: None,
            });
        }
    }
}

fn select_task(app: &mut App, id: TaskId) {
    let path = app::identifier_path(id, &app.parent_map);
    app.tree_state.select(path);
    app.detail_scroll = 0;
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
    let inner_height = app.tree_area.height.saturating_sub(2) as usize;
    visible <= inner_height
}

fn detail_dep_hit(app: &App, row: u16) -> Option<TaskId> {
    let inner_top = app.detail_area.y + 1;
    let content_line = (row.checked_sub(inner_top)?) as usize + app.detail_scroll as usize;
    app.detail_dep_lines
        .iter()
        .find(|(line, _)| *line == content_line)
        .map(|(_, id)| *id)
}

fn handle_mouse(app: &mut App, mouse: event::MouseEvent) {
    match mouse.kind {
        MouseEventKind::Down(_) => {
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
