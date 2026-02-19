---
sidebar_position: 1
---

# Introduction

**Hunky** is a Terminal UI (TUI) application for observing git changes in real-time, built with Rust and [ratatui](https://ratatui.rs).

Hunky helps you observe file changes in a git repository as they happen, making it perfect for working alongside coding agents or watching automated processes modify your codebase.

## Features

- 📸 **Snapshot Tracking** — Captures the current state of `git diff` or `git status`
- 👁️ **Real-time Watching** — File system watcher detects changes to git-tracked files
- 🎯 **Smart Hunk Tracking** — Only shows new changes you haven't viewed
- 📊 **Stream Display** — Shows one hunk at a time with context lines and colored backgrounds
- 🎨 **Enhanced Diff Display** — Colored backgrounds for additions/deletions
- 🎮 **Interactive Modes** — Auto-Stream and Buffered navigation
- ⚡ **Dynamic Speed Control** — Timing adapts to hunk size
- 🔍 **Focus Navigation** — Tab between file list and diff view

## Use Cases

1. **AI Agent Monitoring** — Watch coding agents modify your repository in real-time
2. **Build Process Observation** — See what files are generated or modified during builds
3. **Code Review** — Review changes in a streaming, organized manner
4. **Learning** — Understand how changes propagate through a codebase
