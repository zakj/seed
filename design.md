# Seed (`sd`) — Design Document

A task tracker for AI coding agents and humans. Fast, opinionated, simple.

## Principles

- **Easy for agents**: structured `--json` output, predictable CLI, no interactive
  prompts
- **Easy for humans**: readable KDL files on disk, lightweight TUI, short commands
- **Fast**: single binary, ~2-5ms startup, no daemon, no database
- **Simple**: file-per-task, free status transitions, no workflow enforcement

## Storage

```
.seed/
  tasks/
    1.kdl                // one file per task
    2.kdl
  archive/               // completed/dropped tasks
    3.kdl
```

Tasks are KDL files, one per task, stored in the repo. Independent changes to
different tasks never conflict in version control.

### Why KDL

- Human-readable and editable (comments, multiline strings, clean syntax)
- Round-trip parsing preserves formatting (kdl-rs)
- JSON for agent output via `--json` flag — two serde backends, not two architectures

## Data Model

### Task

```kdl
task id=7 status="in-progress" priority="high" {
  title "Add retry logic to API client"
  description #"""
    The API client silently drops failed requests.

    Found that the catch block in src/api/client.ts:142
    swallows the error without retrying or logging.
  """#
  labels "bug" "api"
  parent 3
  depends 5 6
  created "2026-03-03T10:00:00Z"
  modified "2026-03-03T14:30:00Z"
  log {
    entry ts="2026-03-03T14:30:00Z" agent="claude-session-abc" \
      "Root cause in src/api/client.ts:142"
  }
}
```

### Fields

| Field | Type | Notes |
|-------|------|-------|
| `id` | integer | Sequential, never reused. Human-friendly. |
| `title` | string | Short summary. |
| `status` | enum | `todo`, `in-progress`, `done`, `dropped` |
| `priority` | enum | `critical`, `high`, `normal`, `low`. Optional. |
| `description` | string | Multiline KDL raw string. Markdown content. |
| `labels` | string[] | Flat tags, no taxonomy. |
| `parent` | integer? | ID of parent task. Arbitrary nesting depth. |
| `depends` | integer[] | Task IDs that must be done first. DAG, validated acyclic. |
| `created` | ISO 8601 | Set on creation. |
| `modified` | ISO 8601 | Updated on any change. |
| `log` | entry[] | Append-only. Agent session notes for handoff. |

### Statuses

Four statuses: `todo`, `in-progress`, `done`, `dropped`.

Free transitions — no enforced state machine. The tool doesn't police workflow.

### Dependencies

Dependencies are enforced: `sd done` refuses to close a task with unmet
dependencies. `--force` to override.

Dependencies are separate from parent/child hierarchy. A task can depend on any
other task regardless of tree position.

### IDs

Plain sequential integers. `sd show 7` beats `sd show 7f3a9b2c`. IDs are never
reused; when task 7 is archived, 7 is retired. The next ID is derived from the
highest existing filename across tasks/ and archive/.

## CLI

Binary name: `sd`.

All commands support `--json` for structured output. Human-readable by default.
Never prompts interactively — TUI is the only interactive interface.

### Commands

```
sd add "title"                   Create task, print ID (-q for just ID)
sd list [<id>]                   Tree view (--flat, --json, --status, -l label)
                                 With <id>: scoped to subtree
sd show <id>                     Full task detail
sd edit <id>                     Open description in $EDITOR
sd edit <id> --field value       Flag-based field updates
sd start <id>                    Shorthand: edit --status in-progress
sd done <id>                     Mark done (validates deps/children)
sd drop <id>                     Mark dropped
sd log <id> "message"            Append to task log
sd next                          Ready tasks (deps met, no incomplete children, status todo)
sd prime                         Static markdown guide for AI agent onboarding
sd prime --install <agent>       Install agent hooks
sd archive                       Move resolved tasks to archive (optional age cutoff)
sd completions <shell>           Generate shell completions
sd tui                           Interactive terminal UI (alias: sd t)
```

### Agent-friendly design

- `--json` on every command: compact single-line output, stable schema, typed
  values. `sd show` returns an object; `sd list` / `sd next` return arrays of
  full task objects including `children` IDs, so one call gives the full task
  graph. Resolved deps are stripped so agents don't see false blockers.
- `-q` / `--quiet`: output just the ID for scripting
- Predictable exit codes: 0 success, 1 error, 2 usage (via clap)
- Errors to stderr, structured as JSON when `--json` is active
- Idempotent where sensible (`done` on already-done is a no-op)
- No interactive prompts, ever

### Human-friendly design

- Short binary name (`sd`)
- Tree view by default in `sd list`
- `sd start` / `sd done` / `sd drop` as status shorthands
- `sd tui` for browsing and light editing

## TUI

Lightweight interactive interface behind the `tui` feature flag (default on).
Built on ratatui with crossterm backend. Scope:

- View tasks in a nested tree
- Filter by status, priority, labels
- Navigate with keyboard (vim-style)
- Change status and priority inline
- View full task detail in a pane
- Create tasks (`a` for root, `A` for child of selected)
- Edit task titles inline (`e`), descriptions via `$EDITOR` (`E`)
- Change status (`s`/`d`/`x` for start/done/drop) and priority (`p` → sub-mode)
- Move tasks (`m` → move mode): select a new parent with Enter, `u` to unparent.
  Descendants of the moved task are invalid targets.
- Manage dependencies (`D` → dep mode): navigate and press Enter to toggle deps
  on/off. Cycle detection prevents invalid additions.
- Search (`/`): case-insensitive title substring + `#id` match. Matching tasks
  highlighted in tree. `n`/`N` cycle next/prev match. Works across Normal, Move,
  and Dep modes.
- Zoom (`z`): toggle full-width view of the active pane. `Tab` switches which
  pane is shown. Detail pane shows task title/id in the border when zoomed.
- Footer hints use greedy fitting: right hints are reserved first, then left
  hints are added one at a time until space runs out, giving graceful
  degradation at narrow widths.

Declarative keybinding tables in `tui/keys.rs` are the single source of truth
for key dispatch, footer hints, and help overlay (`?`).

## Agent Priming

`sd prime` outputs a static markdown guide to stdout — a usage reference for AI
agent onboarding. Composable: can be wired into CLAUDE.md or other agent config
via hooks/scripts.

`sd prime --install <agent>` sets up the appropriate hook for a given agent.
Currently supports `claude`, which adds a `SessionStart` hook to
`.claude/settings.local.json`.

## Sync (planned)

External system integration (GitHub Issues, Linear, Jira). Planned architecture:

- Local-first: local state is authoritative
- Conflict resolution: local wins, conflicts logged
- Polling, not webhooks (CLI tool, no server)
- Start with one-way push (local -> external), two-way later
- Config maps statuses and fields between systems

## Technical Choices

- **Language**: Rust
- **CLI**: clap (derive API)
- **TUI**: ratatui + crossterm (optional, `tui` feature flag)
- **Serialization**: kdl-rs for disk, serde_json for --json output
- **Distribution**: single static binary

## Code Patterns

- **Ops module**: core business logic lives in `ops.rs`, decoupled from CLI.
  Most CLI handlers in `main.rs` are thin wrappers that call ops functions and
  format output. This allows future consumers (e.g. TUI) to share the same
  logic.
- **File-per-task storage**: KDL on disk, JSON via `--json`, serde for both
- **Atomic writes**: temp file + rename for crash safety; mtime-based optimistic
  locking
- **Markdown rendering**: a shared IR (`markdown/ir.rs`) parses pulldown_cmark
  events into `Block`/`Inline` trees. CLI (`markdown/mod.rs`) and TUI
  (`tui/markdown.rs`) each walk the IR with their own rendering logic. Supports
  headings, paragraphs, code blocks, blockquotes, ordered/unordered lists,
  tables, rules, and inline formatting (bold, italic, code, links).
- **ANSI styling**: `anstyle` crate for styles, raw escape codes only in
  CLI markdown renderer for nesting
- **Error handling**: `thiserror` enum, `?` propagation, structured JSON errors
  with `--json`
- **Terminal output**: `visible_width()` strips ANSI for layout math; width
  capped at 80; `ActiveStyles` in `term.rs` replays raw SGR sequences across
  line breaks so background/color styles survive wrapping
- **Testing**: integration tests via `assert_cmd` in `tests/cli.rs`; unit tests
  in `markdown/ir.rs` (parser) and `markdown/mod.rs` (CLI rendering)
