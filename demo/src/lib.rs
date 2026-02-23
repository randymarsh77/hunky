//! hunky web demo — runs the **real** hunky TUI in the browser via tui2web.
//!
//! Instead of reimplementing the UI, this crate converts data produced by
//! tui2web's in-memory git into hunky's own [`DiffSnapshot`] and then drives
//! the real [`App`] and [`UI`] rendering code.

mod backend;

use std::collections::VecDeque;
use std::path::PathBuf;
use std::time::SystemTime;

use backend::WebBackend;
use hunky::app::App;
use hunky::diff::{DiffSnapshot, FileChange, Hunk};
use ratatui::Terminal;
use tui2web::git::{GitRepository, InMemoryGitRepository};
use tui2web::fs::{Filesystem, MemoryFilesystem};
use wasm_bindgen::prelude::*;

/// Build a realistic in-memory repository and return a hunky [`DiffSnapshot`].
fn build_demo_snapshot() -> DiffSnapshot {
    let mut fs = MemoryFilesystem::new();
    fs.create_dir("src").unwrap();
    fs.write_file(
        "src/main.rs",
        b"use anyhow::Result;\nuse clap::Parser;\n\nfn main() -> Result<()> {\n    let args = Args::parse();\n    println!(\"Starting hunky...\");\n    Ok(())\n}\n",
    )
    .unwrap();
    fs.write_file(
        "src/app.rs",
        b"pub struct App {\n    running: bool,\n}\n\nimpl App {\n    pub fn new() -> Self {\n        Self { running: true }\n    }\n}\n",
    )
    .unwrap();
    fs.write_file("README.md", b"# hunky\n\nA TUI for streaming git changes.\n")
        .unwrap();

    let mut repo = InMemoryGitRepository::new(fs);
    repo.stage_file("src/main.rs").unwrap();
    repo.stage_file("src/app.rs").unwrap();
    repo.stage_file("README.md").unwrap();
    repo.commit("Initial commit", "demo").unwrap();

    // Make working-tree modifications.
    repo.filesystem_mut()
        .write_file(
            "src/main.rs",
            b"use anyhow::Result;\nuse clap::Parser;\nuse ratatui::Terminal;\n\nfn main() -> Result<()> {\n    let args = Args::parse();\n    let terminal = Terminal::new()?;\n    run_app(terminal, args)\n}\n",
        )
        .unwrap();
    repo.filesystem_mut()
        .write_file(
            "src/app.rs",
            b"pub struct App {\n    running: bool,\n    mode: Mode,\n}\n\npub enum Mode {\n    View,\n    Streaming,\n}\n\nimpl App {\n    pub fn new() -> Self {\n        Self {\n            running: true,\n            mode: Mode::View,\n        }\n    }\n\n    pub fn toggle_mode(&mut self) {\n        self.mode = match self.mode {\n            Mode::View => Mode::Streaming,\n            Mode::Streaming => Mode::View,\n        };\n    }\n}\n",
        )
        .unwrap();
    repo.filesystem_mut().create_dir("src/ui").unwrap();
    repo.filesystem_mut()
        .write_file(
            "src/ui/render.rs",
            b"use ratatui::Frame;\n\npub fn draw(frame: &mut Frame) {\n    // Render the TUI layout\n    let area = frame.size();\n    draw_header(frame, area);\n}\n",
        )
        .unwrap();

    // Convert tui2web diffs into hunky's DiffSnapshot.
    let diffs = repo.diff_unstaged().unwrap();
    let files: Vec<FileChange> = diffs
        .into_iter()
        .map(|d| {
            let path = PathBuf::from(&d.path);
            let hunks: Vec<Hunk> = d
                .hunks
                .into_iter()
                .map(|h| Hunk::new(h.old_start, h.new_start, h.lines, &path))
                .collect();
            FileChange {
                path,
                status: d.status.to_string(),
                hunks,
            }
        })
        .collect();

    DiffSnapshot {
        timestamp: SystemTime::now(),
        files,
    }
}

/// The WebAssembly-exported application.
///
/// This is a thin shell around hunky's real [`App`], bridging the WASM /
/// xterm.js world to the actual TUI code.
#[wasm_bindgen]
pub struct DemoApp {
    app: App,
    terminal: Terminal<WebBackend>,
    key_queue: VecDeque<String>,
    quit: bool,
}

#[wasm_bindgen]
impl DemoApp {
    #[wasm_bindgen(constructor)]
    pub fn new(width: u16, height: u16) -> DemoApp {
        console_error_panic_hook::set_once();

        let snapshot = build_demo_snapshot();
        let app = App::from_snapshot(snapshot);
        let backend = WebBackend::new(width, height);
        let terminal = Terminal::new(backend).unwrap();

        DemoApp {
            app,
            terminal,
            key_queue: VecDeque::new(),
            quit: false,
        }
    }

    pub fn push_key(&mut self, key: String) {
        self.key_queue.push_back(key);
    }

    pub fn tick(&mut self) -> bool {
        if self.quit {
            return false;
        }

        // Process all queued key events through the real App.
        while let Some(key) = self.key_queue.pop_front() {
            if !self.app.handle_key_str(&key) {
                self.quit = true;
                return false;
            }
        }

        // Render using the real App drawing code.
        self.app.draw(&mut self.terminal);
        true
    }

    pub fn get_frame(&self) -> String {
        self.terminal.backend().get_ansi_output().to_string()
    }

    pub fn resize(&mut self, width: u16, height: u16) {
        self.terminal.backend_mut().resize(width, height);
        let _ = self
            .terminal
            .resize(ratatui::layout::Rect::new(0, 0, width, height));
    }

    pub fn should_quit(&self) -> bool {
        self.quit
    }
}
