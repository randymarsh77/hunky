
# Quick Start Guide

## Running Git Stream

### 1. Build and Run

```bash
# In the git-stream directory
cargo run
```

The TUI will launch in your terminal showing:
- **Left panel**: List of changed files
- **Right panel**: Diff hunks for the currently selected file
- **Top bar**: Current mode and speed settings
- **Bottom bar**: Keyboard shortcuts

### 2. Try the Demo

In a separate terminal (while Git Stream is running):

```bash
./demo.sh
```

This will make several file changes that Git Stream will detect and display automatically!

### 3. Manual Testing

While Git Stream is running, edit any file in the repository:

```bash
# In another terminal
echo "# Test change" >> README.md
```

You'll see the changes appear in Git Stream instantly!

## Understanding the Interface

### File List (Left Panel)
- Shows all files with changes
- Number in parentheses shows hunk count
- **Yellow highlight** = currently selected file

### Diff Display (Right Panel)
- Shows diff hunks for the selected file
- **Green lines** = additions (+)
- **Red lines** = deletions (-)
- **White lines** = context
- **Cyan headers** = hunk boundaries

### Modes

**AUTO-STREAM Mode** (default)
- Hunks automatically appear over time
- Speed controlled by 'S' key
- Great for watching an AI agent work

**BUFFERED Mode**
- Press Space to see the next hunk
- Like the traditional `more` command
- Full manual control

Toggle with **Enter** or **Esc**

### Speed Settings

Press **S** to cycle through:
1. **Real-time**: ~100ms between hunks (fast)
2. **Slow**: 5 seconds between hunks
3. **Very Slow**: 10 seconds between hunks

## Example Workflow

1. **Start Git Stream in one terminal:**
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
   - Press `n` to jump to the next file
   - Press `p` to go to the previous file
   - Press `f` to toggle between full diff and filename-only view
   - Press `r` to refresh and capture current git state

5. **Press `q` to quit**

## Tips

- **Use with AI Agents**: Perfect for observing what coding agents like Aider, Cursor, or Copilot are changing
- **Large changes**: Use Buffered mode (Enter) and advance manually with Space
- **Quick overview**: Press `f` to see just filenames and hunk counts
- **Refresh on demand**: Press `r` if you want to capture the latest git state
- **Speed control**: In auto-stream mode, use `s` to slow down or speed up

## Troubleshooting

**"No changes"**: Make sure you have uncommitted changes in your git repo
```bash
git status  # Should show modified files
```

**App not updating**: Press `r` to manually refresh

**Too fast/slow**: Press `s` to cycle through speeds, or press Enter to switch to manual mode

## Next Steps

- Try the demo: `./demo.sh`
- Watch a real coding session
- Use it alongside your favorite AI coding agent
- Customize the key bindings (coming soon!)

Enjoy streaming your git changes! 🚀
