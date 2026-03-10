use std::collections::{HashMap, HashSet};

use chrono::Utc;

use crate::error::Error;
use crate::store::Store;
use crate::task::{
    Priority, Status, Task, TaskId, validate_completion, validate_dag, validate_deps_exist,
    validate_parent,
};

#[derive(Default)]
pub struct Edits {
    pub title: Option<String>,
    pub status: Option<Status>,
    pub priority: Option<Priority>,
    pub description: Option<String>,
    /// `Some(Some(id))` sets the parent, `Some(None)` removes it, `None` leaves unchanged.
    pub parent: Option<Option<TaskId>>,
    pub add_labels: Vec<String>,
    pub rm_labels: Vec<String>,
    pub add_deps: Vec<TaskId>,
    pub rm_deps: Vec<TaskId>,
    pub force: bool,
}

pub struct TaskContext {
    pub task: Task,
    pub parent: Option<Task>,
    pub deps: Vec<Task>,
    pub children: Vec<Task>,
    pub done_ids: HashSet<TaskId>,
}

pub struct ReadyTasks {
    pub ready: Vec<Task>,
    pub done_ids: HashSet<TaskId>,
    pub all_tasks: Vec<Task>,
}

/// Collect IDs of all resolved tasks, including archived ones.
pub fn resolved_ids(store: &Store, tasks: &[Task]) -> Result<HashSet<TaskId>, Error> {
    let mut ids: HashSet<TaskId> = tasks
        .iter()
        .filter(|t| t.status.is_resolved())
        .map(|t| t.id)
        .collect();
    ids.extend(store.load_archived_ids()?);
    Ok(ids)
}

pub fn create_task(
    store: &Store,
    title: String,
    priority: Option<Priority>,
    labels: impl IntoIterator<Item = String>,
    parent: Option<TaskId>,
    deps: &[TaskId],
    description: Option<&str>,
) -> Result<Task, Error> {
    let id = store.allocate_id()?;
    let mut task = Task::new(id, title);
    task.priority = priority.unwrap_or_default();
    task.labels = labels.into_iter().collect();
    task.parent = parent;
    task.depends = deps.iter().copied().collect();
    task.description = description.and_then(normalize_description);

    if parent.is_some() || !deps.is_empty() {
        let all_tasks = store.load_all_tasks()?;
        let mut known_ids: HashSet<TaskId> = all_tasks.iter().map(|t| t.id).collect();
        known_ids.extend(store.load_archived_ids()?);

        if let Some(parent_id) = parent {
            validate_parent(&all_tasks, &known_ids, id, parent_id)?;
        }
        if !deps.is_empty() {
            validate_deps_exist(&known_ids, deps)?;
            validate_dag(&all_tasks, Some(&task))?;
        }
    }

    store.write_task(&task)?;
    Ok(task)
}

pub fn start_task(store: &Store, id: TaskId) -> Result<(Task, bool), Error> {
    let (mut task, mtime) = store.read_task_with_mtime(id)?;
    if task.status == Status::InProgress {
        return Ok((task, false));
    }
    if task.status.is_resolved() {
        return Err(Error::CannotStart(id, task.status));
    }
    task.status = Status::InProgress;
    task.modified = Utc::now();
    store.write_task_checked(&task, mtime)?;
    Ok((task, true))
}

pub fn complete_task(store: &Store, id: TaskId, force: bool) -> Result<(Task, bool), Error> {
    let (mut task, mtime) = store.read_task_with_mtime(id)?;
    if task.status == Status::Done {
        return Ok((task, false));
    }
    if !force {
        let all_tasks = store.load_all_tasks()?;
        let archived_ids = store.load_archived_ids()?;
        validate_completion(&all_tasks, &archived_ids, &task)?;
    }
    task.status = Status::Done;
    task.modified = Utc::now();
    store.write_task_checked(&task, mtime)?;
    Ok((task, true))
}

pub fn drop_task(store: &Store, id: TaskId) -> Result<(Task, bool), Error> {
    let (mut task, mtime) = store.read_task_with_mtime(id)?;
    if task.status == Status::Dropped {
        return Ok((task, false));
    }
    if task.status == Status::Done {
        return Err(Error::CannotDrop(id));
    }
    task.status = Status::Dropped;
    task.modified = Utc::now();
    store.write_task_checked(&task, mtime)?;
    Ok((task, true))
}

pub fn apply_edits(store: &Store, id: TaskId, edits: &Edits) -> Result<(Task, bool), Error> {
    let (mut task, mtime) = store.read_task_with_mtime(id)?;
    let original = task.clone();

    if let Some(ref t) = edits.title {
        task.title = t.clone();
    }
    if let Some(s) = edits.status {
        task.status = s;
    }
    if let Some(p) = edits.priority {
        task.priority = p;
    }
    if let Some(ref d) = edits.description {
        task.description = normalize_description(d);
    }
    match edits.parent {
        Some(Some(p)) => task.parent = Some(p),
        Some(None) => task.parent = None,
        None => {}
    }
    for label in &edits.add_labels {
        task.labels.insert(label.clone());
    }
    task.labels.retain(|l| !edits.rm_labels.contains(l));
    for &dep in &edits.add_deps {
        task.depends.insert(dep);
    }
    task.depends.retain(|d| !edits.rm_deps.contains(d));

    if task == original {
        return Ok((task, false));
    }

    let new_parent = matches!(edits.parent, Some(Some(_)));
    let needs_validation = new_parent || !edits.add_deps.is_empty();
    let needs_completion =
        !edits.force && task.status == Status::Done && original.status != Status::Done;

    if needs_validation || needs_completion {
        let all_tasks = store.load_all_tasks()?;
        let archived_ids = store.load_archived_ids()?;

        if needs_validation {
            let mut known_ids: HashSet<TaskId> = all_tasks.iter().map(|t| t.id).collect();
            known_ids.extend(&archived_ids);

            if let Some(Some(parent)) = edits.parent {
                validate_parent(&all_tasks, &known_ids, id, parent)?;
            }
            if !edits.add_deps.is_empty() {
                validate_deps_exist(&known_ids, &edits.add_deps)?;
                validate_dag(&all_tasks, Some(&task))?;
            }
        }

        if needs_completion {
            validate_completion(&all_tasks, &archived_ids, &task)?;
        }
    }

    task.modified = Utc::now();
    store.write_task_checked(&task, mtime)?;
    Ok((task, true))
}

pub fn load_task_context(
    store: &Store,
    id: TaskId,
    include_archived: bool,
) -> Result<TaskContext, Error> {
    let mut task = store.read_task(id)?;
    let all_tasks = store.load_all_tasks()?;
    let done_ids = resolved_ids(store, &all_tasks)?;
    task.depends.retain(|d| !done_ids.contains(d));

    let task_is_archived = !all_tasks.iter().any(|t| t.id == id);
    let parent = task
        .parent
        .and_then(|pid| all_tasks.iter().find(|t| t.id == pid).cloned());
    let deps: Vec<Task> = task
        .depends
        .iter()
        .filter_map(|dep_id| all_tasks.iter().find(|t| t.id == *dep_id).cloned())
        .collect();
    let archived_children: Vec<Task> = if include_archived || task_is_archived {
        store
            .load_archived_tasks()?
            .into_iter()
            .filter(|t| t.parent == Some(id))
            .collect()
    } else {
        Vec::new()
    };
    let mut children: Vec<Task> = all_tasks
        .iter()
        .filter(|t| t.parent == Some(id))
        .cloned()
        .chain(archived_children)
        .collect();
    children.sort_by(|a, b| a.sort_key(&done_ids).cmp(&b.sort_key(&done_ids)));

    Ok(TaskContext {
        task,
        parent,
        deps,
        children,
        done_ids,
    })
}

pub fn get_ready_tasks(store: &Store) -> Result<ReadyTasks, Error> {
    let all_tasks = store.load_all_tasks()?;
    let done_ids = resolved_ids(store, &all_tasks)?;
    let has_incomplete_child: HashSet<TaskId> = all_tasks
        .iter()
        .filter(|t| !t.status.is_resolved())
        .filter_map(|t| t.parent)
        .collect();
    let mut ready: Vec<Task> = all_tasks
        .iter()
        .filter(|t| {
            t.status == Status::Todo
                && t.depends.iter().all(|d| done_ids.contains(d))
                && !has_incomplete_child.contains(&t.id)
        })
        .cloned()
        .collect();
    ready.sort_by(|a, b| a.sort_key(&done_ids).cmp(&b.sort_key(&done_ids)));
    Ok(ReadyTasks {
        ready,
        done_ids,
        all_tasks,
    })
}

pub fn filter_tasks(tasks: &[Task], status: Option<Status>, labels: &[String]) -> Vec<Task> {
    tasks
        .iter()
        .filter(|t| status.is_none_or(|s| t.status == s))
        .filter(|t| labels.is_empty() || t.labels.iter().any(|l| labels.contains(l)))
        .cloned()
        .collect()
}

pub fn children_map(all_tasks: &[Task]) -> HashMap<TaskId, Vec<TaskId>> {
    let mut map: HashMap<TaskId, Vec<TaskId>> = HashMap::new();
    for t in all_tasks {
        if let Some(pid) = t.parent {
            map.entry(pid).or_default().push(t.id);
        }
    }
    map
}

pub fn normalize_description(s: &str) -> Option<String> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}
