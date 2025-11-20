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
        .open("git-stream-debug.log")
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
    RealTime,
    Slow,        // 1 hunk per 5 seconds
    VerySlow,    // 1 hunk per 10 seconds
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FocusPane {
    FileList,
    HunkView,
}

impl StreamSpeed {
    pub fn duration(&self) -> Duration {
        match self {
            StreamSpeed::RealTime => Duration::from_millis(100),
            StreamSpeed::Slow => Duration::from_secs(5),
            StreamSpeed::VerySlow => Duration::from_secs(10),
        }
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
    compact_mode: bool,
    show_help: bool,
    focus: FocusPane,
    snapshot_receiver: mpsc::UnboundedReceiver<DiffSnapshot>,
    last_auto_advance: Instant,
    scroll_offset: u16,
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
            speed: StreamSpeed::RealTime,
            seen_tracker,
            show_filenames_only: false,
            wrap_lines: false,
            compact_mode: true,
            show_help: false,
            focus: FocusPane::HunkView,
            snapshot_receiver: rx,
            last_auto_advance: Instant::now(),
            scroll_offset: 0,
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
                if elapsed >= self.speed.duration() {
                    self.advance_hunk();
                    self.last_auto_advance = Instant::now();
                }
            }
            
            // Draw UI
            terminal.draw(|f| {
                let ui = UI::new(self);
                ui.draw(f);
            })?;
            
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
                        KeyCode::Enter => {
                            // Toggle between AutoStream and BufferedMore
                            self.mode = match self.mode {
                                StreamMode::AutoStream => StreamMode::BufferedMore,
                                StreamMode::BufferedMore => StreamMode::AutoStream,
                            };
                            self.last_auto_advance = Instant::now();
                        }
                        KeyCode::Esc => {
                            // Also toggle mode
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
                            // Toggle focus between file list and hunk view
                            self.focus = match self.focus {
                                FocusPane::FileList => FocusPane::HunkView,
                                FocusPane::HunkView => FocusPane::FileList,
                            };
                        }
                        KeyCode::BackTab => {
                            // Shift+Tab also goes back (some terminals map Shift+Space to BackTab)
                            self.previous_hunk();
                        }
                        KeyCode::Char('j') | KeyCode::Down => {
                            if self.focus == FocusPane::FileList {
                                // Navigate to next file and jump to its first hunk
                                self.next_file();
                                self.scroll_offset = 0;
                            } else {
                                // Scroll down in hunk view
                                self.scroll_offset = self.scroll_offset.saturating_add(1);
                            }
                        }
                        KeyCode::Char('k') | KeyCode::Up => {
                            if self.focus == FocusPane::FileList {
                                // Navigate to previous file and jump to its first hunk
                                self.previous_file();
                                self.scroll_offset = 0;
                            } else {
                                // Scroll up in hunk view
                                self.scroll_offset = self.scroll_offset.saturating_sub(1);
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
                                StreamSpeed::RealTime => StreamSpeed::Slow,
                                StreamSpeed::Slow => StreamSpeed::VerySlow,
                                StreamSpeed::VerySlow => StreamSpeed::RealTime,
                            };
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
                        KeyCode::Char('h') | KeyCode::Char('H') => {
                            // Toggle help display
                            self.show_help = !self.show_help;
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
        
        self.current_file_index = (self.current_file_index + 1) % snapshot.files.len();
        self.current_hunk_index = 0;
    }
    
    fn previous_file(&mut self) {
        if self.snapshots.is_empty() {
            return;
        }
        
        let snapshot = &self.snapshots[self.current_snapshot_index];
        if snapshot.files.is_empty() {
            return;
        }
        
        if self.current_file_index == 0 {
            self.current_file_index = snapshot.files.len() - 1;
        } else {
            self.current_file_index -= 1;
        }
        self.current_hunk_index = 0;
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
    
    pub fn reached_end(&self) -> bool {
        self.reached_end
    }
    
    pub fn view_mode(&self) -> ViewMode {
        self.view_mode
    }
    
    pub fn mode(&self) -> StreamMode {
        self.mode
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
    
    pub fn compact_mode(&self) -> bool {
        self.compact_mode
    }
    
    pub fn show_help(&self) -> bool {
        self.show_help
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
}
