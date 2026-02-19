---
sidebar_position: 4
---

# Architecture

## Project Structure

```
hunky/
├── src/
│   ├── main.rs      # Entry point and CLI parsing
│   ├── app.rs       # Main application logic and state
│   ├── git.rs       # Git operations (diff, status)
│   ├── diff.rs      # Diff data structures
│   ├── watcher.rs   # File system watcher
│   ├── syntax.rs    # Syntax highlighting
│   └── ui.rs        # TUI rendering with ratatui
├── Cargo.toml       # Rust dependencies
└── flake.nix        # Nix development environment
```

## Key Dependencies

| Crate | Purpose |
|-------|---------|
| `ratatui` | Terminal UI framework |
| `crossterm` | Terminal manipulation |
| `git2` | Git operations via libgit2 |
| `notify` | File system watching |
| `tokio` | Async runtime |
| `syntect` | Syntax highlighting |
| `similar` | Diff generation |
| `clap` | CLI argument parsing |

## Data Flow

1. **Watcher** (`watcher.rs`) monitors the file system for changes
2. **Git** (`git.rs`) captures diffs when changes are detected
3. **Diff** (`diff.rs`) structures the raw diff data into hunks
4. **App** (`app.rs`) manages state, navigation, and mode transitions
5. **UI** (`ui.rs`) renders the TUI using ratatui
