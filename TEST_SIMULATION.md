# Test Simulation Guide

This document explains how to use the test simulation script to validate Hunky's functionality.

## Overview

The `simulate_changes.sh` script creates a controlled environment for testing Hunky by:
1. Cloning a test repository into a nested directory
2. Creating a branch 10 commits behind the default branch
3. Replaying commits one by one every 5 seconds to simulate live development

## Prerequisites

- Bash shell
- Git installed
- Internet connection (for cloning test repo)

## Usage

### Basic Usage

```bash
# Run the script with default settings (clones ratatui/ratatui)
./simulate_changes.sh
```

### Custom Repository

```bash
# Use a different repository
TEST_REPO_URL=https://github.com/owner/repo.git ./simulate_changes.sh
```

### Step-by-Step Test Process

1. **Start the simulation script** in one terminal:
   ```bash
   ./simulate_changes.sh
   ```
   
   The script will:
   - Clone the repository into `test-repo/`
   - Check out a branch 10 commits behind
   - Pause and wait for you to press Enter

2. **Start Hunky** in another terminal:
   ```bash
   cargo run -- --repo test-repo
   ```
   
   You should see:
   - All current changes marked as [SEEN] (grayed out)
   - The indicator showing you're at the end: [END]
   - "New Changes Only" mode active by default

3. **Press Enter** in the simulation terminal to start replaying commits

4. **Observe Hunky** as it:
   - Detects new changes from each replayed commit
   - Streams hunks one at a time
   - Highlights new (unseen) hunks in color
   - Marks viewed hunks as [SEEN]

## Key Bindings During Test

While Hunky is running, test these features:

- `Space` - Advance to next unseen hunk
- `n` - Skip to next file
- `p` - Go to previous file
- `j`/`↓` - Scroll down (if hunk is larger than screen)
- `k`/`↑` - Scroll up
- `v` - Toggle between "All Changes" and "New Changes Only" view
- `c` - Clear seen status (mark all as unseen again)
- `r` - Reset to beginning
- `m` - Toggle between auto-stream and buffered modes
- `s` - Cycle stream speed (real-time, slow, very slow)
- `q` - Quit

## What to Test

### Initial State
- [ ] All existing changes show as [SEEN] with gray color
- [ ] Shows [END] indicator
- [ ] No automatic advancement (nothing new to stream)

### As Commits Are Replayed
- [ ] New hunks appear with colored lines (green/red)
- [ ] Auto-stream mode advances through new hunks automatically
- [ ] Scroll resets to top when advancing to next hunk
- [ ] Only one hunk visible at a time (not all previous hunks)
- [ ] [SEEN] marker appears on hunks after viewing
- [ ] File list shows correct unseen/total counts

### Mode Toggles
- [ ] Pressing `v` switches to "All Changes" mode and shows previously seen hunks
- [ ] Pressing `v` again returns to "New Changes Only"
- [ ] Pressing `c` clears all seen status
- [ ] Pressing `m` switches between auto-stream and buffered modes

### Navigation
- [ ] `n`/`p` correctly navigate between files
- [ ] `j`/`k` scroll within large hunks
- [ ] Scrolling doesn't break when reaching last hunk

## Configuration

Edit these variables in the script to customize behavior:

```bash
TEST_REPO_URL="https://github.com/owner/repo.git"  # Repository to clone
NESTED_DIR="test-repo"                             # Directory name
COMMITS_BEHIND=10                                  # Number of commits to replay
DELAY_BETWEEN_COMMITS=5                            # Seconds between commits
```

## Cleanup

After testing, remove the test repository:

```bash
rm -rf test-repo
```

Or the script will automatically clean it up on the next run.

## Troubleshooting

### "Could not apply commit cleanly"
Some commits may have conflicts when cherry-picked. The script will skip these and continue.

### No new changes appearing
- Check that the simulation script is still running
- Verify Hunky is watching the correct directory (`--repo test-repo`)
- Ensure file watcher is working (check for console errors)

### All changes show immediately
- This might happen if commits have overlapping changes
- Try a different repository or adjust `COMMITS_BEHIND` value

## Example Output

When working correctly, Hunky should show something like:

```
Hunky - New Changes Only | Auto Stream (Real-Time) | Unseen: 3/15 [END]

Files Changed (3):
  src/main.rs (1/2)
  src/app.rs (2/3) ●
  tests/integration.rs (0/1)

diff --git a/src/app.rs b/src/app.rs
index abc123..def456 100644
--- a/src/app.rs
+++ b/src/app.rs
@@ -45,6 +45,8 @@
-    old line
+    new line
+    another new line

Space: Next Hunk | n/p: Next/Prev File | v: Toggle View | c: Clear Seen | q: Quit
```
