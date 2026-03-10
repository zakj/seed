use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant, SystemTime};

use tui_input::Input;
use tui_tree_widget::{TreeItem, TreeState};

use crate::error::Error;
use crate::ops;
use crate::store::Store;
use crate::task::{Task, TaskId};

pub struct EditState {
    pub task_id: TaskId,
    pub input: Input,
    pub error: Option<String>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Panel {
    Tree,
    Detail,
}

pub struct App {
    pub store: Store,
    pub tasks: Vec<Task>,
    pub done_ids: HashSet<TaskId>,
    pub tree_state: TreeState<TaskId>,
    pub children_map: ChildrenMap,
    pub parent_map: HashMap<TaskId, TaskId>,
    pub focused_panel: Panel,
    pub detail_scroll: u16,
    /// Pane areas from the last draw, used for mouse hit-testing.
    pub tree_area: ratatui::layout::Rect,
    pub detail_area: ratatui::layout::Rect,
    /// Maps detail content line indices to dep TaskIds for click navigation.
    pub detail_dep_lines: Vec<(usize, TaskId)>,
    pub edit_state: Option<EditState>,
    pub dir_mtime: Option<SystemTime>,
    pub last_refresh_check: Instant,
}

/// Sorted parent→children mapping. Cached to avoid rebuilding every frame.
pub type ChildrenMap = HashMap<Option<TaskId>, Vec<usize>>;

impl App {
    pub fn new(store: Store) -> Result<Self, Error> {
        let tasks = store.load_all_tasks()?;
        let done_ids = ops::resolved_ids(&store, &tasks)?;
        let children_map = build_children_map(&tasks, &done_ids);
        let parent_map = build_parent_map(&tasks);
        let mut app = Self {
            store,
            tasks,
            done_ids,
            tree_state: TreeState::default(),
            children_map,
            parent_map,
            focused_panel: Panel::Tree,
            detail_scroll: 0,
            tree_area: ratatui::layout::Rect::default(),
            detail_area: ratatui::layout::Rect::default(),
            detail_dep_lines: Vec::new(),
            edit_state: None,
            dir_mtime: None,
            last_refresh_check: Instant::now(),
        };
        app.dir_mtime = app.current_mtime();
        // Start with all non-leaf nodes open.
        for &parent_id in app.children_map.keys().collect::<Vec<_>>() {
            if let Some(parent_id) = parent_id {
                let path = identifier_path(parent_id, &app.parent_map);
                app.tree_state.open(path);
            }
        }
        app.tree_state.select_first();
        Ok(app)
    }

    pub fn reload(&mut self) -> Result<(), Error> {
        self.tasks = self.store.load_all_tasks()?;
        self.done_ids = ops::resolved_ids(&self.store, &self.tasks)?;
        self.children_map = build_children_map(&self.tasks, &self.done_ids);
        self.parent_map = build_parent_map(&self.tasks);
        self.dir_mtime = self.current_mtime();
        Ok(())
    }

    /// Check tasks_dir and config.kdl mtime; reload if changed. Throttled to ~1s.
    pub fn maybe_refresh(&mut self) {
        if self.last_refresh_check.elapsed() < Duration::from_secs(1) {
            return;
        }
        self.last_refresh_check = Instant::now();
        let current = self.current_mtime();
        if current == self.dir_mtime {
            return;
        }
        if self.reload().is_ok() {
            // Open any new parent nodes that appeared.
            for &parent_id in self.children_map.keys().collect::<Vec<_>>() {
                if let Some(parent_id) = parent_id {
                    let path = identifier_path(parent_id, &self.parent_map);
                    self.tree_state.open(path);
                }
            }
        }
    }

    fn current_mtime(&self) -> Option<SystemTime> {
        let tasks_mtime = std::fs::metadata(self.store.tasks_dir())
            .and_then(|m| m.modified())
            .ok();
        let config_mtime = std::fs::metadata(self.store.root().join("config.kdl"))
            .and_then(|m| m.modified())
            .ok();
        // Use the latest of the two.
        match (tasks_mtime, config_mtime) {
            (Some(a), Some(b)) => Some(a.max(b)),
            (a, b) => a.or(b),
        }
    }

    pub fn selected_task(&self) -> Option<&Task> {
        let selected = self.tree_state.selected();
        let id = selected.last()?;
        self.tasks.iter().find(|t| &t.id == id)
    }
}

/// Count visible tree items (expanded nodes + their visible children).
pub fn visible_item_count(
    children_map: &ChildrenMap,
    tree_state: &TreeState<TaskId>,
    tasks: &[Task],
) -> usize {
    let opened = tree_state.opened();
    let mut path = Vec::new();
    count_visible(None, children_map, opened, tasks, &mut path)
}

fn count_visible(
    parent: Option<TaskId>,
    children_map: &ChildrenMap,
    opened: &std::collections::HashSet<Vec<TaskId>>,
    tasks: &[Task],
    path: &mut Vec<TaskId>,
) -> usize {
    let Some(indices) = children_map.get(&parent) else {
        return 0;
    };
    let mut count = 0;
    for &idx in indices {
        let task = &tasks[idx];
        count += 1;
        path.push(task.id);
        if opened.contains(path) {
            count += count_visible(Some(task.id), children_map, opened, tasks, path);
        }
        path.pop();
    }
    count
}

/// Build tree items from cached children map for tui-tree-widget.
pub fn build_tree_items<'a>(
    tasks: &'a [Task],
    done_ids: &HashSet<TaskId>,
    children_map: &ChildrenMap,
) -> Vec<TreeItem<'a, TaskId>> {
    build_tree_children(None, children_map, tasks, done_ids)
}

fn build_tree_children<'a>(
    parent: Option<TaskId>,
    children_map: &ChildrenMap,
    tasks: &'a [Task],
    done_ids: &HashSet<TaskId>,
) -> Vec<TreeItem<'a, TaskId>> {
    let Some(indices) = children_map.get(&parent) else {
        return Vec::new();
    };
    indices
        .iter()
        .map(|&idx| {
            let task = &tasks[idx];
            let grandchildren = build_tree_children(Some(task.id), children_map, tasks, done_ids);
            let text = task_line(task, done_ids);
            if grandchildren.is_empty() {
                TreeItem::new_leaf(task.id, text)
            } else {
                TreeItem::new(task.id, text, grandchildren).expect("task IDs are unique")
            }
        })
        .collect()
}

/// Build sorted children map using indices into the task slice.
fn build_children_map(tasks: &[Task], done_ids: &HashSet<TaskId>) -> ChildrenMap {
    let task_ids: HashSet<TaskId> = tasks.iter().map(|t| t.id).collect();
    let mut children_map: ChildrenMap = HashMap::new();
    for (i, task) in tasks.iter().enumerate() {
        let parent = task.parent.filter(|p| task_ids.contains(p));
        children_map.entry(parent).or_default().push(i);
    }
    for indices in children_map.values_mut() {
        indices.sort_by(|&a, &b| {
            tasks[a]
                .sort_key(done_ids)
                .cmp(&tasks[b].sort_key(done_ids))
        });
    }
    children_map
}

fn build_parent_map(tasks: &[Task]) -> HashMap<TaskId, TaskId> {
    let task_ids: HashSet<TaskId> = tasks.iter().map(|t| t.id).collect();
    tasks
        .iter()
        .filter_map(|t| t.parent.filter(|p| task_ids.contains(p)).map(|p| (t.id, p)))
        .collect()
}

/// Build the identifier path (Vec<TaskId>) for a given task ID.
pub fn identifier_path(id: TaskId, parent_map: &HashMap<TaskId, TaskId>) -> Vec<TaskId> {
    let mut path = vec![id];
    let mut current = id;
    while let Some(&parent) = parent_map.get(&current) {
        path.push(parent);
        current = parent;
    }
    path.reverse();
    path
}

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

fn task_line<'a>(task: &'a Task, done_ids: &HashSet<TaskId>) -> Line<'a> {
    let indicator = task.indicator(task.is_blocked(done_ids));
    let resolved = task.status.is_resolved();

    let mut spans = Vec::new();

    // Status/priority indicator
    if indicator.symbol.trim().is_empty() {
        spans.push(Span::raw("  "));
    } else {
        spans.push(Span::styled(
            format!("{} ", indicator.symbol),
            anstyle_to_ratatui(indicator.color),
        ));
    }

    // Task ID (always dim)
    spans.push(Span::styled(
        format!("#{} ", task.id),
        Style::new().add_modifier(Modifier::DIM),
    ));

    // Title
    if resolved {
        spans.push(Span::styled(
            task.title.as_str(),
            Style::new().add_modifier(Modifier::DIM),
        ));
    } else {
        spans.push(Span::raw(task.title.as_str()));
    }

    Line::from(spans)
}

pub fn anstyle_to_ratatui(s: anstyle::Style) -> Style {
    let mut style = Style::new();
    if let Some(fg) = s.get_fg_color() {
        style = style.fg(ansi_color_to_ratatui(fg));
    }
    if s.get_effects().contains(anstyle::Effects::DIMMED) {
        style = style.add_modifier(Modifier::DIM);
    }
    style
}

fn ansi_color_to_ratatui(c: anstyle::Color) -> Color {
    match c {
        anstyle::Color::Ansi(anstyle::AnsiColor::Red) => Color::Red,
        anstyle::Color::Ansi(anstyle::AnsiColor::Yellow) => Color::Yellow,
        anstyle::Color::Ansi(anstyle::AnsiColor::Blue) => Color::Blue,
        anstyle::Color::Ansi(anstyle::AnsiColor::Green) => Color::Green,
        anstyle::Color::Ansi(anstyle::AnsiColor::Cyan) => Color::Cyan,
        anstyle::Color::Ansi(anstyle::AnsiColor::Magenta) => Color::Magenta,
        anstyle::Color::Ansi(anstyle::AnsiColor::White) => Color::White,
        anstyle::Color::Ansi(anstyle::AnsiColor::Black) => Color::Black,
        _ => Color::Reset,
    }
}
