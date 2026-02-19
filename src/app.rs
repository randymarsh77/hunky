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

use crate::diff::{DiffSnapshot, FileChange};
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
pub enum StreamSpeed {
    Fast,    // 1x multiplier: 0.3s base + 0.2s per change
    Medium,  // 2x multiplier: 0.5s base + 0.5s per change
    Slow,    // 3x multiplier: 0.5s base + 1.0s per change
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum StreamingType {
    Auto(StreamSpeed),  // Automatically advance with timing based on speed
    Buffered,           // Manual advance with Space
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Mode {
    View,                      // View all current changes, full navigation
    Streaming(StreamingType),  // Stream new hunks as they arrive
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
    mode: Mode,
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
    // Snapshot index when we entered Streaming mode (everything before is "seen")
    streaming_start_snapshot: Option<usize>,
    show_extended_help: bool,
    extended_help_scroll_offset: u16,
    // Cached viewport heights to prevent scroll flashing
    last_diff_viewport_height: u16,
    last_help_viewport_height: u16,
    _watcher: FileWatcher,
}

impl App {
    pub async fn new(repo_path: &str) -> Result<Self> {
        let git_repo = GitRepo::new(repo_path)?;
        
        // Get initial snapshot
        let mut initial_snapshot = git_repo.get_diff_snapshot()?;
        
        // Detect staged lines for initial snapshot
        for file in &mut initial_snapshot.files {
            for hunk in &mut file.hunks {
                // Detect which lines are actually staged in git's index
                if let Ok(staged_indices) = git_repo.detect_staged_lines(hunk, &file.path) {
                    hunk.staged_line_indices = staged_indices;
                    
                    // Check if all change lines are staged
                    let total_change_lines = hunk.lines.iter()
                        .filter(|line| {
                            (line.starts_with('+') && !line.starts_with("+++")) ||
                            (line.starts_with('-') && !line.starts_with("---"))
                        })
                        .count();
                    
                    hunk.staged = hunk.staged_line_indices.len() == total_change_lines && total_change_lines > 0;
                }
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
            mode: Mode::View,  // Start in View mode
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
            streaming_start_snapshot: None,  // Not in streaming mode initially
            show_extended_help: false,
            extended_help_scroll_offset: 0,
            last_diff_viewport_height: 20,  // Reasonable default
            last_help_viewport_height: 20,  // Reasonable default
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
                
                // Detect staged lines for all hunks
                for file in &mut snapshot.files {
                    for hunk in &mut file.hunks {
                        // Detect which lines are actually staged in git's index
                        match self.git_repo.detect_staged_lines(hunk, &file.path) {
                            Ok(staged_indices) => {
                                hunk.staged_line_indices = staged_indices;
                                
                                // Check if all change lines are staged
                                let total_change_lines = hunk.lines.iter()
                                    .filter(|line| {
                                        (line.starts_with('+') && !line.starts_with("+++")) ||
                                        (line.starts_with('-') && !line.starts_with("---"))
                                    })
                                    .count();
                                
                                hunk.staged = hunk.staged_line_indices.len() == total_change_lines && total_change_lines > 0;
                                
                                if !hunk.staged_line_indices.is_empty() {
                                    debug_log(format!("Detected {} staged lines in hunk (total: {}, fully staged: {})", 
                                        hunk.staged_line_indices.len(), total_change_lines, hunk.staged));
                                }
                            }
                            Err(e) => {
                                debug_log(format!("Failed to detect staged lines: {}", e));
                            }
                        }
                    }
                }
                
                match self.mode {
                    Mode::View => {
                        // In View mode, update the current snapshot with new staged line info
                        // Replace the current snapshot entirely with the new one
                        if !self.snapshots.is_empty() {
                            self.snapshots[self.current_snapshot_index] = snapshot;
                            debug_log("Updated current snapshot in View mode".to_string());
                        }
                    }
                    Mode::Streaming(_) => {
                        // In Streaming mode, only add snapshots that arrived after we entered streaming
                        // These are "new" changes to stream
                        self.snapshots.push(snapshot);
                        debug_log(format!("Added new snapshot in Streaming mode. Total snapshots: {}", self.snapshots.len()));
                        
                        // If we're on an empty/old snapshot, advance to the new one
                        if let Some(start_idx) = self.streaming_start_snapshot {
                            if self.current_snapshot_index <= start_idx {
                                self.current_snapshot_index = self.snapshots.len() - 1;
                                self.current_file_index = 0;
                                self.current_hunk_index = 0;
                                debug_log("Advanced to new snapshot in Streaming mode".to_string());
                            }
                        }
                    }
                }
            }
            
            // Auto-advance in Streaming Auto mode
            if let Mode::Streaming(StreamingType::Auto(speed)) = self.mode {
                let elapsed = self.last_auto_advance.elapsed();
                // Get current hunk change count (not including context lines) for duration calculation
                let change_count = self.current_file()
                    .and_then(|f| f.hunks.get(self.current_hunk_index))
                    .map(|h| h.count_changes())
                    .unwrap_or(1); // Default to 1 change if no hunk
                if elapsed >= speed.duration_for_hunk(change_count) {
                    self.advance_hunk();
                    self.last_auto_advance = Instant::now();
                }
            }
            
            // Draw UI
            let mut diff_viewport_height = 0;
            let mut help_viewport_height = 0;
            terminal.draw(|f| {
                let ui = UI::new(self);
                let (diff_h, help_h, _file_list_h) = ui.draw(f);
                diff_viewport_height = diff_h;
                help_viewport_height = help_h;
            })?;
            
            // Cache viewport heights for next frame's pre-clamping
            self.last_diff_viewport_height = diff_viewport_height;
            self.last_help_viewport_height = help_viewport_height;
            
            // Clamp scroll offsets after drawing (still needed for content size changes)
            self.clamp_scroll_offset(diff_viewport_height);
            if self.show_help {
                self.clamp_help_scroll_offset(help_viewport_height);
            }
            if self.show_extended_help {
                self.clamp_extended_help_scroll_offset(help_viewport_height);
            }
            
            // Handle input (non-blocking)
            if event::poll(Duration::from_millis(50))? {
                if let Event::Key(key) = event::read()? {
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Char('Q') => break,
                        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => break,
                        KeyCode::Char(' ') if key.modifiers.contains(KeyModifiers::SHIFT) => {
                            // Shift+Space goes to previous hunk (works in View and Streaming Buffered)
                            debug_log(format!("Shift+Space pressed, mode: {:?}", self.mode));
                            match self.mode {
                                Mode::View => {
                                    self.previous_hunk();
                                }
                                Mode::Streaming(StreamingType::Buffered) => {
                                    // In buffered mode, allow going back if there's a previous hunk
                                    self.previous_hunk();
                                }
                                Mode::Streaming(StreamingType::Auto(_)) => {
                                    // Auto mode doesn't support going back
                                    debug_log("Auto mode - ignoring Shift+Space".to_string());
                                }
                            }
                        }
                        KeyCode::Char('m') => self.cycle_mode(),
                        KeyCode::Char(' ') => {
                            // Advance to next hunk
                            self.advance_hunk();
                        }
                        KeyCode::Char('b') | KeyCode::Char('B') => {
                            // 'b' for back - alternative to Shift+Space
                            debug_log("B key pressed (back)".to_string());
                            match self.mode {
                                Mode::View => {
                                    self.previous_hunk();
                                }
                                Mode::Streaming(StreamingType::Buffered) => {
                                    self.previous_hunk();
                                }
                                Mode::Streaming(StreamingType::Auto(_)) => {
                                    debug_log("Auto mode - ignoring back".to_string());
                                }
                            }
                        }
                        KeyCode::Tab => self.cycle_focus_forward(),
                        KeyCode::BackTab => self.cycle_focus_backward(),
                        KeyCode::Char('j') | KeyCode::Down => {
                            if self.show_extended_help {
                                // Scroll down in extended help - pre-clamp to prevent flashing
                                let content_height = self.extended_help_content_height() as u16;
                                let viewport_height = self.last_help_viewport_height;
                                if content_height > viewport_height {
                                    let max_scroll = content_height.saturating_sub(viewport_height);
                                    if self.extended_help_scroll_offset < max_scroll {
                                        self.extended_help_scroll_offset = self.extended_help_scroll_offset.saturating_add(1);
                                    }
                                }
                            } else {
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
                                            // Scroll down in hunk view - pre-clamp to prevent flashing
                                            let content_height = self.current_hunk_content_height() as u16;
                                            let viewport_height = self.last_diff_viewport_height;
                                            if content_height > viewport_height {
                                                let max_scroll = content_height.saturating_sub(viewport_height);
                                                if self.scroll_offset < max_scroll {
                                                    self.scroll_offset = self.scroll_offset.saturating_add(1);
                                                }
                                            }
                                        }
                                    }
                                    FocusPane::HelpSidebar => {
                                        // Scroll down in help sidebar - pre-clamp to prevent flashing
                                        let content_height = self.help_content_height() as u16;
                                        let viewport_height = self.last_help_viewport_height;
                                        if content_height > viewport_height {
                                            let max_scroll = content_height.saturating_sub(viewport_height);
                                            if self.help_scroll_offset < max_scroll {
                                                self.help_scroll_offset = self.help_scroll_offset.saturating_add(1);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        KeyCode::Char('k') | KeyCode::Up => {
                            if self.show_extended_help {
                                // Scroll up in extended help
                                self.extended_help_scroll_offset = self.extended_help_scroll_offset.saturating_sub(1);
                            } else {
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
                        KeyCode::Char('s') | KeyCode::Char('S') => {
                            // Stage/unstage current selection (smart toggle)
                            self.stage_current_selection();
                        }
                        KeyCode::Char('w') => {
                            // Toggle line wrapping
                            self.wrap_lines = !self.wrap_lines;
                        }
                        KeyCode::Char('y') => {
                            // Toggle syntax highlighting
                            self.syntax_highlighting = !self.syntax_highlighting;
                        }
                        KeyCode::Char('l') | KeyCode::Char('L') => self.toggle_line_selection_mode(),
                        KeyCode::Char('h') => {
                            // Toggle help sidebar
                            self.show_help = !self.show_help;
                            self.help_scroll_offset = 0;
                            // If hiding help and focus was on help sidebar, move focus to hunk view
                            if !self.show_help && self.focus == FocusPane::HelpSidebar {
                                self.focus = FocusPane::HunkView;
                            }
                        }
                        KeyCode::Char('H') => {
                            // Toggle extended help view
                            self.show_extended_help = !self.show_extended_help;
                            self.extended_help_scroll_offset = 0;
                        }
                        KeyCode::Esc => {
                            // Reset to defaults
                            self.show_extended_help = false;
                            self.extended_help_scroll_offset = 0;
                            self.mode = Mode::View;
                            self.line_selection_mode = false;
                            self.focus = FocusPane::HunkView;
                            self.show_help = false;
                            self.help_scroll_offset = 0;
                        }
                        _ => {}
                    }
                }
            }
        }
        
        Ok(())
    }
    
    fn advance_hunk(&mut self) {
        if self.snapshots.is_empty() {
            return;
        }
        
        let snapshot = &self.snapshots[self.current_snapshot_index];
        if snapshot.files.is_empty() {
            return;
        }
        
        // Clear line memory for current hunk before moving
        let old_hunk_key = (self.current_file_index, self.current_hunk_index);
        self.hunk_line_memory.remove(&old_hunk_key);
        
        // Bounds check
        if self.current_file_index >= snapshot.files.len() {
            return;
        }
        
        let file_hunks_len = snapshot.files[self.current_file_index].hunks.len();
        
        // Advance to next hunk
        self.current_hunk_index += 1;
        self.scroll_offset = 0;
        
        // If we've gone past the last hunk in this file, move to next file
        if self.current_hunk_index >= file_hunks_len {
            self.current_file_index += 1;
            self.current_hunk_index = 0;
            
            // If no more files, stay at the last hunk of the last file
            if self.current_file_index >= snapshot.files.len() {
                self.current_file_index = snapshot.files.len().saturating_sub(1);
                if let Some(last_file) = snapshot.files.get(self.current_file_index) {
                    self.current_hunk_index = last_file.hunks.len().saturating_sub(1);
                }
            }
        }
    }
    
    fn previous_hunk(&mut self) {
        debug_log("previous_hunk called".to_string());
        if self.snapshots.is_empty() {
            debug_log("No snapshots, returning".to_string());
            return;
        }
        
        // Check if we have files before proceeding
        let files_len = self.snapshots[self.current_snapshot_index].files.len();
        if files_len == 0 {
            debug_log("No files in snapshot, returning".to_string());
            return;
        }
        
        debug_log(format!("Before: file_idx={}, hunk_idx={}", self.current_file_index, self.current_hunk_index));
        
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
        
        debug_log(format!("After: file_idx={}, hunk_idx={}", self.current_file_index, self.current_hunk_index));
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

    fn cycle_mode(&mut self) {
        self.mode = match self.mode {
            Mode::View => {
                self.streaming_start_snapshot = Some(self.current_snapshot_index);
                debug_log(format!(
                    "Entering Streaming mode, baseline snapshot: {}",
                    self.current_snapshot_index
                ));
                Mode::Streaming(StreamingType::Buffered)
            }
            Mode::Streaming(StreamingType::Buffered) => {
                Mode::Streaming(StreamingType::Auto(StreamSpeed::Fast))
            }
            Mode::Streaming(StreamingType::Auto(StreamSpeed::Fast)) => {
                Mode::Streaming(StreamingType::Auto(StreamSpeed::Medium))
            }
            Mode::Streaming(StreamingType::Auto(StreamSpeed::Medium)) => {
                Mode::Streaming(StreamingType::Auto(StreamSpeed::Slow))
            }
            Mode::Streaming(StreamingType::Auto(StreamSpeed::Slow)) => {
                self.streaming_start_snapshot = None;
                self.current_snapshot_index = self.snapshots.len() - 1;
                self.current_file_index = 0;
                self.current_hunk_index = 0;
                debug_log("Exiting Streaming mode, back to View".to_string());
                Mode::View
            }
        };
        self.last_auto_advance = Instant::now();
    }

    fn cycle_focus_forward(&mut self) {
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

        if old_focus == FocusPane::HunkView
            && self.focus != FocusPane::HunkView
            && self.line_selection_mode
        {
            let hunk_key = (self.current_file_index, self.current_hunk_index);
            self.hunk_line_memory.insert(hunk_key, self.selected_line_index);
            self.line_selection_mode = false;
        }
    }

    fn cycle_focus_backward(&mut self) {
        let old_focus = self.focus;
        self.focus = match self.focus {
            FocusPane::FileList => {
                if self.show_help {
                    FocusPane::HelpSidebar
                } else {
                    FocusPane::HunkView
                }
            }
            FocusPane::HunkView => FocusPane::FileList,
            FocusPane::HelpSidebar => FocusPane::HunkView,
        };

        if old_focus == FocusPane::HunkView
            && self.focus != FocusPane::HunkView
            && self.line_selection_mode
        {
            let hunk_key = (self.current_file_index, self.current_hunk_index);
            self.hunk_line_memory.insert(hunk_key, self.selected_line_index);
            self.line_selection_mode = false;
        }
    }

    fn toggle_line_selection_mode(&mut self) {
        if self.focus == FocusPane::HunkView {
            if self.line_selection_mode {
                let hunk_key = (self.current_file_index, self.current_hunk_index);
                self.hunk_line_memory
                    .insert(hunk_key, self.selected_line_index);
                self.line_selection_mode = false;
            } else {
                self.line_selection_mode = true;
                let hunk_key = (self.current_file_index, self.current_hunk_index);

                if let Some(&saved_line) = self.hunk_line_memory.get(&hunk_key) {
                    self.selected_line_index = saved_line;
                } else {
                    self.select_first_change_line();
                }
            }
        }
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
                                        // Check if line is already staged
                                        let is_staged = hunk.staged_line_indices.contains(&self.selected_line_index);
                                        
                                        if is_staged {
                                            // Unstage the single line
                                            match self.git_repo.unstage_single_line(hunk, self.selected_line_index, &file.path) {
                                                Ok(_) => {
                                                    // Remove this line from staged indices
                                                    hunk.staged_line_indices.remove(&self.selected_line_index);
                                                    debug_log(format!("Unstaged line {} in {}", self.selected_line_index, file.path.display()));
                                                }
                                                Err(e) => {
                                                    debug_log(format!("Failed to unstage line: {}. Note: Line-level unstaging is experimental and may not work for all hunks. Consider unstaging the entire hunk with Shift+U instead.", e));
                                                }
                                            }
                                        } else {
                                            // Stage the single line
                                            match self.git_repo.stage_single_line(hunk, self.selected_line_index, &file.path) {
                                                Ok(_) => {
                                                    // Mark this line as staged
                                                    hunk.staged_line_indices.insert(self.selected_line_index);
                                                    debug_log(format!("Staged line {} in {}", self.selected_line_index, file.path.display()));
                                                }
                                                Err(e) => {
                                                    debug_log(format!("Failed to stage line: {}. Note: Line-level staging is experimental and may not work for all hunks. Consider staging the entire hunk with Shift+S instead.", e));
                                                }
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
                                // Count total change lines and staged lines
                                let total_change_lines = hunk.lines.iter().enumerate()
                                    .filter(|(_, line)| {
                                        (line.starts_with('+') && !line.starts_with("+++")) ||
                                        (line.starts_with('-') && !line.starts_with("---"))
                                    })
                                    .count();
                                
                                let staged_lines_count = hunk.staged_line_indices.len();
                                
                                // Determine staging state
                                if staged_lines_count == 0 {
                                    // Fully unstaged -> Stage everything
                                    match self.git_repo.stage_hunk(hunk, &file.path) {
                                        Ok(_) => {
                                            hunk.staged = true;
                                            // Mark all change lines as staged
                                            hunk.staged_line_indices.clear();
                                            for (idx, line) in hunk.lines.iter().enumerate() {
                                                if (line.starts_with('+') && !line.starts_with("+++")) ||
                                                   (line.starts_with('-') && !line.starts_with("---")) {
                                                    hunk.staged_line_indices.insert(idx);
                                                }
                                            }
                                            debug_log(format!("Staged hunk in {}", file.path.display()));
                                        }
                                        Err(e) => {
                                            debug_log(format!("Failed to stage hunk: {}", e));
                                        }
                                    }
                                } else if staged_lines_count == total_change_lines {
                                    // Fully staged -> Unstage everything
                                    match self.git_repo.unstage_hunk(hunk, &file.path) {
                                        Ok(_) => {
                                            hunk.staged = false;
                                            hunk.staged_line_indices.clear();
                                            debug_log(format!("Unstaged hunk in {}", file.path.display()));
                                        }
                                        Err(e) => {
                                            debug_log(format!("Failed to unstage hunk: {}", e));
                                        }
                                    }
                                } else {
                                    // Partially staged -> Stage the remaining unstaged lines
                                    // Find which lines are not yet staged
                                    let mut all_staged = true;
                                    for (idx, line) in hunk.lines.iter().enumerate() {
                                        if (line.starts_with('+') && !line.starts_with("+++")) ||
                                           (line.starts_with('-') && !line.starts_with("---")) {
                                            if !hunk.staged_line_indices.contains(&idx) {
                                                // Try to stage this line
                                                match self.git_repo.stage_single_line(hunk, idx, &file.path) {
                                                    Ok(_) => {
                                                        hunk.staged_line_indices.insert(idx);
                                                    }
                                                    Err(e) => {
                                                        debug_log(format!("Failed to stage line {}: {}", idx, e));
                                                        all_staged = false;
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    
                                    if all_staged {
                                        hunk.staged = true;
                                        debug_log(format!("Completed staging of partially staged hunk in {}", file.path.display()));
                                    } else {
                                        debug_log(format!("Partially completed staging in {}", file.path.display()));
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
                                        hunk.staged_line_indices.clear();
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
                                        // Mark all change lines as staged
                                        hunk.staged_line_indices.clear();
                                        for (idx, line) in hunk.lines.iter().enumerate() {
                                            if (line.starts_with('+') && !line.starts_with("+++")) ||
                                               (line.starts_with('-') && !line.starts_with("---")) {
                                                hunk.staged_line_indices.insert(idx);
                                            }
                                        }
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
    
    pub fn mode(&self) -> Mode {
        self.mode
    }
    
    pub fn line_selection_mode(&self) -> bool {
        self.line_selection_mode
    }
    
    pub fn selected_line_index(&self) -> usize {
        self.selected_line_index
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
    
    pub fn show_extended_help(&self) -> bool {
        self.show_extended_help
    }
    
    pub fn extended_help_scroll_offset(&self) -> u16 {
        self.extended_help_scroll_offset
    }
    
    pub fn syntax_highlighting(&self) -> bool {
        self.syntax_highlighting
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
        26 // Number of help lines in draw_help_sidebar
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
    
    /// Get the height (line count) of the extended help content
    pub fn extended_help_content_height(&self) -> usize {
        108 // Exact number of lines in draw_extended_help
    }
    
    /// Clamp extended help scroll offset to valid range based on content and viewport height
    pub fn clamp_extended_help_scroll_offset(&mut self, viewport_height: u16) {
        let content_height = self.extended_help_content_height() as u16;
        if content_height > viewport_height {
            let max_scroll = content_height.saturating_sub(viewport_height);
            self.extended_help_scroll_offset = self.extended_help_scroll_offset.min(max_scroll);
        } else {
            self.extended_help_scroll_offset = 0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diff::Hunk;
    use crate::ui::UI;
    use ratatui::{backend::TestBackend, Terminal};
    use std::fs;
    use std::path::PathBuf;
    use std::process::Command;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct TestRepo {
        path: PathBuf,
    }

    impl TestRepo {
        fn new() -> Self {
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("failed to get system time")
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "hunky-app-tests-{}-{}",
                std::process::id(),
                unique
            ));
            fs::create_dir_all(&path).expect("failed to create temp directory");
            run_git(&path, &["init"]);
            run_git(&path, &["config", "user.name", "Test User"]);
            run_git(&path, &["config", "user.email", "test@example.com"]);
            Self { path }
        }

        fn write_file(&self, rel_path: &str, content: &str) {
            fs::write(self.path.join(rel_path), content).expect("failed to write file");
        }

        fn commit_all(&self, message: &str) {
            run_git(&self.path, &["add", "."]);
            run_git(&self.path, &["commit", "-m", message]);
        }
    }

    impl Drop for TestRepo {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn run_git(repo_path: &std::path::Path, args: &[&str]) -> String {
        let output = Command::new("git")
            .args(args)
            .current_dir(repo_path)
            .output()
            .expect("failed to execute git");
        assert!(
            output.status.success(),
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).to_string()
    }

    fn render_buffer_to_string(terminal: &Terminal<TestBackend>) -> String {
        let buffer = terminal.backend().buffer();
        let mut rows = Vec::new();
        for y in 0..buffer.area.height {
            let mut row = String::new();
            for x in 0..buffer.area.width {
                row.push_str(
                    buffer
                        .cell((x, y))
                        .expect("buffer cell should be available")
                        .symbol(),
                );
            }
            rows.push(row);
        }
        rows.join("\n")
    }

    fn sample_snapshot() -> DiffSnapshot {
        let file1 = PathBuf::from("a.txt");
        let file2 = PathBuf::from("b.txt");
        DiffSnapshot {
            timestamp: SystemTime::now(),
            files: vec![
                FileChange {
                    path: file1.clone(),
                    status: "Modified".to_string(),
                    hunks: vec![Hunk::new(
                        1,
                        1,
                        vec!["-old\n".to_string(), "+new\n".to_string()],
                        &file1,
                    )],
                },
                FileChange {
                    path: file2.clone(),
                    status: "Modified".to_string(),
                    hunks: vec![Hunk::new(
                        1,
                        1,
                        vec!["-old2\n".to_string(), "+new2\n".to_string()],
                        &file2,
                    )],
                },
            ],
        }
    }

    #[tokio::test]
    async fn cycle_mode_transitions_and_resets_streaming_state() {
        let repo = TestRepo::new();
        let mut app = App::new(repo.path.to_str().expect("path should be utf-8"))
            .await
            .expect("failed to create app");
        app.snapshots = vec![sample_snapshot()];
        app.current_snapshot_index = 0;

        app.cycle_mode();
        assert_eq!(app.mode, Mode::Streaming(StreamingType::Buffered));
        assert_eq!(app.streaming_start_snapshot, Some(0));

        app.cycle_mode();
        assert_eq!(
            app.mode,
            Mode::Streaming(StreamingType::Auto(StreamSpeed::Fast))
        );
        app.cycle_mode();
        assert_eq!(
            app.mode,
            Mode::Streaming(StreamingType::Auto(StreamSpeed::Medium))
        );
        app.cycle_mode();
        assert_eq!(
            app.mode,
            Mode::Streaming(StreamingType::Auto(StreamSpeed::Slow))
        );

        app.current_file_index = 1;
        app.current_hunk_index = 1;
        app.cycle_mode();
        assert_eq!(app.mode, Mode::View);
        assert_eq!(app.streaming_start_snapshot, None);
        assert_eq!(app.current_file_index, 0);
        assert_eq!(app.current_hunk_index, 0);
    }

    #[tokio::test]
    async fn focus_cycle_saves_line_mode_and_handles_help_sidebar() {
        let repo = TestRepo::new();
        let mut app = App::new(repo.path.to_str().expect("path should be utf-8"))
            .await
            .expect("failed to create app");
        app.show_help = true;
        app.focus = FocusPane::HunkView;
        app.line_selection_mode = true;
        app.selected_line_index = 3;

        app.cycle_focus_forward();
        assert_eq!(app.focus, FocusPane::HelpSidebar);
        assert!(!app.line_selection_mode);
        assert_eq!(
            app.hunk_line_memory
                .get(&(app.current_file_index, app.current_hunk_index)),
            Some(&3)
        );

        app.cycle_focus_forward();
        assert_eq!(app.focus, FocusPane::FileList);
        app.cycle_focus_backward();
        assert_eq!(app.focus, FocusPane::HelpSidebar);
    }

    #[tokio::test]
    async fn toggle_line_selection_mode_restores_saved_line() {
        let repo = TestRepo::new();
        repo.write_file("example.txt", "line 1\nline 2\n");
        repo.commit_all("initial");
        repo.write_file("example.txt", "line 1\nline 2 updated\n");

        let mut app = App::new(repo.path.to_str().expect("path should be utf-8"))
            .await
            .expect("failed to create app");
        app.focus = FocusPane::HunkView;

        app.toggle_line_selection_mode();
        assert!(app.line_selection_mode);
        app.selected_line_index = 1;

        app.toggle_line_selection_mode();
        assert!(!app.line_selection_mode);
        app.selected_line_index = 0;

        app.toggle_line_selection_mode();
        assert!(app.line_selection_mode);
        assert_eq!(app.selected_line_index, 1);
    }

    #[tokio::test]
    async fn advance_hunk_stops_at_last_hunk() {
        let repo = TestRepo::new();
        let mut app = App::new(repo.path.to_str().expect("path should be utf-8"))
            .await
            .expect("failed to create app");
        app.snapshots = vec![sample_snapshot()];
        app.current_snapshot_index = 0;
        app.current_file_index = 1;
        app.current_hunk_index = 0;

        app.advance_hunk();
        assert_eq!(app.current_file_index, 1);
        assert_eq!(app.current_hunk_index, 0);
    }

    #[tokio::test]
    async fn ui_draw_renders_mode_and_help_states() {
        let repo = TestRepo::new();
        let mut app = App::new(repo.path.to_str().expect("path should be utf-8"))
            .await
            .expect("failed to create app");

        app.mode = Mode::Streaming(StreamingType::Auto(StreamSpeed::Fast));
        let ui = UI::new(&app);
        let backend = TestBackend::new(160, 30);
        let mut terminal = Terminal::new(backend).expect("failed to create terminal");
        terminal
            .draw(|frame| {
                ui.draw(frame);
            })
            .expect("failed to draw ui");
        let rendered = render_buffer_to_string(&terminal);
        assert!(rendered.contains("STREAMING (Auto - Fast)"));

        app.show_help = true;
        app.show_extended_help = false;
        let ui = UI::new(&app);
        terminal
            .draw(|frame| {
                ui.draw(frame);
            })
            .expect("failed to draw ui");
        let rendered = render_buffer_to_string(&terminal);
        assert!(rendered.contains("Keys"));

        app.show_extended_help = true;
        let ui = UI::new(&app);
        terminal
            .draw(|frame| {
                ui.draw(frame);
            })
            .expect("failed to draw ui");
        let rendered = render_buffer_to_string(&terminal);
        assert!(rendered.contains("Extended Help"));
    }
}
