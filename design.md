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
  graph. Resolved deps are stripped so agents don't see false blockers. A task
  in `archive/` carries `archived: true`, which is the only way a client can
  tell — an archived task is serialized exactly like any other.
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

## Mac App

A native macOS app in `mac/`, built with SwiftUI. It is a client of the CLI,
not a second implementation: it shells out to `sd list --json` for reads and
to `sd add`, `sd edit` and `sd archive` for writes, so validation, DAG checks,
ID allocation and atomic writes stay in one place. No FFI, no shared Rust
code, no daemon.

### Layout

- `SeedKit` is the library: the model (`Task`, `TaskTree`, `Relation`), the
  CLI bridge (`SeedCLI`), the `Store` that owns one repository's tasks, its
  FSEvents watch and its command queue, and `Recents`. Everything worth a test
  lives here; `SeedKitTests` covers it, the queue included, against a stand-in
  `sd`.
- `Seed` is the SwiftUI layer. `Workspace` is one window's UI state
  (selection, expansion, search, the description draft) over one `Store`, and
  views reach the store as `workspace.store`. There is no `.xcodeproj`; Xcode
  opens `Package.swift`.
- `build.sh` assembles `Seed.app`. The release `sd` is bundled at
  `Contents/MacOS/sd` and both plist version keys are stamped from it, so the
  app and the CLI it runs are always one version. `defaults write
  net.zakj.seed seedBinaryPath <path>` points a build at another `sd`; there
  is no UI for it.
- `swift format` is the formatter and the linter (`mac/.swift-format`: 4-space
  indent, 100 columns). `mise run check` runs it, the Swift tests and the Rust
  side; the Swift tasks skip off macOS so `check` runs anywhere.

### Data flow

- **Reads reload everything.** Every change reloads the full list and filters
  client-side; `--status` on `sd list` would drop children whose parents were
  filtered out and break the tree. A reload is about 10 ms on this repository.
- **Freshness.** An FSEvents stream on `.seed` (not the repository, since the
  watch is recursive) sees every write, because `sd` writes through a temp
  file and a rename. Its 150 ms latency folds one command's several files
  into one reload. The stream is armed by the first reload that finds a
  `.seed`, since a stream over a missing path is inert, and the app also
  reloads on activation because App Nap coalesces the watcher by seconds.
- **Writes are serialized** through one task chain: `sd` refuses a write whose
  file changed since it read it, so overlapping edits would fail the second.
  A command's task resolves after the reload it triggers has landed, carrying
  stdout or nil on failure, which is how the composer and the description
  editor know whether to close. Reloads cancel their predecessor so an older
  read cannot land after a newer one. Failures queue for a window-modal alert.
- **Arguments** go as `--flag=value`, and `add`'s title after `--`: a value
  starting with `-` reads as a flag, and descriptions often open with a
  bullet.
- **Ordering** mirrors `Task::sort_key` (status rank, priority, id) so the
  tree matches `sd list`; the tests pin it.

### Windows

- One `Workspace` and one `Store` per window. The scene is a
  `WindowGroup(for: URL.self)` keyed by repository, so opening one already
  showing focuses that window. A window that adopts a repository at launch,
  on restore or from the Finder writes it back to the scene binding, or
  `openWindow(value:)` cannot tell and opens a second window onto the same
  store, defeating the write serialization. `@SceneStorage` keeps the
  repository across restoration. Menu commands reach the focused window's
  workspace through `focusedSceneValue`.
- `.newItem` is replaced, so ⌘N is New Task and there is no New Window; a
  second window comes from Open Repository or Open Recent. Recents are one
  list whose head is also what a repository-less window opens.
- A folder with no `.seed` is a state, not a failure: the pane offers `sd
  init`, and `sd prime --install claude` when the folder already carries
  `.claude` or `CLAUDE.md`.
- One-shot events (reveal a task, show a confirmation, focus search) travel
  through observable state as `Stamped<Value>`, which is equal only to itself,
  so setting the same value twice is still a change `onChange` sees.

### Tasks and the tree

- Expansion is the app's, not `List(children:)`'s, which owns its own state
  and cannot open a row the app needs to show. `TaskGraph` flattens to
  `OutlineRow`s against a set of expanded ids. `reveal` opens ancestors, and
  resets scope and search if they hide the task, for selection the app moves
  (a new subtask, a link); never for a click.
- Double-click and the row menu go through
  `contextMenu(forSelectionType:menu:primaryAction:)` on the list; ← and →
  collapse and expand. Search keeps the tree and dims the rows carried along
  as context. The smart lists stay flat: a parent that cannot be started is
  noise in "what can I start".
- The Task menu and a row's context menu render one `TaskActions`, whose
  buttons carry their key equivalents; a shortcut inside a context menu is
  displayed but never registered.
- A new task is named in the detail pane and nothing is written until
  Return, because `sd` has no delete. Escape discards.
- Archiving is a File submenu of counted outcomes ("Untouched for a Week
  (9)"), each behind a confirmation since nothing in the app unarchives.
  "Untouched" because `sd archive` compares a task's last change, not when it
  resolved.
- Everything is reachable from the keyboard: ⌘F search, ⌘1–⌘4 the smart
  lists, ⌘E the description, ⌘⇧R relations, ⌘⇧C copy the id, with a
  confirmation the window shows and announces to VoiceOver.

### Detail pane

- A document, not a form: title, a dim id-and-dates line, then status,
  priority and labels in one strip, with labels dropping to their own row
  whole when the strip will not fit. Three type sizes, 17 for the title, 14
  for prose and 13 for the rest; hierarchy is carried by colour.
- Descriptions and log entries render through `gonzalezreal/textual` as one
  document, which is what lets a selection span paragraphs; SwiftUI has no
  shared selection across sibling `Text`s. Heading sizes are the one thing
  overridden, since Textual's h1 is 33pt. A ```` ```math ```` fence is
  relabelled `latex` before parsing and `build.sh` drops the SwiftUIMath
  bundle: its bundle accessor traps inside a signed `.app`. Both go away with
  textual PR 82.
- Reading is the common case, so the rendered text keeps selection and live
  links, and editing is a deliberate act: ⌘E or the footer control swaps in
  an `NSTextView` that fills the pane below the header and scrolls inside
  itself, the one arrangement AppKit keeps the caret on screen for. The draft
  lives on the `Workspace` beside the id it was typed for, and every route
  out (⌘E, Escape, a click elsewhere, ⌘N, a new selection, teardown) calls
  one idempotent `commitEditing()`. Leaving saves; the editor's undo covers a
  mistake before that.
- Relations read as one wrapping line of links each, and a task with none
  shows none; the menus add the first. One picker sheet serves blocked-by,
  blocks and parent as segments (⌘⇧[ and ⌘⇧] switch), keeping the filter and
  highlight. It dims what `sd` would refuse (a loop, a task inside its own
  descendant) and omits resolved tasks, whose dependency `sd` would strip.
- Both pickers are filter-first: focus stays in the filter field, which steps
  the highlight with ↑ and ↓ and activates with Return, so the list needs no
  focus. The labels popover is fixed at 220 by 200 because a popover takes
  its size at presentation and never resizes.

### Distribution

- CI runs `swift format lint`, `swift test` and `build.sh` on `macos-26`,
  then checks the bundle: the files, that the bundled `sd` and the plist agree
  with `Cargo.toml`, and the signature. The release job zips the app with
  `ditto` (plain `zip` drops the xattrs the signature depends on), checks the
  plist against the tag, and ships `Seed-<tag>-arm64.zip` beside the CLI
  tarballs. Apple Silicon only; the CLI tarballs cover Intel.
- Built on `macos-26` for the SDK's chrome; `Package.swift` keeps the 15.0
  deployment target, and nothing in the app needs API newer than
  `searchFocused` (15).
- The signature is ad-hoc, so macOS quarantines the download and blocks the
  first launch; the README carries the `xattr` override. Notarizing needs a
  Developer ID. The bundle ID is `net.zakj.seed`.
- Homebrew avoids the override: the cask (`mac/seed.rb.in`) clears the flag on
  install and upgrade. The release workflow's `homebrew` job publishes it to
  `zakj/homebrew-tap` and needs the `HOMEBREW_TAP_DEPLOY_KEY` secret, the
  private half of a write-enabled deploy key on the tap.
- The icon is `icon.svg`, rendered to the checked-in `Seed.icns` by
  `icon.sh`. It is drawn full-bleed for macOS 26's squircle mask, so earlier
  versions show it square and oversized; the fix is an Icon Composer asset
  (#114).

### Rejected

Decided against on evidence; do not re-litigate without new evidence.

- `List` for the pickers: it gives arrow keys only to a focused list, has no
  hover state, and spends its click on selection where these rows spend it on
  ticking.
- `ViewThatFits` to size the labels popover: a popover never resizes after
  presentation, so it opened stuck small from the menu bar.
- A tap gesture in a list row for double-click: it competes with the list for
  the click and selection breaks intermittently. It broke twice.
- Watching the repository root: recursive over `target/`, `node_modules/` and
  `.git`.
- An absolute-only path in the prime hook: it dangles when the app moves.
- A parent line in the detail pane: deferred; the tree carries parent.
- A Settings window for the `sd` path: a developer knob that cost a window.
  The `defaults` key stays.
- An editor that grows to its text: the measurement and its cache cost more
  than the layout was worth, so the editor fills the pane.

## Agent Priming

`sd prime` outputs a static markdown guide to stdout — a usage reference for AI
agent onboarding. Composable: can be wired into CLAUDE.md or other agent config
via hooks/scripts.

`sd prime --install <agent>` sets up the appropriate hook for a given agent.
Currently supports `claude`, which adds a `SessionStart` hook to
`.claude/settings.local.json`.

The hook tries `PATH` first and falls back to the binary that installed it:

```sh
sd prime 2>/dev/null || '/path/to/sd' prime
```

`PATH` first so that an `sd` installed later, or moved between package managers,
is the one that runs. The fallback is what a GUI-only user has — the copy inside
Seed.app is the only `sd` on their machine, and nothing put it on `PATH`. Claude
Code reports a hook it cannot run to a debug log and nowhere else, so a hook
naming an `sd` that is not there costs the agent its guide with nothing on screen
to say so. Re-installing replaces a hook holding either of the two commands `sd`
itself writes, rather than adding a second one beside a broken first. Any other
hook that reaches `sd prime` was written by a person and carries the rest of
their line with it, so it is left where it is.

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
