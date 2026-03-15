use std::time::Duration;

use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind, MouseEventKind};
use ratatui::layout::Position;
use tui_input::Input;
use tui_input::backend::crossterm::EventHandler;

use std::collections::HashSet;

use super::app::{self, App, DepState, EditState, MoveState, Panel};
use super::keys::{self, Command};
use crate::error::Error;
use crate::ops::{self, Edits};
use crate::store::Store;
use crate::task::{self, Priority, Task, TaskId};

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

    // Drain all pending events to prevent scroll wheel events from starving
    // keyboard input.
    loop {
        let ev = event::read()?;
        if app.edit_state.is_some() {
            return Ok(handle_edit_event(app, &ev));
        }
        let action = match ev {
            Event::Key(key) if key.kind == KeyEventKind::Press => handle_key(app, key.code),
            Event::Mouse(mouse) => {
                handle_mouse(app, mouse);
                Action::Continue
            }
            _ => Action::Continue,
        };
        if !matches!(action, Action::Continue) {
            return Ok(action);
        }
        if !event::poll(Duration::ZERO)? {
            return Ok(Action::Continue);
        }
    }
}

fn handle_key(app: &mut App, code: KeyCode) -> Action {
    // Clear status on any keypress.
    app.status_message = None;

    if app.help_scroll.is_some() {
        return handle_help_key(app, code);
    }

    if app.priority_selection.is_some() {
        return handle_priority_key(app, code);
    }

    if app.move_state.is_some() {
        return handle_move_key(app, code);
    }

    if app.dep_state.is_some() {
        return handle_dep_key(app, code);
    }

    let tables: &[&[keys::Hint]] = match app.focused_panel {
        Panel::Tree => &[keys::GLOBAL, keys::NAV, keys::TREE],
        Panel::Detail => &[keys::GLOBAL, keys::DETAIL],
    };
    let Some(cmd) = keys::resolve(tables, code) else {
        return Action::Continue;
    };
    execute(app, cmd)
}

fn execute(app: &mut App, cmd: Command) -> Action {
    if app.focused_panel == Panel::Tree
        && let Some(action) = handle_tree_nav(app, cmd)
    {
        return action;
    }

    match cmd {
        Command::Quit => return Action::Quit,
        Command::ShowHelp => {
            app.help_scroll = Some(0);
        }
        Command::TogglePanel => {
            app.focused_panel = match app.focused_panel {
                Panel::Tree => Panel::Detail,
                Panel::Detail => Panel::Tree,
            };
        }

        Command::Toggle => {
            app.tree_state.toggle_selected();
        }

        // Detail pane
        Command::First => app.detail_scroll = 0,
        Command::Last => app.detail_scroll = u16::MAX,
        Command::ScrollDown => {
            app.detail_scroll = app.detail_scroll.saturating_add(1);
        }
        Command::ScrollUp => {
            app.detail_scroll = app.detail_scroll.saturating_sub(1);
        }
        Command::ScrollRight => {
            app.detail_hscroll = app.detail_hscroll.saturating_add(2);
        }
        Command::ScrollLeft => {
            app.detail_hscroll = app.detail_hscroll.saturating_sub(2);
        }

        // Editing
        Command::EditTitle => {
            if let Some(task) = app.selected_task() {
                app.edit_state = Some(EditState {
                    task_id: task.id,
                    input: Input::new(task.title.clone()),
                    error: None,
                    is_new: false,
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

        Command::CopyId => {
            if let Some(task) = app.selected_task() {
                let text = task.id.to_string();
                match arboard::Clipboard::new().and_then(|mut cb| cb.set_text(&text)) {
                    Ok(()) => app.set_status(format!("Copied #{text}")),
                    Err(e) => app.set_status(format!("Copy failed: {e}")),
                }
            }
        }

        Command::PriorityMode => {
            if let Some(task) = app.selected_task() {
                let idx = super::ui::PRIORITIES
                    .iter()
                    .position(|&p| p == task.priority)
                    .unwrap_or(super::ui::DEFAULT_PRIORITY_INDEX);
                app.priority_selection = Some(idx);
            }
        }

        Command::MoveMode => {
            if let Some(task) = app.selected_task() {
                let invalid = app::descendants(task.id, &app.children_map, &app.tasks);
                app.move_state = Some(MoveState {
                    task_id: task.id,
                    original_parent: task.parent,
                    invalid_targets: invalid,
                });
            }
        }

        Command::DepMode => {
            if let Some(task) = app.selected_task() {
                let original_deps = task.depends.iter().copied().collect();
                app.dep_state = Some(DepState {
                    task_id: task.id,
                    original_deps,
                    added: HashSet::new(),
                    removed: HashSet::new(),
                });
            }
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
        app.detail_hscroll = 0;
    }
}

fn sibling_navigate(app: &mut App, direction: isize) {
    let Some(&current_id) = app.tree_state.selected().last() else {
        return;
    };
    let parent = app.parent_map.get(&current_id).copied();
    let Some(siblings) = app.children_map.get(&parent) else {
        return;
    };
    let Some(pos) = siblings
        .iter()
        .position(|&idx| app.tasks[idx].id == current_id)
    else {
        return;
    };
    let new_pos = pos as isize + direction;
    if new_pos < 0 || new_pos >= siblings.len() as isize {
        return;
    }
    let target_id = app.tasks[siblings[new_pos as usize]].id;
    select_task(app, target_id);
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

fn handle_help_key(app: &mut App, code: KeyCode) -> Action {
    let scroll = app.help_scroll.as_mut().unwrap();
    match code {
        KeyCode::Char('?') | KeyCode::Esc => app.help_scroll = None,
        KeyCode::Char('q') => return Action::Quit,
        KeyCode::Char('j') | KeyCode::Down => *scroll = scroll.saturating_add(1),
        KeyCode::Char('k') | KeyCode::Up => *scroll = scroll.saturating_sub(1),
        _ => {}
    }
    Action::Continue
}

fn handle_priority_key(app: &mut App, code: KeyCode) -> Action {
    let Some(cmd) = keys::resolve(&[keys::PRIORITY], code) else {
        return Action::Continue;
    };

    let priority = match cmd {
        Command::SetCritical => Some(Priority::Critical),
        Command::SetHigh => Some(Priority::High),
        Command::SetNormal => Some(Priority::Normal),
        Command::SetLow => Some(Priority::Low),
        Command::NavigateDown => {
            if let Some(idx) = app.priority_selection.as_mut() {
                *idx = (*idx + 1) % super::ui::PRIORITIES.len();
            }
            return Action::Continue;
        }
        Command::NavigateUp => {
            if let Some(idx) = app.priority_selection.as_mut() {
                *idx = (*idx + super::ui::PRIORITIES.len() - 1) % super::ui::PRIORITIES.len();
            }
            return Action::Continue;
        }
        Command::Confirm => app.priority_selection.map(|idx| super::ui::PRIORITIES[idx]),
        Command::Cancel => {
            app.priority_selection = None;
            return Action::Continue;
        }
        _ => return Action::Continue,
    };

    app.priority_selection = None;

    if let Some(priority) = priority
        && let Some(task) = app.selected_task()
    {
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

/// Handle navigation commands shared across tree/move/dep modes.
/// Returns `Some(Action)` if the command was a global or nav command, `None` otherwise.
fn handle_tree_nav(app: &mut App, cmd: Command) -> Option<Action> {
    match cmd {
        Command::Quit => Some(Action::Quit),
        Command::ShowHelp => {
            app.help_scroll = Some(0);
            Some(Action::Continue)
        }
        Command::NavigateDown => {
            navigate(app, |ts| {
                ts.key_down();
            });
            Some(Action::Continue)
        }
        Command::NavigateUp => {
            navigate(app, |ts| {
                ts.key_up();
            });
            Some(Action::Continue)
        }
        Command::SiblingDown => {
            sibling_navigate(app, 1);
            Some(Action::Continue)
        }
        Command::SiblingUp => {
            sibling_navigate(app, -1);
            Some(Action::Continue)
        }
        Command::Collapse => {
            app.tree_state.key_left();
            Some(Action::Continue)
        }
        Command::Expand => {
            app.tree_state.key_right();
            Some(Action::Continue)
        }
        Command::First => {
            app.tree_state.select_first();
            app.detail_scroll = 0;
            app.detail_hscroll = 0;
            Some(Action::Continue)
        }
        Command::Last => {
            app.tree_state.select_last();
            app.detail_scroll = 0;
            app.detail_hscroll = 0;
            Some(Action::Continue)
        }
        _ => None,
    }
}

fn handle_move_key(app: &mut App, code: KeyCode) -> Action {
    let Some(cmd) = keys::resolve(&[keys::GLOBAL, keys::NAV, keys::MOVE], code) else {
        return Action::Continue;
    };

    if let Some(action) = handle_tree_nav(app, cmd) {
        return action;
    }

    match cmd {
        Command::Unparent => {
            let ms = app.move_state.as_ref().unwrap();
            if ms.original_parent.is_none() {
                app.set_status("Already a root task");
                return Action::Continue;
            }
            let task_id = ms.task_id;
            app.move_state = None;
            let edits = Edits {
                parent: Some(None),
                ..Edits::default()
            };
            match ops::apply_edits(&app.store, task_id, &edits) {
                Ok(_) => {
                    let _ = app.reload();
                    select_task(app, task_id);
                    app.set_status("Task unparented");
                }
                Err(e) => {
                    app.set_status(e.to_string());
                    select_task(app, task_id);
                }
            }
        }
        Command::Confirm => {
            let ms = app.move_state.as_ref().unwrap();
            let task_id = ms.task_id;
            let new_parent = app.selected_task().map(|t| t.id);

            // Validate: can't move under self or own descendant.
            if let Some(target) = new_parent
                && (target == task_id || ms.invalid_targets.contains(&target))
            {
                app.set_status("Cannot move under own descendant");
                return Action::Continue;
            }

            // Check if actually changed.
            if new_parent == ms.original_parent {
                app.move_state = None;
                select_task(app, task_id);
                return Action::Continue;
            }

            // Validate parent chain.
            if let Some(target) = new_parent {
                let task_ids: HashSet<TaskId> = app.tasks.iter().map(|t| t.id).collect();
                if let Err(e) = task::validate_parent(&app.tasks, &task_ids, task_id, target) {
                    app.set_status(e.to_string());
                    return Action::Continue;
                }
            }

            app.move_state = None;
            let edits = Edits {
                parent: Some(new_parent),
                ..Edits::default()
            };
            match ops::apply_edits(&app.store, task_id, &edits) {
                Ok(_) => {
                    let _ = app.reload();
                    if let Some(parent_id) = new_parent {
                        let path = app::identifier_path(parent_id, &app.parent_map);
                        app.tree_state.open(path);
                    }
                    select_task(app, task_id);
                    app.set_status("Task moved");
                }
                Err(e) => {
                    app.set_status(e.to_string());
                    select_task(app, task_id);
                }
            }
        }
        Command::Cancel => {
            let task_id = app.move_state.as_ref().unwrap().task_id;
            app.move_state = None;
            select_task(app, task_id);
        }
        _ => {}
    }
    Action::Continue
}

fn handle_dep_key(app: &mut App, code: KeyCode) -> Action {
    let Some(cmd) = keys::resolve(&[keys::GLOBAL, keys::NAV, keys::DEP], code) else {
        return Action::Continue;
    };

    if let Some(action) = handle_tree_nav(app, cmd) {
        return action;
    }

    match cmd {
        Command::ToggleDep => {
            let Some(selected) = app.selected_task() else {
                return Action::Continue;
            };
            let selected_id = selected.id;
            let ds = app.dep_state.as_mut().unwrap();

            if selected_id == ds.task_id {
                app.set_status("Cannot depend on self");
                return Action::Continue;
            }

            let is_effective_dep = ds.is_effective_dep(selected_id);

            if is_effective_dep {
                // Toggle off.
                if ds.added.contains(&selected_id) {
                    ds.added.remove(&selected_id);
                } else {
                    ds.removed.insert(selected_id);
                }
            } else {
                // Toggle on — validate DAG first.
                let subject = app.tasks.iter().find(|t| t.id == ds.task_id).unwrap();
                let mut test_task = subject.clone();
                // Apply pending edits to get effective dep set.
                for &id in &ds.added {
                    test_task.depends.insert(id);
                }
                for &id in &ds.removed {
                    test_task.depends.remove(&id);
                }
                test_task.depends.insert(selected_id);
                if task::validate_dag(&app.tasks, Some(&test_task)).is_err() {
                    app.set_status("Would create a dependency cycle");
                    return Action::Continue;
                }
                if ds.removed.contains(&selected_id) {
                    ds.removed.remove(&selected_id);
                } else {
                    ds.added.insert(selected_id);
                }
            }
        }
        Command::Confirm => {
            let ds = app.dep_state.take().unwrap();
            if !ds.added.is_empty() || !ds.removed.is_empty() {
                let edits = Edits {
                    add_deps: ds.added.into_iter().collect(),
                    rm_deps: ds.removed.into_iter().collect(),
                    ..Edits::default()
                };
                match ops::apply_edits(&app.store, ds.task_id, &edits) {
                    Ok(_) => {
                        let _ = app.reload();
                        app.set_status("Dependencies updated");
                    }
                    Err(e) => app.set_status(e.to_string()),
                }
            }
            select_task(app, ds.task_id);
        }
        Command::Cancel => {
            let ds = app.dep_state.take().unwrap();
            select_task(app, ds.task_id);
        }
        _ => {}
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
                if edit.is_new {
                    let _ = app.store.delete_task(edit.task_id);
                    let _ = app.reload();
                    return Action::Continue;
                }
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
        KeyCode::Esc => {
            if edit.is_new {
                let _ = app.store.delete_task(edit.task_id);
                let _ = app.reload();
            }
        }
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
        ops::NewTask {
            title: placeholder,
            parent,
            ..Default::default()
        },
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
                is_new: true,
            });
        }
    }
}

fn select_task(app: &mut App, id: TaskId) {
    let path = app::identifier_path(id, &app.parent_map);
    app.tree_state.select(path);
    app.detail_scroll = 0;
    app.detail_hscroll = 0;
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
                app.detail_hscroll = 0;
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
        MouseEventKind::ScrollRight => {
            if let Some(Panel::Detail) = hit_panel(app, mouse.column, mouse.row) {
                app.detail_hscroll = app.detail_hscroll.saturating_add(1);
            }
        }
        MouseEventKind::ScrollLeft => {
            if let Some(Panel::Detail) = hit_panel(app, mouse.column, mouse.row) {
                app.detail_hscroll = app.detail_hscroll.saturating_sub(1);
            }
        }
        _ => {}
    }
}
