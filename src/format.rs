use std::collections::{HashMap, HashSet};

use crate::markdown;
use crate::task::{Status, Style, Task, TaskId};
use crate::term::{visible_width, wrap_words};

// ANSI styles
const DIM: anstyle::Style = anstyle::Style::new().dimmed();
const BOLD: anstyle::Style = anstyle::Style::new().bold();
const RESET: anstyle::Reset = anstyle::Reset;

// Decoration symbols
const SYM_DOT: &str = "·";
const SYM_RULE: &str = "─";
const SYM_TREE_MID: &str = "├─";
const SYM_TREE_END: &str = "└─";
const SYM_TREE_PIPE: &str = "│";

const BLOCKED: Style = Style {
    symbol: "⋯",
    label: "blocked",
    color: anstyle::AnsiColor::Red.on_default(),
};

fn format_date(dt: &chrono::DateTime<chrono::Utc>) -> String {
    use chrono::Datelike;
    if dt.year() == chrono::Utc::now().year() {
        dt.format("%b %-d").to_string()
    } else {
        dt.format("%b %-d, %Y").to_string()
    }
}

fn format_datetime(dt: &chrono::DateTime<chrono::Utc>) -> String {
    use chrono::Datelike;
    if dt.year() == chrono::Utc::now().year() {
        dt.format("%b %-d %H:%M").to_string()
    } else {
        dt.format("%b %-d, %Y %H:%M").to_string()
    }
}

fn rule(width: usize) -> String {
    SYM_RULE.repeat(width)
}

pub fn format_task_detail(
    task: &Task,
    parent: Option<&Task>,
    deps: &[Task],
    children: &[Task],
    resolved: &HashSet<TaskId>,
    terminal_width: Option<usize>,
) -> String {
    let width = terminal_width.unwrap_or(80).min(80);
    let mut out = String::new();

    // Header rule: ── status [· ▲ priority] ───────── date · #id ──
    let ss = task.status.style();
    let mut left = format!(
        "{SYM_RULE}{SYM_RULE} {RESET}{}{}{RESET}{DIM}",
        ss.color, ss.label
    );
    if !task.priority.is_default() {
        let ps = task.priority.style();
        left.push_str(&format!(
            " {SYM_DOT} {RESET}{}{}{RESET} {}{DIM}",
            ps.color, ps.symbol, ps.label,
        ));
    }
    left.push(' ');
    let date = format_date(&task.modified);
    let right = format!(" {date} {SYM_DOT} #{} {SYM_RULE}{SYM_RULE}", task.id);
    let fill = width.saturating_sub(visible_width(&left) + visible_width(&right));
    out.push_str(&format!(
        "{DIM}{left}{}{right}{RESET}\n",
        SYM_RULE.repeat(fill)
    ));

    // Title (bold, word-wrapped)
    let title = format!("{BOLD}{}{RESET}", task.title);
    wrap_words(&title, &mut out, width, "", "");

    // Heavy rule with optional right-aligned labels
    if task.labels.is_empty() {
        out.push_str(&format!("{DIM}{}{RESET}\n", rule(width)));
    } else {
        let labels = task
            .labels
            .iter()
            .cloned()
            .collect::<Vec<_>>()
            .join(&format!(" {SYM_DOT} "));
        let right_width = 1 + visible_width(&labels) + 1 + 2; // space labels space ──
        let fill = width.saturating_sub(right_width);
        out.push_str(&format!(
            "{DIM}{}{RESET} {labels} {DIM}{SYM_RULE}{SYM_RULE}{RESET}\n",
            SYM_RULE.repeat(fill)
        ));
    }

    // Parent
    if let Some(p) = parent {
        out.push_str(&format!(
            "{BOLD}Parent:{RESET} {}\n",
            format_related(p, p.is_blocked(resolved))
        ));
    }

    // Dependencies
    if deps.len() == 1 {
        let d = &deps[0];
        out.push_str(&format!(
            "{BOLD}Depends on:{RESET} {}\n",
            format_related(d, d.is_blocked(resolved))
        ));
    } else if deps.len() > 1 {
        out.push_str(&format!("{BOLD}Depends on:{RESET}\n"));
        for d in deps {
            out.push_str(&format!(
                "  {}\n",
                format_related(d, d.is_blocked(resolved))
            ));
        }
    }

    // Children
    if !children.is_empty() {
        out.push_str(&format!("{BOLD}Children:{RESET}\n"));
        for child in children {
            let blocked = child.is_blocked(resolved);
            out.push_str(&format!("  {}\n", format_related(child, blocked)));
        }
    }

    // Description
    if let Some(desc) = &task.description {
        out.push('\n');
        out.push_str(&markdown::render(desc, terminal_width));
    }

    // Log entries
    for entry in &task.log {
        let ts = format_datetime(&entry.timestamp);
        let agent = match &entry.agent {
            Some(a) => format!(" ({a}) "),
            None => String::from(" "),
        };
        let label = format!("{SYM_RULE}{SYM_RULE} {ts}{agent}");
        let fill = width.saturating_sub(visible_width(&label));
        out.push_str(&format!("\n{DIM}{label}{}{RESET}\n", SYM_RULE.repeat(fill)));
        out.push_str(&markdown::render(&entry.message, terminal_width));
    }
    out
}

fn indicator(task: &Task, blocked: bool) -> Style {
    if blocked {
        return BLOCKED;
    }
    match task.status {
        Status::Todo if !task.priority.is_default() => task.priority.style(),
        s => s.style(),
    }
}

fn format_related(task: &Task, blocked: bool) -> String {
    let style = indicator(task, blocked);
    if style.symbol.trim().is_empty() {
        format!("#{} {}", task.id, task.title)
    } else {
        format!(
            "{}{}{RESET} #{} {}",
            style.color, style.symbol, task.id, task.title
        )
    }
}

struct ListRow<'a> {
    task: &'a Task,
    tree: String,
}

fn tree_depth(parent_map: &HashMap<TaskId, TaskId>) -> usize {
    let mut max = 0;
    for &id in parent_map.keys() {
        let mut d = 0;
        let mut current = id;
        let mut visited = HashSet::new();
        visited.insert(current);
        while let Some(&p) = parent_map.get(&current) {
            if !visited.insert(p) {
                break;
            }
            d += 1;
            current = p;
        }
        max = max.max(d);
    }
    max
}

fn format_list_row(
    row: &ListRow,
    id_width: usize,
    tree_width: usize,
    done_ids: &HashSet<TaskId>,
) -> String {
    let task = row.task;
    let blocked = task.is_blocked(done_ids);
    let style = indicator(task, blocked);
    let is_done = task.status.is_resolved();

    let id_str = format!("#{}", task.id);
    let col1_width = tree_width + id_width + 1;
    let col1_pad = " ".repeat(col1_width.saturating_sub(visible_width(&row.tree) + id_str.len()));

    if is_done {
        format!(
            "{DIM}{}{id_str}{col1_pad}  {}{}{RESET}{DIM} {}{RESET}\n",
            row.tree, style.color, style.symbol, task.title
        )
    } else {
        format!(
            "{DIM}{}{RESET}{id_str}{col1_pad}  {}{}{RESET} {}\n",
            row.tree, style.color, style.symbol, task.title
        )
    }
}

pub fn format_task_list(tasks: &[Task], flat: bool, done_ids: &HashSet<TaskId>) -> String {
    if tasks.is_empty() {
        return String::from("No tasks.\n");
    }
    if flat {
        format_flat(tasks, done_ids)
    } else {
        format_tree(tasks, done_ids)
    }
}

fn format_flat(tasks: &[Task], done_ids: &HashSet<TaskId>) -> String {
    let id_width = tasks
        .iter()
        .map(|t| t.id.to_string().len())
        .max()
        .unwrap_or(1);
    let mut sorted: Vec<&Task> = tasks.iter().collect();
    sorted.sort_by(|a, b| a.sort_key(done_ids).cmp(&b.sort_key(done_ids)));
    let mut out = String::new();
    for task in sorted {
        let row = ListRow {
            task,
            tree: String::new(),
        };
        out.push_str(&format_list_row(&row, id_width, 0, done_ids));
    }
    out
}

fn format_tree(tasks: &[Task], done_ids: &HashSet<TaskId>) -> String {
    let task_ids: HashSet<TaskId> = tasks.iter().map(|t| t.id).collect();
    let mut children_map: HashMap<Option<TaskId>, Vec<&Task>> = HashMap::new();
    let mut parent_map: HashMap<TaskId, TaskId> = HashMap::new();
    for task in tasks {
        // Treat tasks with missing parents as roots
        let parent = task.parent.filter(|p| task_ids.contains(p));
        children_map.entry(parent).or_default().push(task);
        if let Some(p) = parent {
            parent_map.insert(task.id, p);
        }
    }

    for siblings in children_map.values_mut() {
        siblings.sort_by(|a, b| a.sort_key(done_ids).cmp(&b.sort_key(done_ids)));
    }

    let mut rows: Vec<ListRow> = Vec::new();
    collect_tree_rows(&children_map, None, "", &mut rows);

    let id_width = tasks
        .iter()
        .map(|t| t.id.to_string().len())
        .max()
        .unwrap_or(1);
    let depth = tree_depth(&parent_map);
    let tree_width = depth * 3;

    let mut out = String::new();
    for row in &rows {
        out.push_str(&format_list_row(row, id_width, tree_width, done_ids));
    }
    out
}

fn collect_tree_rows<'a>(
    children_map: &HashMap<Option<TaskId>, Vec<&'a Task>>,
    parent: Option<TaskId>,
    prefix: &str,
    rows: &mut Vec<ListRow<'a>>,
) {
    let Some(children) = children_map.get(&parent) else {
        return;
    };
    let count = children.len();
    for (i, task) in children.iter().enumerate() {
        let is_last = i == count - 1;
        let tree = if parent.is_none() {
            String::new()
        } else {
            let connector = if is_last { SYM_TREE_END } else { SYM_TREE_MID };
            format!("{prefix}{connector} ")
        };
        rows.push(ListRow { task, tree });
        let next_prefix = if parent.is_none() {
            String::new()
        } else if is_last {
            format!("{prefix}   ")
        } else {
            format!("{prefix}{SYM_TREE_PIPE}  ")
        };
        collect_tree_rows(children_map, Some(task.id), &next_prefix, rows);
    }
}
