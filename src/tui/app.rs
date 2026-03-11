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
    pub status_message: Option<(String, Instant)>,
    pub priority_selection: Option<usize>,
    pub show_help: bool,
    pub help_scroll: u16,
    pub dir_mtime: Option<SystemTime>,
    pub last_refresh_check: Instant,
}

/// Sorted parent→children mapping. Cached to avoid rebuilding every frame.
pub type ChildrenMap = HashMap<Option<TaskId>, Vec<usize>>;

impl App {
    pub fn new(store: Store) -> Result<Self, Error> {
        let tasks = store.load_all_tasks()?;
        let done_ids = ops::resolved_ids(&store, &tasks)?;
        let task_ids: HashSet<TaskId> = tasks.iter().map(|t| t.id).collect();
        let children_map = build_children_map(&tasks, &done_ids, &task_ids);
        let parent_map = build_parent_map(&tasks, &task_ids);
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
            status_message: None,
            priority_selection: None,
            show_help: false,
            help_scroll: 0,
            dir_mtime: None,
            last_refresh_check: Instant::now(),
        };
        app.dir_mtime = app.current_mtime();
        app.open_all_parents();
        Ok(app)
    }

    pub fn reload(&mut self) -> Result<(), Error> {
        self.tasks = self.store.load_all_tasks()?;
        self.done_ids = ops::resolved_ids(&self.store, &self.tasks)?;
        let task_ids: HashSet<TaskId> = self.tasks.iter().map(|t| t.id).collect();
        self.children_map = build_children_map(&self.tasks, &self.done_ids, &task_ids);
        self.parent_map = build_parent_map(&self.tasks, &task_ids);
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
            self.open_all_parents();
        }
    }

    fn current_mtime(&self) -> Option<SystemTime> {
        self.store.mtime()
    }

    fn open_all_parents(&mut self) {
        for &parent_id in self.children_map.keys().collect::<Vec<_>>() {
            if let Some(parent_id) = parent_id {
                let path = identifier_path(parent_id, &self.parent_map);
                self.tree_state.open(path);
            }
        }
    }

    pub fn set_status(&mut self, msg: impl Into<String>) {
        self.status_message = Some((msg.into(), Instant::now()));
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
    inner_width: u16,
) -> Vec<TreeItem<'a, TaskId>> {
    build_tree_children(None, children_map, tasks, done_ids, inner_width, 0)
}

fn build_tree_children<'a>(
    parent: Option<TaskId>,
    children_map: &ChildrenMap,
    tasks: &'a [Task],
    done_ids: &HashSet<TaskId>,
    inner_width: u16,
    depth: usize,
) -> Vec<TreeItem<'a, TaskId>> {
    let Some(indices) = children_map.get(&parent) else {
        return Vec::new();
    };
    indices
        .iter()
        .map(|&idx| {
            let task = &tasks[idx];
            let grandchildren = build_tree_children(
                Some(task.id),
                children_map,
                tasks,
                done_ids,
                inner_width,
                depth + 1,
            );
            let text = task_line(task, done_ids, inner_width, depth);
            if grandchildren.is_empty() {
                TreeItem::new_leaf(task.id, text)
            } else {
                TreeItem::new(task.id, text, grandchildren).expect("task IDs are unique")
            }
        })
        .collect()
}

/// Build sorted children map using indices into the task slice.
fn build_children_map(
    tasks: &[Task],
    done_ids: &HashSet<TaskId>,
    task_ids: &HashSet<TaskId>,
) -> ChildrenMap {
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

fn build_parent_map(tasks: &[Task], task_ids: &HashSet<TaskId>) -> HashMap<TaskId, TaskId> {
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

fn task_line<'a>(
    task: &'a Task,
    done_ids: &HashSet<TaskId>,
    inner_width: u16,
    depth: usize,
) -> Line<'a> {
    use unicode_width::UnicodeWidthStr;

    let indicator = task.indicator(task.is_blocked(done_ids));
    let resolved = task.status.is_resolved();
    let dim = Style::new().add_modifier(Modifier::DIM);

    let id_str = format!("#{}", task.id);
    // Content width: inner panel width minus indent (2*depth) minus node symbol (2)
    let content_width = (inner_width as usize).saturating_sub(2 * depth + 2);
    let indicator_w = 2; // symbol + space
    let id_w = id_str.len();
    // Reserve space for at least: indicator + 1 space + id
    let title_max = content_width.saturating_sub(indicator_w + 1 + id_w);
    let title_w = UnicodeWidthStr::width(task.title.as_str());

    let title_style = if resolved { dim } else { Style::default() };

    let (title_span, padding) = if title_w > title_max {
        let mut truncated = String::new();
        let mut w = 0;
        for ch in task.title.chars() {
            let cw = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
            if w + cw > title_max.saturating_sub(1) {
                break;
            }
            truncated.push(ch);
            w += cw;
        }
        truncated.push('…');
        let actual_w = UnicodeWidthStr::width(truncated.as_str());
        let pad = content_width
            .saturating_sub(indicator_w + actual_w + id_w)
            .max(1);
        (Span::styled(truncated, title_style), pad)
    } else {
        let pad = content_width
            .saturating_sub(indicator_w + title_w + id_w)
            .max(1);
        (Span::styled(task.title.as_str(), title_style), pad)
    };

    Line::from(vec![
        Span::styled(
            format!("{} ", indicator.symbol),
            anstyle_to_ratatui(indicator.color),
        ),
        title_span,
        Span::raw(" ".repeat(padding)),
        Span::styled(id_str, dim),
    ])
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
