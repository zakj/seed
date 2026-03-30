use assert_cmd::Command;
use tempfile::TempDir;

fn sd() -> Command {
    assert_cmd::cargo_bin_cmd!("sd")
}

fn init_project() -> TempDir {
    let dir = TempDir::new().unwrap();
    sd().arg("init").current_dir(dir.path()).assert().success();
    dir
}

#[test]
fn init_creates_directory_structure() {
    let dir = TempDir::new().unwrap();
    sd().arg("init")
        .current_dir(dir.path())
        .assert()
        .success()
        .stdout(predicates::str::contains("Initialized"));
    assert!(dir.path().join(".seed").is_dir());
    assert!(dir.path().join(".seed/tasks").is_dir());
    assert!(dir.path().join(".seed/config.kdl").is_file());
}

#[test]
fn init_double_init_fails() {
    let dir = init_project();
    sd().arg("init")
        .current_dir(dir.path())
        .assert()
        .failure()
        .stderr(predicates::str::contains("already initialized"));
}

#[test]
fn add_and_show_roundtrip() {
    let dir = init_project();
    sd().args(["add", "Test task", "-p", "high", "-l", "bug", "-l", "api"])
        .current_dir(dir.path())
        .assert()
        .success()
        .stdout(predicates::str::contains("Created task 1"));

    sd().args(["show", "1"])
        .current_dir(dir.path())
        .assert()
        .success()
        .stdout(predicates::str::contains("Test task"))
        .stdout(predicates::str::contains("high"))
        .stdout(predicates::str::contains("bug"))
        .stdout(predicates::str::contains("api"));
}

#[test]
fn add_with_parent_and_deps() {
    let dir = init_project();
    sd().args(["add", "Parent"])
        .current_dir(dir.path())
        .assert()
        .success();
    sd().args(["add", "Dep"])
        .current_dir(dir.path())
        .assert()
        .success();
    sd().args(["add", "Child", "--parent", "1", "--dep", "2"])
        .current_dir(dir.path())
        .assert()
        .success();

    sd().args(["show", "3"])
        .current_dir(dir.path())
        .assert()
        .success()
        .stdout(predicates::str::contains(
            "Parent:\x1b[0m ○\x1b[0m #1 Parent",
        ))
        .stdout(predicates::str::contains(
            "Depends on:\x1b[0m ○\x1b[0m #2 Dep",
        ));
}

#[test]
fn add_quiet_outputs_only_id() {
    let dir = init_project();
    sd().args(["add", "Quiet task", "-q"])
        .current_dir(dir.path())
        .assert()
        .success()
        .stdout("1\n");
}

#[test]
fn list_tree_and_flat() {
    let dir = init_project();
    sd().args(["add", "Parent"])
        .current_dir(dir.path())
        .assert()
        .success();
    sd().args(["add", "Child", "--parent", "1"])
        .current_dir(dir.path())
        .assert()
        .success();

    let tree = sd()
        .args(["list"])
        .current_dir(dir.path())
        .assert()
        .success();
    let tree_out = String::from_utf8(tree.get_output().stdout.clone()).unwrap();
    assert!(tree_out.contains("└─"));

    let flat = sd()
        .args(["list", "--flat"])
        .current_dir(dir.path())
        .assert()
        .success();
    let flat_out = String::from_utf8(flat.get_output().stdout.clone()).unwrap();
    assert!(!flat_out.contains("└──"));
}

#[test]
fn list_status_filter() {
    let dir = init_project();
    sd().args(["add", "Todo task"])
        .current_dir(dir.path())
        .assert()
        .success();
    sd().args(["add", "Done task"])
        .current_dir(dir.path())
        .assert()
        .success();
    sd().args(["done", "2", "--force"])
        .current_dir(dir.path())
        .assert()
        .success();

    let out = sd()
        .args(["list", "--status", "todo"])
        .current_dir(dir.path())
        .assert()
        .success();
    let stdout = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    assert!(stdout.contains("Todo task"));
    assert!(!stdout.contains("Done task"));
}

#[test]
fn list_sorts_by_status_and_priority() {
    let dir = init_project();
    sd().args(["add", "Low todo", "-p", "low"])
        .current_dir(dir.path())
        .assert()
        .success();
    sd().args(["add", "High todo", "-p", "high"])
        .current_dir(dir.path())
        .assert()
        .success();
    sd().args(["add", "In progress"])
        .current_dir(dir.path())
        .assert()
        .success();
    sd().args(["start", "3"])
        .current_dir(dir.path())
        .assert()
        .success();
    sd().args(["add", "Done task"])
        .current_dir(dir.path())
        .assert()
        .success();
    sd().args(["done", "4"])
        .current_dir(dir.path())
        .assert()
        .success();
    sd().args(["add", "Blocked", "--dep", "1"])
        .current_dir(dir.path())
        .assert()
        .success();

    let out = sd()
        .args(["list", "--flat", "--json"])
        .current_dir(dir.path())
        .assert()
        .success();
    let stdout = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    let tasks: Vec<serde_json::Value> = serde_json::from_str(&stdout).unwrap();
    let titles: Vec<&str> = tasks.iter().map(|t| t["title"].as_str().unwrap()).collect();
    assert_eq!(
        titles,
        vec![
            "In progress",
            "High todo",
            "Low todo",
            "Blocked",
            "Done task"
        ]
    );
}

#[test]
fn edit_changes_fields() {
    let dir = init_project();
    sd().args(["add", "Original"])
        .current_dir(dir.path())
        .assert()
        .success();
    sd().args([
        "edit",
        "1",
        "--title",
        "Updated",
        "--priority",
        "critical",
        "--add-label",
        "new-label",
    ])
    .current_dir(dir.path())
    .assert()
    .success();

    sd().args(["show", "1"])
        .current_dir(dir.path())
        .assert()
        .success()
        .stdout(predicates::str::contains("Updated"))
        .stdout(predicates::str::contains("critical"))
        .stdout(predicates::str::contains("new-label"));
}

#[test]
fn start_done_cancel_set_status() {
    let dir = init_project();
    sd().args(["add", "Task"])
        .current_dir(dir.path())
        .assert()
        .success();

    sd().args(["start", "1"])
        .current_dir(dir.path())
        .assert()
        .success()
        .stdout(predicates::str::contains("in-progress"));
    sd().args(["done", "1"])
        .current_dir(dir.path())
        .assert()
        .success()
        .stdout(predicates::str::contains("marked done"));

    sd().args(["add", "Task2"])
        .current_dir(dir.path())
        .assert()
        .success();
    sd().args(["drop", "2"])
        .current_dir(dir.path())
        .assert()
        .success()
        .stdout(predicates::str::contains("dropped"));
}

#[test]
fn done_already_done_is_noop() {
    let dir = init_project();
    sd().args(["add", "Task"])
        .current_dir(dir.path())
        .assert()
        .success();
    sd().args(["done", "1"])
        .current_dir(dir.path())
        .assert()
        .success();
    sd().args(["done", "1"])
        .current_dir(dir.path())
        .assert()
        .success()
        .stdout(predicates::str::contains("already done"));
}

#[test]
fn done_with_unmet_deps_fails() {
    let dir = init_project();
    sd().args(["add", "Dep"])
        .current_dir(dir.path())
        .assert()
        .success();
    sd().args(["add", "Task", "--dep", "1"])
        .current_dir(dir.path())
        .assert()
        .success();

    sd().args(["done", "2"])
        .current_dir(dir.path())
        .assert()
        .failure()
        .stderr(predicates::str::contains("unmet dependencies"));
}

#[test]
fn done_with_force_overrides_unmet_deps() {
    let dir = init_project();
    sd().args(["add", "Dep"])
        .current_dir(dir.path())
        .assert()
        .success();
    sd().args(["add", "Task", "--dep", "1"])
        .current_dir(dir.path())
        .assert()
        .success();

    sd().args(["done", "2", "--force"])
        .current_dir(dir.path())
        .assert()
        .success();
}

#[test]
fn done_blocked_by_incomplete_children() {
    let dir = init_project();
    sd().args(["add", "Parent"])
        .current_dir(dir.path())
        .assert()
        .success();
    sd().args(["add", "Child", "--parent", "1"])
        .current_dir(dir.path())
        .assert()
        .success();

    // Can't complete parent with incomplete child
    sd().args(["done", "1"])
        .current_dir(dir.path())
        .assert()
        .failure()
        .stderr(predicates::str::contains("incomplete children"));

    // Force overrides
    sd().args(["done", "1", "--force"])
        .current_dir(dir.path())
        .assert()
        .success();
}

#[test]
fn done_after_completing_children() {
    let dir = init_project();
    sd().args(["add", "Parent"])
        .current_dir(dir.path())
        .assert()
        .success();
    sd().args(["add", "Child", "--parent", "1"])
        .current_dir(dir.path())
        .assert()
        .success();

    // Complete child, then parent succeeds without --force
    sd().args(["done", "2"])
        .current_dir(dir.path())
        .assert()
        .success();
    sd().args(["done", "1"])
        .current_dir(dir.path())
        .assert()
        .success()
        .stdout(predicates::str::contains("marked done"));
}

#[test]
fn json_output_is_valid() {
    let dir = init_project();
    sd().args(["add", "Task", "-p", "high"])
        .current_dir(dir.path())
        .assert()
        .success();

    let out = sd()
        .args(["show", "1", "--json"])
        .current_dir(dir.path())
        .assert()
        .success();
    let stdout = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    let val: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(val["id"], 1);
    assert_eq!(val["title"], "Task");
    assert_eq!(val["status"], "todo");
    assert_eq!(val["priority"], "high");

    // Normal priority is omitted from JSON
    sd().args(["add", "Default task"])
        .current_dir(dir.path())
        .assert()
        .success();
    let out = sd()
        .args(["show", "2", "--json"])
        .current_dir(dir.path())
        .assert()
        .success();
    let stdout = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    let val: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert!(val.get("priority").is_none());
}

#[test]
fn json_list_is_valid() {
    let dir = init_project();
    sd().args(["add", "A"])
        .current_dir(dir.path())
        .assert()
        .success();
    sd().args(["add", "B"])
        .current_dir(dir.path())
        .assert()
        .success();

    let out = sd()
        .args(["list", "--json"])
        .current_dir(dir.path())
        .assert()
        .success();
    let stdout = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    let val: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert!(val.as_array().unwrap().len() == 2);
}

#[test]
fn circular_dependency_rejected() {
    let dir = init_project();
    sd().args(["add", "A"])
        .current_dir(dir.path())
        .assert()
        .success();
    sd().args(["add", "B", "--dep", "1"])
        .current_dir(dir.path())
        .assert()
        .success();

    sd().args(["edit", "1", "--add-dep", "2"])
        .current_dir(dir.path())
        .assert()
        .failure()
        .stderr(predicates::str::contains("cycle"));
}

#[test]
fn kdl_file_is_readable() {
    let dir = init_project();
    sd().args(["add", "Test task", "-p", "high", "-l", "bug"])
        .current_dir(dir.path())
        .assert()
        .success();

    let content = std::fs::read_to_string(dir.path().join(".seed/tasks/1.kdl")).unwrap();
    assert!(content.contains("task"));
    assert!(content.contains("id=1"));
    assert!(content.contains("title \"Test task\""));
    assert!(content.contains("high"));
    assert!(content.contains("bug"));
}

#[test]
fn not_a_seed_project() {
    let dir = TempDir::new().unwrap();
    sd().args(["list"])
        .current_dir(dir.path())
        .assert()
        .failure()
        .stderr(predicates::str::contains("not a seed project"));
}

#[test]
fn task_not_found() {
    let dir = init_project();
    sd().args(["show", "999"])
        .current_dir(dir.path())
        .assert()
        .failure()
        .stderr(predicates::str::contains("not found"));
    sd().args(["edit", "999", "--title", "nope"])
        .current_dir(dir.path())
        .assert()
        .failure()
        .stderr(predicates::str::contains("not found"));
}

#[test]
fn edit_remove_label() {
    let dir = init_project();
    sd().args(["add", "Task", "-l", "keep", "-l", "remove"])
        .current_dir(dir.path())
        .assert()
        .success();
    sd().args(["edit", "1", "--rm-label", "remove"])
        .current_dir(dir.path())
        .assert()
        .success();

    sd().args(["show", "1"])
        .current_dir(dir.path())
        .assert()
        .success()
        .stdout(predicates::str::contains("keep"));
    let out = sd()
        .args(["show", "1"])
        .current_dir(dir.path())
        .assert()
        .success();
    let stdout = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    assert!(!stdout.contains("remove"));
}

#[test]
fn next_shows_ready_tasks() {
    let dir = init_project();
    sd().args(["add", "Ready"])
        .current_dir(dir.path())
        .assert()
        .success();
    sd().args(["add", "Blocked", "--dep", "1"])
        .current_dir(dir.path())
        .assert()
        .success();
    sd().args(["add", "In progress"])
        .current_dir(dir.path())
        .assert()
        .success();
    sd().args(["start", "3"])
        .current_dir(dir.path())
        .assert()
        .success();

    let out = sd()
        .args(["next", "--json"])
        .current_dir(dir.path())
        .assert()
        .success();
    let stdout = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    let tasks: Vec<serde_json::Value> = serde_json::from_str(&stdout).unwrap();
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0]["title"], "Ready");
}

#[test]
fn next_unblocks_after_done() {
    let dir = init_project();
    sd().args(["add", "Dep"])
        .current_dir(dir.path())
        .assert()
        .success();
    sd().args(["add", "Blocked", "--dep", "1"])
        .current_dir(dir.path())
        .assert()
        .success();

    // Blocked task not in next
    let out = sd()
        .args(["next", "--json"])
        .current_dir(dir.path())
        .assert()
        .success();
    let stdout = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    let tasks: Vec<serde_json::Value> = serde_json::from_str(&stdout).unwrap();
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0]["title"], "Dep");

    // After completing dep, blocked task appears
    sd().args(["done", "1"])
        .current_dir(dir.path())
        .assert()
        .success();
    let out = sd()
        .args(["next", "--json"])
        .current_dir(dir.path())
        .assert()
        .success();
    let stdout = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    let tasks: Vec<serde_json::Value> = serde_json::from_str(&stdout).unwrap();
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0]["title"], "Blocked");
}

#[test]
fn next_sorts_by_priority() {
    let dir = init_project();
    sd().args(["add", "No priority"])
        .current_dir(dir.path())
        .assert()
        .success();
    sd().args(["add", "Low", "-p", "low"])
        .current_dir(dir.path())
        .assert()
        .success();
    sd().args(["add", "Critical", "-p", "critical"])
        .current_dir(dir.path())
        .assert()
        .success();
    sd().args(["add", "High", "-p", "high"])
        .current_dir(dir.path())
        .assert()
        .success();

    let out = sd()
        .args(["next", "--json"])
        .current_dir(dir.path())
        .assert()
        .success();
    let stdout = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    let tasks: Vec<serde_json::Value> = serde_json::from_str(&stdout).unwrap();
    let titles: Vec<&str> = tasks.iter().map(|t| t["title"].as_str().unwrap()).collect();
    assert_eq!(titles, vec!["Critical", "High", "No priority", "Low"]);
}

#[test]
fn next_excludes_tasks_with_incomplete_children() {
    let dir = init_project();
    sd().args(["add", "Parent"])
        .current_dir(dir.path())
        .assert()
        .success();
    sd().args(["add", "Child", "--parent", "1"])
        .current_dir(dir.path())
        .assert()
        .success();
    sd().args(["add", "Standalone"])
        .current_dir(dir.path())
        .assert()
        .success();

    // Parent should not appear in next because it has an incomplete child
    let out = sd()
        .args(["next", "--json"])
        .current_dir(dir.path())
        .assert()
        .success();
    let stdout = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    let tasks: Vec<serde_json::Value> = serde_json::from_str(&stdout).unwrap();
    let titles: Vec<&str> = tasks.iter().map(|t| t["title"].as_str().unwrap()).collect();
    assert_eq!(titles, vec!["Child", "Standalone"]);

    // After completing the child, parent appears
    sd().args(["done", "2"])
        .current_dir(dir.path())
        .assert()
        .success();
    let out = sd()
        .args(["next", "--json"])
        .current_dir(dir.path())
        .assert()
        .success();
    let stdout = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    let tasks: Vec<serde_json::Value> = serde_json::from_str(&stdout).unwrap();
    let titles: Vec<&str> = tasks.iter().map(|t| t["title"].as_str().unwrap()).collect();
    assert_eq!(titles, vec!["Parent", "Standalone"]);
}

#[test]
fn log_appends_entry() {
    let dir = init_project();
    sd().args(["add", "Task"])
        .current_dir(dir.path())
        .assert()
        .success();
    sd().args(["log", "1", "First note"])
        .current_dir(dir.path())
        .assert()
        .success();
    sd().args(["log", "1", "Second note", "--agent", "test-agent"])
        .current_dir(dir.path())
        .assert()
        .success();

    let out = sd()
        .args(["show", "1", "--json"])
        .current_dir(dir.path())
        .assert()
        .success();
    let stdout = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    let task: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let log = task["log"].as_array().unwrap();
    assert_eq!(log.len(), 2);
    assert_eq!(log[0]["message"], "First note");
    assert_eq!(log[1]["message"], "Second note");
    assert_eq!(log[1]["agent"], "test-agent");
}

#[test]
fn prime_outputs_usage_guide() {
    let dir = init_project();
    let out = sd()
        .args(["prime"])
        .current_dir(dir.path())
        .assert()
        .success();
    let stdout = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    assert!(stdout.contains("## Workflow"));
    assert!(stdout.contains("## Key Commands"));
    assert!(stdout.contains("sd next"));
    assert!(stdout.contains("--json"));
}

#[test]
fn list_shows_blocked_indicator() {
    let dir = init_project();
    sd().args(["add", "Dep"])
        .current_dir(dir.path())
        .assert()
        .success();
    sd().args(["add", "Blocked task", "--dep", "1"])
        .current_dir(dir.path())
        .assert()
        .success();

    let out = sd()
        .args(["list"])
        .current_dir(dir.path())
        .assert()
        .success();
    let stdout = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    // Blocked task should show red ⋯ indicator
    assert!(stdout.contains("\x1b[31m⋯"));
}

#[test]
fn json_error_output() {
    let dir = TempDir::new().unwrap();
    sd().args(["list", "--json"])
        .current_dir(dir.path())
        .assert()
        .failure()
        .stderr(predicates::str::contains("\"error\""));
}

#[test]
fn archive_moves_resolved_tasks() {
    let dir = init_project();
    sd().args(["add", "Done task"])
        .current_dir(dir.path())
        .assert()
        .success();
    sd().args(["add", "Todo task"])
        .current_dir(dir.path())
        .assert()
        .success();
    sd().args(["done", "1"])
        .current_dir(dir.path())
        .assert()
        .success();

    sd().args(["archive"])
        .current_dir(dir.path())
        .assert()
        .success()
        .stdout(predicates::str::contains("Archived 1 task."));

    assert!(dir.path().join(".seed/archive/1.kdl").exists());
    assert!(!dir.path().join(".seed/tasks/1.kdl").exists());
    assert!(dir.path().join(".seed/tasks/2.kdl").exists());
}

#[test]
fn archive_list_hides_archived() {
    let dir = init_project();
    sd().args(["add", "Done task"])
        .current_dir(dir.path())
        .assert()
        .success();
    sd().args(["add", "Todo task"])
        .current_dir(dir.path())
        .assert()
        .success();
    sd().args(["done", "1"])
        .current_dir(dir.path())
        .assert()
        .success();
    sd().args(["archive"])
        .current_dir(dir.path())
        .assert()
        .success();

    // Normal list hides archived
    let out = sd()
        .args(["list", "--json"])
        .current_dir(dir.path())
        .assert()
        .success();
    let stdout = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    let tasks: Vec<serde_json::Value> = serde_json::from_str(&stdout).unwrap();
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0]["title"], "Todo task");

    // -a includes archived
    let out = sd()
        .args(["list", "-a", "--json"])
        .current_dir(dir.path())
        .assert()
        .success();
    let stdout = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    let tasks: Vec<serde_json::Value> = serde_json::from_str(&stdout).unwrap();
    assert_eq!(tasks.len(), 2);
}

#[test]
fn archive_show_works_for_archived() {
    let dir = init_project();
    sd().args(["add", "Archived task"])
        .current_dir(dir.path())
        .assert()
        .success();
    sd().args(["done", "1"])
        .current_dir(dir.path())
        .assert()
        .success();
    sd().args(["archive"])
        .current_dir(dir.path())
        .assert()
        .success();

    sd().args(["show", "1"])
        .current_dir(dir.path())
        .assert()
        .success()
        .stdout(predicates::str::contains("Archived task"));
}

#[test]
fn archive_cutoff_filters_by_age() {
    let dir = init_project();
    sd().args(["add", "Done task"])
        .current_dir(dir.path())
        .assert()
        .success();
    sd().args(["done", "1"])
        .current_dir(dir.path())
        .assert()
        .success();

    // Large cutoff — task is too recent to archive
    sd().args(["archive", "999d"])
        .current_dir(dir.path())
        .assert()
        .success()
        .stdout(predicates::str::contains("No tasks to archive"));
    assert!(dir.path().join(".seed/tasks/1.kdl").exists());

    // No cutoff — archives all resolved
    sd().args(["archive"])
        .current_dir(dir.path())
        .assert()
        .success()
        .stdout(predicates::str::contains("Archived 1 task"));
}

#[test]
fn archive_nothing_to_archive() {
    let dir = init_project();
    sd().args(["add", "Todo task"])
        .current_dir(dir.path())
        .assert()
        .success();

    sd().args(["archive"])
        .current_dir(dir.path())
        .assert()
        .success()
        .stdout(predicates::str::contains("No tasks to archive"));
}

#[test]
fn done_with_archived_dep_succeeds() {
    let dir = init_project();
    sd().args(["add", "Dep"])
        .current_dir(dir.path())
        .assert()
        .success();
    sd().args(["add", "Task", "--dep", "1"])
        .current_dir(dir.path())
        .assert()
        .success();
    sd().args(["done", "1"])
        .current_dir(dir.path())
        .assert()
        .success();
    sd().args(["archive"])
        .current_dir(dir.path())
        .assert()
        .success();

    // Task 2 depends on archived task 1 — should not block
    sd().args(["done", "2"])
        .current_dir(dir.path())
        .assert()
        .success()
        .stdout(predicates::str::contains("marked done"));
}

#[test]
fn edit_archived_task_fails() {
    let dir = init_project();
    sd().args(["add", "Task"])
        .current_dir(dir.path())
        .assert()
        .success();
    sd().args(["done", "1"])
        .current_dir(dir.path())
        .assert()
        .success();
    sd().args(["archive"])
        .current_dir(dir.path())
        .assert()
        .success();

    sd().args(["edit", "1", "--title", "Nope"])
        .current_dir(dir.path())
        .assert()
        .failure()
        .stderr(predicates::str::contains("archived"));
}

#[test]
fn add_with_nonexistent_parent_fails() {
    let dir = init_project();
    sd().args(["add", "Orphan", "--parent", "99"])
        .current_dir(dir.path())
        .assert()
        .failure()
        .stderr(predicates::str::contains("not found"));
}

#[test]
fn edit_parent_cycle_fails() {
    let dir = init_project();
    sd().args(["add", "A"])
        .current_dir(dir.path())
        .assert()
        .success();
    sd().args(["add", "B", "--parent", "1"])
        .current_dir(dir.path())
        .assert()
        .success();

    // Self-reference
    sd().args(["edit", "1", "--parent", "1"])
        .current_dir(dir.path())
        .assert()
        .failure()
        .stderr(predicates::str::contains("cycle"));

    // Longer cycle: A->B->A
    sd().args(["edit", "1", "--parent", "2"])
        .current_dir(dir.path())
        .assert()
        .failure()
        .stderr(predicates::str::contains("cycle"));
}

#[test]
fn add_dep_on_archived_task_succeeds() {
    let dir = init_project();
    sd().args(["add", "Dep"])
        .current_dir(dir.path())
        .assert()
        .success();
    sd().args(["done", "1"])
        .current_dir(dir.path())
        .assert()
        .success();
    sd().args(["archive"])
        .current_dir(dir.path())
        .assert()
        .success();

    sd().args(["add", "Task", "--dep", "1"])
        .current_dir(dir.path())
        .assert()
        .success();
}

#[test]
fn start_done_task_fails() {
    let dir = init_project();
    sd().args(["add", "Task"])
        .current_dir(dir.path())
        .assert()
        .success();
    sd().args(["done", "1"])
        .current_dir(dir.path())
        .assert()
        .success();

    sd().args(["start", "1"])
        .current_dir(dir.path())
        .assert()
        .failure()
        .stderr(predicates::str::contains("cannot start"));
}

#[test]
fn start_already_in_progress_is_noop() {
    let dir = init_project();
    sd().args(["add", "Task"])
        .current_dir(dir.path())
        .assert()
        .success();
    sd().args(["start", "1"])
        .current_dir(dir.path())
        .assert()
        .success();

    sd().args(["start", "1"])
        .current_dir(dir.path())
        .assert()
        .success()
        .stdout(predicates::str::contains("already in-progress"));
}

#[test]
fn drop_done_task_fails() {
    let dir = init_project();
    sd().args(["add", "Task"])
        .current_dir(dir.path())
        .assert()
        .success();
    sd().args(["done", "1"])
        .current_dir(dir.path())
        .assert()
        .success();

    sd().args(["drop", "1"])
        .current_dir(dir.path())
        .assert()
        .failure()
        .stderr(predicates::str::contains("cannot drop"));
}

#[test]
fn edit_status_done_validates_deps() {
    let dir = init_project();
    sd().args(["add", "Dep"])
        .current_dir(dir.path())
        .assert()
        .success();
    sd().args(["add", "Task", "--dep", "1"])
        .current_dir(dir.path())
        .assert()
        .success();

    sd().args(["edit", "2", "--status", "done"])
        .current_dir(dir.path())
        .assert()
        .failure()
        .stderr(predicates::str::contains("unmet dependencies"));
}

#[test]
fn orphaned_children_appear_in_tree() {
    let dir = init_project();
    sd().args(["add", "Parent"])
        .current_dir(dir.path())
        .assert()
        .success();
    sd().args(["add", "Child", "--parent", "1"])
        .current_dir(dir.path())
        .assert()
        .success();
    sd().args(["done", "1", "--force"])
        .current_dir(dir.path())
        .assert()
        .success();
    sd().args(["archive"])
        .current_dir(dir.path())
        .assert()
        .success();

    // Child's parent is archived; child should still appear in list
    let out = sd()
        .args(["list"])
        .current_dir(dir.path())
        .assert()
        .success();
    let stdout = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    assert!(stdout.contains("Child"));
}

#[test]
fn edit_no_parent() {
    let dir = init_project();
    sd().args(["add", "Parent"])
        .current_dir(dir.path())
        .assert()
        .success();
    sd().args(["add", "Child", "--parent", "1"])
        .current_dir(dir.path())
        .assert()
        .success();
    sd().args(["edit", "2", "--no-parent"])
        .current_dir(dir.path())
        .assert()
        .success();

    let out = sd()
        .args(["show", "2", "--json"])
        .current_dir(dir.path())
        .assert()
        .success();
    let stdout = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    let task: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert!(task.get("parent").is_none());
}

#[test]
fn list_label_filter() {
    let dir = init_project();
    sd().args(["add", "Bug task", "-l", "bug"])
        .current_dir(dir.path())
        .assert()
        .success();
    sd().args(["add", "Feature task", "-l", "feature"])
        .current_dir(dir.path())
        .assert()
        .success();
    sd().args(["add", "Unlabeled"])
        .current_dir(dir.path())
        .assert()
        .success();

    let out = sd()
        .args(["list", "-l", "bug"])
        .current_dir(dir.path())
        .assert()
        .success();
    let stdout = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    assert!(stdout.contains("Bug task"));
    assert!(!stdout.contains("Feature task"));
    assert!(!stdout.contains("Unlabeled"));
}

#[test]
fn edit_noop_prints_no_changes() {
    let dir = init_project();
    sd().args(["add", "Task"])
        .current_dir(dir.path())
        .assert()
        .success();
    sd().args(["edit", "1", "--title", "Task"])
        .current_dir(dir.path())
        .assert()
        .success()
        .stdout(predicates::str::contains("No changes to task 1"));
}

#[test]
fn edit_empty_description_clears() {
    let dir = init_project();
    sd().args(["add", "Task", "-d", "some description"])
        .current_dir(dir.path())
        .assert()
        .success();
    sd().args(["edit", "1", "-d", ""])
        .current_dir(dir.path())
        .assert()
        .success();

    let out = sd()
        .args(["show", "1", "--json"])
        .current_dir(dir.path())
        .assert()
        .success();
    let stdout = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    let task: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert!(task.get("description").is_none());
}

#[test]
fn list_include_archived_strips_resolved_deps() {
    let dir = init_project();
    // Task 1 is a dep, task 2 depends on it
    sd().args(["add", "Dep"])
        .current_dir(dir.path())
        .assert()
        .success();
    sd().args(["add", "Task", "--dep", "1"])
        .current_dir(dir.path())
        .assert()
        .success();
    sd().args(["done", "1"])
        .current_dir(dir.path())
        .assert()
        .success();
    sd().args(["archive"])
        .current_dir(dir.path())
        .assert()
        .success();

    // With -a, the archived dep should still be stripped from JSON depends
    let out = sd()
        .args(["list", "-a", "--json"])
        .current_dir(dir.path())
        .assert()
        .success();
    let stdout = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    let tasks: Vec<serde_json::Value> = serde_json::from_str(&stdout).unwrap();
    let task2 = tasks.iter().find(|t| t["title"] == "Task").unwrap();
    let deps = task2["depends"].as_array();
    assert!(
        deps.is_none() || deps.unwrap().is_empty(),
        "resolved archived dep should be stripped from JSON output"
    );
}

#[test]
fn log_nonexistent_task_fails() {
    let dir = init_project();
    sd().args(["log", "999", "note"])
        .current_dir(dir.path())
        .assert()
        .failure()
        .stderr(predicates::str::contains("not found"));
}

#[test]
fn list_subtree() {
    let dir = init_project();
    sd().args(["add", "Parent"])
        .current_dir(dir.path())
        .assert()
        .success();
    sd().args(["add", "Child A", "--parent", "1"])
        .current_dir(dir.path())
        .assert()
        .success();
    sd().args(["add", "Child B", "--parent", "1"])
        .current_dir(dir.path())
        .assert()
        .success();
    sd().args(["add", "Unrelated"])
        .current_dir(dir.path())
        .assert()
        .success();

    // Subtree of task 1 should include parent and both children, not unrelated
    let out = sd()
        .args(["list", "1"])
        .current_dir(dir.path())
        .assert()
        .success();
    let stdout = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    assert!(stdout.contains("Parent"));
    assert!(stdout.contains("Child A"));
    assert!(stdout.contains("Child B"));
    assert!(!stdout.contains("Unrelated"));

    // Tree structure preserved
    assert!(stdout.contains("├─") || stdout.contains("└─"));
}

#[test]
fn list_subtree_json() {
    let dir = init_project();
    sd().args(["add", "Parent"])
        .current_dir(dir.path())
        .assert()
        .success();
    sd().args(["add", "Child", "--parent", "1"])
        .current_dir(dir.path())
        .assert()
        .success();
    sd().args(["add", "Unrelated"])
        .current_dir(dir.path())
        .assert()
        .success();

    let out = sd()
        .args(["list", "1", "--json"])
        .current_dir(dir.path())
        .assert()
        .success();
    let stdout = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    let tasks: Vec<serde_json::Value> = serde_json::from_str(&stdout).unwrap();
    assert_eq!(tasks.len(), 2);
    assert_eq!(tasks[0]["title"], "Parent");
    assert_eq!(tasks[1]["title"], "Child");
}

#[test]
fn list_subtree_with_filter() {
    let dir = init_project();
    sd().args(["add", "Parent"])
        .current_dir(dir.path())
        .assert()
        .success();
    sd().args(["add", "Child", "--parent", "1"])
        .current_dir(dir.path())
        .assert()
        .success();
    sd().args(["done", "2", "--force"])
        .current_dir(dir.path())
        .assert()
        .success();

    let out = sd()
        .args(["list", "1", "--status", "todo"])
        .current_dir(dir.path())
        .assert()
        .success();
    let stdout = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    assert!(stdout.contains("Parent"));
    assert!(!stdout.contains("Child"));
}

#[test]
fn list_subtree_nonexistent() {
    let dir = init_project();
    sd().args(["list", "999"])
        .current_dir(dir.path())
        .assert()
        .failure()
        .stderr(predicates::str::contains("not found"));
}

#[test]
fn json_output_is_compact() {
    let dir = init_project();
    sd().args(["add", "Test task"])
        .current_dir(dir.path())
        .assert()
        .success();

    // list --json should be compact (single line)
    let out = sd()
        .args(["list", "--json"])
        .current_dir(dir.path())
        .assert()
        .success();
    let stdout = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    let lines: Vec<&str> = stdout.trim().lines().collect();
    assert_eq!(lines.len(), 1, "JSON output should be a single line");

    // show --json should be compact too
    let out = sd()
        .args(["show", "1", "--json"])
        .current_dir(dir.path())
        .assert()
        .success();
    let stdout = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    let lines: Vec<&str> = stdout.trim().lines().collect();
    assert_eq!(lines.len(), 1, "JSON output should be a single line");
}
