mod error;
mod format;
mod markdown;
mod store;
mod task;
mod term;

use std::collections::{HashMap, HashSet};
use std::env;
use std::fs;
use std::io::Write;

use chrono::Utc;
use clap::{CommandFactory, Parser, Subcommand};

use error::Error;
use store::Store;
use task::{Priority, Status, Task, validate_dag, validate_deps_exist, validate_parent};

#[derive(Parser)]
#[command(
    name = "sd",
    about = "A task tracker for AI coding agents and humans",
    version
)]
struct Cli {
    /// Output as JSON
    #[arg(long, global = true)]
    json: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Initialize a new seed project
    Init,

    /// Add a new task
    Add(AddArgs),

    /// Show task details
    Show {
        /// Task ID
        id: u32,

        /// Include archived children
        #[arg(short = 'a', long)]
        include_archived: bool,
    },

    /// List tasks
    #[command(alias = "ls")]
    List {
        /// Show flat list instead of tree
        #[arg(long)]
        flat: bool,

        /// Filter by status
        #[arg(short, long)]
        status: Option<Status>,

        /// Filter by label (tasks matching any given label)
        #[arg(short, long)]
        label: Vec<String>,

        /// Include archived tasks
        #[arg(short = 'a', long)]
        include_archived: bool,
    },

    /// Edit a task (opens $EDITOR with no flags)
    Edit(EditArgs),

    /// Start a task (set status to in-progress)
    Start {
        /// Task ID
        id: u32,
    },

    /// Mark a task as done
    #[command(
        long_about = "Mark a task as done.\n\nValidates that all dependencies are resolved and all children are \
complete. Use --force to skip validation."
    )]
    Done {
        /// Task ID
        id: u32,

        /// Force completion even with unmet dependencies or incomplete children
        #[arg(short, long)]
        force: bool,
    },

    /// Cancel a task
    Cancel {
        /// Task ID
        id: u32,
    },

    /// Append a log entry to a task
    Log {
        /// Task ID
        id: u32,

        /// Log message
        message: String,

        /// Tag the entry with an agent name (e.g. "claude")
        #[arg(long)]
        agent: Option<String>,
    },

    /// Output markdown context dump for agent priming
    #[command(hide = true)]
    Prime {
        /// Install agent hooks (e.g. "claude")
        #[arg(long)]
        install: Option<String>,
    },

    /// Show tasks ready to work on
    #[command(
        long_about = "Show tasks ready to work on.\n\nLists todo tasks whose dependencies are all resolved and \
whose children are all complete. Sorted by priority."
    )]
    Next,

    /// Archive resolved tasks
    #[command(
        long_about = "Archive resolved tasks.\n\nMoves done and cancelled tasks from .seed/tasks/ to \
.seed/archive/. Without a cutoff, archives all resolved tasks."
    )]
    Archive {
        /// Only archive tasks resolved longer ago than this duration (e.g. "7d", "2w")
        cutoff: Option<String>,
    },

    /// Generate shell completions
    #[command(hide = true)]
    Completions { shell: clap_complete::Shell },
}

#[derive(clap::Args)]
struct AddArgs {
    /// Task title
    title: String,

    /// Priority
    #[arg(short, long)]
    priority: Option<Priority>,

    /// Labels
    #[arg(short, long)]
    label: Vec<String>,

    /// Parent task ID
    #[arg(long)]
    parent: Option<u32>,

    /// Dependencies (task IDs)
    #[arg(long)]
    dep: Vec<u32>,

    /// Description
    #[arg(short, long)]
    description: Option<String>,

    /// Only output the task ID
    #[arg(short, long)]
    quiet: bool,
}

#[derive(clap::Args)]
struct EditArgs {
    /// Task ID
    id: u32,

    #[command(flatten)]
    mods: EditModifications,
}

#[derive(clap::Args, Default, PartialEq)]
struct EditModifications {
    /// New title
    #[arg(short, long)]
    title: Option<String>,

    /// New status
    #[arg(short, long)]
    status: Option<Status>,

    /// New priority
    #[arg(short, long)]
    priority: Option<Priority>,

    /// New description
    #[arg(short, long)]
    description: Option<String>,

    /// New parent task ID
    #[arg(long)]
    parent: Option<u32>,

    /// Remove parent
    #[arg(long, conflicts_with = "parent")]
    no_parent: bool,

    /// Add a label
    #[arg(long)]
    add_label: Vec<String>,

    /// Remove a label
    #[arg(long)]
    rm_label: Vec<String>,

    /// Add a dependency
    #[arg(long)]
    add_dep: Vec<u32>,

    /// Remove a dependency
    #[arg(long)]
    rm_dep: Vec<u32>,

    /// Force completion even with unmet dependencies or incomplete children
    #[arg(short, long, requires = "status")]
    force: bool,
}

fn main() {
    let cli = Cli::parse();
    if let Err(e) = run(&cli) {
        if cli.json {
            let err = serde_json::json!({ "error": e.to_string() });
            eprintln!("{err}");
        } else {
            eprintln!("error: {e}");
        }
        std::process::exit(1);
    }
}

fn run(cli: &Cli) -> Result<(), Error> {
    match &cli.command {
        Command::Init => cmd_init(cli),
        Command::Add(args) => cmd_add(cli, args),
        Command::Show {
            id,
            include_archived,
        } => cmd_show(cli, *id, *include_archived),
        Command::List {
            flat,
            status,
            label,
            include_archived,
        } => cmd_list(cli, *flat, *status, label, *include_archived),
        Command::Edit(args) => {
            if args.mods != EditModifications::default() {
                cmd_edit(cli, args)
            } else {
                cmd_edit_interactive(cli, args)
            }
        }
        Command::Start { id } => cmd_start(cli, *id),
        Command::Done { id, force } => cmd_done(cli, *id, *force),
        Command::Cancel { id } => cmd_cancel(cli, *id),
        Command::Log { id, message, agent } => cmd_log(cli, *id, message, agent.as_deref()),
        Command::Prime { install } => match install {
            Some(agent) => cmd_prime_install(agent),
            None => cmd_prime(),
        },
        Command::Next => cmd_next(cli),
        Command::Archive { cutoff } => cmd_archive(cli, cutoff.as_deref()),
        Command::Completions { shell } => {
            clap_complete::generate(*shell, &mut Cli::command(), "sd", &mut std::io::stdout());
            Ok(())
        }
    }
}

fn cmd_init(cli: &Cli) -> Result<(), Error> {
    let cwd = env::current_dir()?;
    let store = Store::init(&cwd)?;
    if cli.json {
        let out = serde_json::json!({ "path": store.root() });
        println!("{out}");
    } else {
        println!("Initialized seed project at {}", store.root().display());
    }
    Ok(())
}

fn cmd_add(cli: &Cli, args: &AddArgs) -> Result<(), Error> {
    let cwd = env::current_dir()?;
    let store = Store::find(&cwd)?;
    let id = store.allocate_id()?;

    let mut task = Task::new(id, args.title.clone());
    task.priority = args.priority.unwrap_or_default();
    task.labels = args.label.clone();
    task.parent = args.parent;
    task.depends = args.dep.clone();
    task.description = args.description.as_deref().and_then(normalize_description);

    if args.parent.is_some() || !args.dep.is_empty() {
        let all_tasks = store.load_all_tasks()?;
        let mut known_ids: HashSet<u32> = all_tasks.iter().map(|t| t.id).collect();
        known_ids.extend(store.load_archived_ids()?);

        if let Some(parent) = args.parent {
            validate_parent(&all_tasks, &known_ids, id, parent)?;
        }
        if !args.dep.is_empty() {
            validate_deps_exist(&known_ids, &args.dep)?;
            let mut tasks_with_new = all_tasks;
            tasks_with_new.push(task.clone());
            validate_dag(&tasks_with_new)?;
        }
    }

    store.write_task(&task)?;

    if cli.json {
        println!("{}", serde_json::to_string_pretty(&task).unwrap());
    } else if args.quiet {
        println!("{id}");
    } else {
        println!("Created task {id}: {}", args.title);
    }
    Ok(())
}

fn cmd_show(cli: &Cli, id: u32, include_archived: bool) -> Result<(), Error> {
    let cwd = env::current_dir()?;
    let store = Store::find(&cwd)?;
    let mut task = store.read_task(id)?;

    let all_tasks = store.load_all_tasks()?;
    let mut done_ids: HashSet<u32> = all_tasks
        .iter()
        .filter(|t| t.status.is_resolved())
        .map(|t| t.id)
        .collect();
    done_ids.extend(store.load_archived_ids()?);
    // Strip resolved deps so agents don't see false blockers.
    task.depends.retain(|d| !done_ids.contains(d));

    let task_is_archived = !all_tasks.iter().any(|t| t.id == id);
    let parent_task = task
        .parent
        .and_then(|pid| all_tasks.iter().find(|t| t.id == pid).cloned());
    let dep_tasks: Vec<Task> = task
        .depends
        .iter()
        .filter_map(|id| all_tasks.iter().find(|t| t.id == *id).cloned())
        .collect();
    let mut children: Vec<Task> = all_tasks
        .into_iter()
        .filter(|t| t.parent == Some(id))
        .collect();
    if include_archived || task_is_archived {
        children.extend(
            store
                .load_archived_tasks()?
                .into_iter()
                .filter(|t| t.parent == Some(id)),
        );
    }
    children.sort_by(|a, b| a.sort_key(&done_ids).cmp(&b.sort_key(&done_ids)));

    if cli.json {
        let mut value = serde_json::to_value(&task).unwrap();
        if !children.is_empty() {
            let ids: Vec<u32> = children.iter().map(|c| c.id).collect();
            value["children"] = serde_json::to_value(ids).unwrap();
        }
        println!("{}", serde_json::to_string_pretty(&value).unwrap());
    } else {
        let width = terminal_size::terminal_size().map(|(w, _)| w.0 as usize);
        print!(
            "{}",
            format::format_task_detail(
                &task,
                parent_task.as_ref(),
                &dep_tasks,
                &children,
                &done_ids,
                width,
            )
        );
    }
    Ok(())
}

fn cmd_list(
    cli: &Cli,
    flat: bool,
    status: Option<Status>,
    labels: &[String],
    include_archived: bool,
) -> Result<(), Error> {
    let cwd = env::current_dir()?;
    let store = Store::find(&cwd)?;
    let mut tasks = store.load_all_tasks()?;
    if include_archived {
        tasks.extend(store.load_archived_tasks()?);
        tasks.sort_by_key(|t| t.id);
    }
    let mut done_ids: HashSet<u32> = tasks
        .iter()
        .filter(|t| t.status.is_resolved())
        .map(|t| t.id)
        .collect();
    if !include_archived {
        done_ids.extend(store.load_archived_ids()?);
    }

    let filtered: Vec<Task>;
    let needs_filter = status.is_some() || !labels.is_empty();
    let display = if needs_filter {
        filtered = tasks
            .iter()
            .filter(|t| status.is_none_or(|s| t.status == s))
            .filter(|t| labels.is_empty() || t.labels.iter().any(|l| labels.contains(l)))
            .cloned()
            .collect();
        &filtered
    } else {
        &tasks
    };
    if cli.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&prepare_json(display, &done_ids)).unwrap()
        );
    } else {
        print!("{}", format::format_task_list(display, flat, &done_ids));
    }
    Ok(())
}

fn cmd_edit(cli: &Cli, args: &EditArgs) -> Result<(), Error> {
    let cwd = env::current_dir()?;
    let store = Store::find(&cwd)?;
    let (mut task, mtime) = store.read_task_with_mtime(args.id)?;
    let original = task.clone();

    if let Some(t) = &args.mods.title {
        task.title = t.clone();
    }
    if let Some(s) = args.mods.status {
        task.status = s;
    }
    if let Some(p) = args.mods.priority {
        task.priority = p;
    }
    if let Some(d) = &args.mods.description {
        task.description = normalize_description(d);
    }
    if let Some(p) = args.mods.parent {
        task.parent = Some(p);
    }
    if args.mods.no_parent {
        task.parent = None;
    }
    for label in &args.mods.add_label {
        if !task.labels.contains(label) {
            task.labels.push(label.clone());
        }
    }
    task.labels.retain(|l| !args.mods.rm_label.contains(l));
    for dep in &args.mods.add_dep {
        if !task.depends.contains(dep) {
            task.depends.push(*dep);
        }
    }
    task.depends.retain(|d| !args.mods.rm_dep.contains(d));

    if task == original {
        print_task(cli, &task, format_args!("No changes to task {}", args.id));
        return Ok(());
    }

    let needs_validation = args.mods.parent.is_some() || !args.mods.add_dep.is_empty();
    let needs_completion =
        !args.mods.force && task.status == Status::Done && original.status != Status::Done;

    if needs_validation || needs_completion {
        let all_tasks = store.load_all_tasks()?;
        let archived_ids = store.load_archived_ids()?;

        if needs_validation {
            let mut known_ids: HashSet<u32> = all_tasks.iter().map(|t| t.id).collect();
            known_ids.extend(&archived_ids);

            if let Some(parent) = args.mods.parent {
                validate_parent(&all_tasks, &known_ids, args.id, parent)?;
            }
            if !args.mods.add_dep.is_empty() {
                validate_deps_exist(&known_ids, &args.mods.add_dep)?;
                let mut tasks_with_updated: Vec<Task> = all_tasks
                    .iter()
                    .filter(|t| t.id != args.id)
                    .cloned()
                    .collect();
                tasks_with_updated.push(task.clone());
                validate_dag(&tasks_with_updated)?;
            }
        }

        if needs_completion {
            validate_completion(&all_tasks, &archived_ids, &task)?;
        }
    }

    task.modified = Utc::now();
    store.write_task_checked(&task, mtime)?;
    print_task(cli, &task, format_args!("Updated task {}", args.id));
    Ok(())
}

fn cmd_edit_interactive(cli: &Cli, args: &EditArgs) -> Result<(), Error> {
    let editor = env::var("VISUAL")
        .or_else(|_| env::var("EDITOR"))
        .map_err(|_| Error::NoEditor)?;

    let cwd = env::current_dir()?;
    let store = Store::find(&cwd)?;
    let (mut task, mtime) = store.read_task_with_mtime(args.id)?;

    let original = task.description.as_deref().unwrap_or("");
    let mut tmpfile = tempfile::Builder::new().suffix(".md").tempfile()?;
    tmpfile.write_all(original.as_bytes())?;

    let status = std::process::Command::new("sh")
        .arg("-c")
        .arg(format!("{} \"$1\"", &editor))
        .arg("--")
        .arg(tmpfile.path())
        .status()?;
    if !status.success() {
        return Err(Error::EditorFailed(status));
    }

    let edited = fs::read_to_string(tmpfile.path())?;
    let trimmed = edited.trim();

    if trimmed == original.trim() {
        return Ok(());
    }

    task.description = normalize_description(trimmed);
    task.modified = Utc::now();
    store.write_task_checked(&task, mtime)?;
    print_task(cli, &task, format_args!("Updated task {}", args.id));
    Ok(())
}

fn cmd_start(cli: &Cli, id: u32) -> Result<(), Error> {
    let cwd = env::current_dir()?;
    let store = Store::find(&cwd)?;
    let (mut task, mtime) = store.read_task_with_mtime(id)?;

    if task.status == Status::InProgress {
        print_task(cli, &task, format_args!("Task {id} is already in-progress"));
        return Ok(());
    }
    if task.status.is_resolved() {
        return Err(Error::CannotStart(id, task.status));
    }

    task.status = Status::InProgress;
    task.modified = Utc::now();
    store.write_task_checked(&task, mtime)?;
    print_task(cli, &task, format_args!("Task {id} marked in-progress"));
    Ok(())
}

fn cmd_done(cli: &Cli, id: u32, force: bool) -> Result<(), Error> {
    let cwd = env::current_dir()?;
    let store = Store::find(&cwd)?;
    let (mut task, mtime) = store.read_task_with_mtime(id)?;

    if task.status == Status::Done {
        print_task(cli, &task, format_args!("Task {id} is already done"));
        return Ok(());
    }

    if !force {
        let all_tasks = store.load_all_tasks()?;
        let archived_ids = store.load_archived_ids()?;
        validate_completion(&all_tasks, &archived_ids, &task)?;
    }

    task.status = Status::Done;
    task.modified = Utc::now();
    store.write_task_checked(&task, mtime)?;
    print_task(cli, &task, format_args!("Task {id} marked done"));
    Ok(())
}

fn cmd_cancel(cli: &Cli, id: u32) -> Result<(), Error> {
    let cwd = env::current_dir()?;
    let store = Store::find(&cwd)?;
    let (mut task, mtime) = store.read_task_with_mtime(id)?;

    if task.status == Status::Cancelled {
        print_task(cli, &task, format_args!("Task {id} is already cancelled"));
        return Ok(());
    }
    if task.status == Status::Done {
        return Err(Error::CannotCancel(id));
    }

    task.status = Status::Cancelled;
    task.modified = Utc::now();
    store.write_task_checked(&task, mtime)?;
    print_task(cli, &task, format_args!("Task {id} marked cancelled"));
    Ok(())
}

fn cmd_prime() -> Result<(), Error> {
    print!("{}", include_str!("prime.md"));
    Ok(())
}

fn cmd_prime_install(agent: &str) -> Result<(), Error> {
    if agent != "claude" {
        return Err(Error::UnsupportedAgent {
            name: agent.to_owned(),
            supported: vec!["claude".to_owned()],
        });
    }

    let cwd = env::current_dir()?;
    let store = Store::find(&cwd)?;
    let project_root = store
        .root()
        .parent()
        .ok_or_else(|| Error::Io(std::io::Error::other(".seed has no parent directory")))?;
    let claude_dir = project_root.join(".claude");
    let settings_path = claude_dir.join("settings.local.json");

    let mut settings: serde_json::Value = if settings_path.exists() {
        let content = fs::read_to_string(&settings_path)?;
        serde_json::from_str(&content)
            .map_err(|e| Error::InvalidConfig(format!("{}: {e}", settings_path.display())))?
    } else {
        if !claude_dir.exists() {
            fs::create_dir_all(&claude_dir)?;
        }
        serde_json::json!({})
    };

    let hooks = settings
        .as_object_mut()
        .ok_or_else(|| {
            Error::InvalidConfig(format!("{}: expected object", settings_path.display()))
        })?
        .entry("hooks")
        .or_insert_with(|| serde_json::json!({}));
    let session_start = hooks
        .as_object_mut()
        .ok_or_else(|| {
            Error::InvalidConfig(format!(
                "{}: \"hooks\" should be an object",
                settings_path.display()
            ))
        })?
        .entry("SessionStart")
        .or_insert_with(|| serde_json::json!([]));
    let entries = session_start.as_array_mut().ok_or_else(|| {
        Error::InvalidConfig(format!(
            "{}: \"hooks.SessionStart\" should be an array",
            settings_path.display()
        ))
    })?;

    let already = entries.iter().any(|entry| {
        entry
            .get("hooks")
            .and_then(|h| h.as_array())
            .is_some_and(|hooks| {
                hooks.iter().any(|h| {
                    h.get("command")
                        .and_then(|c| c.as_str())
                        .is_some_and(|c| c == "sd prime")
                })
            })
    });
    if already {
        println!(
            "sd prime hook already installed in {}",
            settings_path.display()
        );
        return Ok(());
    }

    entries.push(serde_json::json!({
        "matcher": "",
        "hooks": [{ "type": "command", "command": "sd prime" }]
    }));

    fs::write(
        &settings_path,
        serde_json::to_string_pretty(&settings).unwrap() + "\n",
    )?;
    println!("Installed sd prime hook in {}", settings_path.display());
    Ok(())
}

fn cmd_log(cli: &Cli, id: u32, message: &str, agent: Option<&str>) -> Result<(), Error> {
    let cwd = env::current_dir()?;
    let store = Store::find(&cwd)?;
    let (mut task, mtime) = store.read_task_with_mtime(id)?;

    task.log.push(task::LogEntry {
        timestamp: Utc::now(),
        agent: agent.map(|s| s.to_owned()),
        message: message.to_owned(),
    });
    task.modified = Utc::now();
    store.write_task_checked(&task, mtime)?;
    print_task(cli, &task, format_args!("Logged to task {id}"));
    Ok(())
}

fn cmd_next(cli: &Cli) -> Result<(), Error> {
    let cwd = env::current_dir()?;
    let store = Store::find(&cwd)?;
    let all_tasks = store.load_all_tasks()?;
    let mut done_ids: HashSet<u32> = all_tasks
        .iter()
        .filter(|t| t.status.is_resolved())
        .map(|t| t.id)
        .collect();
    done_ids.extend(store.load_archived_ids()?);
    let has_incomplete_child: HashSet<u32> = all_tasks
        .iter()
        .filter(|t| !t.status.is_resolved())
        .filter_map(|t| t.parent)
        .collect();
    let mut ready: Vec<&Task> = all_tasks
        .iter()
        .filter(|t| {
            t.status == Status::Todo
                && t.depends.iter().all(|d| done_ids.contains(d))
                && !has_incomplete_child.contains(&t.id)
        })
        .collect();
    ready.sort_by(|a, b| a.sort_key(&done_ids).cmp(&b.sort_key(&done_ids)));

    if cli.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&prepare_json(&ready, &done_ids)).unwrap()
        );
    } else if ready.is_empty() {
        println!("No tasks ready.");
    } else {
        let owned: Vec<Task> = ready.into_iter().cloned().collect();
        print!("{}", format::format_task_list(&owned, true, &done_ids));
    }
    Ok(())
}

fn cmd_archive(cli: &Cli, cutoff: Option<&str>) -> Result<(), Error> {
    let cwd = env::current_dir()?;
    let store = Store::find(&cwd)?;
    let all_tasks = store.load_all_tasks()?;

    let cutoff_time = match cutoff {
        Some(s) => {
            let dur =
                humantime::parse_duration(s).map_err(|e| Error::InvalidDuration(e.to_string()))?;
            let chrono_dur = chrono::Duration::from_std(dur)
                .map_err(|_| Error::InvalidDuration("duration too large".into()))?;
            Some(Utc::now() - chrono_dur)
        }
        None => None,
    };

    let to_archive: Vec<&Task> = all_tasks
        .iter()
        .filter(|t| t.status.is_resolved() && cutoff_time.is_none_or(|before| t.modified <= before))
        .collect();

    if to_archive.is_empty() {
        if cli.json {
            println!("[]");
        } else {
            println!("No tasks to archive.");
        }
        return Ok(());
    }

    store.ensure_archive_dir()?;
    for task in &to_archive {
        store.archive_task(task.id)?;
    }

    if cli.json {
        let ids: Vec<u32> = to_archive.iter().map(|t| t.id).collect();
        println!("{}", serde_json::to_string_pretty(&ids).unwrap());
    } else {
        let n = to_archive.len();
        println!("Archived {n} task{}.", if n == 1 { "" } else { "s" });
    }
    Ok(())
}

fn print_task(cli: &Cli, task: &Task, message: impl std::fmt::Display) {
    if cli.json {
        println!("{}", serde_json::to_string_pretty(task).unwrap());
    } else {
        println!("{message}");
    }
}

fn normalize_description(s: &str) -> Option<String> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// Strip resolved deps so agents don't see false blockers.
fn prepare_json(tasks: &[impl std::borrow::Borrow<Task>], done_ids: &HashSet<u32>) -> Vec<Task> {
    let mut tasks: Vec<Task> = tasks
        .iter()
        .map(|t| {
            let mut t = t.borrow().clone();
            t.depends.retain(|d| !done_ids.contains(d));
            t
        })
        .collect();
    tasks.sort_by(|a, b| a.sort_key(done_ids).cmp(&b.sort_key(done_ids)));
    tasks
}

fn validate_completion(
    all_tasks: &[Task],
    archived_ids: &HashSet<u32>,
    task: &Task,
) -> Result<(), Error> {
    let task_map: HashMap<u32, &Task> = all_tasks.iter().map(|t| (t.id, t)).collect();

    let unmet: Vec<u32> = task
        .depends
        .iter()
        .filter(|dep_id| match task_map.get(dep_id) {
            Some(dep) => !dep.status.is_resolved(),
            None => !archived_ids.contains(dep_id),
        })
        .copied()
        .collect();
    if !unmet.is_empty() {
        return Err(Error::UnmetDependencies(unmet));
    }

    let incomplete: Vec<u32> = all_tasks
        .iter()
        .filter(|t| t.parent == Some(task.id) && !t.status.is_resolved())
        .map(|t| t.id)
        .collect();
    if !incomplete.is_empty() {
        return Err(Error::IncompleteChildren(incomplete));
    }
    Ok(())
}
