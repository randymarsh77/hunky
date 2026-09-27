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

**[📖 Quick Start Guide](QUICKSTART.md)** | Get up and running in minutes!

## Overview

Hunky helps you observe file changes in a git repository as they happen, making it perfect for working alongside coding agents or watching automated processes modify your codebase.

## Features

- 📸 **Snapshot Tracking**: Captures the current state of `git diff` or `git status`
- 👁️ **Real-time Watching**: File system watcher detects changes to git-tracked files
- 🎯 **Smart Hunk Tracking**: Intelligent "seen" tracking - only shows new changes you haven't viewed
- 📊 **Stream Display**: Shows one hunk at a time with context lines and colored backgrounds
- 🎨 **Enhanced Diff Display**: Colored backgrounds for additions/deletions, context lines, file headers
- 🎮 **Interactive Modes**:
  - **Auto-Stream Mode**: Automatically advances through hunks at dynamic speeds
  - **Buffered Mode**: Manual navigation with Space (next) and Shift+Space (previous)
- 👀 **View Modes**:
  - **New Changes Only**: Stream only unseen hunks (perfect for AI agent monitoring)
  - **All Changes**: Review all current changes
- 🗂️ **File Grouping**: Changes are grouped by file with unseen/total counts
- ⚡ **Dynamic Speed Control**: Fast/Medium/Slow - timing adapts to hunk size
- 🔍 **Focus Navigation**: Tab to switch between file list and diff view
- 💪 **Line Wrapping**: Toggle with 'W' key for long lines
- ℹ️ **Help Sidebar**: Built-in help with 'H' key

## Installation

### Prerequisites

- Nix with flakes enabled (recommended)
- Or: Rust toolchain (cargo, rustc)

### Release binary compatibility

- Linux release binaries (`*-unknown-linux-gnu`) are for Linux hosts.
- On Windows, those Linux binaries run only inside WSL (with a Linux userland), not in native Windows terminals.
- Native Windows releases are published as `hunky-x86_64-pc-windows-msvc.zip` and `hunky-aarch64-pc-windows-msvc.zip`.

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

# Specify a different repository:
hunky --repo /path/to/repo
cargo run -- --repo /path/to/repo

# Skip the startup animation (any key also skips it):
hunky --no-splash
```

**See the [Quick Start Guide](QUICKSTART.md) for detailed instructions and tips!**

### Key Bindings

| Key | Action |
|-----|--------|
| `q` or `Q` | Quit the application |
| `Ctrl+C` | Quit the application |
| `Tab` | Toggle focus between file list and diff view |
| `Space` | Advance to next hunk |
| `Shift+Space` | Go back to previous hunk |
| `j` or `↓` | Scroll down (diff view) or navigate files (file list, when focused) |
| `k` or `↑` | Scroll up (diff view) or navigate files (file list, when focused) |
| `n` | Next file |
| `p` | Previous file |
| `m` | Toggle between Auto-Stream and Buffered modes |
| `v` | Toggle between "All Changes" and "New Changes Only" view modes |
| `s` | Cycle through stream speeds (Fast → Medium → Slow) |
| `w` | Toggle line wrapping |
| `h` | Toggle help sidebar |
| `c` | Clear all seen hunks (reset tracking) |
| `f` | Toggle between showing all hunks vs. file names only |
| `r` | Refresh - capture a new snapshot of git changes |

### View Modes

**All Changes Mode**: Cycles through the current git status, showing all hunks repeatedly.

**New Changes Only Mode** (Default): Only shows hunks that haven't been seen yet. Once a hunk is displayed, it's marked as "seen" and won't be shown again. This is perfect for watching new changes as they come in from AI agents or automated processes.

- Hunks are tracked with a unique ID based on file path, line numbers, and content hash
- When a file changes and invalidates a hunk, it's automatically removed from the seen list
- Press `c` to clear all seen hunks and start fresh
- File list shows "unseen/total" hunk counts: e.g., `main.rs (2/5)` means 2 unseen out of 5 total hunks

### Stream Modes

**Auto-Stream Mode**: Changes appear automatically at the selected speed with dynamic timing based on hunk size. Perfect for watching an AI agent work.

**Buffered Mode**: Manual control with Space to advance, Shift+Space to go back. Like the classic `more` pager.

Toggle with the **M** key.

### Stream Speeds

- **Fast**: 0.3s base + 0.2s per change line (snappy for quick reviews)
- **Medium**: 0.5s base + 0.5s per change line (comfortable pace)
- **Slow**: 0.5s base + 1.0s per change line (relaxed for careful reading)

Timing automatically scales with the size of each hunk - larger changes get more time!

Cycle with the **S** key.

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

### Coverage

```bash
# Install coverage tooling once
cargo install cargo-llvm-cov --locked

# Summary report
cargo cov

# LCOV output for CI tooling
cargo cov-lcov

# HTML report (opens target/llvm-cov/html/index.html)
cargo cov-html
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
