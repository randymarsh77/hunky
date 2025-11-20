# Bug Fixes: Scrolling and End-of-Stream

## Issues Fixed

### 1. Hunks Out of View
**Problem**: When there are many hunks per file, they overflow the visible area and cannot be seen.

**Solution**: Implemented scrolling support
- Added `scroll_offset: u16` to track vertical scroll position
- Bound to `j`/`k` keys and arrow keys (↑/↓)
- Applied via `.scroll((offset, 0))` on the Paragraph widget
- Resets to 0 when changing files

**Usage**:
```
j or ↓  - Scroll down
k or ↑  - Scroll up
```

### 2. Infinite Loop at End
**Problem**: When reaching the final hunk in NewChangesOnly mode, the app gets stuck repeatedly trying to advance and displaying the last hunk.

**Solution**: Added end-of-stream detection
- New field: `reached_end: bool` tracks if we've exhausted all unseen hunks
- `advance_hunk()` checks this flag and stops advancing in NewChangesOnly mode
- `skip_to_next_unseen_hunk()` detects when no more unseen hunks exist
- Visual indicator: Title shows `[END]` when reached

**Behavior**:
- In **NewChangesOnly mode**: Stops at the last unseen hunk, shows `[END]`
- In **AllChanges mode**: Continues cycling through all hunks normally
- Press `c` to clear seen hunks and reset the end flag
- Press `v` to toggle modes (also resets the end flag)
- New changes reset the end flag automatically

## Implementation Details

### New App Fields
```rust
pub struct App {
    // ... existing fields
    scroll_offset: u16,      // Vertical scroll position
    reached_end: bool,       // True when no more unseen hunks
}
```

### Key Methods Updated

**`advance_hunk()`**:
```rust
fn advance_hunk(&mut self) {
    // Stop if we've reached the end in NewChangesOnly mode
    if self.view_mode == ViewMode::NewChangesOnly && self.reached_end {
        return;
    }
    // ... rest of logic
}
```

**`skip_to_next_unseen_hunk()`**:
```rust
fn skip_to_next_unseen_hunk(&mut self) {
    // Track starting position
    let start_file = self.current_file_index;
    let start_hunk = self.current_hunk_index;
    
    loop {
        // ... search for unseen hunks
        
        // Detect if we've wrapped around
        if self.current_file_index > start_file || ... {
            self.reached_end = true;
            break;
        }
    }
}
```

**UI Updates**:
```rust
// Title shows end indicator
.title(format!(
    "{} (Hunk {}/{}{})",
    file.path.to_string_lossy(),
    hunk_index + 1,
    total_hunks,
    if reached_end { " [END]" } else { "" }
))
// Enable scrolling
.scroll((scroll_offset, 0))
```

### Event Handling
```rust
KeyCode::Char('j') | KeyCode::Down => {
    self.scroll_offset = self.scroll_offset.saturating_add(1);
}
KeyCode::Char('k') | KeyCode::Up => {
    self.scroll_offset = self.scroll_offset.saturating_sub(1);
}
KeyCode::Char('n') | KeyCode::Char('p') => {
    // ... change file
    self.scroll_offset = 0;  // Reset scroll on file change
}
KeyCode::Char('c') | KeyCode::Char('v') => {
    // ... clear or toggle
    self.reached_end = false;  // Reset end flag
}
```

## User Experience

### Before
- ❌ Large diffs invisible (overflow out of view)
- ❌ Stuck in infinite loop at the end
- ❌ No indication of completion

### After
- ✅ Scroll through large diffs with j/k or arrows
- ✅ Clean stop when all unseen hunks are shown
- ✅ Clear `[END]` indicator in title
- ✅ Scroll resets when changing files
- ✅ End flag resets appropriately

## Edge Cases Handled

1. **Wrapped around detection**: Tracks starting position to detect when we've checked all files
2. **Scroll bounds**: Uses `saturating_add/sub` to prevent overflow
3. **Mode switching**: Resets end flag when toggling view modes
4. **Refresh**: Resets both scroll and end flag on refresh
5. **AllChanges mode**: End detection only applies to NewChangesOnly mode

## Testing

To test these fixes:

1. **Scrolling**:
   ```bash
   # Create a file with many changes
   cargo run
   # Press 'j' repeatedly to scroll down
   # Press 'k' to scroll back up
   ```

2. **End detection**:
   ```bash
   # Start with some changes
   cargo run
   # Wait for all hunks to be displayed
   # Should show [END] in title
   # Should stop advancing automatically
   # Press 'c' to clear and start over
   ```

3. **Mode interaction**:
   ```bash
   # Get to [END] state
   # Press 'v' to toggle to AllChanges mode
   # Should continue cycling through hunks
   # Press 'v' again to go back
   ```

## Future Enhancements

- [ ] Page up/down keys for faster scrolling
- [ ] Auto-scroll to follow new content
- [ ] Scroll indicators (show position in content)
- [ ] Horizontal scrolling for long lines
- [ ] Jump to top/bottom (g/G vim-style)

---

These fixes significantly improve the usability of Hunky, especially when working with large changesets or watching long-running AI agent sessions.
