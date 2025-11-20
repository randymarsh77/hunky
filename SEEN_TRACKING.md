# Seen Tracking Feature

## Overview

Git Stream now includes intelligent "seen" tracking for hunks, allowing you to stream only **new changes** as they appear, without repeatedly showing hunks you've already viewed.

## How It Works

### Hunk Identification

Each hunk is uniquely identified by:
- **File path**: The path to the modified file
- **Line numbers**: Old and new start line numbers
- **Content hash**: A hash of the actual diff content

This means if the same hunk appears again (same location, same changes), it's recognized as already seen.

### View Modes

#### 1. All Changes Mode
- Shows all hunks in the current git status
- Cycles through repeatedly
- Traditional behavior

#### 2. New Changes Only Mode (Default)
- Only shows hunks that haven't been seen yet
- Perfect for watching AI agents or automated processes
- Once displayed, a hunk is marked as "seen" and skipped
- The counter at the top shows remaining unseen hunks

### Visual Indicators

**File List:**
```
README.md (2/5)    ← 2 unseen out of 5 total hunks
app.rs (0/10)      ← All 10 hunks have been seen
```

**Diff Display:**
```
@@ -10,3 +10,5 @@         ← Unseen hunk (cyan)
@@ -20,4 +22,6 @@ [SEEN]  ← Already seen (dark gray)
```

## Usage

### Key Bindings

- **`v`**: Toggle between "All Changes" and "New Changes Only" modes
- **`c`**: Clear all seen hunks (start fresh)
- **`Space`**: Advance to next hunk (automatically marks current as seen)

### Typical Workflow

1. Start Git Stream in "New Changes Only" mode
2. Watch as an AI agent makes changes
3. Hunks appear as they're created
4. Once viewed, they disappear from the stream
5. The unseen counter shows how many changes remain
6. Press `c` to reset if you want to review all changes again

## Implementation Details

### Data Structures

**`HunkId`**: Unique identifier for each hunk
```rust
pub struct HunkId {
    pub file_path: PathBuf,
    pub old_start: usize,
    pub new_start: usize,
    pub content_hash: u64,
}
```

**`SeenTracker`**: Manages the set of seen hunks
```rust
pub struct SeenTracker {
    seen_hunks: HashSet<HunkId>,
}
```

**`Hunk`**: Extended with seen state
```rust
pub struct Hunk {
    pub old_start: usize,
    pub new_start: usize,
    pub lines: Vec<String>,
    pub seen: bool,          // ← New field
    pub id: HunkId,          // ← New field
}
```

### Invalidation

When a file changes in a way that affects existing hunks:
- The file watcher detects the change
- A new snapshot is created
- Old hunks with different content get new IDs
- They appear as "unseen" again (correct behavior)

This means:
- If code is edited in place, the old hunk becomes invalid
- The new version appears as an unseen hunk
- You only see the latest state of changes

## Benefits

### For AI Agent Observation
- Focus only on new changes as they happen
- Don't get overwhelmed by repeated views of the same changes
- Clear progress indicator (unseen count)
- Reset with `c` when starting a new task

### For Code Review
- Review each change once
- Track progress through a large changeset
- Skip around with `n`/`p` without losing your place

### For Learning
- Watch how changes propagate
- See cause and effect
- Understand what's being modified in real-time

## Example Session

```bash
# Start Git Stream
cargo run

# Header shows:
# Git Stream | New Only | AUTO-STREAM | Real-time | Unseen: 15

# Watch as changes flow in...
# File list updates: src/main.rs (3/5)
# Hunks appear one by one
# Counter decreases: Unseen: 12... 11... 10...

# Press 'v' to see all changes
# Header now: Git Stream | All Changes | ...

# Press 'v' again to go back to new only
# Press 'c' to clear and start fresh
# Header: Git Stream | New Only | ... | Unseen: 15
```

## Future Enhancements

Potential additions:
- [ ] Persistence: Save seen state between sessions
- [ ] Per-file seen tracking
- [ ] Time-based expiry (mark as unseen after X minutes)
- [ ] Export seen/unseen reports
- [ ] Regex filters for auto-marking as seen
- [ ] Integration with git commits (reset on commit)

## Technical Notes

- Seen state is stored in memory (lost on restart)
- Content hashing uses Rust's `DefaultHasher`
- O(1) lookup for seen status using `HashSet`
- Minimal performance impact even with thousands of hunks
- Thread-safe design for async file watching

---

This feature transforms Git Stream from a simple diff viewer into an intelligent change monitor, perfect for modern development workflows with AI assistance.
