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

A native macOS app in `mac/`, built with SwiftUI. It is a client of the CLI, not
a second implementation: it shells out to `sd --json` for reads and to
`sd add`/`sd edit` for writes, so validation, DAG checks, ID allocation, and
atomic writes stay in one place. No FFI, no shared Rust code, no daemon.

- **Freshness**: an FSEvents stream watches `.seed`. Because `sd` writes through
  a temp file and a rename, a directory watch sees every change an agent makes.
  The stream's latency is the debounce — 150ms, which is what folds the several
  files one `sd` command touches into one reload. An occluded app gets App
  Napped, which coalesces that by seconds, so the app also reloads whenever it
  becomes active. A stream over a path that does not exist is inert rather than
  refused, so the watch is armed by the first reload that finds a store rather
  than by whoever opened the window — one rule, instead of every route into a
  repository having to remember. FSEvents reports a tree against a path rather
  than an inode, so
  `tasks/`, an `archive/` that does not exist until the first `sd archive`, and a
  directory replaced wholesale all arrive on the one watch without re-arming
  anything. `.seed` rather than the repository: the watch is recursive, and the
  project around it changes for reasons that are none of ours.
- **`mise run check` covers both languages.** `swift test` compiles the app
  target as well as the tests, so one command is also the check that the app
  still builds for the macOS version it claims to support. Off macOS the task
  skips rather than fails, so the same `check` runs anywhere. CI runs it as a
  second job on a `macos-15` runner — the app's own minimum, so the deployment
  target is a claim CI tests rather than one the developer's newer SDK hides.
- **Writes are serialized**: commands run one at a time, and a reload cancels the
  one before it. `sd` refuses a write whose task file changed since it read it,
  so two overlapping edits lose one silently; and three things ask for a reload
  (a command finishing, the watcher, the app coming forward), so an older read
  can otherwise land after a newer one. A command hands back the task carrying
  its output, which is how the description editor knows to stay open on failure
  rather than discarding what was typed.
- **Values are passed as `--flag=value`**, and `add`'s positional title after
  `--`. A value of its own that starts with `-` reads as a flag, and a
  description opening with a bullet list is the most ordinary text there is.
- **One `Workspace` per window**: the scene is a `WindowGroup(for: URL.self)`
  keyed by repository, so each window owns its tasks, its watcher, and its
  reloads, and two repositories can be open at once. Menu commands reach the
  right one through `focusedSceneValue`. A window opened by `openWindow(value:)`
  for a repository that is already open focuses that window instead of building
  a second view of it. That only works for windows whose scene value is set, and
  a window can adopt a repository three other ways — at launch, on restore, and
  from the Finder through `onOpenURL` — so it writes the repository back into
  the scene binding. Without that write-back the everyday Open Recent duplicates
  a window, and two `Workspace`es on one store defeat the write serialization.
  Restoration hands a window back without its value, so each window also keeps
  its repository in `@SceneStorage`. Replacing the `.newItem` command group puts
  New Task on ⌘N and removes the standard New Window with it; a second window
  comes from Open Repository… or from opening a folder. Tabs are `NSWindow`'s own,
  and appear at two windows exactly as they do anywhere else in macOS. The
  window owns its column visibility so the menu can say whether ⌘B will show or
  hide the sidebar, and toggling animates the way the toolbar's own button does;
  ⌘. does the same without earning a second menu item.
- **The pickers are one idea in one file.** `Pickers.swift` holds both, plus the
  parts they share — the filter field steps the highlight, the scroll view keeps
  it visible, a row is drawn as highlighted, and `PickerRow` draws the tick. They had been spelled out
  twice, and their paddings had already drifted apart.
- **Distribution is a zip, not an installer.** CI assembles the bundle on every
  run — `swift test` covers the code, but nothing else exercises `build.sh`, so
  without that step the Info.plist, the icon, the bundled `sd` and the signature
  only break at release time. Releases carry `Seed-<tag>-arm64.zip`, packaged
  with `ditto` because `zip` drops the symlinks and xattrs an `.app` signature
  depends on. Apple Silicon only: shipping one download beats asking a GUI user
  which chip they have, and the CLI tarballs already cover Intel. The signature
  is ad-hoc, so the download is quarantined and needs one right-click → Open;
  notarizing it needs a Developer ID, which is a paid account rather than a
  code change.
- **Bundled `sd`**: `build.sh` builds the Rust binary and copies it into
  `Seed.app/Contents/MacOS/sd`, so the app and the CLI it shells out to are
  always the same version and a launched app's bare `PATH` never matters. Always
  the release build — the app shells out on every reload and every edit, where
  debug costs ~85ms against ~10ms. A Settings override can point elsewhere.
- **A dependency is one edge read from both ends.** `sd` stores only what a
  task waits on, so what it holds up is derived, and both read as one wrapping
  line of links — the shape labels settled on. Neither line appears when empty:
  the menus are how a task gets its first relation, which keeps the pane quiet
  for the tasks that have none. Parent has no line of its own; the tree carries
  it, and a search no longer flattens the tree.
- **One picker serves blockers, blocked-by and parent**, the labels popover
  pointed at tasks. It dims what `sd` would refuse — a task already waiting on
  this one, one it already waits on, or a descendant — rather than offering a
  move and reporting the failure afterwards. Resolved tasks are left out of both
  dependency lists: `sd` drops a dependency once it is met, so the tick would
  vanish on the next reload. It is a sheet because the menu bar opens it too, and a
  menu item has nothing to anchor a popover to.
- **The three relations are segments of that one picker**, not three commands.
  They ask the same question of the same list of tasks, and the one you want is
  usually not the one you chose on the way in — so the segments switch between
  them without closing anything, keeping the filter and, when the task appears
  in both lists, the highlight. That collapses a three-item submenu to a single
  `Relations…` on ⌘⇧R, which is also the answer to a task with no relation yet:
  the pane shows a line only once there is something to show, so before that the
  menu is the whole affordance. ⌘⇧[ and ⌘⇧] move between segments, since focus
  belongs to the filter field and a segmented control is not in the tab order.
  The sheet is headed with the task rather than the relation — the segments name
  the relation, and a sheet that never says what it is about is worse for it.
- **A new task is named in the detail pane**, not in a sheet: the pane already
  edits titles inline, and a modal collecting one string was the first thing a
  new user met. Nothing is written until the title is committed — `sd` has no
  delete, so creating first would leave a dropped task holding an id every time
  someone changed their mind. Return creates, Escape discards, and leaving the
  field keeps whatever was typed and discards an empty one, which is the rule
  the title field beside it already follows. Only the title is offered, because
  nothing else would have anywhere to write yet.
- **A folder with no `.seed` is a state, not a failure.** It offers to run the
  two commands the CLI would: `sd init`, then — only if the folder already
  carries `.claude` or `CLAUDE.md`, which is what pre-ticks the box —
  `sd prime --install claude`. Priming needs a store to write beside, so the
  order is fixed, and a priming failure is reported without undoing the
  repository. Nothing here arms the watcher: the reload that follows any command
  does that, which is also what picks up an `sd init` run in a terminal while the
  window sits on this pane.
- **Everything is reachable from the keyboard.** ⌘F focuses the search field,
  ⌘1–⌘4 pick the four smart lists in the order the sidebar shows them, ⌘E starts
  and ends a description edit, ⌘⇧R opens the relations picker, and the labels
  popover has a menu item. The two that had no keyboard path — labels and the
  description — needed their state to move onto the window, since a menu item
  has no view to reach into. Copying an id is the third: it used to answer a
  click with a pill in the dates row and answer ⌘⇧C with nothing, which is why
  the shortcut read as not existing. One `copyID` on the workspace now raises
  one confirmation, so the two routes cannot look different.
- **The confirmation is the window's, not the pane's.** An id can be copied from
  a row's context menu while you are reading the list, so the acknowledgement
  sits at the bottom of the window rather than beside the id — and an overlay
  takes no part in layout, which retires the constraint the old pill worked
  under (it had to skip its vertical padding or it would grow the dates row and
  nudge the pane down). macOS ships no toast, but it ships the parts:
  `.regularMaterial` follows light and dark, turns opaque under Reduce
  Transparency and takes vibrancy from what it floats over, and `.tint` follows
  the accent colour from System Settings — all of which a mixed colour would
  have to reimplement and would still get wrong in the dark. It is spoken
  through an accessibility announcement, since nothing moves a cursor to a badge
  that merely appears, and it carries a token so copying the same id twice reads
  as two events rather than one.
- **Archiving is a menu of counted outcomes**, not a duration typed blind: the
  app already holds every resolved task, so each item says what it will move —
  `All Completed Tasks (14)`, `Untouched for a Day (9)` — and disables itself at
  zero. "Untouched" rather than "finished" because `sd archive` compares a
  task's last change, not when it was resolved. Nothing in the app brings an
  archived task back, so each runs behind a confirmation. `sd list -a` marks
  archived tasks so they are not counted twice.
- **Recent repositories** are the File menu's `Open Recent`, and their head is
  also what a window with no repository of its own opens — one list rather than
  a most-recent path stored separately from the menu.
- **The tree's expansion is the app's**, not `List(children:)`'s: that owns its
  own state, and nothing can open a row the app needs to show. `TaskGraph`
  flattens to `OutlineRow`s against a set of expanded ids, and rows draw their
  own indent and triangle. Reveal is for selection the app moves — a new
  subtask, a link in the detail pane — and never for a click, so arrowing
  through the list cannot unfold it. A search keeps the tree and dims the tasks
  carried along to show where a match sits — a query means "find this in my
  tree". The smart lists stay flat: those answer "what can I start", and a
  parent that cannot be started is noise in that answer. Double-click toggles a row through
  `contextMenu(forSelectionType:menu:primaryAction:)` on the list, which is also
  where the row menu lives: routed by the list's own selection, both act on what
  is selected, and neither competes with it. A tap gesture in the row does — put
  one there and selection stops working intermittently. A childless row draws
  the triangle invisibly, because omitting it makes the row measure differently
  and a level's titles stop lining up.
- **Filtering is client-side**: the app always loads the full task list. Passing
  `--status` to `sd list` would return children whose parents were filtered out,
  breaking the tree.
- **Ordering** mirrors `Task::sort_key` — status rank, then priority, then id —
  so the tree matches `sd list`. `SeedKitTests` pins this.
- **The CLI is driven asynchronously**: both pipes are drained by readability
  handlers while the process runs, because reading one to the end before the
  other deadlocks the moment the other fills its buffer. No thread is parked
  waiting for `sd`.
- **Markdown**: descriptions are parsed by `apple/swift-markdown` (cmark-gfm)
  and reduced to a flat block model in `SeedKit/Markdown.swift`, which SwiftUI
  renders natively. Separate from the Rust IR by necessity, but both sit on a
  real CommonMark+GFM parser rather than a hand-rolled one. Inline spans become
  `AttributedString` in `SeedKit`, built from cmark's tree — re-emitting them as
  markdown source for the renderer to parse again would reinterpret what the
  first pass had already resolved, so `\*literal\*` came back as emphasis.

The app icon is `icon.svg`, rendered to the checked-in `Seed.icns` by `icon.sh`
(needs `rsvg-convert`), so a build never depends on either. The artwork is
full-bleed and square: macOS 26 masks a legacy `.icns` into its own squircle and
applies a material to it, and art inset for the older shadow grid gets scaled up
to fill that mask and goes visibly soft. No legacy icon renders well on both, so
this one is drawn for 26; a version applying no mask shows it square and
oversized. The fix is an Icon Composer asset, which needs `actool` and a document
authored once in the GUI.

The app targets macOS 15. Nothing in it needs macOS 26 — the only thing standing
between it and macOS 14 is `searchFocused`, which ⌘F uses to put the cursor in
the search field.

Layout: `SeedKit` holds the model, CLI bridge, markdown parser, and the recent
repositories, and is unit tested — `Seed` is an executable target, so anything
worth a test lives in the library. `Seed` is the SwiftUI layer. `mac/build.sh` assembles `Seed.app` —
there is no `.xcodeproj`; Xcode opens `Package.swift` directly.

The detail pane is a document, not a form: title, a dim line carrying the id and
dates, then status, priority, and labels in one control strip, separated by
hairlines rather than the boxes `formStyle(.grouped)` draws. Parent and
blocked-by appear only when set. Three type sizes, no more: 17 for the title, 14
for prose, and 13 for everything else, where hierarchy is carried by colour —
secondary for bylines and relations, tertiary for ids. Small text reads as
decoration rather than as information a developer is meant to use.

- **Click the description to edit it.** Rendered markdown and an editable field
  cannot be the same view, and a button placed anywhere is a button away from
  the text it edits. Clicking the rendered description swaps in the source with
  focus; Escape, ⌘E again, or a click elsewhere saves it, the way the title field
  already behaves. **The draft lives on the window, next to the id of the task it
  was typed against** — `Workspace.editing`, not a `@State` string in the pane
  beside a flag. That pairing is what makes "which task has unsaved text" a
  question anything can ask, and every route out of an edit answers it by calling
  one idempotent `commitEditing()`: ⌘E, Escape, a click on empty space, ⌘N, a new
  selection, and the pane being torn down. Some of those arrive twice, and AppKit
  decides the order — clearing the draft before writing it is what makes the
  second arrival harmless. Earlier versions gated the commit on a flag that
  `selection` cleared on its way past, which silently dropped a description
  whenever the pane was swapped without the mouse, and aimed ⌘E at
  `NSApp.keyWindow`, which is a different window from the workspace's whenever a
  sheet or popover is open. Neither question exists once the draft carries its
  own id. What each state affords is said in one tertiary line under the
  description rather than in a tooltip — a tooltip covers the words it is
  describing — and that line always takes its own height, so neither hovering nor
  starting an edit moves the text below it. There is no cancel — the editor's own undo covers a mistake before
  you leave, and the tasks are in version control. Both fields are the same
  wrapping `NSTextField`, so the editor grows with its text rather than being a
  fixed box: empty, that box was a wall of nothing; long, it was a scroller
  inside a scroller. Taking focus leaves the caret at the end rather than
  selecting everything, since a description is usually added to. Clicking empty
  space moves no responder on its own, and a scroll view takes the click before
  anything drawn behind it could, so the field watches for a click outside itself
  and ends the edit — passing the event on untouched, so whatever the click was
  for still happens. Log entries render as markdown too — agents write them with
  code spans and lists.
- **The control strip fits or stacks.** Status and priority are pop-up buttons;
  labels are text, because they are glanced at far more often than changed.
  `ViewThatFits` drops the labels to a row of their own, whole, when they will
  not sit beside the pickers — they read as one run of text, so splitting them
  would read as two lists. Status and priority render through
  `Label(_:systemImage:)`: a pop-up button flattens composite content down to
  its first piece, so a hand-built stack loses either the icon or the text.
- **Labels are edited in a popover**, fixed at 220 by 200 with a filter field
  that also creates. A menu of every label in the repository takes both its
  height and its width from the repository rather than from the task. Creating
  sits in the list rather than after it, so ↓ reaches it like any other row. The
  height is fixed rather than fitted to the rows because a popover takes its
  size when it is presented and never resizes: a height that fit the filtered
  list would be the height the next filter is stuck with. The filter is cleared
  as the popover opens, not as the button is clicked — the menu bar opens it
  too, and by the time the popover itself appears it has already claimed its
  highlight.
- **Both pickers are filter-first.** Focus stays in the filter field, so the
  list below it never sees an arrow key: the field steps a highlight through the
  rows itself and Return acts on the highlighted one. A narrowed filter keeps
  the highlight when it survives the cut and takes the first row when it does
  not, and a click moves it too, so mouse and keyboard leave the same mark. A
  `List` was the obvious home for this and is the wrong one — it gives selection
  and arrow keys only to a focused list, offers no hover state, and takes the
  single click these rows spend on ticking. Because Return belongs to the field,
  the relations sheet's Done button is a cancel action: Escape closes it.

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
