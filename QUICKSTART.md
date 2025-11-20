
# Quick Start Guide

## Running Hunky

### 1. Try the Demo

In a separate terminal:

```bash
./simulation.sh
```

### 2. Build and Run

```bash
# In the hunky directory
cargo run
```

The TUI will launch in your terminal showing:
- **Top bar**: Current mode, speed settings, and view mode
- **Left panel**: List of changed files with (unseen/total) hunk counts
- **Main panel**: Diff hunks for the currently selected file
- **Help sidebar**: Keyboard shortcuts (toggle with 'H')

Resume the simulation.
This will make several file changes that Hunky will detect and display automatically!

### 3. Manual Testing

While Hunky is running, edit any file in the repository:

```bash
# In another terminal
echo "# Test change" >> README.md
```

You'll see the changes appear in Hunky instantly!

## Understanding the Interface

### Header Bar
- **Title**: Shows "Hunky" and current view mode (All Changes / New Only)
- **Mode**: AUTO-STREAM or BUFFERED
- **Speed**: Fast, Medium, or Slow (dynamic based on hunk size)
- **Unseen**: Count of hunks you haven't viewed yet
- **Focus indicator**: `[FOCUSED]` shows which pane is active (use Tab to switch)
- **Help hint**: "H: Help" on the right

### File List (Left Panel)
- Shows all files with changes
- **(unseen/total)** shows hunk counts per file
- **Yellow highlight** = currently selected file
- **Cyan border** = focused (press Tab to focus, then use arrows to navigate files)

### Diff Display (Main Panel)
- Shows **one hunk at a time** for easier reading
- **Green background** = additions (+)
- **Red background** = deletions (-)
- **Dark gray context** = surrounding code (up to 5 lines before/after)
- **Cyan border** = focused (press Tab to focus, then use arrows to scroll)
- Hunk counter shows current position (e.g., "Hunk 3/12")

### Help Sidebar (Right Panel)
- Press **H** to toggle visibility
- Shows all available keyboard shortcuts
- Collapses to give more space for diffs

### Modes

**AUTO-STREAM Mode** (default)
- Hunks automatically advance based on hunk size
- Speed controlled by 'S' key
- Great for watching an AI agent work
- Timing: Base delay + time per change line

**BUFFERED Mode**
- Press Space to see the next hunk
- Press Shift+Space to go back to previous hunk
- Like the traditional `more` command
- Full manual control

Toggle with **M** key

### View Modes

**New Changes Only** (default)
- Only shows hunks you haven't seen yet
- Perfect for monitoring ongoing work
- Shows unseen count in header
- Press **V** to toggle

**All Changes**
- Shows all current hunks
- Good for reviewing everything
- Press **V** to toggle back

### Speed Settings

Press **S** to cycle through:
1. **Fast**: 0.3s base + 0.2s per change (snappy)
2. **Medium**: 0.5s base + 0.5s per change (comfortable)
3. **Slow**: 0.5s base + 1.0s per change (relaxed)

Times scale with hunk size - larger changes get more reading time!

## Example Workflow

1. **Start Hunky in one terminal:**
   ```bash
   cargo run
   ```

2. **In another terminal, make changes:**
   ```bash
   # Edit a file
   vim src/main.rs
   
   # Or run an automated process
   cargo fmt
   
   # Or let an AI agent modify files
   aider --yes "add error handling to main.rs"
   ```

3. **Watch the changes stream in real-time!**

4. **Navigate:**
   - Press **Tab** to switch focus between file list and diff view
   - When file list is focused:
     - Arrow keys (↑/↓) or j/k navigate files and jump to first hunk
   - When diff view is focused:
     - Arrow keys (↑/↓) or j/k scroll within the hunk
   - Press **Space** to advance to next hunk
   - Press **Shift+Space** to go back to previous hunk
   - Press **n/p** to jump between files directly
   - Press **h** to toggle help sidebar
   - Press **w** to toggle line wrapping
   - Press **f** to toggle filename-only view
   - Press **v** to toggle between "All Changes" and "New Only" modes
   - Press **m** to toggle between AUTO-STREAM and BUFFERED modes
   - Press **s** to cycle through speeds
   - Press **c** to clear all seen hunks (start fresh)
   - Press **r** to refresh and capture current git state

5. **Press `q` to quit**

## Tips

- **Use with AI Agents**: Perfect for observing what coding agents like Aider, Cursor, or Copilot are changing
- **Focus Navigation**: Press Tab to switch focus, then use arrow keys differently based on which pane is active
- **New Changes Only**: Use "New Only" mode (V) to only see changes you haven't reviewed yet
- **Large changes**: Use Buffered mode (M) and advance manually with Space
- **Go back**: Shift+Space lets you review previous hunks
- **Quick overview**: Press `f` to see just filenames and hunk counts
- **Help always available**: Press `h` to show/hide the help sidebar
- **Speed adapts**: Speed settings automatically adjust timing based on hunk size
- **Refresh on demand**: Press `r` if you want to capture the latest git state
- **Clear and restart**: Press `c` to mark all hunks as unseen and start fresh

## Troubleshooting

**"No changes"**: Make sure you have uncommitted changes in your git repo
```bash
git status  # Should show modified files
```

**App not updating**: Press `r` to manually refresh

**Too fast/slow**: Press `s` to cycle through speeds (Fast/Medium/Slow), or press M to switch to manual BUFFERED mode

**Want to see everything**: Press `v` to toggle from "New Only" to "All Changes" view

## Next Steps

- Try the demo: `./demo.sh`
- Watch a real coding session
- Use it alongside your favorite AI coding agent
- Customize the key bindings (coming soon!)

Enjoy streaming your git changes! 🚀
