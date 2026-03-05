use std::path::PathBuf;

use crate::task::{Status, TaskId};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("not a seed project (no .seed directory found)")]
    NotFound,

    #[error("already initialized: {0}")]
    AlreadyInitialized(PathBuf),

    #[error("task {0} not found")]
    TaskNotFound(TaskId),

    #[error("task {0} is archived")]
    TaskArchived(TaskId),

    #[error("conflict: task {0} was modified by another process")]
    Conflict(TaskId),

    #[error("cycle detected: this would create a circular reference")]
    CycleDetected,

    #[error("unmet dependencies: tasks {0:?} are not done")]
    UnmetDependencies(Vec<TaskId>),

    #[error("incomplete children: tasks {0:?} are not done")]
    IncompleteChildren(Vec<TaskId>),

    #[error("cannot start task {0}: task is {1}")]
    CannotStart(TaskId, Status),

    #[error("cannot cancel task {0}: task is done")]
    CannotCancel(TaskId),

    #[error("invalid task file: {0}")]
    InvalidTaskFile(String),

    #[error("invalid duration: {0}")]
    InvalidDuration(String),

    #[error("invalid config: {0}")]
    InvalidConfig(String),

    #[error("$VISUAL or $EDITOR must be set")]
    NoEditor,

    #[error("editor exited with {0}")]
    EditorFailed(std::process::ExitStatus),

    #[error("{0}")]
    Io(#[from] std::io::Error),

    #[error("KDL parse error: {0}")]
    Kdl(#[from] kdl::KdlError),
}
