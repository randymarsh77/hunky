---
sidebar_position: 3
---

# Key Bindings

| Key | Action |
|-----|--------|
| `q` / `Q` | Quit the application |
| `Ctrl+C` | Quit the application |
| `Tab` | Toggle focus between file list and diff view |
| `Space` | Advance to next hunk |
| `Shift+Space` | Go back to previous hunk |
| `j` / `↓` | Scroll down (diff) or navigate files (file list) |
| `k` / `↑` | Scroll up (diff) or navigate files (file list) |
| `n` | Next file |
| `p` | Previous file |
| `m` | Toggle Auto-Stream / Buffered mode |
| `v` | Toggle All Changes / New Changes Only |
| `s` | Cycle stream speed (Fast → Medium → Slow) |
| `w` | Toggle line wrapping |
| `h` | Toggle help sidebar |
| `c` | Clear all seen hunks |
| `f` | Toggle hunk / filename-only view |
| `r` | Refresh — capture a new snapshot |

## View Modes

**All Changes** — Cycles through current git status, showing all hunks.

**New Changes Only** (default) — Only shows unseen hunks. Press `c` to reset.

## Stream Modes

**Auto-Stream** — Hunks advance automatically at the selected speed.

**Buffered** — Manual control with Space / Shift+Space.

## Stream Speeds

| Speed | Base Delay | Per Change Line |
|-------|-----------|-----------------|
| Fast | 0.3 s | + 0.2 s |
| Medium | 0.5 s | + 0.5 s |
| Slow | 0.5 s | + 1.0 s |
