mod error;
mod format;
mod markdown;
mod ops;
mod store;
mod task;
mod term;

use std::collections::HashSet;
use std::env;
use std::fs;
use std::io::Write;

use chrono::Utc;
use clap::{CommandFactory, Parser, Subcommand};

use error::Error;
use store::Store;
use task::{Priority, Status, Task, TaskId};

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
        id: TaskId,

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

    /// Edit a task (opens description in $EDITOR with no flags)
    Edit(EditArgs),

    /// Start a task (set status to in-progress)
    Start {
        /// Task ID
        id: TaskId,
    },

    /// Mark a task as done
    #[command(
        long_about = "Mark a task as done.\n\nValidates that all dependencies are resolved and all children are \
complete. Use --force to skip validation."
    )]
    Done {
        /// Task ID
        id: TaskId,

        /// Force completion even with unmet dependencies or incomplete children
        #[arg(short, long)]
        force: bool,
    },

    /// Drop a task
    Drop {
        /// Task ID
        id: TaskId,
    },

    /// Append a log entry to a task
    Log {
        /// Task ID
        id: TaskId,

        /// Log message
        message: String,

        /// Tag the entry with an agent name (e.g. "claude")
        #[arg(long)]
        agent: Option<String>,
    },

    /// Output markdown context dump for agent priming
    #[command(hide = true)]
    Prime {
        /// Install agent hooks
        #[arg(long)]
        install: Option<task::Agent>,
    },

    /// Show tasks ready to work on
    #[command(
        long_about = "Show tasks ready to work on.\n\nLists todo tasks whose dependencies are all resolved and \
whose children are all complete. Sorted by priority."
    )]
    Next,

    /// Archive resolved tasks
    #[command(
        long_about = "Archive resolved tasks.\n\nMoves done and dropped tasks from .seed/tasks/ to \
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
    parent: Option<TaskId>,

    /// Dependencies (task IDs)
    #[arg(long)]
    dep: Vec<TaskId>,

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
    id: TaskId,

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
    parent: Option<TaskId>,

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
    add_dep: Vec<TaskId>,

    /// Remove a dependency
    #[arg(long)]
    rm_dep: Vec<TaskId>,

    /// Force completion even with unmet dependencies or incomplete children
    #[arg(short, long, requires = "status")]
    force: bool,
}

impl From<&EditModifications> for ops::Edits {
    fn from(m: &EditModifications) -> Self {
        let parent = if m.no_parent {
            Some(None)
        } else {
            m.parent.map(Some)
        };
        Self {
            title: m.title.clone(),
            status: m.status,
            priority: m.priority,
            description: m.description.clone(),
            parent,
            add_labels: m.add_label.clone(),
            rm_labels: m.rm_label.clone(),
            add_deps: m.add_dep.clone(),
            rm_deps: m.rm_dep.clone(),
            force: m.force,
        }
    }
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
        Command::Drop { id } => cmd_drop(cli, *id),
        Command::Log { id, message, agent } => cmd_log(cli, *id, message, agent.as_deref()),
        Command::Prime { install } => match install {
            Some(agent) => cmd_prime_install(*agent),
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

fn find_store() -> Result<Store, Error> {
    let cwd = env::current_dir()?;
    Store::find(&cwd)
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
    let store = find_store()?;
    let task = ops::create_task(
        &store,
        args.title.clone(),
        args.priority,
        args.label.iter().cloned(),
        args.parent,
        &args.dep,
        args.description.as_deref(),
    )?;

    if cli.json {
        println!("{}", serde_json::to_string_pretty(&task)?);
    } else if args.quiet {
        println!("{}", task.id);
    } else {
        println!("Created task {}: {}", task.id, args.title);
    }
    Ok(())
}

fn cmd_show(cli: &Cli, id: TaskId, include_archived: bool) -> Result<(), Error> {
    let store = find_store()?;
    let ctx = ops::load_task_context(&store, id, include_archived)?;

    if cli.json {
        let mut value = serde_json::to_value(&ctx.task)?;
        let ids: Vec<TaskId> = ctx.children.iter().map(|c| c.id).collect();
        value["children"] = serde_json::to_value(ids)?;
        println!("{}", serde_json::to_string_pretty(&value)?);
    } else {
        let width = terminal_size::terminal_size().map(|(w, _)| w.0 as usize);
        let dep_refs: Vec<&Task> = ctx.deps.iter().collect();
        let child_refs: Vec<&Task> = ctx.children.iter().collect();
        print!(
            "{}",
            format::format_task_detail(
                &ctx.task,
                ctx.parent.as_ref(),
                &dep_refs,
                &child_refs,
                &ctx.done_ids,
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
    let store = find_store()?;
    let mut tasks = store.load_all_tasks()?;
    if include_archived {
        tasks.extend(store.load_archived_tasks()?);
        tasks.sort_by_key(|t| t.id);
    }
    let done_ids = ops::resolved_ids(&store, &tasks)?;

    let filtered;
    let needs_filter = status.is_some() || !labels.is_empty();
    let display = if needs_filter {
        filtered = ops::filter_tasks(&tasks, status, labels);
        &filtered
    } else {
        &tasks
    };
    if cli.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&prepare_json(display, &done_ids, &tasks))?
        );
    } else {
        print!("{}", format::format_task_list(display, flat, &done_ids));
    }
    Ok(())
}

fn cmd_edit(cli: &Cli, args: &EditArgs) -> Result<(), Error> {
    let store = find_store()?;
    let edits = ops::Edits::from(&args.mods);
    let (task, changed) = ops::apply_edits(&store, args.id, &edits)?;

    if changed {
        print_task(cli, &task, format_args!("Updated task {}", args.id))?;
    } else {
        print_task(cli, &task, format_args!("No changes to task {}", args.id))?;
    }
    Ok(())
}

fn cmd_edit_interactive(cli: &Cli, args: &EditArgs) -> Result<(), Error> {
    let editor = env::var("VISUAL")
        .or_else(|_| env::var("EDITOR"))
        .map_err(|_| Error::NoEditor)?;

    let store = find_store()?;
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

    task.description = ops::normalize_description(trimmed);
    task.modified = Utc::now();
    store.write_task_checked(&task, mtime)?;
    print_task(cli, &task, format_args!("Updated task {}", args.id))?;
    Ok(())
}

fn cmd_start(cli: &Cli, id: TaskId) -> Result<(), Error> {
    let store = find_store()?;
    let (task, changed) = ops::start_task(&store, id)?;

    if changed {
        print_task(cli, &task, format_args!("Task {id} marked in-progress"))?;
    } else {
        print_task(cli, &task, format_args!("Task {id} is already in-progress"))?;
    }
    Ok(())
}

fn cmd_done(cli: &Cli, id: TaskId, force: bool) -> Result<(), Error> {
    let store = find_store()?;
    let (task, changed) = ops::complete_task(&store, id, force)?;

    if changed {
        print_task(cli, &task, format_args!("Task {id} marked done"))?;
    } else {
        print_task(cli, &task, format_args!("Task {id} is already done"))?;
    }
    Ok(())
}

fn cmd_drop(cli: &Cli, id: TaskId) -> Result<(), Error> {
    let store = find_store()?;
    let (task, changed) = ops::drop_task(&store, id)?;

    if changed {
        print_task(cli, &task, format_args!("Task {id} marked dropped"))?;
    } else {
        print_task(cli, &task, format_args!("Task {id} is already dropped"))?;
    }
    Ok(())
}

fn cmd_prime() -> Result<(), Error> {
    print!("{}", include_str!("prime.md"));
    Ok(())
}

fn cmd_prime_install(_agent: task::Agent) -> Result<(), Error> {
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
        serde_json::to_string_pretty(&settings)? + "\n",
    )?;
    println!("Installed sd prime hook in {}", settings_path.display());
    println!("Restart Claude Code for the hook to take effect.");
    Ok(())
}

fn cmd_log(cli: &Cli, id: TaskId, message: &str, agent: Option<&str>) -> Result<(), Error> {
    let store = find_store()?;
    let (mut task, mtime) = store.read_task_with_mtime(id)?;

    task.log.push(task::LogEntry {
        timestamp: Utc::now(),
        agent: agent.map(|s| s.to_owned()),
        message: message.to_owned(),
    });
    task.modified = Utc::now();
    store.write_task_checked(&task, mtime)?;
    print_task(cli, &task, format_args!("Logged to task {id}"))?;
    Ok(())
}

fn cmd_next(cli: &Cli) -> Result<(), Error> {
    let store = find_store()?;
    let result = ops::get_ready_tasks(&store)?;

    if cli.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&prepare_json(
                &result.ready,
                &result.done_ids,
                &result.all_tasks
            ))?
        );
    } else if result.ready.is_empty() {
        println!("No tasks ready.");
    } else {
        print!(
            "{}",
            format::format_task_list(&result.ready, true, &result.done_ids)
        );
    }
    Ok(())
}

fn cmd_archive(cli: &Cli, cutoff: Option<&str>) -> Result<(), Error> {
    let store = find_store()?;
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
        let ids: Vec<TaskId> = to_archive.iter().map(|t| t.id).collect();
        println!("{}", serde_json::to_string_pretty(&ids)?);
    } else {
        let n = to_archive.len();
        println!("Archived {n} task{}.", if n == 1 { "" } else { "s" });
    }
    Ok(())
}

fn print_task(cli: &Cli, task: &Task, message: impl std::fmt::Display) -> Result<(), Error> {
    if cli.json {
        println!("{}", serde_json::to_string_pretty(task)?);
    } else {
        println!("{message}");
    }
    Ok(())
}

/// Strip resolved deps and inject children IDs so agents get the full task graph.
fn prepare_json(
    tasks: &[impl std::borrow::Borrow<Task>],
    done_ids: &HashSet<TaskId>,
    all_tasks: &[Task],
) -> Vec<serde_json::Value> {
    let children = ops::children_map(all_tasks);
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
        .into_iter()
        .map(|t| {
            let id = t.id;
            let mut v = serde_json::to_value(t).unwrap();
            let child_ids = children.get(&id).map(Vec::as_slice).unwrap_or_default();
            v["children"] = serde_json::to_value(child_ids).unwrap();
            v
        })
        .collect()
}
