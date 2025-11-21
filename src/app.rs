use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::{Backend, CrosstermBackend},
    Terminal,
};
use std::collections::HashMap;
use std::io::{self, Write};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

use crate::diff::{DiffSnapshot, FileChange, SeenTracker};
use crate::git::GitRepo;
use crate::ui::UI;
use crate::watcher::FileWatcher;

// Debug logging helper
fn debug_log(msg: String) {
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open("hunky-debug.log")
    {
        let _ = writeln!(file, "[{}] {}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs(), msg);
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ViewMode {
    AllChanges,      // Cycle through current git status (show all hunks)
    NewChangesOnly,  // Only show new unseen hunks
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum StreamMode {
    AutoStream,  // Automatically show hunks as they arrive
    BufferedMore, // Manual "more" mode - press space to see next hunk
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum StreamSpeed {
    Fast,    // 1x multiplier: 0.3s base + 0.2s per change
    Medium,  // 2x multiplier: 0.5s base + 0.5s per change
    Slow,    // 3x multiplier: 0.5s base + 1.0s per change
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FocusPane {
    FileList,
    HunkView,
    HelpSidebar,
}

impl StreamSpeed {
    pub fn duration_for_hunk(&self, change_count: usize) -> Duration {
        let (base_ms, per_change_ms) = match self {
            StreamSpeed::Fast => (300, 200),     // 0.3s base + 0.2s per change
            StreamSpeed::Medium => (500, 500),   // 0.5s base + 0.5s per change
            StreamSpeed::Slow => (500, 1000),    // 0.5s base + 1.0s per change
        };
        let total_ms = base_ms + (per_change_ms * change_count as u64);
        Duration::from_millis(total_ms)
    }
}

pub struct App {
    git_repo: GitRepo,
    snapshots: Vec<DiffSnapshot>,
    current_snapshot_index: usize,
    current_file_index: usize,
    current_hunk_index: usize,
    view_mode: ViewMode,
    mode: StreamMode,
    speed: StreamSpeed,
    seen_tracker: SeenTracker,
    show_filenames_only: bool,
    wrap_lines: bool,
    show_help: bool,
    syntax_highlighting: bool,
    focus: FocusPane,
    line_selection_mode: bool,
    selected_line_index: usize,
    // Track last selected line per hunk (file_index, hunk_index) -> line_index
    hunk_line_memory: HashMap<(usize, usize), usize>,
    snapshot_receiver: mpsc::UnboundedReceiver<DiffSnapshot>,
    last_auto_advance: Instant,
    scroll_offset: u16,
    help_scroll_offset: u16,
    reached_end: bool,
    _watcher: FileWatcher,
}

impl App {
    pub async fn new(repo_path: &str) -> Result<Self> {
        let git_repo = GitRepo::new(repo_path)?;
        
        // Get initial snapshot
        let mut initial_snapshot = git_repo.get_diff_snapshot()?;
        
        // Mark all initial hunks as seen
        let mut seen_tracker = SeenTracker::new();
        for file in &mut initial_snapshot.files {
            for hunk in &mut file.hunks {
                hunk.seen = true;
                seen_tracker.mark_seen(&hunk.id);
            }
        }
        
        // Set up file watcher
        let (tx, rx) = mpsc::unbounded_channel();
        let watcher = FileWatcher::new(git_repo.clone(), tx)?;
        
        let app = Self {
            git_repo,
            snapshots: vec![initial_snapshot],
            current_snapshot_index: 0,
            current_file_index: 0,
            current_hunk_index: 0,
            view_mode: ViewMode::NewChangesOnly,
            mode: StreamMode::AutoStream,
            speed: StreamSpeed::Fast,
            seen_tracker,
            show_filenames_only: false,
            wrap_lines: false,
            show_help: false,
            syntax_highlighting: true,  // Enabled by default
            focus: FocusPane::HunkView,
            line_selection_mode: false,
            selected_line_index: 0,
            hunk_line_memory: HashMap::new(),
            snapshot_receiver: rx,
            last_auto_advance: Instant::now(),
            scroll_offset: 0,
            help_scroll_offset: 0,
            reached_end: true,  // Start at end since all initial hunks are seen
            _watcher: watcher,
        };
        
        Ok(app)
    }
    
    pub async fn run(&mut self) -> Result<()> {
        // Setup terminal
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;
        
        let result = self.run_loop(&mut terminal).await;
        
        // Restore terminal
        disable_raw_mode()?;
        execute!(
            terminal.backend_mut(),
            LeaveAlternateScreen,
            DisableMouseCapture
        )?;
        terminal.show_cursor()?;
        
        result
    }
    
    async fn run_loop<B: Backend>(&mut self, terminal: &mut Terminal<B>) -> Result<()> {
        loop {
            // Check for new snapshots
            while let Ok(mut snapshot) = self.snapshot_receiver.try_recv() {
                debug_log(format!("Received snapshot with {} files", snapshot.files.len()));
                
                // Mark hunks as seen/unseen based on SeenTracker
                let mut has_unseen = false;
                for file in &mut snapshot.files {
                    for hunk in &mut file.hunks {
                        hunk.seen = self.seen_tracker.is_seen(&hunk.id);
                        if !hunk.seen {
                            has_unseen = true;
                            debug_log(format!("Found unseen hunk in {}: {:?}", file.path.display(), hunk.id));
                        }
                    }
                }
                
                debug_log(format!("Snapshot has unseen hunks: {}", has_unseen));
                
                self.snapshots.push(snapshot);
                
                // If we have new unseen hunks and we were at the end, reset to start streaming
                if has_unseen && self.reached_end {
                    debug_log("Resetting from end to stream new hunks".to_string());
                    self.reached_end = false;
                    // Switch to the latest snapshot
                    self.current_snapshot_index = self.snapshots.len() - 1;
                    self.current_file_index = 0;
                    self.current_hunk_index = 0;
                    // Skip to the first unseen hunk
                    self.skip_to_next_unseen_hunk();
                    debug_log(format!("Now at file {} hunk {}", self.current_file_index, self.current_hunk_index));
                }
            }
            
            // Auto-advance in AutoStream mode
            if self.mode == StreamMode::AutoStream {
                let elapsed = self.last_auto_advance.elapsed();
                // Get current hunk change count (not including context lines) for duration calculation
                let change_count = self.current_file()
                    .and_then(|f| f.hunks.get(self.current_hunk_index))
                    .map(|h| h.count_changes())
                    .unwrap_or(1); // Default to 1 change if no hunk
                if elapsed >= self.speed.duration_for_hunk(change_count) {
                    self.advance_hunk();
                    self.last_auto_advance = Instant::now();
                }
            }
            
            // Draw UI
            let mut diff_viewport_height = 0;
            let mut help_viewport_height = 0;
            terminal.draw(|f| {
                let ui = UI::new(self);
                let (diff_h, help_h) = ui.draw(f);
                diff_viewport_height = diff_h;
                help_viewport_height = help_h;
            })?;
            
            // Clamp scroll offsets after drawing
            self.clamp_scroll_offset(diff_viewport_height);
            if self.show_help {
                self.clamp_help_scroll_offset(help_viewport_height);
            }
            
            // Handle input (non-blocking)
            if event::poll(Duration::from_millis(50))? {
                if let Event::Key(key) = event::read()? {
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Char('Q') => break,
                        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => break,
                        KeyCode::Char(' ') if key.modifiers.contains(KeyModifiers::SHIFT) => {
                            // Shift+Space goes to previous hunk
                            self.previous_hunk();
                        }
                        KeyCode::Char('m') => {
                            // Toggle between AutoStream and BufferedMore
                            self.mode = match self.mode {
                                StreamMode::AutoStream => StreamMode::BufferedMore,
                                StreamMode::BufferedMore => StreamMode::AutoStream,
                            };
                            self.last_auto_advance = Instant::now();
                        }
                        KeyCode::Char(' ') => {
                            // Advance to next hunk
                            self.advance_hunk();
                        }
                        KeyCode::Tab => {
                            // Cycle focus between panes
                            let old_focus = self.focus;
                            self.focus = match self.focus {
                                FocusPane::FileList => FocusPane::HunkView,
                                FocusPane::HunkView => {
                                    if self.show_help {
                                        FocusPane::HelpSidebar
                                    } else {
                                        FocusPane::FileList
                                    }
                                }
                                FocusPane::HelpSidebar => FocusPane::FileList,
                            };
                            
                            // Exit line mode when leaving hunk view
                            if old_focus == FocusPane::HunkView && self.focus != FocusPane::HunkView {
                                if self.line_selection_mode {
                                    // Save the current line before exiting
                                    let hunk_key = (self.current_file_index, self.current_hunk_index);
                                    self.hunk_line_memory.insert(hunk_key, self.selected_line_index);
                                    self.line_selection_mode = false;
                                }
                            }
                        }
                        KeyCode::BackTab => {
                            // Shift+Tab also goes back (some terminals map Shift+Space to BackTab)
                            self.previous_hunk();
                        }
                        KeyCode::Char('j') | KeyCode::Down => {
                            match self.focus {
                                FocusPane::FileList => {
                                    // Navigate to next file and jump to its first hunk
                                    self.next_file();
                                    self.scroll_offset = 0;
                                }
                                FocusPane::HunkView => {
                                    if self.line_selection_mode {
                                        // Navigate to next change line
                                        self.next_change_line();
                                    } else {
                                        // Scroll down in hunk view - increment first, will clamp after draw
                                        self.scroll_offset = self.scroll_offset.saturating_add(1);
                                    }
                                }
                                FocusPane::HelpSidebar => {
                                    // Scroll down in help sidebar - increment first, will clamp after draw
                                    self.help_scroll_offset = self.help_scroll_offset.saturating_add(1);
                                }
                            }
                        }
                        KeyCode::Char('k') | KeyCode::Up => {
                            match self.focus {
                                FocusPane::FileList => {
                                    // Navigate to previous file and jump to its first hunk
                                    self.previous_file();
                                    self.scroll_offset = 0;
                                }
                                FocusPane::HunkView => {
                                    if self.line_selection_mode {
                                        // Navigate to previous change line
                                        self.previous_change_line();
                                    } else {
                                        // Scroll up in hunk view
                                        self.scroll_offset = self.scroll_offset.saturating_sub(1);
                                    }
                                }
                                FocusPane::HelpSidebar => {
                                    // Scroll up in help sidebar
                                    self.help_scroll_offset = self.help_scroll_offset.saturating_sub(1);
                                }
                            }
                        }
                        KeyCode::Char('n') => {
                            // Next file
                            self.next_file();
                            self.scroll_offset = 0;
                        }
                        KeyCode::Char('p') => {
                            // Previous file
                            self.previous_file();
                            self.scroll_offset = 0;
                        }
                        KeyCode::Char('f') => {
                            // Toggle filenames only
                            self.show_filenames_only = !self.show_filenames_only;
                        }
                        KeyCode::Char('s') => {
                            // Cycle through speeds
                            self.speed = match self.speed {
                                StreamSpeed::Fast => StreamSpeed::Medium,
                                StreamSpeed::Medium => StreamSpeed::Slow,
                                StreamSpeed::Slow => StreamSpeed::Fast,
                            };
                        }
                        KeyCode::Char('S') => {
                            // Stage current selection
                            self.stage_current_selection();
                        }
                        KeyCode::Char('v') => {
                            // Toggle view mode
                            self.view_mode = match self.view_mode {
                                ViewMode::AllChanges => ViewMode::NewChangesOnly,
                                ViewMode::NewChangesOnly => ViewMode::AllChanges,
                            };
                            self.reached_end = false;
                        }
                        KeyCode::Char('w') => {
                            // Toggle line wrapping
                            self.wrap_lines = !self.wrap_lines;
                        }
                        KeyCode::Char('y') => {
                            // Toggle syntax highlighting
                            self.syntax_highlighting = !self.syntax_highlighting;
                        }
                        KeyCode::Char('l') | KeyCode::Char('L') => {
                            // Toggle line selection mode (only when hunk view is focused)
                            if self.focus == FocusPane::HunkView {
                                if self.line_selection_mode {
                                    // Exiting line mode: save current line for this hunk
                                    let hunk_key = (self.current_file_index, self.current_hunk_index);
                                    self.hunk_line_memory.insert(hunk_key, self.selected_line_index);
                                    self.line_selection_mode = false;
                                } else {
                                    // Entering line mode: restore saved line or select first
                                    self.line_selection_mode = true;
                                    let hunk_key = (self.current_file_index, self.current_hunk_index);
                                    
                                    if let Some(&saved_line) = self.hunk_line_memory.get(&hunk_key) {
                                        // Restore previously selected line
                                        self.selected_line_index = saved_line;
                                    } else {
                                        // No saved line, find first change line
                                        self.select_first_change_line();
                                    }
                                }
                            }
                        }
                        KeyCode::Char('h') | KeyCode::Char('H') => {
                            // Toggle help display
                            self.show_help = !self.show_help;
                            self.help_scroll_offset = 0;
                            // If hiding help and focus was on help sidebar, move focus to hunk view
                            if !self.show_help && self.focus == FocusPane::HelpSidebar {
                                self.focus = FocusPane::HunkView;
                            }
                        }
                        KeyCode::Char('c') => {
                            // Clear seen hunks
                            self.seen_tracker.clear();
                            self.current_hunk_index = 0;
                            self.reached_end = false;
                        }
                        KeyCode::Char('r') => {
                            // Refresh - get new snapshot
                            let snapshot = self.git_repo.get_diff_snapshot()?;
                            self.snapshots.push(snapshot);
                            self.current_snapshot_index = self.snapshots.len() - 1;
                            self.current_file_index = 0;
                            self.current_hunk_index = 0;
                            self.scroll_offset = 0;
                            self.reached_end = false;
                        }
                        _ => {}
                    }
                }
            }
        }
        
        Ok(())
    }
    
    fn advance_hunk(&mut self) {
        // In NewChangesOnly mode, don't advance if we've reached the end
        if self.view_mode == ViewMode::NewChangesOnly && self.reached_end {
            return;
        }
        
        if self.snapshots.is_empty() {
            return;
        }
        
        let snapshot = &mut self.snapshots[self.current_snapshot_index];
        if snapshot.files.is_empty() {
            return;
        }
        
        // Mark current hunk as seen
        if let Some(file) = snapshot.files.get_mut(self.current_file_index) {
            if let Some(hunk) = file.hunks.get_mut(self.current_hunk_index) {
                if !hunk.seen {
                    hunk.seen = true;
                    self.seen_tracker.mark_seen(&hunk.id);
                }
            }
        }
        
        // Clear line memory for current hunk before moving
        let old_hunk_key = (self.current_file_index, self.current_hunk_index);
        self.hunk_line_memory.remove(&old_hunk_key);
        
        // Check if we have files before proceeding
        let snapshot_ref = &self.snapshots[self.current_snapshot_index];
        if snapshot_ref.files.is_empty() {
            return;
        }
        
        // Bounds check for current file index
        if self.current_file_index >= snapshot_ref.files.len() {
            self.current_file_index = 0;
            return;
        }
        
        // Store the length we need before borrowing
        let file_hunks_len = snapshot_ref.files[self.current_file_index].hunks.len();
        
        // Advance to next hunk
        self.current_hunk_index += 1;
        
        // Reset scroll when advancing to a new hunk
        self.scroll_offset = 0;
        
        // In NewChangesOnly mode, skip already-seen hunks
        if self.view_mode == ViewMode::NewChangesOnly {
            self.skip_to_next_unseen_hunk();
        }
        
        // If we've gone past the last hunk in this file, move to next file
        if self.current_hunk_index >= file_hunks_len {
            self.next_file();
        }
    }
    
    fn previous_hunk(&mut self) {
        if self.snapshots.is_empty() {
            return;
        }
        
        // Check if we have files before proceeding
        let files_len = self.snapshots[self.current_snapshot_index].files.len();
        if files_len == 0 {
            return;
        }
        
        // Clear line memory for current hunk before moving
        let old_hunk_key = (self.current_file_index, self.current_hunk_index);
        self.hunk_line_memory.remove(&old_hunk_key);
        
        // Reset scroll when moving to a different hunk
        self.scroll_offset = 0;
        
        // If we're at the first hunk of the current file, go to previous file's last hunk
        if self.current_hunk_index == 0 {
            self.previous_file();
            // Set to the last hunk of the new file
            let snapshot = &self.snapshots[self.current_snapshot_index];
            if self.current_file_index < snapshot.files.len() {
                let last_hunk_index = snapshot.files[self.current_file_index].hunks.len().saturating_sub(1);
                self.current_hunk_index = last_hunk_index;
            }
        } else {
            // Just go back one hunk in the current file
            self.current_hunk_index = self.current_hunk_index.saturating_sub(1);
        }
        
        // Clear the reached_end flag when going backwards
        self.reached_end = false;
    }
    
    fn skip_to_next_unseen_hunk(&mut self) {
        if self.snapshots.is_empty() {
            return;
        }
        
        let snapshot = &self.snapshots[self.current_snapshot_index];
        let total_files = snapshot.files.len();
        let mut files_checked = 0;
        
        // Keep advancing until we find an unseen hunk or run out of hunks/files
        loop {
            if self.current_file_index >= snapshot.files.len() {
                // Wrapped around or exhausted all files
                self.reached_end = true;
                break;
            }
            
            let file = &snapshot.files[self.current_file_index];
            
            // Check if current hunk is unseen
            if let Some(hunk) = file.hunks.get(self.current_hunk_index) {
                if !self.seen_tracker.is_seen(&hunk.id) {
                    // Found an unseen hunk
                    return;
                }
                // This hunk is seen, try next
                self.current_hunk_index += 1;
            } else {
                // No more hunks in this file, try next file
                self.current_file_index += 1;
                self.current_hunk_index = 0;
                files_checked += 1;
                
                // If we've checked all files, we're done
                if files_checked >= total_files {
                    self.reached_end = true;
                    break;
                }
            }
        }
    }
    
    fn next_file(&mut self) {
        if self.snapshots.is_empty() {
            return;
        }
        
        let snapshot = &self.snapshots[self.current_snapshot_index];
        if snapshot.files.is_empty() {
            return;
        }
        
        // Clear line memory for old file
        let old_file_index = self.current_file_index;
        
        // Calculate next file index before clearing memory
        let files_len = snapshot.files.len();
        self.current_file_index = (self.current_file_index + 1) % files_len;
        self.current_hunk_index = 0;
        
        // Now clear the memory for the old file (after we're done with snapshot)
        self.clear_line_memory_for_file(old_file_index);
    }
    
    fn previous_file(&mut self) {
        if self.snapshots.is_empty() {
            return;
        }
        
        let snapshot = &self.snapshots[self.current_snapshot_index];
        if snapshot.files.is_empty() {
            return;
        }
        
        // Clear line memory for old file
        let old_file_index = self.current_file_index;
        
        // Calculate previous file index before clearing memory
        let files_len = snapshot.files.len();
        if self.current_file_index == 0 {
            self.current_file_index = files_len - 1;
        } else {
            self.current_file_index -= 1;
        }
        self.current_hunk_index = 0;
        
        // Now clear the memory for the old file (after we're done with snapshot)
        self.clear_line_memory_for_file(old_file_index);
    }
    
    fn next_change_line(&mut self) {
        if let Some(snapshot) = self.current_snapshot() {
            if let Some(file) = snapshot.files.get(self.current_file_index) {
                if let Some(hunk) = file.hunks.get(self.current_hunk_index) {
                    // Build list of change lines (filter same way as UI does)
                    let changes: Vec<(usize, &String)> = hunk.lines.iter()
                        .enumerate()
                        .filter(|(_, line)| {
                            (line.starts_with('+') && !line.starts_with("+++")) ||
                            (line.starts_with('-') && !line.starts_with("---"))
                        })
                        .collect();
                    
                    if !changes.is_empty() {
                        // Find where we are in the changes list
                        let current_in_changes = changes.iter()
                            .position(|(idx, _)| *idx == self.selected_line_index);
                        
                        match current_in_changes {
                            Some(pos) if pos + 1 < changes.len() => {
                                // Move to next change
                                self.selected_line_index = changes[pos + 1].0;
                            }
                            None => {
                                // Not on a change line, go to first
                                self.selected_line_index = changes[0].0;
                            }
                            _ => {
                                // At the end, stay there (or could wrap to first)
                            }
                        }
                    }
                }
            }
        }
    }
    
    fn previous_change_line(&mut self) {
        if let Some(snapshot) = self.current_snapshot() {
            if let Some(file) = snapshot.files.get(self.current_file_index) {
                if let Some(hunk) = file.hunks.get(self.current_hunk_index) {
                    // Build list of change lines (filter same way as UI does)
                    let changes: Vec<(usize, &String)> = hunk.lines.iter()
                        .enumerate()
                        .filter(|(_, line)| {
                            (line.starts_with('+') && !line.starts_with("+++")) ||
                            (line.starts_with('-') && !line.starts_with("---"))
                        })
                        .collect();
                    
                    if !changes.is_empty() {
                        // Find where we are in the changes list
                        let current_in_changes = changes.iter()
                            .position(|(idx, _)| *idx == self.selected_line_index);
                        
                        match current_in_changes {
                            Some(pos) if pos > 0 => {
                                // Move to previous change
                                self.selected_line_index = changes[pos - 1].0;
                            }
                            None => {
                                // Not on a change line, go to last
                                self.selected_line_index = changes[changes.len() - 1].0;
                            }
                            _ => {
                                // At the beginning, stay there (or could wrap to last)
                            }
                        }
                    }
                }
            }
        }
    }
    
    fn select_first_change_line(&mut self) {
        if let Some(snapshot) = self.current_snapshot() {
            if let Some(file) = snapshot.files.get(self.current_file_index) {
                if let Some(hunk) = file.hunks.get(self.current_hunk_index) {
                    // Find first change line
                    for (idx, line) in hunk.lines.iter().enumerate() {
                        if (line.starts_with('+') && !line.starts_with("+++")) ||
                           (line.starts_with('-') && !line.starts_with("---")) {
                            self.selected_line_index = idx;
                            return;
                        }
                    }
                }
            }
        }
        // Fallback
        self.selected_line_index = 0;
    }
    
    fn clear_line_memory_for_file(&mut self, file_index: usize) {
        // Remove all entries for this file
        self.hunk_line_memory.retain(|(f_idx, _), _| *f_idx != file_index);
    }
    
    fn stage_current_selection(&mut self) {
        match self.focus {
            FocusPane::HunkView => {
                // Check if we're in line selection mode
                if self.line_selection_mode {
                    // Stage/unstage a single line
                    if let Some(snapshot) = self.snapshots.get_mut(self.current_snapshot_index) {
                        if let Some(file) = snapshot.files.get_mut(self.current_file_index) {
                            if let Some(hunk) = file.hunks.get_mut(self.current_hunk_index) {
                                // Get the selected line
                                if let Some(selected_line) = hunk.lines.get(self.selected_line_index) {
                                    // Only stage change lines (+ or -)
                                    if (selected_line.starts_with('+') && !selected_line.starts_with("+++")) ||
                                       (selected_line.starts_with('-') && !selected_line.starts_with("---")) {
                                        // Stage the single line
                                        match self.git_repo.stage_single_line(hunk, self.selected_line_index, &file.path) {
                                            Ok(_) => {
                                                debug_log(format!("Staged line in {}", file.path.display()));
                                            }
                                            Err(e) => {
                                                debug_log(format!("Failed to stage line: {}", e));
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                } else {
                    // Toggle staging for the current hunk
                    if let Some(snapshot) = self.snapshots.get_mut(self.current_snapshot_index) {
                        if let Some(file) = snapshot.files.get_mut(self.current_file_index) {
                            if let Some(hunk) = file.hunks.get_mut(self.current_hunk_index) {
                                if hunk.staged {
                                    // Unstage the hunk
                                    match self.git_repo.unstage_hunk(hunk, &file.path) {
                                        Ok(_) => {
                                            hunk.staged = false;
                                            debug_log(format!("Unstaged hunk in {}", file.path.display()));
                                        }
                                        Err(e) => {
                                            debug_log(format!("Failed to unstage hunk: {}", e));
                                        }
                                    }
                                } else {
                                    // Stage the hunk
                                    match self.git_repo.stage_hunk(hunk, &file.path) {
                                        Ok(_) => {
                                            hunk.staged = true;
                                            debug_log(format!("Staged hunk in {}", file.path.display()));
                                        }
                                        Err(e) => {
                                            debug_log(format!("Failed to stage hunk: {}", e));
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            FocusPane::FileList => {
                // Toggle staging for the entire file
                if let Some(snapshot) = self.snapshots.get_mut(self.current_snapshot_index) {
                    if let Some(file) = snapshot.files.get_mut(self.current_file_index) {
                        // Check if any hunks are staged
                        let any_staged = file.hunks.iter().any(|h| h.staged);
                        
                        if any_staged {
                            // Unstage the file
                            match self.git_repo.unstage_file(&file.path) {
                                Ok(_) => {
                                    // Mark all hunks as unstaged
                                    for hunk in &mut file.hunks {
                                        hunk.staged = false;
                                    }
                                    debug_log(format!("Unstaged file {}", file.path.display()));
                                }
                                Err(e) => {
                                    debug_log(format!("Failed to unstage file: {}", e));
                                }
                            }
                        } else {
                            // Stage the file
                            match self.git_repo.stage_file(&file.path) {
                                Ok(_) => {
                                    // Mark all hunks as staged
                                    for hunk in &mut file.hunks {
                                        hunk.staged = true;
                                    }
                                    debug_log(format!("Staged file {}", file.path.display()));
                                }
                                Err(e) => {
                                    debug_log(format!("Failed to stage file: {}", e));
                                }
                            }
                        }
                    }
                }
            }
            FocusPane::HelpSidebar => {
                // No staging action for help sidebar
            }
        }
    }
    
    pub fn current_snapshot(&self) -> Option<&DiffSnapshot> {
        self.snapshots.get(self.current_snapshot_index)
    }
    
    pub fn current_file(&self) -> Option<&FileChange> {
        self.current_snapshot()?
            .files
            .get(self.current_file_index)
    }
    
    pub fn current_file_index(&self) -> usize {
        self.current_file_index
    }
    
    pub fn current_hunk_index(&self) -> usize {
        self.current_hunk_index
    }
    
    pub fn scroll_offset(&self) -> u16 {
        self.scroll_offset
    }
    
    pub fn help_scroll_offset(&self) -> u16 {
        self.help_scroll_offset
    }
    
    pub fn reached_end(&self) -> bool {
        self.reached_end
    }
    
    pub fn view_mode(&self) -> ViewMode {
        self.view_mode
    }
    
    pub fn mode(&self) -> StreamMode {
        self.mode
    }
    
    pub fn line_selection_mode(&self) -> bool {
        self.line_selection_mode
    }
    
    pub fn selected_line_index(&self) -> usize {
        self.selected_line_index
    }
    
    pub fn speed(&self) -> StreamSpeed {
        self.speed
    }
    
    pub fn focus(&self) -> FocusPane {
        self.focus
    }
    
    pub fn show_filenames_only(&self) -> bool {
        self.show_filenames_only
    }
    
    pub fn wrap_lines(&self) -> bool {
        self.wrap_lines
    }
    
    pub fn show_help(&self) -> bool {
        self.show_help
    }
    
    pub fn syntax_highlighting(&self) -> bool {
        self.syntax_highlighting
    }
    
    pub fn unseen_hunk_count(&self) -> usize {
        if let Some(snapshot) = self.current_snapshot() {
            snapshot.files.iter()
                .flat_map(|f| &f.hunks)
                .filter(|h| !self.seen_tracker.is_seen(&h.id))
                .count()
        } else {
            0
        }
    }
    
    /// Get the height (line count) of the current hunk content
    pub fn current_hunk_content_height(&self) -> usize {
        if let Some(snapshot) = self.current_snapshot() {
            if let Some(file) = snapshot.files.get(self.current_file_index) {
                if let Some(hunk) = file.hunks.get(self.current_hunk_index) {
                    // Count: file header (2) + blank + hunk header + blank + context before (max 5) + changes + context after (max 5)
                    let mut context_before = 0;
                    let mut changes = 0;
                    let mut context_after = 0;
                    let mut in_changes = false;
                    
                    for line in &hunk.lines {
                        if line.starts_with('+') || line.starts_with('-') {
                            in_changes = true;
                            changes += 1;
                        } else if !in_changes {
                            context_before += 1;
                        } else {
                            context_after += 1;
                        }
                    }
                    
                    // Limit context to 5 lines each
                    let context_before_shown = context_before.min(5);
                    let context_after_shown = context_after.min(5);
                    
                    return 2 + 1 + 1 + 1 + context_before_shown + changes + context_after_shown;
                }
            }
        }
        0
    }
    
    /// Get the height (line count) of the help sidebar content
    pub fn help_content_height(&self) -> usize {
        17 // Number of help lines in draw_help_sidebar
    }
    
    /// Clamp scroll offset to valid range based on content and viewport height
    pub fn clamp_scroll_offset(&mut self, viewport_height: u16) {
        let content_height = self.current_hunk_content_height() as u16;
        if content_height > viewport_height {
            let max_scroll = content_height.saturating_sub(viewport_height);
            self.scroll_offset = self.scroll_offset.min(max_scroll);
        } else {
            self.scroll_offset = 0;
        }
    }
    
    /// Clamp help scroll offset to valid range based on content and viewport height
    pub fn clamp_help_scroll_offset(&mut self, viewport_height: u16) {
        let content_height = self.help_content_height() as u16;
        if content_height > viewport_height {
            let max_scroll = content_height.saturating_sub(viewport_height);
            self.help_scroll_offset = self.help_scroll_offset.min(max_scroll);
        } else {
            self.help_scroll_offset = 0;
        }
    }
}
