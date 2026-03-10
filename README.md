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

## Details

See [design.md](design.md) for architecture, data model, and CLI reference.
