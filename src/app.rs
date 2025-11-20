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
use std::io;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

use crate::diff::{DiffSnapshot, FileChange};
use crate::git::GitRepo;
use crate::ui::UI;
use crate::watcher::FileWatcher;

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
    mode: StreamMode,
    speed: StreamSpeed,
    show_filenames_only: bool,
    snapshot_receiver: mpsc::UnboundedReceiver<DiffSnapshot>,
    last_auto_advance: Instant,
    _watcher: FileWatcher,
}

impl App {
    pub async fn new() -> Result<Self> {
        let git_repo = GitRepo::new(".")?;
        
        // Get initial snapshot
        let initial_snapshot = git_repo.get_diff_snapshot()?;
        
        // Set up file watcher
        let (tx, rx) = mpsc::unbounded_channel();
        let watcher = FileWatcher::new(git_repo.clone(), tx)?;
        
        let app = Self {
            git_repo,
            snapshots: vec![initial_snapshot],
            current_snapshot_index: 0,
            current_file_index: 0,
            current_hunk_index: 0,
            mode: StreamMode::AutoStream,
            speed: StreamSpeed::RealTime,
            show_filenames_only: false,
            snapshot_receiver: rx,
            last_auto_advance: Instant::now(),
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
            while let Ok(snapshot) = self.snapshot_receiver.try_recv() {
                self.snapshots.push(snapshot);
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
                        KeyCode::Char('n') => {
                            // Next file
                            self.next_file();
                        }
                        KeyCode::Char('p') => {
                            // Previous file
                            self.previous_file();
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
                        KeyCode::Char('r') => {
                            // Refresh - get new snapshot
                            let snapshot = self.git_repo.get_diff_snapshot()?;
                            self.snapshots.push(snapshot);
                            self.current_snapshot_index = self.snapshots.len() - 1;
                            self.current_file_index = 0;
                            self.current_hunk_index = 0;
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
        
        let file = &snapshot.files[self.current_file_index];
        
        // Advance to next hunk
        self.current_hunk_index += 1;
        
        // If we've gone past the last hunk in this file, move to next file
        if self.current_hunk_index >= file.hunks.len() {
            self.next_file();
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
    
    pub fn mode(&self) -> StreamMode {
        self.mode
    }
    
    pub fn speed(&self) -> StreamSpeed {
        self.speed
    }
    
    pub fn show_filenames_only(&self) -> bool {
        self.show_filenames_only
    }
}
