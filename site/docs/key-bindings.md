---
sidebar_position: 3
---

# Key Bindings

Press `H` in Hunky to show these in a sidebar, or `Shift+H` for extended help.

## Navigation

| Key | Action |
|-----|--------|
| `q` / `Ctrl+C` | Quit |
| `Tab` / `Shift+Tab` | Cycle focus: file list, diff, help sidebar |
| `Space` | Next hunk |
| `b` / `Shift+Space` | Previous hunk (not in auto-stream) |
| `j` / `↓`, `k` / `↑` | Next/previous file (file list), scroll (diff), next/previous changed line (line mode) |
| `n` / `p` | Next/previous file |

## Modes

| Key | Action |
|-----|--------|
| `m` | Cycle mode: View → Streaming (Buffered) → Streaming (Auto Fast → Medium → Slow) → View |
| `r` | Review a commit: choose from the last 20 with `j`/`k` and `Enter` |
| `Esc` | Leave review, or reset to defaults (View mode, line mode off, help hidden) |

**View** shows every current change. **Streaming** records the state when you enter it and shows only hunks that appear afterwards. Buffered waits for `Space`; Auto advances on its own, allowing each hunk a base delay plus time per changed line:

| Speed | Base | Per changed line |
|-------|------|------------------|
| Fast | 0.3 s | 0.2 s |
| Medium | 0.5 s | 0.5 s |
| Slow | 0.5 s | 1.0 s |

## Staging and commits

| Key | Action |
|-----|--------|
| `s` | Stage/unstage the current hunk, or the selected line in line mode. In review, accept the hunk. |
| `l` | Toggle line mode |
| `c` | Commit with your configured git editor |

## Display

| Key | Action |
|-----|--------|
| `w` | Toggle line wrapping |
| `y` | Toggle syntax highlighting |
| `f` | Toggle filenames only |
| `h` | Toggle help sidebar |
| `H` | Toggle extended help |
