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
- `sd add "title"` — create a task (`-q` for just the ID; `--parent`, `--dep`, `--description`)
- `sd edit <id>` — **agents must always use flags** (`--title`, `--status`, `--priority`, `--description`, `--add-dep/--rm-dep`, `--add-label/--rm-label`, `--parent/--no-parent`); bare `sd edit` opens an interactive editor
- `sd drop <id>` — mark a task as dropped
- `sd archive` — move resolved tasks to archive

## Conventions

- Statuses: `todo`, `in-progress`, `done`, `dropped`
- Priorities: `critical`, `high`, `normal` (default), `low`
- Parents group subtasks; deps enforce ordering — a task won't appear in `sd next` until all deps are resolved
- Task descriptions define *what* needs doing; use `sd log` for decisions, rationale, and outcomes
- Use single quotes for shell arguments containing backticks to avoid command substitution
- Always use long flags (e.g. `--parent`, `--priority`) — short flags can be ambiguous (`-p` is `--priority`, not `--parent`)
- `--json` output: `sd show` returns an object (with `children` IDs); `sd list` and `sd next` return arrays
