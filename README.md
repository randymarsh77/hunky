# Hunky

```
 ██╗  ██╗██╗   ██╗███╗   ██╗██╗  ██╗██╗   ██╗
 ██║  ██║██║   ██║████╗  ██║██║ ██╔╝╚██╗ ██╔╝
 ███████║██║   ██║██╔██╗ ██║█████╔╝  ╚████╔╝ 
 ██╔══██║██║   ██║██║╚██╗██║██╔═██╗   ╚██╔╝  
 ██║  ██║╚██████╔╝██║ ╚████║██║  ██╗   ██║   
 ╚═╝  ╚═╝ ╚═════╝ ╚═╝  ╚═══╝╚═╝  ╚═╝   ╚═╝   
    Git changes, streamed in real-time 🔥
```

A Terminal UI (TUI) application for observing git changes in real-time, built with Rust and ratatui.

**Status**: ✨ Working and demonstrating real-time change detection!

## Overview

Hunky helps you observe file changes in a git repository as they happen, making it perfect for working alongside coding agents or watching automated processes modify your codebase.

## Features

- 📸 **Snapshot Tracking**: Captures the current state of `git diff` or `git status`
- 👁️ **Real-time Watching**: File system watcher detects changes to git-tracked files
- 📊 **Stream Display**: Shows changes as a stream with customizable speed
- 🎨 **Syntax Highlighting**: Language detection and syntax highlighting for diff hunks
- 🎮 **Interactive Modes**:
  - **Auto-Stream Mode**: Automatically advances through hunks at configurable speeds
  - **Buffered Mode**: Manual "more"-like navigation with space bar
- 🗂️ **File Grouping**: Changes are grouped by file with easy navigation
- ⚡ **Speed Control**: Real-time, slow (5s), or very slow (10s) hunk display

## Installation

### Prerequisites

- Nix with flakes enabled (recommended)
- Or: Rust toolchain (cargo, rustc)

### With Nix

```bash
# Enter the development environment
nix develop

# Build the project
cargo build --release

# Run it
cargo run --release
```

### Without Nix

Make sure you have Rust installed, then:

```bash
cargo build --release
cargo run --release
```

## Usage

Navigate to a git repository and run:

```bash
hunky
# or during development:
cargo run
```

### Key Bindings

| Key | Action |
|-----|--------|
| `q` or `Q` | Quit the application |
| `Ctrl+C` | Quit the application |
| `Enter` or `Esc` | Toggle between Auto-Stream and Buffered modes |
| `Space` | Advance to next hunk (in any mode) |
| `j` or `↓` | Scroll diff view down |
| `k` or `↑` | Scroll diff view up |
| `v` | Toggle between "All Changes" and "New Changes Only" view modes |
| `c` | Clear all seen hunks (reset tracking) |
| `n` | Next file |
| `p` | Previous file |
| `f` | Toggle between showing all hunks vs. file names only |
| `s` | Cycle through stream speeds (Real-time → Slow → Very Slow) |
| `r` | Refresh - capture a new snapshot of git changes |

### View Modes

**All Changes Mode**: Cycles through the current git status, showing all hunks repeatedly.

**New Changes Only Mode** (Default): Only shows hunks that haven't been seen yet. Once a hunk is displayed, it's marked as "seen" and won't be shown again. This is perfect for watching new changes as they come in from AI agents or automated processes.

- Hunks are tracked with a unique ID based on file path, line numbers, and content hash
- When a file changes and invalidates a hunk, it's automatically removed from the seen list
- Press `c` to clear all seen hunks and start fresh
- File list shows "unseen/total" hunk counts: e.g., `main.rs (2/5)` means 2 unseen out of 5 total hunks

### Stream Modes

**Auto-Stream Mode**: Changes appear automatically at the selected speed. Perfect for watching an AI agent work.

**Buffered Mode**: Manual control with space bar to advance. Like the classic `more` pager.

### Stream Speeds

- **Real-time**: Hunks appear as fast as possible (~100ms delay)
- **Slow**: 1 hunk every 5 seconds
- **Very Slow**: 1 hunk every 10 seconds

## Project Structure

```
hunky/
├── src/
│   ├── main.rs      # Entry point
│   ├── app.rs       # Main application logic and state
│   ├── git.rs       # Git operations (diff, status)
│   ├── diff.rs      # Diff data structures
│   ├── watcher.rs   # File system watcher
│   ├── syntax.rs    # Syntax highlighting
│   └── ui.rs        # TUI rendering with ratatui
├── Cargo.toml       # Rust dependencies
└── flake.nix        # Nix development environment
```

## Use Cases

1. **AI Agent Monitoring**: Watch in real-time as coding agents modify your repository
2. **Build Process Observation**: See what files are being generated or modified during builds
3. **Code Review**: Review changes in a streaming, organized manner
4. **Learning**: Understand how changes propagate through a codebase

## Dependencies

- `ratatui` - Terminal UI framework
- `crossterm` - Terminal manipulation
- `git2` - Git operations
- `notify` - File system watching
- `tokio` - Async runtime
- `syntect` - Syntax highlighting
- `similar` - Diff generation

## Development

The project uses Nix flakes for reproducible builds. The development environment includes:
- Rust toolchain
- rust-analyzer
- All necessary system dependencies

### Building

```bash
cargo build
```

### Running Tests

```bash
cargo test
```

### Running in Development Mode

```bash
cargo run
```

## License

This project is open source. Feel free to use and modify as needed.

## Future Enhancements

- [ ] Enhanced syntax highlighting integration in diffs
- [ ] Filter changes by file pattern
- [ ] Save/export snapshots
- [ ] Diff between snapshots
- [ ] Configurable key bindings
- [ ] Theme customization
- [ ] Search within diffs
- [ ] Git branch awareness
- [ ] Staged vs unstaged changes view

## Contributing

Contributions welcome! This is a tool for developers watching their code evolve.
