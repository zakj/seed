use std::collections::{BTreeSet, HashMap, HashSet};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TaskId(u32);

impl TaskId {
    pub fn as_u32(self) -> u32 {
        self.0
    }
}

impl From<u32> for TaskId {
    fn from(v: u32) -> Self {
        Self(v)
    }
}

impl std::fmt::Display for TaskId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for TaskId {
    type Err = std::num::ParseIntError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        s.parse::<u32>().map(Self)
    }
}

pub fn kdl_int(v: TaskId) -> kdl::KdlEntry {
    kdl::KdlEntry::new(i128::from(v.as_u32()))
}

pub struct Style {
    pub symbol: &'static str,
    pub label: &'static str,
    pub color: anstyle::Style,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, clap::ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum Status {
    #[value(name = "todo")]
    Todo,
    #[value(name = "in-progress")]
    InProgress,
    #[value(name = "done")]
    Done,
    #[value(name = "dropped")]
    Dropped,
}

impl Status {
    pub const fn style(self) -> Style {
        use anstyle::AnsiColor;
        match self {
            Self::Todo => Style {
                symbol: " ",
                label: "todo",
                color: AnsiColor::Yellow.on_default(),
            },
            Self::InProgress => Style {
                symbol: "●",
                label: "in-progress",
                color: AnsiColor::Blue.on_default(),
            },
            Self::Done => Style {
                symbol: "✓",
                label: "done",
                color: AnsiColor::Green.on_default(),
            },
            Self::Dropped => Style {
                symbol: "×",
                label: "dropped",
                color: anstyle::Style::new().dimmed(),
            },
        }
    }

    pub const fn as_str(self) -> &'static str {
        self.style().label
    }

    pub const fn is_resolved(self) -> bool {
        matches!(self, Self::Done | Self::Dropped)
    }

    const fn sort_rank(self, blocked: bool) -> u8 {
        match (self, blocked) {
            (Self::InProgress, _) => 0,
            (Self::Todo, false) => 1,
            (Self::Todo, true) => 2,
            (Self::Done, _) => 3,
            (Self::Dropped, _) => 3,
        }
    }
}

impl std::str::FromStr for Status {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "todo" => Ok(Self::Todo),
            "in-progress" => Ok(Self::InProgress),
            "done" => Ok(Self::Done),
            "dropped" | "cancelled" => Ok(Self::Dropped),
            other => Err(format!("unknown status: {other}")),
        }
    }
}

impl std::fmt::Display for Status {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str((*self).as_str())
    }
}

#[derive(
    Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, clap::ValueEnum,
)]
#[serde(rename_all = "kebab-case")]
pub enum Priority {
    #[value(name = "critical")]
    Critical,
    #[value(name = "high")]
    High,
    #[default]
    #[value(name = "normal")]
    Normal,
    #[value(name = "low")]
    Low,
}

impl Priority {
    pub const fn style(self) -> Style {
        use anstyle::AnsiColor;
        match self {
            Self::Critical => Style {
                symbol: "!",
                label: "critical",
                color: AnsiColor::Red.on_default(),
            },
            Self::High => Style {
                symbol: "↑",
                label: "high",
                color: AnsiColor::Yellow.on_default(),
            },
            Self::Normal => Style {
                symbol: " ",
                label: "normal",
                color: anstyle::Style::new(),
            },
            Self::Low => Style {
                symbol: "↓",
                label: "low",
                color: anstyle::Style::new().dimmed(),
            },
        }
    }

    pub const fn as_str(self) -> &'static str {
        self.style().label
    }

    pub const fn is_default(&self) -> bool {
        matches!(self, Self::Normal)
    }
}

impl std::str::FromStr for Priority {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "critical" => Ok(Self::Critical),
            "high" => Ok(Self::High),
            "normal" => Ok(Self::Normal),
            "low" => Ok(Self::Low),
            other => Err(format!("unknown priority: {other}")),
        }
    }
}

impl std::fmt::Display for Priority {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str((*self).as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum Agent {
    #[value(name = "claude")]
    Claude,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LogEntry {
    pub timestamp: DateTime<Utc>,
    // Free-form string, not Option<Agent>: the Agent enum exists only for
    // --install (per-agent hook setup); log entries accept any agent name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Task {
    pub id: TaskId,
    pub title: String,
    pub status: Status,
    #[serde(skip_serializing_if = "Priority::is_default")]
    pub priority: Priority,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "BTreeSet::is_empty")]
    pub labels: BTreeSet<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent: Option<TaskId>,
    #[serde(skip_serializing_if = "BTreeSet::is_empty")]
    pub depends: BTreeSet<TaskId>,
    pub created: DateTime<Utc>,
    pub modified: DateTime<Utc>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub log: Vec<LogEntry>,
}

impl Task {
    pub fn is_blocked(&self, done_ids: &HashSet<TaskId>) -> bool {
        self.status == Status::Todo
            && !self.depends.is_empty()
            && self.depends.iter().any(|d| !done_ids.contains(d))
    }

    pub fn sort_key(&self, done_ids: &HashSet<TaskId>) -> impl Ord {
        (
            self.status.sort_rank(self.is_blocked(done_ids)),
            self.priority,
            self.id,
        )
    }

    pub fn new(id: TaskId, title: String) -> Self {
        let now = Utc::now();
        Self {
            id,
            title,
            status: Status::Todo,
            priority: Priority::default(),
            description: None,
            labels: BTreeSet::new(),
            parent: None,
            depends: BTreeSet::new(),
            created: now,
            modified: now,
            log: Vec::new(),
        }
    }

    pub fn to_kdl(&self) -> kdl::KdlDocument {
        let mut task_node = kdl::KdlNode::new("task");
        task_node.push(kdl::KdlEntry::new_prop("id", i128::from(self.id.as_u32())));
        task_node.push(kdl::KdlEntry::new_prop("status", self.status.as_str()));
        if !self.priority.is_default() {
            task_node.push(kdl::KdlEntry::new_prop("priority", self.priority.as_str()));
        }

        let children = task_node.ensure_children();

        let mut title_node = kdl::KdlNode::new("title");
        title_node.push(kdl::KdlEntry::new(self.title.clone()));
        children.nodes_mut().push(title_node);

        if let Some(desc) = &self.description {
            let mut desc_node = kdl::KdlNode::new("description");
            desc_node.push(kdl::KdlEntry::new(desc.clone()));
            children.nodes_mut().push(desc_node);
        }

        if !self.labels.is_empty() {
            let mut labels_node = kdl::KdlNode::new("labels");
            for label in &self.labels {
                labels_node.push(kdl::KdlEntry::new(label.clone()));
            }
            children.nodes_mut().push(labels_node);
        }

        if let Some(parent) = self.parent {
            let mut parent_node = kdl::KdlNode::new("parent");
            parent_node.push(kdl_int(parent));
            children.nodes_mut().push(parent_node);
        }

        if !self.depends.is_empty() {
            let mut depends_node = kdl::KdlNode::new("depends");
            for dep in &self.depends {
                depends_node.push(kdl_int(*dep));
            }
            children.nodes_mut().push(depends_node);
        }

        let mut created_node = kdl::KdlNode::new("created");
        created_node.push(kdl::KdlEntry::new(self.created.to_rfc3339()));
        children.nodes_mut().push(created_node);

        let mut modified_node = kdl::KdlNode::new("modified");
        modified_node.push(kdl::KdlEntry::new(self.modified.to_rfc3339()));
        children.nodes_mut().push(modified_node);

        if !self.log.is_empty() {
            let mut log_node = kdl::KdlNode::new("log");
            let log_children = log_node.ensure_children();
            for entry in &self.log {
                let mut entry_node = kdl::KdlNode::new("entry");
                entry_node.push(kdl::KdlEntry::new_prop("ts", entry.timestamp.to_rfc3339()));
                if let Some(agent) = &entry.agent {
                    entry_node.push(kdl::KdlEntry::new_prop("agent", agent.clone()));
                }
                entry_node.push(kdl::KdlEntry::new(entry.message.clone()));
                log_children.nodes_mut().push(entry_node);
            }
            children.nodes_mut().push(log_node);
        }

        let mut doc = kdl::KdlDocument::new();
        doc.nodes_mut().push(task_node);
        doc.autoformat();
        doc
    }

    pub fn from_kdl(doc: &kdl::KdlDocument) -> Result<Self, Error> {
        let task_node = doc
            .nodes()
            .iter()
            .find(|n| n.name().value() == "task")
            .ok_or_else(|| Error::InvalidTaskFile("missing task node".into()))?;

        let id = get_prop_task_id(task_node, "id")?;
        let status: Status = get_prop_str(task_node, "status")?
            .parse()
            .map_err(Error::InvalidTaskFile)?;
        let priority = match task_node.get("priority").and_then(|v| v.as_string()) {
            Some(s) => s.parse().map_err(Error::InvalidTaskFile)?,
            None => Priority::default(),
        };

        let children = task_node
            .children()
            .ok_or_else(|| Error::InvalidTaskFile("task node has no children".into()))?;

        let title = get_child_str(children, "title")?
            .ok_or_else(|| Error::InvalidTaskFile("missing title".into()))?
            .to_owned();

        let description = get_child_str(children, "description")?.map(|s| s.to_owned());

        let labels = get_child_strings(children, "labels");
        let parent = get_child_task_id(children, "parent")?;
        let depends = get_child_task_ids(children, "depends")?;

        let created = parse_datetime(
            get_child_str(children, "created")?
                .ok_or_else(|| Error::InvalidTaskFile("missing created".into()))?,
        )?;
        let modified = parse_datetime(
            get_child_str(children, "modified")?
                .ok_or_else(|| Error::InvalidTaskFile("missing modified".into()))?,
        )?;

        let log = parse_log_entries(children)?;

        Ok(Task {
            id,
            title,
            status,
            priority,
            description,
            labels,
            parent,
            depends,
            created,
            modified,
            log,
        })
    }
}

fn get_prop_task_id(node: &kdl::KdlNode, key: &str) -> Result<TaskId, Error> {
    let raw = node
        .get(key)
        .and_then(|v| v.as_integer())
        .ok_or_else(|| Error::InvalidTaskFile(format!("missing or invalid property: {key}")))?;
    let v: u32 = raw
        .try_into()
        .map_err(|_| Error::InvalidTaskFile(format!("{key} out of range: {raw}")))?;
    Ok(TaskId::from(v))
}

fn get_prop_str<'a>(node: &'a kdl::KdlNode, key: &str) -> Result<&'a str, Error> {
    node.get(key)
        .and_then(|v| v.as_string())
        .ok_or_else(|| Error::InvalidTaskFile(format!("missing or invalid property: {key}")))
}

fn get_child_str<'a>(doc: &'a kdl::KdlDocument, name: &str) -> Result<Option<&'a str>, Error> {
    match doc.nodes().iter().find(|n| n.name().value() == name) {
        Some(node) => {
            let val = node
                .entries()
                .first()
                .and_then(|e| e.value().as_string())
                .ok_or_else(|| {
                    Error::InvalidTaskFile(format!("{name} node missing string value"))
                })?;
            Ok(Some(val))
        }
        None => Ok(None),
    }
}

fn get_child_strings(doc: &kdl::KdlDocument, name: &str) -> BTreeSet<String> {
    doc.nodes()
        .iter()
        .find(|n| n.name().value() == name)
        .map(|node| {
            node.entries()
                .iter()
                .filter_map(|e| e.value().as_string().map(|s| s.to_owned()))
                .collect()
        })
        .unwrap_or_default()
}

fn get_child_task_id(doc: &kdl::KdlDocument, name: &str) -> Result<Option<TaskId>, Error> {
    match doc.nodes().iter().find(|n| n.name().value() == name) {
        Some(node) => {
            let raw = node
                .entries()
                .first()
                .and_then(|e| e.value().as_integer())
                .ok_or_else(|| {
                    Error::InvalidTaskFile(format!("{name} node missing integer value"))
                })?;
            let v: u32 = raw
                .try_into()
                .map_err(|_| Error::InvalidTaskFile(format!("{name} out of range: {raw}")))?;
            Ok(Some(TaskId::from(v)))
        }
        None => Ok(None),
    }
}

fn get_child_task_ids(doc: &kdl::KdlDocument, name: &str) -> Result<BTreeSet<TaskId>, Error> {
    match doc.nodes().iter().find(|n| n.name().value() == name) {
        Some(node) => node
            .entries()
            .iter()
            .map(|e| {
                let raw = e.value().as_integer().ok_or_else(|| {
                    Error::InvalidTaskFile(format!("{name} has non-integer value"))
                })?;
                let v: u32 = raw.try_into().map_err(|_| {
                    Error::InvalidTaskFile(format!("{name} value out of range: {raw}"))
                })?;
                Ok(TaskId::from(v))
            })
            .collect(),
        None => Ok(BTreeSet::new()),
    }
}

fn parse_datetime(s: &str) -> Result<DateTime<Utc>, Error> {
    s.parse::<DateTime<Utc>>()
        .map_err(|e| Error::InvalidTaskFile(format!("invalid datetime: {e}")))
}

fn parse_log_entries(doc: &kdl::KdlDocument) -> Result<Vec<LogEntry>, Error> {
    let Some(log_node) = doc.nodes().iter().find(|n| n.name().value() == "log") else {
        return Ok(Vec::new());
    };
    let Some(log_children) = log_node.children() else {
        return Ok(Vec::new());
    };
    log_children
        .nodes()
        .iter()
        .filter(|n| n.name().value() == "entry")
        .map(|node| {
            let timestamp = parse_datetime(get_prop_str(node, "ts")?)?;
            let agent = node
                .get("agent")
                .and_then(|v| v.as_string())
                .map(|s| s.to_owned());
            let message = node
                .entries()
                .iter()
                .find(|e| e.name().is_none())
                .and_then(|e| e.value().as_string())
                .ok_or_else(|| Error::InvalidTaskFile("log entry missing message".into()))?
                .to_owned();
            Ok(LogEntry {
                timestamp,
                agent,
                message,
            })
        })
        .collect()
}

pub fn validate_deps_exist(known_ids: &HashSet<TaskId>, deps: &[TaskId]) -> Result<(), Error> {
    if let Some(dep) = deps.iter().find(|d| !known_ids.contains(d)) {
        return Err(Error::TaskNotFound(*dep));
    }
    Ok(())
}

pub fn validate_parent(
    tasks: &[Task],
    known_ids: &HashSet<TaskId>,
    id: TaskId,
    parent: TaskId,
) -> Result<(), Error> {
    if !known_ids.contains(&parent) {
        return Err(Error::TaskNotFound(parent));
    }
    // Walk the parent chain to detect cycles
    let parent_map: HashMap<TaskId, TaskId> = tasks
        .iter()
        .filter_map(|t| t.parent.map(|p| (t.id, p)))
        .collect();
    let mut visited = HashSet::new();
    visited.insert(id);
    let mut current = parent;
    while visited.insert(current) {
        match parent_map.get(&current) {
            Some(&p) => current = p,
            None => return Ok(()),
        }
    }
    Err(Error::CycleDetected)
}

/// DFS cycle detection over the dependency graph.
/// When `overlay` is provided, it is inserted into (or replaces) the dep map
/// so callers can validate a new/edited task without cloning the full task list.
pub fn validate_dag(tasks: &[Task], overlay: Option<&Task>) -> Result<(), Error> {
    let mut dep_map: HashMap<TaskId, &BTreeSet<TaskId>> =
        tasks.iter().map(|t| (t.id, &t.depends)).collect();
    if let Some(t) = overlay {
        dep_map.insert(t.id, &t.depends);
    }
    let mut visited = HashSet::new();
    let mut in_stack = HashSet::new();

    for &id in dep_map.keys() {
        if !visited.contains(&id) && has_cycle(id, &dep_map, &mut visited, &mut in_stack) {
            return Err(Error::CycleDetected);
        }
    }
    Ok(())
}

fn has_cycle(
    id: TaskId,
    dep_map: &HashMap<TaskId, &BTreeSet<TaskId>>,
    visited: &mut HashSet<TaskId>,
    in_stack: &mut HashSet<TaskId>,
) -> bool {
    visited.insert(id);
    in_stack.insert(id);

    if let Some(deps) = dep_map.get(&id) {
        for &dep in *deps {
            if !visited.contains(&dep) {
                if has_cycle(dep, dep_map, visited, in_stack) {
                    return true;
                }
            } else if in_stack.contains(&dep) {
                return true;
            }
        }
    }

    in_stack.remove(&id);
    false
}
