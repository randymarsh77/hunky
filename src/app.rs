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
use std::io::{self};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

use crate::diff::{CommitInfo, DiffSnapshot, FileChange};
use crate::git::GitRepo;
use crate::ui::UI;
use crate::watcher::FileWatcher;

// Debug logging helper
fn debug_log(msg: String) {
    crate::logger::debug(msg);
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum StreamSpeed {
    Fast,   // 1x multiplier: 0.3s base + 0.2s per change
    Medium, // 2x multiplier: 0.5s base + 0.5s per change
    Slow,   // 3x multiplier: 0.5s base + 1.0s per change
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum StreamingType {
    Auto(StreamSpeed), // Automatically advance with timing based on speed
    Buffered,          // Manual advance with Space
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Mode {
    View,                     // View all current changes, full navigation
    Streaming(StreamingType), // Stream new hunks as they arrive
    Review,                   // Review hunks in a specific commit
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
            StreamSpeed::Fast => (300, 200),   // 0.3s base + 0.2s per change
            StreamSpeed::Medium => (500, 500), // 0.5s base + 0.5s per change
            StreamSpeed::Slow => (500, 1000),  // 0.5s base + 1.0s per change
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
    needs_full_redraw: bool,
    _watcher: FileWatcher,
    // Review mode state
    review_commits: Vec<CommitInfo>,
    review_commit_cursor: usize,
    review_selecting_commit: bool,
    review_snapshot: Option<DiffSnapshot>,
}

impl App {
    /// Toggle file-level staging and return whether snapshot refresh is needed.
    fn toggle_file_staging_for_change(git_repo: &GitRepo, file: &mut FileChange) -> bool {
        let any_staged = file.hunks.iter().any(|h| h.staged);
        if any_staged {
            match git_repo.unstage_file(&file.path) {
                Ok(_) => {
                    for hunk in &mut file.hunks {
                        hunk.staged = false;
                        hunk.staged_line_indices.clear();
                    }
                    debug_log(format!("Unstaged file {}", file.path.display()));
                    true
                }
                Err(e) => {
                    debug_log(format!("Failed to unstage file: {}", e));
                    false
                }
            }
        } else {
            match git_repo.stage_file(&file.path) {
                Ok(_) => {
                    for hunk in &mut file.hunks {
                        hunk.staged = true;
                        hunk.staged_line_indices.clear();
                        for (idx, line) in hunk.lines.iter().enumerate() {
                            if Self::is_diff_change_line(line) {
                                hunk.staged_line_indices.insert(idx);
                            }
                        }
                    }
                    debug_log(format!("Staged file {}", file.path.display()));
                    true
                }
                Err(e) => {
                    debug_log(format!("Failed to stage file: {}", e));
                    false
                }
            }
        }
    }

    fn is_diff_change_line(line: &str) -> bool {
        (line.starts_with('+') && !line.starts_with("+++"))
            || (line.starts_with('-') && !line.starts_with("---"))
    }

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
                    let total_change_lines = hunk
                        .lines
                        .iter()
                        .filter(|line| {
                            (line.starts_with('+') && !line.starts_with("+++"))
                                || (line.starts_with('-') && !line.starts_with("---"))
                        })
                        .count();

                    hunk.staged = hunk.staged_line_indices.len() == total_change_lines
                        && total_change_lines > 0;
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
            mode: Mode::View, // Start in View mode
            show_filenames_only: false,
            wrap_lines: false,
            show_help: false,
            syntax_highlighting: true, // Enabled by default
            focus: FocusPane::HunkView,
            line_selection_mode: false,
            selected_line_index: 0,
            hunk_line_memory: HashMap::new(),
            snapshot_receiver: rx,
            last_auto_advance: Instant::now(),
            scroll_offset: 0,
            help_scroll_offset: 0,
            streaming_start_snapshot: None, // Not in streaming mode initially
            show_extended_help: false,
            extended_help_scroll_offset: 0,
            last_diff_viewport_height: 20, // Reasonable default
            last_help_viewport_height: 20, // Reasonable default
            needs_full_redraw: true,
            _watcher: watcher,
            review_commits: Vec::new(),
            review_commit_cursor: 0,
            review_selecting_commit: false,
            review_snapshot: None,
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
                debug_log(format!(
                    "Received snapshot with {} files",
                    snapshot.files.len()
                ));

                // Detect staged lines for all hunks
                for file in &mut snapshot.files {
                    for hunk in &mut file.hunks {
                        // Detect which lines are actually staged in git's index
                        match self.git_repo.detect_staged_lines(hunk, &file.path) {
                            Ok(staged_indices) => {
                                hunk.staged_line_indices = staged_indices;

                                // Check if all change lines are staged
                                let total_change_lines = hunk
                                    .lines
                                    .iter()
                                    .filter(|line| {
                                        (line.starts_with('+') && !line.starts_with("+++"))
                                            || (line.starts_with('-') && !line.starts_with("---"))
                                    })
                                    .count();

                                hunk.staged = hunk.staged_line_indices.len() == total_change_lines
                                    && total_change_lines > 0;

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
                    Mode::Review => {
                        // In Review mode, ignore live snapshot updates (reviewing a commit)
                        debug_log("Ignoring snapshot update in Review mode".to_string());
                    }
                    Mode::Streaming(_) => {
                        // In Streaming mode, only add snapshots that arrived after we entered streaming
                        // These are "new" changes to stream
                        self.snapshots.push(snapshot);
                        debug_log(format!(
                            "Added new snapshot in Streaming mode. Total snapshots: {}",
                            self.snapshots.len()
                        ));

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
                let change_count = self
                    .current_file()
                    .and_then(|f| f.hunks.get(self.current_hunk_index))
                    .map(|h| h.count_changes())
                    .unwrap_or(1); // Default to 1 change if no hunk
                if elapsed >= speed.duration_for_hunk(change_count) {
                    self.advance_hunk();
                    self.last_auto_advance = Instant::now();
                }
            }

            // Draw UI
            if self.needs_full_redraw {
                terminal.clear()?;
                self.needs_full_redraw = false;
            }

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
                    // If the commit picker overlay is active, handle its keys first
                    if self.review_selecting_commit {
                        match key.code {
                            KeyCode::Char('q') | KeyCode::Char('Q') => break,
                            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                                break
                            }
                            KeyCode::Char('j') | KeyCode::Down => {
                                if !self.review_commits.is_empty()
                                    && self.review_commit_cursor + 1 < self.review_commits.len()
                                {
                                    self.review_commit_cursor += 1;
                                }
                            }
                            KeyCode::Char('k') | KeyCode::Up => {
                                self.review_commit_cursor =
                                    self.review_commit_cursor.saturating_sub(1);
                            }
                            KeyCode::Enter => {
                                self.select_review_commit();
                            }
                            KeyCode::Esc => {
                                // Cancel commit selection, go back to View mode
                                self.review_selecting_commit = false;
                                self.review_commits.clear();
                                self.mode = Mode::View;
                                debug_log("Cancelled review commit selection".to_string());
                            }
                            _ => {}
                        }
                        continue;
                    }

                    match key.code {
                        KeyCode::Char('q') | KeyCode::Char('Q') => break,
                        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            break
                        }
                        KeyCode::Char(' ') if key.modifiers.contains(KeyModifiers::SHIFT) => {
                            // Shift+Space goes to previous hunk (works in View, Streaming Buffered, and Review)
                            debug_log(format!("Shift+Space pressed, mode: {:?}", self.mode));
                            match self.mode {
                                Mode::View | Mode::Review => {
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
                        KeyCode::Char('r') | KeyCode::Char('R') => {
                            if self.mode != Mode::Review {
                                self.enter_review_mode();
                            }
                        }
                        KeyCode::Char('c') => {
                            if self.mode != Mode::Review {
                                if let Err(e) = self.open_commit_mode() {
                                    debug_log(format!("Failed to open commit mode: {}", e));
                                }
                            }
                        }
                        KeyCode::Char('m') => {
                            if self.mode != Mode::Review {
                                self.cycle_mode();
                            }
                        }
                        KeyCode::Char(' ') => {
                            // Advance to next hunk
                            self.advance_hunk();
                        }
                        KeyCode::Char('b') | KeyCode::Char('B') => {
                            // 'b' for back - alternative to Shift+Space
                            debug_log("B key pressed (back)".to_string());
                            match self.mode {
                                Mode::View | Mode::Review => {
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
                                        self.extended_help_scroll_offset =
                                            self.extended_help_scroll_offset.saturating_add(1);
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
                                            let content_height =
                                                self.current_hunk_content_height() as u16;
                                            let viewport_height = self.last_diff_viewport_height;
                                            if content_height > viewport_height {
                                                let max_scroll =
                                                    content_height.saturating_sub(viewport_height);
                                                if self.scroll_offset < max_scroll {
                                                    self.scroll_offset =
                                                        self.scroll_offset.saturating_add(1);
                                                }
                                            }
                                        }
                                    }
                                    FocusPane::HelpSidebar => {
                                        // Scroll down in help sidebar - pre-clamp to prevent flashing
                                        let content_height = self.help_content_height() as u16;
                                        let viewport_height = self.last_help_viewport_height;
                                        if content_height > viewport_height {
                                            let max_scroll =
                                                content_height.saturating_sub(viewport_height);
                                            if self.help_scroll_offset < max_scroll {
                                                self.help_scroll_offset =
                                                    self.help_scroll_offset.saturating_add(1);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        KeyCode::Char('k') | KeyCode::Up => {
                            if self.show_extended_help {
                                // Scroll up in extended help
                                self.extended_help_scroll_offset =
                                    self.extended_help_scroll_offset.saturating_sub(1);
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
                                            self.scroll_offset =
                                                self.scroll_offset.saturating_sub(1);
                                        }
                                    }
                                    FocusPane::HelpSidebar => {
                                        // Scroll up in help sidebar
                                        self.help_scroll_offset =
                                            self.help_scroll_offset.saturating_sub(1);
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
                            if self.mode == Mode::Review {
                                // In review mode, toggle acceptance of the current hunk (in-memory)
                                self.toggle_review_acceptance();
                            } else {
                                // Stage/unstage current selection (smart toggle)
                                self.stage_current_selection();
                            }
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
                            self.toggle_line_selection_mode()
                        }
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
                            if self.mode == Mode::Review {
                                // Exit review mode, go back to View
                                self.exit_review_mode();
                            } else {
                                // Reset to defaults
                                self.show_extended_help = false;
                                self.extended_help_scroll_offset = 0;
                                self.mode = Mode::View;
                                self.line_selection_mode = false;
                                self.focus = FocusPane::HunkView;
                                self.show_help = false;
                                self.help_scroll_offset = 0;
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        Ok(())
    }

    fn advance_hunk(&mut self) {
        // Get needed info from snapshot without holding borrow on self
        let (files_len, file_hunks_len) = {
            let snapshot = match self.active_snapshot() {
                Some(s) if !s.files.is_empty() => s,
                _ => return,
            };
            let fl = snapshot.files.len();
            if self.current_file_index >= fl {
                return;
            }
            (fl, snapshot.files[self.current_file_index].hunks.len())
        };

        // Clear line memory for current hunk before moving
        let old_hunk_key = (self.current_file_index, self.current_hunk_index);
        self.hunk_line_memory.remove(&old_hunk_key);

        // Advance to next hunk
        self.current_hunk_index += 1;
        self.scroll_offset = 0;

        // If we've gone past the last hunk in this file, move to next file
        if self.current_hunk_index >= file_hunks_len {
            self.current_file_index += 1;
            self.current_hunk_index = 0;

            // If no more files, behavior depends on mode
            if self.current_file_index >= files_len {
                if self.mode == Mode::View || self.mode == Mode::Review {
                    // View/Review mode: wrap to the first hunk of the first file
                    self.current_file_index = 0;
                    self.current_hunk_index = 0;
                } else {
                    // Streaming/Buffered: pager semantics, stay at last hunk
                    self.current_file_index = files_len.saturating_sub(1);
                    if let Some(snapshot) = self.active_snapshot() {
                        if let Some(last_file) = snapshot.files.get(self.current_file_index) {
                            self.current_hunk_index = last_file.hunks.len().saturating_sub(1);
                        }
                    }
                }
            }
        }
    }

    fn previous_hunk(&mut self) {
        debug_log("previous_hunk called".to_string());
        let files_len = match self.active_snapshot() {
            Some(s) => s.files.len(),
            None => {
                debug_log("No snapshot, returning".to_string());
                return;
            }
        };

        if files_len == 0 {
            debug_log("No files in snapshot, returning".to_string());
            return;
        }

        debug_log(format!(
            "Before: file_idx={}, hunk_idx={}",
            self.current_file_index, self.current_hunk_index
        ));

        // Clear line memory for current hunk before moving
        let old_hunk_key = (self.current_file_index, self.current_hunk_index);
        self.hunk_line_memory.remove(&old_hunk_key);

        // Reset scroll when moving to a different hunk
        self.scroll_offset = 0;

        // If we're at the first hunk of the current file, go to previous file's last hunk
        if self.current_hunk_index == 0 {
            if self.mode != Mode::View && self.mode != Mode::Review && self.current_file_index == 0
            {
                // Streaming/Buffered: pager semantics, stay at first hunk
            } else {
                self.previous_file();
                // Set to the last hunk of the new file
                if let Some(snapshot) = self.active_snapshot() {
                    if self.current_file_index < snapshot.files.len() {
                        let last_hunk_index = snapshot.files[self.current_file_index]
                            .hunks
                            .len()
                            .saturating_sub(1);
                        self.current_hunk_index = last_hunk_index;
                    }
                }
            }
        } else {
            // Just go back one hunk in the current file
            self.current_hunk_index = self.current_hunk_index.saturating_sub(1);
        }

        debug_log(format!(
            "After: file_idx={}, hunk_idx={}",
            self.current_file_index, self.current_hunk_index
        ));
    }

    fn next_file(&mut self) {
        let files_len = match self.active_snapshot() {
            Some(s) if !s.files.is_empty() => s.files.len(),
            _ => return,
        };

        // Clear line memory for old file
        let old_file_index = self.current_file_index;

        // Calculate next file index before clearing memory
        self.current_file_index = (self.current_file_index + 1) % files_len;
        self.current_hunk_index = 0;

        // Now clear the memory for the old file (after we're done with snapshot)
        self.clear_line_memory_for_file(old_file_index);
    }

    fn previous_file(&mut self) {
        let files_len = match self.active_snapshot() {
            Some(s) if !s.files.is_empty() => s.files.len(),
            _ => return,
        };

        // Clear line memory for old file
        let old_file_index = self.current_file_index;

        // Calculate previous file index before clearing memory
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
                    let changes: Vec<(usize, &String)> = hunk
                        .lines
                        .iter()
                        .enumerate()
                        .filter(|(_, line)| {
                            (line.starts_with('+') && !line.starts_with("+++"))
                                || (line.starts_with('-') && !line.starts_with("---"))
                        })
                        .collect();

                    if !changes.is_empty() {
                        // Find where we are in the changes list
                        let current_in_changes = changes
                            .iter()
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
                    let changes: Vec<(usize, &String)> = hunk
                        .lines
                        .iter()
                        .enumerate()
                        .filter(|(_, line)| {
                            (line.starts_with('+') && !line.starts_with("+++"))
                                || (line.starts_with('-') && !line.starts_with("---"))
                        })
                        .collect();

                    if !changes.is_empty() {
                        // Find where we are in the changes list
                        let current_in_changes = changes
                            .iter()
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
                        if (line.starts_with('+') && !line.starts_with("+++"))
                            || (line.starts_with('-') && !line.starts_with("---"))
                        {
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
        self.hunk_line_memory
            .retain(|(f_idx, _), _| *f_idx != file_index);
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
            Mode::Review => {
                // cycle_mode should not be called in Review mode, but handle gracefully
                Mode::Review
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
            self.hunk_line_memory
                .insert(hunk_key, self.selected_line_index);
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
            self.hunk_line_memory
                .insert(hunk_key, self.selected_line_index);
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
        let mut refresh_needed = false;

        match self.focus {
            FocusPane::HunkView => {
                // Check if we're in line selection mode
                if self.line_selection_mode {
                    // Stage/unstage a single line
                    if let Some(snapshot) = self.snapshots.get_mut(self.current_snapshot_index) {
                        if let Some(file) = snapshot.files.get_mut(self.current_file_index) {
                            if matches!(file.status.as_str(), "Added" | "Deleted") {
                                refresh_needed =
                                    Self::toggle_file_staging_for_change(&self.git_repo, file)
                                        || refresh_needed;
                            } else if let Some(hunk) = file.hunks.get_mut(self.current_hunk_index) {
                                // Get the selected line
                                if let Some(selected_line) =
                                    hunk.lines.get(self.selected_line_index)
                                {
                                    // Only stage change lines (+ or -)
                                    if (selected_line.starts_with('+')
                                        && !selected_line.starts_with("+++"))
                                        || (selected_line.starts_with('-')
                                            && !selected_line.starts_with("---"))
                                    {
                                        // Check if line is already staged
                                        let is_staged = hunk
                                            .staged_line_indices
                                            .contains(&self.selected_line_index);

                                        if is_staged {
                                            // Unstage the single line
                                            match self.git_repo.unstage_single_line(
                                                hunk,
                                                self.selected_line_index,
                                                &file.path,
                                            ) {
                                                Ok(_) => {
                                                    // Remove this line from staged indices
                                                    hunk.staged_line_indices
                                                        .remove(&self.selected_line_index);
                                                    debug_log(format!(
                                                        "Unstaged line {} in {}",
                                                        self.selected_line_index,
                                                        file.path.display()
                                                    ));
                                                    refresh_needed = true;
                                                }
                                                Err(e) => {
                                                    debug_log(format!("Failed to unstage line: {}. Note: Line-level unstaging is experimental and may not work for all hunks. Consider unstaging the entire hunk with Shift+U instead.", e));
                                                }
                                            }
                                        } else {
                                            // Stage the single line
                                            match self.git_repo.stage_single_line(
                                                hunk,
                                                self.selected_line_index,
                                                &file.path,
                                            ) {
                                                Ok(_) => {
                                                    // Mark this line as staged
                                                    hunk.staged_line_indices
                                                        .insert(self.selected_line_index);
                                                    debug_log(format!(
                                                        "Staged line {} in {}",
                                                        self.selected_line_index,
                                                        file.path.display()
                                                    ));
                                                    refresh_needed = true;
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
                            if matches!(file.status.as_str(), "Added" | "Deleted") {
                                refresh_needed =
                                    Self::toggle_file_staging_for_change(&self.git_repo, file)
                                        || refresh_needed;
                            } else if let Some(hunk) = file.hunks.get_mut(self.current_hunk_index) {
                                match self.git_repo.toggle_hunk_staging(hunk, &file.path) {
                                    Ok(is_staged_now) => {
                                        if is_staged_now {
                                            debug_log(format!(
                                                "Staged hunk in {}",
                                                file.path.display()
                                            ));
                                        } else {
                                            debug_log(format!(
                                                "Unstaged hunk in {}",
                                                file.path.display()
                                            ));
                                        }
                                        refresh_needed = true;
                                    }
                                    Err(e) => {
                                        debug_log(format!("Failed to toggle hunk staging: {}", e));
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
                                    refresh_needed = true;
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
                                            if (line.starts_with('+') && !line.starts_with("+++"))
                                                || (line.starts_with('-')
                                                    && !line.starts_with("---"))
                                            {
                                                hunk.staged_line_indices.insert(idx);
                                            }
                                        }
                                    }
                                    debug_log(format!("Staged file {}", file.path.display()));
                                    refresh_needed = true;
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

        if refresh_needed {
            self.refresh_current_snapshot_from_git();
        }
    }

    fn open_commit_mode(&mut self) -> Result<()> {
        // Temporarily suspend the TUI so git/editor can take over the terminal.
        disable_raw_mode()?;
        execute!(io::stdout(), LeaveAlternateScreen, DisableMouseCapture)?;

        let commit_result = self.git_repo.commit_with_editor();

        // Always restore TUI state before returning.
        enable_raw_mode()?;
        execute!(io::stdout(), EnterAlternateScreen, EnableMouseCapture)?;

        let status = commit_result?;
        if !status.success() {
            debug_log(format!(
                "git commit exited with status {:?} (possibly canceled or nothing to commit)",
                status.code()
            ));
        }

        self.refresh_current_snapshot_from_git();
        self.last_auto_advance = Instant::now();
        self.needs_full_redraw = true;
        Ok(())
    }

    fn annotate_staged_lines(&self, snapshot: &mut DiffSnapshot) {
        for file in &mut snapshot.files {
            for hunk in &mut file.hunks {
                match self.git_repo.detect_staged_lines(hunk, &file.path) {
                    Ok(staged_indices) => {
                        hunk.staged_line_indices = staged_indices;

                        let total_change_lines = hunk
                            .lines
                            .iter()
                            .filter(|line| {
                                (line.starts_with('+') && !line.starts_with("+++"))
                                    || (line.starts_with('-') && !line.starts_with("---"))
                            })
                            .count();

                        hunk.staged = hunk.staged_line_indices.len() == total_change_lines
                            && total_change_lines > 0;
                    }
                    Err(e) => {
                        debug_log(format!("Failed to detect staged lines: {}", e));
                    }
                }
            }
        }
    }

    fn refresh_current_snapshot_from_git(&mut self) {
        let previous_selected_line = self.selected_line_index;

        match self.git_repo.get_diff_snapshot() {
            Ok(mut snapshot) => {
                self.annotate_staged_lines(&mut snapshot);

                if self.snapshots.is_empty() {
                    self.snapshots.push(snapshot);
                    self.current_snapshot_index = 0;
                } else {
                    self.snapshots[self.current_snapshot_index] = snapshot;
                }

                // Clamp indices after snapshot replacement
                if let Some(current_snapshot) = self.snapshots.get(self.current_snapshot_index) {
                    if current_snapshot.files.is_empty() {
                        self.current_file_index = 0;
                        self.current_hunk_index = 0;
                        self.selected_line_index = 0;
                        return;
                    }

                    if self.current_file_index >= current_snapshot.files.len() {
                        self.current_file_index = current_snapshot.files.len().saturating_sub(1);
                    }

                    if let Some(file) = current_snapshot.files.get(self.current_file_index) {
                        if file.hunks.is_empty() {
                            self.current_hunk_index = 0;
                            self.selected_line_index = 0;
                            return;
                        }

                        if self.current_hunk_index >= file.hunks.len() {
                            self.current_hunk_index = file.hunks.len().saturating_sub(1);
                        }

                        if self.line_selection_mode {
                            self.select_nearest_change_line(previous_selected_line);
                        } else {
                            self.selected_line_index = 0;
                        }
                    }
                }
            }
            Err(e) => {
                debug_log(format!(
                    "Failed to refresh snapshot after staging action: {}",
                    e
                ));
            }
        }
    }

    fn select_nearest_change_line(&mut self, preferred_index: usize) {
        if let Some(snapshot) = self.current_snapshot() {
            if let Some(file) = snapshot.files.get(self.current_file_index) {
                if let Some(hunk) = file.hunks.get(self.current_hunk_index) {
                    let change_indices: Vec<usize> = hunk
                        .lines
                        .iter()
                        .enumerate()
                        .filter_map(|(idx, line)| {
                            ((line.starts_with('+') && !line.starts_with("+++"))
                                || (line.starts_with('-') && !line.starts_with("---")))
                            .then_some(idx)
                        })
                        .collect();

                    if change_indices.is_empty() {
                        self.selected_line_index = 0;
                        return;
                    }

                    let mut best_idx = change_indices[0];
                    let mut best_dist = usize::abs_diff(best_idx, preferred_index);

                    for &idx in change_indices.iter().skip(1) {
                        let dist = usize::abs_diff(idx, preferred_index);
                        if dist < best_dist || (dist == best_dist && idx < best_idx) {
                            best_idx = idx;
                            best_dist = dist;
                        }
                    }

                    self.selected_line_index = best_idx;
                    return;
                }
            }
        }

        self.selected_line_index = 0;
    }

    /// Get the active snapshot for navigation — works in both normal and review modes.
    fn active_snapshot(&self) -> Option<&DiffSnapshot> {
        if self.mode == Mode::Review {
            self.review_snapshot.as_ref()
        } else {
            self.snapshots.get(self.current_snapshot_index)
        }
    }

    fn enter_review_mode(&mut self) {
        match self.git_repo.get_recent_commits(20) {
            Ok(commits) => {
                if commits.is_empty() {
                    debug_log("No commits found for review".to_string());
                    return;
                }
                self.review_commits = commits;
                self.review_commit_cursor = 0;
                self.review_selecting_commit = true;
                self.mode = Mode::Review;
                debug_log("Entered review mode, showing commit picker".to_string());
            }
            Err(e) => {
                debug_log(format!("Failed to get commits for review: {}", e));
            }
        }
    }

    fn select_review_commit(&mut self) {
        if self.review_commit_cursor >= self.review_commits.len() {
            return;
        }
        let sha = self.review_commits[self.review_commit_cursor].sha.clone();
        debug_log(format!(
            "Loading commit diff for {}",
            &sha[..7.min(sha.len())]
        ));

        match self.git_repo.get_commit_diff(&sha) {
            Ok(snapshot) => {
                self.review_snapshot = Some(snapshot);
                self.review_selecting_commit = false;
                self.current_file_index = 0;
                self.current_hunk_index = 0;
                self.scroll_offset = 0;
                self.line_selection_mode = false;
                self.focus = FocusPane::HunkView;
                debug_log("Loaded commit diff for review".to_string());
            }
            Err(e) => {
                debug_log(format!("Failed to load commit diff: {}", e));
                self.review_selecting_commit = false;
                self.review_commits.clear();
                self.mode = Mode::View;
            }
        }
    }

    fn exit_review_mode(&mut self) {
        self.mode = Mode::View;
        self.review_selecting_commit = false;
        self.review_commits.clear();
        self.review_snapshot = None;
        self.current_file_index = 0;
        self.current_hunk_index = 0;
        self.scroll_offset = 0;
        self.line_selection_mode = false;
        self.focus = FocusPane::HunkView;
        self.show_help = false;
        self.help_scroll_offset = 0;
        debug_log("Exited review mode".to_string());
    }

    fn toggle_review_acceptance(&mut self) {
        if let Some(ref mut snapshot) = self.review_snapshot {
            if let Some(file) = snapshot.files.get_mut(self.current_file_index) {
                if let Some(hunk) = file.hunks.get_mut(self.current_hunk_index) {
                    hunk.accepted = !hunk.accepted;
                    debug_log(format!(
                        "Hunk accepted={} in {}",
                        hunk.accepted,
                        file.path.display()
                    ));
                }
            }
        }
    }

    pub fn current_snapshot(&self) -> Option<&DiffSnapshot> {
        if self.mode == Mode::Review {
            self.review_snapshot.as_ref()
        } else {
            self.snapshots.get(self.current_snapshot_index)
        }
    }

    pub fn current_file(&self) -> Option<&FileChange> {
        self.current_snapshot()?.files.get(self.current_file_index)
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

    pub fn review_selecting_commit(&self) -> bool {
        self.review_selecting_commit
    }

    pub fn review_commits(&self) -> &[CommitInfo] {
        &self.review_commits
    }

    pub fn review_commit_cursor(&self) -> usize {
        self.review_commit_cursor
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
        32 // Number of help lines in draw_help_sidebar
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
#[path = "../tests/app.rs"]
mod tests;

#[cfg(all(test, not(test)))]
mod tests {
    use super::*;
    use crate::diff::Hunk;
    use crate::ui::UI;
    use ratatui::{backend::TestBackend, Terminal};
    use std::fs;
    use std::path::PathBuf;
    use std::process::Command;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    static TEST_DIR_COUNTER: AtomicU64 = AtomicU64::new(0);

    struct TestRepo {
        path: PathBuf,
    }

    impl TestRepo {
        fn new() -> Self {
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("failed to get system time")
                .as_nanos();
            let counter = TEST_DIR_COUNTER.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "hunky-app-tests-{}-{}-{}",
                std::process::id(),
                unique,
                counter
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
    async fn navigation_and_scroll_helpers_cover_core_branches() {
        let repo = TestRepo::new();
        let mut app = App::new(repo.path.to_str().expect("path should be utf-8"))
            .await
            .expect("failed to create app");
        let mut snapshot = sample_snapshot();
        snapshot.files[0].hunks[0].lines = vec![
            " context a\n".to_string(),
            "-old\n".to_string(),
            "+new\n".to_string(),
            " context b\n".to_string(),
        ];
        app.snapshots = vec![snapshot];
        app.current_snapshot_index = 0;

        assert_eq!(
            StreamSpeed::Fast.duration_for_hunk(2),
            Duration::from_millis(700)
        );
        assert_eq!(
            StreamSpeed::Medium.duration_for_hunk(1),
            Duration::from_millis(1000)
        );
        assert_eq!(
            StreamSpeed::Slow.duration_for_hunk(3),
            Duration::from_millis(3500)
        );

        app.select_first_change_line();
        assert_eq!(app.selected_line_index, 1);
        app.next_change_line();
        assert_eq!(app.selected_line_index, 2);
        app.previous_change_line();
        assert_eq!(app.selected_line_index, 1);

        app.hunk_line_memory.insert((0, 0), 1);
        app.current_file_index = 0;
        app.next_file();
        assert_eq!(app.current_file_index, 1);
        assert_eq!(app.current_hunk_index, 0);
        assert!(!app.hunk_line_memory.contains_key(&(0, 0)));
        app.previous_file();
        assert_eq!(app.current_file_index, 0);

        app.scroll_offset = 50;
        app.clamp_scroll_offset(20);
        assert_eq!(app.scroll_offset, 0);
        app.help_scroll_offset = 50;
        app.clamp_help_scroll_offset(10);
        assert_eq!(app.help_scroll_offset, 17);
        app.extended_help_scroll_offset = 500;
        app.clamp_extended_help_scroll_offset(20);
        assert_eq!(app.extended_help_scroll_offset, 88);
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

    #[tokio::test]
    async fn stage_current_selection_handles_line_hunk_and_file_modes() {
        let repo = TestRepo::new();
        repo.write_file("example.txt", "line 1\nline 2\nline 3\n");
        repo.commit_all("initial");
        repo.write_file("example.txt", "line 1\nline two updated\nline 3\n");

        let mut app = App::new(repo.path.to_str().expect("path should be utf-8"))
            .await
            .expect("failed to create app");
        app.current_snapshot_index = 0;
        app.current_file_index = 0;
        app.current_hunk_index = 0;
        app.focus = FocusPane::HunkView;
        app.line_selection_mode = true;

        let selected = app.snapshots[0].files[0].hunks[0]
            .lines
            .iter()
            .position(|line| line.starts_with('+') && !line.starts_with("+++"))
            .expect("expected added line");
        app.selected_line_index = selected;

        // Line mode: stage selected line and verify index changed
        app.stage_current_selection();
        let cached_after_line_stage = run_git(&repo.path, &["diff", "--cached", "--name-only"]);
        assert!(cached_after_line_stage.contains("example.txt"));

        // Reset to clean index before hunk-mode checks
        run_git(&repo.path, &["reset", "HEAD", "--", "example.txt"]);
        app.refresh_current_snapshot_from_git();

        // Hunk mode: stage current hunk and verify index changed
        app.line_selection_mode = false;
        app.stage_current_selection();
        let cached_after_hunk_stage = run_git(&repo.path, &["diff", "--cached", "--name-only"]);
        assert!(cached_after_hunk_stage.contains("example.txt"));

        // Reset to clean index before file-mode checks
        run_git(&repo.path, &["reset", "HEAD", "--", "example.txt"]);
        app.refresh_current_snapshot_from_git();

        app.focus = FocusPane::FileList;
        app.stage_current_selection();
        let cached_after_file_stage = run_git(&repo.path, &["diff", "--cached", "--name-only"]);
        assert!(cached_after_file_stage.contains("example.txt"));

        app.stage_current_selection();
        let cached_after_file_unstage = run_git(&repo.path, &["diff", "--cached", "--name-only"]);
        assert!(cached_after_file_unstage.trim().is_empty());
    }

    #[tokio::test]
    #[ignore = "Known flaky hunk restage path; run explicitly during debugging"]
    async fn hunk_toggle_can_restage_after_unstage_on_simple_file() {
        let repo = TestRepo::new();
        repo.write_file("example.txt", "line 1\nline 2\nline 3\n");
        repo.commit_all("initial");
        repo.write_file("example.txt", "line 1\nline two updated\nline 3\n");

        let mut app = App::new(repo.path.to_str().expect("path should be utf-8"))
            .await
            .expect("failed to create app");
        app.current_snapshot_index = 0;
        app.current_file_index = 0;
        app.current_hunk_index = 0;
        app.focus = FocusPane::HunkView;
        app.line_selection_mode = false;

        // Stage hunk
        app.stage_current_selection();
        let cached_after_stage = run_git(&repo.path, &["diff", "--cached", "--name-only"]);
        assert!(cached_after_stage.contains("example.txt"));

        // Unstage hunk
        app.stage_current_selection();
        let cached_after_unstage = run_git(&repo.path, &["diff", "--cached", "--name-only"]);
        assert!(cached_after_unstage.trim().is_empty());

        // Restage hunk (regression target)
        app.stage_current_selection();
        let cached_after_restage = run_git(&repo.path, &["diff", "--cached", "--name-only"]);
        assert!(
            cached_after_restage.contains("example.txt"),
            "expected example.txt to be restaged, got:\n{}",
            cached_after_restage
        );
    }

    #[tokio::test]
    async fn ui_draw_renders_file_list_variants() {
        let repo = TestRepo::new();
        repo.write_file("a.txt", "one\n");
        repo.write_file("b.txt", "two\n");
        repo.commit_all("initial");
        repo.write_file("a.txt", "one changed\n");
        repo.write_file("b.txt", "two changed\n");

        let mut app = App::new(repo.path.to_str().expect("path should be utf-8"))
            .await
            .expect("failed to create app");
        app.current_snapshot_index = 0;
        app.current_file_index = 0;
        app.current_hunk_index = 0;
        app.mode = Mode::View;
        app.show_help = true;
        app.focus = FocusPane::FileList;

        let backend = TestBackend::new(120, 35);
        let mut terminal = Terminal::new(backend).expect("failed to create terminal");
        terminal
            .draw(|frame| {
                UI::new(&app).draw(frame);
            })
            .expect("failed to draw ui");
        let rendered = render_buffer_to_string(&terminal);
        assert!(rendered.contains("Files"));
        assert!(rendered.contains("Help"));

        app.show_filenames_only = true;
        app.wrap_lines = true;
        app.line_selection_mode = true;
        app.select_first_change_line();
        terminal
            .draw(|frame| {
                UI::new(&app).draw(frame);
            })
            .expect("failed to draw ui");
        let rendered = render_buffer_to_string(&terminal);
        assert!(rendered.contains("File Info"));
    }

    #[tokio::test]
    async fn navigation_handles_empty_and_boundary_states() {
        let repo = TestRepo::new();
        let mut app = App::new(repo.path.to_str().expect("path should be utf-8"))
            .await
            .expect("failed to create app");

        app.snapshots.clear();
        app.advance_hunk();
        app.previous_hunk();
        app.next_file();
        app.previous_file();

        app.snapshots = vec![DiffSnapshot {
            timestamp: SystemTime::now(),
            files: vec![],
        }];
        app.current_snapshot_index = 0;
        app.advance_hunk();
        app.previous_hunk();
        app.next_file();
        app.previous_file();

        let mut snapshot = sample_snapshot();
        snapshot.files[0].hunks[0].lines = vec![
            " context before\n".to_string(),
            "-old\n".to_string(),
            "+new\n".to_string(),
            " context after\n".to_string(),
        ];
        app.snapshots = vec![snapshot];
        app.current_snapshot_index = 0;
        app.current_file_index = 0;
        app.current_hunk_index = 0;

        app.selected_line_index = 0;
        app.next_change_line();
        assert_eq!(app.selected_line_index, 1);
        app.selected_line_index = 2;
        app.next_change_line();
        assert_eq!(app.selected_line_index, 2);

        app.selected_line_index = 0;
        app.previous_change_line();
        assert_eq!(app.selected_line_index, 2);
        app.selected_line_index = 1;
        app.previous_change_line();
        assert_eq!(app.selected_line_index, 1);

        app.snapshots[0].files[0].hunks[0].lines = vec![" context only\n".to_string()];
        app.selected_line_index = 9;
        app.select_first_change_line();
        assert_eq!(app.selected_line_index, 0);

        app.focus = FocusPane::HelpSidebar;
        app.stage_current_selection();

        app.current_file_index = 99;
        assert_eq!(app.current_hunk_content_height(), 0);

        app.snapshots[0].files[0].hunks[0].lines = vec![
            "-old\n".to_string(),
            "+new\n".to_string(),
            " context after 1\n".to_string(),
            " context after 2\n".to_string(),
            " context after 3\n".to_string(),
            " context after 4\n".to_string(),
            " context after 5\n".to_string(),
            " context after 6\n".to_string(),
        ];
        app.current_file_index = 0;
        app.current_hunk_index = 0;
        app.scroll_offset = 99;
        app.clamp_scroll_offset(5);
        assert!(app.scroll_offset > 0);

        app.extended_help_scroll_offset = 20;
        app.clamp_extended_help_scroll_offset(200);
        assert_eq!(app.extended_help_scroll_offset, 0);
    }

    #[tokio::test]
    async fn ui_draw_renders_mini_compact_help_and_empty_states() {
        let repo = TestRepo::new();
        repo.write_file("example.rs", "fn main() {\n    println!(\"one\");\n}\n");
        repo.commit_all("initial");
        repo.write_file(
            "example.rs",
            "fn main() {\n    println!(\"two\");\n    println!(\"three\");\n}\n",
        );

        let mut app = App::new(repo.path.to_str().expect("path should be utf-8"))
            .await
            .expect("failed to create app");

        let mut terminal =
            Terminal::new(TestBackend::new(36, 20)).expect("failed to create terminal");
        terminal
            .draw(|frame| {
                UI::new(&app).draw(frame);
            })
            .expect("failed to draw mini layout");
        let rendered = render_buffer_to_string(&terminal);
        assert!(rendered.contains("Hunky"));

        app.mode = Mode::Streaming(StreamingType::Auto(StreamSpeed::Medium));
        terminal = Terminal::new(TestBackend::new(52, 20)).expect("failed to create terminal");
        terminal
            .draw(|frame| {
                UI::new(&app).draw(frame);
            })
            .expect("failed to draw compact layout");
        let rendered = render_buffer_to_string(&terminal);
        assert!(rendered.contains("STM:M"));

        app.show_help = true;
        app.focus = FocusPane::HelpSidebar;
        terminal = Terminal::new(TestBackend::new(90, 24)).expect("failed to create terminal");
        terminal
            .draw(|frame| {
                UI::new(&app).draw(frame);
            })
            .expect("failed to draw focused help");
        let rendered = render_buffer_to_string(&terminal);
        assert!(rendered.contains("Keys [FOCUSED]"));

        app.syntax_highlighting = false;
        app.wrap_lines = true;
        terminal
            .draw(|frame| {
                UI::new(&app).draw(frame);
            })
            .expect("failed to draw non-highlighted wrapped diff");
        let rendered = render_buffer_to_string(&terminal);
        assert!(rendered.contains("+ "));

        app.snapshots[0].files[0].hunks.clear();
        app.show_help = false;
        app.focus = FocusPane::HunkView;
        terminal
            .draw(|frame| {
                UI::new(&app).draw(frame);
            })
            .expect("failed to draw no-hunks state");
        let rendered = render_buffer_to_string(&terminal);
        assert!(rendered.contains("No hunks to display yet"));

        app.snapshots.clear();
        terminal
            .draw(|frame| {
                UI::new(&app).draw(frame);
            })
            .expect("failed to draw no-snapshot state");
        let rendered = render_buffer_to_string(&terminal);
        assert!(rendered.contains("No changes"));
    }
}
