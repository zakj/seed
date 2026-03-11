use std::collections::HashSet;
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use fs2::FileExt;

use crate::error::Error;
use crate::task::{Task, TaskId, kdl_int};

pub struct Store {
    root: PathBuf,
}

impl Store {
    /// Walk up from `start` to find a `.seed/` directory.
    pub fn find(start: &Path) -> Result<Self, Error> {
        let mut dir = start.to_path_buf();
        loop {
            let candidate = dir.join(".seed");
            if candidate.is_dir() {
                return Ok(Self { root: candidate });
            }
            if !dir.pop() {
                return Err(Error::NotFound);
            }
        }
    }

    /// Initialize a new `.seed/` directory at the given path.
    pub fn init(at: &Path) -> Result<Self, Error> {
        let root = at.join(".seed");
        match fs::create_dir(&root) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                return Err(Error::AlreadyInitialized(root));
            }
            Err(e) => return Err(e.into()),
        }
        fs::create_dir(root.join("tasks"))?;
        fs::write(root.join("config.kdl"), "next-id 1\n")?;
        Ok(Self { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn tasks_dir(&self) -> PathBuf {
        self.root.join("tasks")
    }

    /// Latest mtime across tasks dir and config file.
    pub fn mtime(&self) -> Option<SystemTime> {
        let tasks_mtime = fs::metadata(self.tasks_dir())
            .and_then(|m| m.modified())
            .ok();
        let config_mtime = fs::metadata(self.config_path())
            .and_then(|m| m.modified())
            .ok();
        match (tasks_mtime, config_mtime) {
            (Some(a), Some(b)) => Some(a.max(b)),
            (a, b) => a.or(b),
        }
    }

    fn task_path(&self, id: TaskId) -> PathBuf {
        self.tasks_dir().join(format!("{id}.kdl"))
    }

    fn archive_dir(&self) -> PathBuf {
        self.root.join("archive")
    }

    fn archive_path(&self, id: TaskId) -> PathBuf {
        self.archive_dir().join(format!("{id}.kdl"))
    }

    fn config_path(&self) -> PathBuf {
        self.root.join("config.kdl")
    }

    /// Atomically allocate the next task ID under an exclusive file lock.
    pub fn allocate_id(&self) -> Result<TaskId, Error> {
        let file = File::open(self.config_path())?;
        file.lock_exclusive()?;
        let _unlock = LockGuard(&file);

        let mut content = String::new();
        (&file).read_to_string(&mut content)?;
        let mut doc: kdl::KdlDocument = content.parse()?;
        let node = doc
            .nodes_mut()
            .iter_mut()
            .find(|n| n.name().value() == "next-id")
            .ok_or_else(|| Error::InvalidConfig("missing next-id".into()))?;
        let raw = node
            .entries()
            .first()
            .and_then(|e| e.value().as_integer())
            .ok_or_else(|| Error::InvalidConfig("next-id is not an integer".into()))?;
        let id: u32 = raw
            .try_into()
            .map_err(|_| Error::InvalidConfig(format!("next-id out of range: {raw}")))?;
        let next = id
            .checked_add(1)
            .ok_or_else(|| Error::InvalidConfig("next-id overflow".into()))?;
        node.clear();
        node.push(kdl_int(TaskId::from(next)));
        atomic_write(&self.config_path(), &doc.to_string(), None)?;
        Ok(TaskId::from(id))
    }

    pub fn read_task(&self, id: TaskId) -> Result<Task, Error> {
        let path = self.task_path(id);
        let content = match fs::read_to_string(&path) {
            Ok(c) => c,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                fs::read_to_string(self.archive_path(id)).map_err(|e2| match e2.kind() {
                    std::io::ErrorKind::NotFound => Error::TaskNotFound(id),
                    _ => e2.into(),
                })?
            }
            Err(e) => return Err(e.into()),
        };
        let doc: kdl::KdlDocument = content.parse()?;
        Task::from_kdl(&doc)
    }

    /// Read a task along with its file's mtime for optimistic concurrency.
    /// Returns `TaskArchived` if the task only exists in the archive,
    /// since archived tasks cannot be written back to the tasks directory.
    pub fn read_task_with_mtime(&self, id: TaskId) -> Result<(Task, SystemTime), Error> {
        let path = self.task_path(id);
        let mut file = match File::open(&path) {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return match File::open(self.archive_path(id)) {
                    Ok(_) => Err(Error::TaskArchived(id)),
                    Err(_) => Err(Error::TaskNotFound(id)),
                };
            }
            Err(e) => return Err(e.into()),
        };
        let mtime = file.metadata()?.modified()?;
        let mut content = String::new();
        file.read_to_string(&mut content)?;
        let doc: kdl::KdlDocument = content.parse()?;
        let task = Task::from_kdl(&doc)?;
        Ok((task, mtime))
    }

    pub fn write_task(&self, task: &Task) -> Result<(), Error> {
        let path = self.task_path(task.id);
        let doc = task.to_kdl();
        atomic_write(&path, &doc.to_string(), None)
    }

    /// Write a task, but only if the file hasn't been modified since `expected_mtime`.
    pub fn write_task_checked(&self, task: &Task, expected_mtime: SystemTime) -> Result<(), Error> {
        let path = self.task_path(task.id);
        let doc = task.to_kdl();
        atomic_write(&path, &doc.to_string(), Some((task.id, expected_mtime)))
    }

    pub fn ensure_archive_dir(&self) -> Result<(), Error> {
        fs::create_dir_all(self.archive_dir())?;
        Ok(())
    }

    pub fn archive_task(&self, id: TaskId) -> Result<(), Error> {
        fs::rename(self.task_path(id), self.archive_path(id))?;
        Ok(())
    }

    pub fn load_all_tasks(&self) -> Result<Vec<Task>, Error> {
        Self::load_tasks_from(&self.tasks_dir())
    }

    pub fn load_archived_tasks(&self) -> Result<Vec<Task>, Error> {
        Self::load_tasks_from(&self.archive_dir())
    }

    pub fn load_archived_ids(&self) -> Result<HashSet<TaskId>, Error> {
        let entries = match fs::read_dir(self.archive_dir()) {
            Ok(rd) => rd.collect::<Result<Vec<_>, _>>()?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(HashSet::new()),
            Err(e) => return Err(e.into()),
        };
        Ok(entries
            .into_iter()
            .filter_map(|e| e.path().file_stem()?.to_str()?.parse::<TaskId>().ok())
            .collect())
    }

    fn load_tasks_from(dir: &Path) -> Result<Vec<Task>, Error> {
        let entries = match fs::read_dir(dir) {
            Ok(rd) => rd.collect::<Result<Vec<_>, _>>()?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(e.into()),
        };
        let mut tasks: Vec<Task> = entries
            .into_iter()
            .filter(|entry| entry.path().extension().is_some_and(|e| e == "kdl"))
            .map(|entry| {
                let content = fs::read_to_string(entry.path())?;
                let doc: kdl::KdlDocument = content.parse()?;
                Task::from_kdl(&doc)
            })
            .collect::<Result<_, _>>()?;
        tasks.sort_by_key(|t| t.id);
        Ok(tasks)
    }
}

struct LockGuard<'a>(&'a File);

impl Drop for LockGuard<'_> {
    fn drop(&mut self) {
        let _ = self.0.unlock();
    }
}

fn atomic_write(
    path: &Path,
    content: &str,
    check: Option<(TaskId, SystemTime)>,
) -> Result<(), Error> {
    let dir = path.parent().expect("task path has parent");
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("tmp");
    let tmp = dir.join(format!(".tmp.{}.{}", stem, std::process::id()));
    fs::write(&tmp, content)?;
    let result = (|| {
        // TOCTOU: mtime check and rename aren't atomic, but acceptable for a
        // single-user CLI—just guards against clobbering concurrent edits.
        if let Some((task_id, expected_mtime)) = check {
            let current_mtime = fs::metadata(path)?.modified()?;
            if current_mtime != expected_mtime {
                return Err(Error::Conflict(task_id));
            }
        }
        fs::rename(&tmp, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}
