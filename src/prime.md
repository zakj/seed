# Task Tracking with sd

This project uses `sd` for task management. Run `sd <command> --help` for detailed flag reference.

## Workflow

1. `sd next` — find a task to work on (todo, all deps met, no incomplete children)
2. `sd start <id>` — **always** claim before doing any work
3. Do the work
4. `sd log <id> --agent claude "summary"` — briefly log what was done; helps future agents on related tasks
5. `sd done <id>` — mark complete (validates deps and children; `--force` to skip)

## Key Commands

- `sd list` — tree view of all tasks (alias: `sd ls`); common filters: `--status`, `--label`
- `sd list <id>` — tree view scoped to a task and its descendants
- `sd show <id>` — full detail: description, children, deps, and log
- `sd next` — tasks ready to work on (todo, deps met, no incomplete children)
- `sd start <id>` / `sd done <id>` / `sd drop <id>` — status shortcuts
- `sd add "title"` — create a task (`-q` for just the ID; `--parent`, `--dep`, `--description`)
- `sd edit <id>` — **agents must always use flags** (`--title`, `--status`, `--priority`, `--description`, `--add-dep/--rm-dep`, `--add-label/--rm-label`, `--parent/--no-parent`); bare `sd edit` opens an interactive editor
- `sd log <id> --agent claude "summary"` — append to task log
- `sd archive` — move resolved tasks to archive

## Reading Task Output

`sd show` and `sd list` produce human-readable output — use them directly. Don't pipe `--json` through a script just to reformat; use `--json` only when you need to branch on field values.

`sd list` output uses indicator symbols. For todo tasks, the symbol encodes priority:

    ! critical  ↑ high  ○ normal  ↓ low  ⋯ blocked

For all other statuses, the symbol encodes status:

    ● in-progress  ✓ done  × dropped

Example `sd list` output:

    ↑ #5 Implement auth middleware
    ├─ ● #12 Add session handling
    └─ ✓ #11 Define auth routes
    ○ #6 Update API docs

## JSON Output

Use `--json` when you need to branch on field values programmatically. Prefer `sd list --json` over looping `sd show` — one call gives you the full task graph.

- `sd show --json` returns an object; `sd list --json` and `sd next --json` return arrays
- Task fields: `id`, `title`, `status`, `priority`, `description`, `labels`, `parent`, `depends`, `created`, `modified`, `log`, `children`
- Fields omitted when empty/default: `priority` (normal), `description`, `labels`, `depends`, `log`, `parent`
- `children` contains direct child IDs (injected, not stored); resolved deps are stripped from `depends`
- `log` entries: `{ timestamp, message, agent? }`

## Conventions

- Statuses: `todo`, `in-progress`, `done`, `dropped`
- Priorities: `critical`, `high`, `normal` (default), `low`
- Parents group subtasks; deps enforce ordering — a task won't appear in `sd next` until all deps are resolved
- Task descriptions define *what* needs doing; use `sd log` for decisions, rationale, and outcomes
- Use single quotes for shell arguments containing backticks to avoid command substitution
- Always use long flags (e.g. `--parent`, `--priority`) — short flags can be ambiguous (`-p` is `--priority`, not `--parent`)
