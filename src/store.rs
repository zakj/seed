use std::collections::HashSet;
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::error::Error;
use crate::task::{Task, TaskId};

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
        Ok(Self { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn tasks_dir(&self) -> PathBuf {
        self.root.join("tasks")
    }

    /// Latest mtime of the tasks directory.
    pub fn mtime(&self) -> Option<SystemTime> {
        fs::metadata(self.tasks_dir())
            .and_then(|m| m.modified())
            .ok()
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

    /// Derive the next task ID by scanning tasks/ and archive/ for the highest existing ID.
    pub fn next_id(&self) -> TaskId {
        let max = Self::max_id_in(&self.tasks_dir()).max(Self::max_id_in(&self.archive_dir()));
        TaskId::from(max.checked_add(1).expect("task ID overflow"))
    }

    /// Write a new task file, failing if a file for this ID already exists.
    pub fn create_task_file(&self, task: &Task) -> Result<(), Error> {
        use std::io::Write;
        let path = self.task_path(task.id);
        let content = task.to_kdl().to_string();
        match File::create_new(&path) {
            Ok(mut file) => Ok(file.write_all(content.as_bytes())?),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                Err(Error::Conflict(task.id))
            }
            Err(e) => Err(e.into()),
        }
    }

    fn max_id_in(dir: &Path) -> u32 {
        let entries = match fs::read_dir(dir) {
            Ok(rd) => rd,
            Err(_) => return 0,
        };
        entries
            .filter_map(|e| e.ok())
            .filter_map(|e| {
                e.file_name()
                    .to_str()?
                    .strip_suffix(".kdl")?
                    .parse::<u32>()
                    .ok()
            })
            .max()
            .unwrap_or(0)
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

    /// Remove a task file entirely. Only safe for childless tasks with no dependents.
    pub fn delete_task(&self, id: TaskId) -> Result<(), Error> {
        fs::remove_file(self.task_path(id))?;
        Ok(())
    }

    pub fn load_all_tasks(&self) -> Result<Vec<Task>, Error> {
        Self::load_tasks_from(&self.tasks_dir())
    }

    pub fn load_tasks(&self, include_archived: bool) -> Result<Vec<Task>, Error> {
        let mut tasks = self.load_all_tasks()?;
        if include_archived {
            tasks.extend(self.load_archived_tasks()?);
            tasks.sort_by_key(|t| t.id);
        }
        Ok(tasks)
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
