use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant, SystemTime};

use tui_input::Input;
use tui_tree_widget::{TreeItem, TreeState};

use crate::error::Error;
use crate::ops;
use crate::store::Store;
use crate::task::{Task, TaskId};

use super::keys;

pub struct EditState {
    pub task_id: TaskId,
    pub input: Input,
    pub error: Option<String>,
    pub is_new: bool,
}

pub struct MoveState {
    pub task_id: TaskId,
    pub original_parent: Option<TaskId>,
    pub invalid_targets: HashSet<TaskId>,
}

pub struct DepState {
    pub task_id: TaskId,
    pub original_deps: HashSet<TaskId>,
    pub added: HashSet<TaskId>,
    pub removed: HashSet<TaskId>,
}

impl DepState {
    pub fn is_effective_dep(&self, id: TaskId) -> bool {
        (self.original_deps.contains(&id) && !self.removed.contains(&id))
            || self.added.contains(&id)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Panel {
    Tree,
    Detail,
}

pub struct HelpOverlay {
    pub scroll: u16,
}

pub struct Search {
    pub input: Option<Input>,
    pub query: String,
    pub matches: Vec<TaskId>,
    pub original_selection: Vec<TaskId>,
}

pub enum Mode {
    Normal(Panel),
    Edit(EditState),
    Move(MoveState),
    Dep(DepState),
    Priority(usize),
}

impl Mode {
    /// Take ownership of the current mode, replacing it with Normal(Tree).
    pub fn take(&mut self) -> Mode {
        std::mem::replace(self, Mode::Normal(Panel::Tree))
    }

    pub fn key_tables(&self) -> &'static [&'static [keys::Hint]] {
        match self {
            Mode::Normal(Panel::Tree) => &[keys::GLOBAL, keys::NAV, keys::SEARCH, keys::TREE],
            Mode::Normal(Panel::Detail) => &[keys::GLOBAL, keys::DETAIL],
            Mode::Move(_) => &[keys::GLOBAL, keys::NAV, keys::SEARCH, keys::MOVE],
            Mode::Dep(_) => &[keys::GLOBAL, keys::NAV, keys::SEARCH, keys::DEP],
            Mode::Priority(_) => &[keys::PRIORITY],
            Mode::Edit(_) => &[],
        }
    }
}

pub struct App {
    pub store: Store,
    pub include_archived: bool,
    pub tasks: Vec<Task>,
    pub done_ids: HashSet<TaskId>,
    pub tree_state: TreeState<TaskId>,
    pub children_map: ChildrenMap,
    pub parent_map: HashMap<TaskId, TaskId>,
    pub mode: Mode,
    pub help: Option<HelpOverlay>,
    pub search: Option<Search>,
    pub detail_scroll: u16,
    pub detail_hscroll: u16,
    /// Pane areas from the last draw, used for mouse hit-testing.
    pub tree_area: ratatui::layout::Rect,
    pub detail_area: ratatui::layout::Rect,
    /// Maps detail content line indices to dep TaskIds for click navigation.
    pub detail_dep_lines: Vec<(usize, TaskId)>,
    pub status_message: Option<(String, Instant)>,
    pub dir_mtime: Option<SystemTime>,
    pub last_refresh_check: Instant,
}

/// Sorted parent→children mapping. Cached to avoid rebuilding every frame.
pub type ChildrenMap = HashMap<Option<TaskId>, Vec<usize>>;

impl App {
    pub fn new(store: Store, include_archived: bool) -> Result<Self, Error> {
        let tasks = store.load_tasks(include_archived)?;
        let done_ids = ops::resolved_ids(&store, &tasks)?;
        let task_ids: HashSet<TaskId> = tasks.iter().map(|t| t.id).collect();
        let children_map = build_children_map(&tasks, &done_ids, &task_ids);
        let parent_map = build_parent_map(&tasks, &task_ids);
        let mut app = Self {
            store,
            include_archived,
            tasks,
            done_ids,
            tree_state: TreeState::default(),
            children_map,
            parent_map,
            mode: Mode::Normal(Panel::Tree),
            help: None,
            search: None,
            detail_scroll: 0,
            detail_hscroll: 0,
            tree_area: ratatui::layout::Rect::default(),
            detail_area: ratatui::layout::Rect::default(),
            detail_dep_lines: Vec::new(),
            status_message: None,
            dir_mtime: None,
            last_refresh_check: Instant::now(),
        };
        app.dir_mtime = app.current_mtime();
        app.open_all_parents();
        Ok(app)
    }

    pub fn panel(&self) -> Panel {
        match &self.mode {
            Mode::Normal(panel) => *panel,
            _ => Panel::Tree,
        }
    }

    pub fn reload(&mut self) -> Result<(), Error> {
        self.tasks = self.store.load_tasks(self.include_archived)?;
        self.done_ids = ops::resolved_ids(&self.store, &self.tasks)?;
        let task_ids: HashSet<TaskId> = self.tasks.iter().map(|t| t.id).collect();
        self.children_map = build_children_map(&self.tasks, &self.done_ids, &task_ids);
        self.parent_map = build_parent_map(&self.tasks, &task_ids);
        self.dir_mtime = self.current_mtime();
        if let Some(ref search) = self.search {
            let query = search.query.clone();
            let matches = self.compute_matches(&query);
            self.search.as_mut().unwrap().matches = matches;
        }
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

    /// Compute matching task IDs in tree display order.
    pub fn compute_matches(&self, query: &str) -> Vec<TaskId> {
        if query.is_empty() {
            return Vec::new();
        }
        let lower = query.to_lowercase();
        let numeric = query.strip_prefix('#').unwrap_or(query);
        let id_match: Option<TaskId> = numeric.parse().ok();

        let mut matches = Vec::new();
        self.collect_matches_in_tree_order(None, &lower, id_match, &mut matches);
        matches
    }

    fn collect_matches_in_tree_order(
        &self,
        parent: Option<TaskId>,
        lower_query: &str,
        id_match: Option<TaskId>,
        out: &mut Vec<TaskId>,
    ) {
        let Some(indices) = self.children_map.get(&parent) else {
            return;
        };
        for &idx in indices {
            let task = &self.tasks[idx];
            if task.title.to_lowercase().contains(lower_query) || id_match == Some(task.id) {
                out.push(task.id);
            }
            self.collect_matches_in_tree_order(Some(task.id), lower_query, id_match, out);
        }
    }

    /// Return the set of currently matching task IDs for rendering.
    pub fn search_match_ids(&self) -> HashSet<TaskId> {
        self.search
            .as_ref()
            .map(|s| s.matches.iter().copied().collect())
            .unwrap_or_default()
    }
}

/// Collect all descendant IDs of a task via the children map.
pub fn descendants(task_id: TaskId, children_map: &ChildrenMap, tasks: &[Task]) -> HashSet<TaskId> {
    let mut result = HashSet::new();
    let mut stack = vec![task_id];
    while let Some(id) = stack.pop() {
        if let Some(indices) = children_map.get(&Some(id)) {
            for &idx in indices {
                let child_id = tasks[idx].id;
                if result.insert(child_id) {
                    stack.push(child_id);
                }
            }
        }
    }
    result
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

/// Overlay context for rendering tree items in move/dep mode.
pub enum TreeOverlay<'a> {
    Move(&'a MoveState),
    Dep(&'a DepState),
}

/// Context for rendering tree items, bundled to reduce argument count.
pub struct TreeContext<'a, 'b> {
    pub done_ids: &'a HashSet<TaskId>,
    pub children_map: &'a ChildrenMap,
    pub inner_width: u16,
    pub overlay: Option<&'b TreeOverlay<'b>>,
    pub search_matches: &'a HashSet<TaskId>,
}

/// Build tree items from cached children map for tui-tree-widget.
pub fn build_tree_items<'a>(
    tasks: &'a [Task],
    ctx: &TreeContext<'_, '_>,
) -> Vec<TreeItem<'a, TaskId>> {
    build_tree_children(None, tasks, ctx, 0)
}

fn build_tree_children<'a>(
    parent: Option<TaskId>,
    tasks: &'a [Task],
    ctx: &TreeContext<'_, '_>,
    depth: usize,
) -> Vec<TreeItem<'a, TaskId>> {
    let Some(indices) = ctx.children_map.get(&parent) else {
        return Vec::new();
    };
    indices
        .iter()
        .map(|&idx| {
            let task = &tasks[idx];
            let grandchildren = build_tree_children(Some(task.id), tasks, ctx, depth + 1);
            let text = task_line(task, depth, ctx);
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

/// Resolved styling for a single task in the tree, based on overlay state.
#[derive(Default)]
struct TaskStyle {
    prefix: &'static str,
    prefix_color: Color,
    style_override: Option<Style>,
}

fn resolve_task_style(task_id: TaskId, overlay: Option<&TreeOverlay<'_>>) -> TaskStyle {
    let accent = Style::new().fg(Color::Yellow);
    match overlay {
        Some(TreeOverlay::Move(ms)) => {
            if task_id == ms.task_id {
                TaskStyle {
                    style_override: Some(accent),
                    ..Default::default()
                }
            } else if ms.invalid_targets.contains(&task_id) {
                TaskStyle {
                    style_override: Some(Style::new().fg(Color::Red).add_modifier(Modifier::DIM)),
                    ..Default::default()
                }
            } else {
                TaskStyle::default()
            }
        }
        Some(TreeOverlay::Dep(ds)) => {
            if task_id == ds.task_id {
                TaskStyle {
                    style_override: Some(accent),
                    ..Default::default()
                }
            } else if ds.added.contains(&task_id) {
                TaskStyle {
                    prefix: "[+] ",
                    prefix_color: Color::Green,
                    ..Default::default()
                }
            } else if ds.removed.contains(&task_id) {
                TaskStyle {
                    prefix: "[-] ",
                    prefix_color: Color::Red,
                    ..Default::default()
                }
            } else if ds.original_deps.contains(&task_id) {
                TaskStyle {
                    prefix: "[✓] ",
                    prefix_color: Color::DarkGray,
                    ..Default::default()
                }
            } else {
                TaskStyle::default()
            }
        }
        None => TaskStyle::default(),
    }
}

fn task_line<'a>(task: &'a Task, depth: usize, ctx: &TreeContext<'_, '_>) -> Line<'a> {
    use unicode_width::UnicodeWidthStr;

    let indicator = task.indicator(task.is_blocked(ctx.done_ids));
    let resolved = task.status.is_resolved();
    let dim = Style::new().add_modifier(Modifier::DIM);
    let ts = resolve_task_style(task.id, ctx.overlay);

    let id_str = format!("#{}", task.id);
    let prefix_w = UnicodeWidthStr::width(ts.prefix);
    // Content width: inner panel width minus indent (2*depth) minus node symbol (2)
    let content_width = (ctx.inner_width as usize).saturating_sub(2 * depth + 2);
    let indicator_w = 2; // symbol + space
    let id_w = id_str.len();
    let title_max = content_width.saturating_sub(prefix_w + indicator_w + 1 + id_w);
    let title_w = UnicodeWidthStr::width(task.title.as_str());

    let title_style = ts
        .style_override
        .unwrap_or(if resolved { dim } else { Style::default() });

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
            .saturating_sub(prefix_w + indicator_w + actual_w + id_w)
            .max(1);
        (Span::styled(truncated, title_style), pad)
    } else {
        let pad = content_width
            .saturating_sub(prefix_w + indicator_w + title_w + id_w)
            .max(1);
        (Span::styled(task.title.as_str(), title_style), pad)
    };

    let indicator_style = ts
        .style_override
        .unwrap_or_else(|| anstyle_to_ratatui(indicator.color));

    let is_match = ctx.search_matches.contains(&task.id);
    let id_style = if is_match {
        Style::new().fg(Color::Yellow)
    } else {
        dim
    };

    let mut spans = Vec::new();
    if !ts.prefix.is_empty() {
        spans.push(Span::styled(ts.prefix, Style::new().fg(ts.prefix_color)));
    }
    spans.extend([
        Span::styled(format!("{} ", indicator.symbol), indicator_style),
        title_span,
        Span::raw(" ".repeat(padding)),
        Span::styled(id_str, id_style),
    ]);
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
