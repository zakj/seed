# Task Tracking with sd

This project uses `sd` for task management. Run `sd <command> --help` for detailed flag reference. Use `--json` on any command for machine-readable output.

## Workflow

1. `sd next` — find a task to work on (todo, all deps met, no incomplete children)
2. `sd start <id>` — **always** claim before doing any work
3. Do the work
4. `sd log <id> --agent claude "summary"` — briefly log what was done; helps future agents on related tasks
5. `sd done <id>` — mark complete (validates deps and children; `--force` to skip)

## Key Commands

- `sd list` — tree view of all tasks (alias: `sd ls`)
- `sd show <id>` — full detail with children and log
- `sd add "title"` — create a task (`-q` for just the ID)
- `sd edit <id>` — open description in `$EDITOR` (or use flags to modify fields)
- `sd archive` — move resolved tasks to archive

## Conventions

- Statuses: `todo`, `in-progress`, `done`, `dropped`
- Priorities: `critical`, `high`, `normal` (default), `low`
- Tasks form a DAG: parent/child hierarchy + dependency edges, both validated acyclic
- Task files live in `.seed/tasks/<id>.kdl` (KDL format, human-editable)
- Use single quotes for shell arguments containing backticks to avoid command substitution
