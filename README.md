# Seed (`sd`)

A task tracker for AI coding agents and humans. Fast, file-per-task, no database.

## Install

Download a pre-built binary from
[GitHub Releases](https://github.com/zakj/seed/releases), or build from
source:

```
cargo install --path .
```

## Quick start

```sh
sd init                     # create .seed/ in your project
sd add "Fix login bug"      # create a task
sd list                     # see all tasks
sd start 1                  # mark in-progress
sd done 1                   # mark done
```

Use `--json` on any command for structured output. `sd tui` (or `sd t`) launches
an interactive terminal interface for browsing and managing tasks.

## AI agent integration

`sd prime` outputs a static usage guide for AI agent onboarding. To
automatically install the appropriate hook for your agent:

```sh
sd prime --install claude
```

This adds a `SessionStart` hook to `.claude/settings.local.json` so that agents
are primed with sd context at the start of each session. Restart Claude Code
after installing for the hook to take effect.

The hook runs whichever `sd` is on `PATH`, falling back to the one that installed
it, so it keeps working for an agent whose `PATH` does not carry `sd` at all.

## Mac app

A native SwiftUI app lives in `mac/`. It talks to the same `sd` binary and the
same `.seed` directory, so it stays in sync with agents working in the terminal.

Download `Seed-<version>-arm64.zip` from
[Releases](https://github.com/zakj/seed/releases), or build it:

```sh
mise run mac:build      # builds mac/Seed.app
open mac/Seed.app
```

The app is signed ad-hoc rather than with a Developer ID, so macOS quarantines
the download and refuses it on a double-click. Right-click → Open the first
time, or clear the flag:

```sh
xattr -d com.apple.quarantine /Applications/Seed.app
```

Apple Silicon only. The `sd` binary itself ships for Intel too.

Point it at any folder with File → Open Repository — if it isn't a Seed
repository yet, the app offers to create one. Each
repository opens in its own window, so you can watch several at once — merge
them into tabs from the Window menu. The app bundles its own copy of `sd`, so it
works without one installed.

## Details

See [design.md](design.md) for architecture, data model, and CLI reference.
