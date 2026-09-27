---
sidebar_position: 1
---

# Introduction

**Hunky** is a terminal UI that watches a git repository and shows each diff as it lands. Run it next to a coding agent to see what it's changing, stage what you want, and commit.

[Try it in your browser](https://hunky.sh).

## Features

- **Live diffs** — Watches the working tree and updates as files change.
- **Streaming** — Hides everything that existed when you started and shows new hunks one at a time, stepped manually or auto-advanced at a pace that scales with hunk size.
- **Staging** — Stage the current hunk, or switch to line mode and stage individual lines. Commit with your configured git editor.
- **Commit review** — Pick one of the last 20 commits and walk its hunks, marking each one accepted.
