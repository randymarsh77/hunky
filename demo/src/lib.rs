//! hunky web demo — a simulated TUI running in the browser via tui2web.
//!
//! Renders a hunky-like interface with a file list, diff hunks, and keyboard
//! navigation.  Uses tui2web's in-memory git and filesystem implementations
//! to provide realistic data without any OS dependencies.

use std::collections::VecDeque;

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
    Terminal,
};
use tui2web::git::{FileStatus, GitRepository, InMemoryGitRepository};
use tui2web::fs::{Filesystem, MemoryFilesystem};
use tui2web::WebBackend;
use wasm_bindgen::prelude::*;

/// Simulated file change for display.
struct FileChange {
    path: String,
    status: String,
    hunks: Vec<Hunk>,
}

/// Simulated diff hunk.
#[allow(dead_code)]
struct Hunk {
    old_start: usize,
    new_start: usize,
    lines: Vec<String>,
}

/// Build the simulated repository and extract diff data for the UI.
fn build_demo_data() -> Vec<FileChange> {
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

    // Make modifications.
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
    repo.filesystem_mut()
        .create_dir("src/ui")
        .unwrap();
    repo.filesystem_mut()
        .write_file(
            "src/ui/render.rs",
            b"use ratatui::Frame;\n\npub fn draw(frame: &mut Frame) {\n    // Render the TUI layout\n    let area = frame.size();\n    draw_header(frame, area);\n}\n",
        )
        .unwrap();

    let diffs = repo.diff_unstaged().unwrap();

    let mut changes: Vec<FileChange> = Vec::new();
    for d in &diffs {
        let status = match d.status {
            FileStatus::Added => "Added",
            FileStatus::Modified => "Modified",
            FileStatus::Deleted => "Deleted",
            FileStatus::Untracked => "Untracked",
        };
        let hunks: Vec<Hunk> = d
            .hunks
            .iter()
            .map(|h| Hunk {
                old_start: h.old_start,
                new_start: h.new_start,
                lines: h.lines.clone(),
            })
            .collect();
        changes.push(FileChange {
            path: d.path.clone(),
            status: status.to_string(),
            hunks,
        });
    }
    changes
}

/// The WebAssembly-exported application struct.
#[wasm_bindgen]
pub struct App {
    terminal: Terminal<WebBackend>,
    key_queue: VecDeque<String>,
    should_quit: bool,
    files: Vec<FileChange>,
    current_file: usize,
    current_hunk: usize,
    scroll_offset: u16,
    show_help: bool,
    mode_label: &'static str,
    tick_count: u64,
}

#[wasm_bindgen]
impl App {
    #[wasm_bindgen(constructor)]
    pub fn new(width: u16, height: u16) -> App {
        console_error_panic_hook::set_once();

        let backend = WebBackend::new(width, height);
        let terminal = Terminal::new(backend).unwrap();
        let files = build_demo_data();

        App {
            terminal,
            key_queue: VecDeque::new(),
            should_quit: false,
            files,
            current_file: 0,
            current_hunk: 0,
            scroll_offset: 0,
            show_help: false,
            mode_label: "VIEW",
            tick_count: 0,
        }
    }

    pub fn push_key(&mut self, key: String) {
        self.key_queue.push_back(key);
    }

    pub fn tick(&mut self) -> bool {
        while let Some(key) = self.key_queue.pop_front() {
            self.handle_input(&key);
        }

        if !self.should_quit {
            self.tick_count += 1;
            self.render();
        }

        !self.should_quit
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
        self.should_quit
    }
}

impl App {
    fn handle_input(&mut self, key: &str) {
        if self.show_help {
            match key {
                "h" | "H" | "Escape" | "?" => self.show_help = false,
                _ => {}
            }
            return;
        }

        match key {
            "q" | "Escape" => self.should_quit = true,
            "j" | "ArrowDown" => {
                if !self.files.is_empty() {
                    let file = &self.files[self.current_file];
                    if self.current_hunk + 1 < file.hunks.len() {
                        self.current_hunk += 1;
                        self.scroll_offset = 0;
                    }
                }
            }
            "k" | "ArrowUp" => {
                if self.current_hunk > 0 {
                    self.current_hunk -= 1;
                    self.scroll_offset = 0;
                }
            }
            "J" => {
                if self.current_file + 1 < self.files.len() {
                    self.current_file += 1;
                    self.current_hunk = 0;
                    self.scroll_offset = 0;
                }
            }
            "K" => {
                if self.current_file > 0 {
                    self.current_file -= 1;
                    self.current_hunk = 0;
                    self.scroll_offset = 0;
                }
            }
            "Tab" => {
                if self.current_file + 1 < self.files.len() {
                    self.current_file += 1;
                    self.current_hunk = 0;
                    self.scroll_offset = 0;
                } else {
                    self.current_file = 0;
                    self.current_hunk = 0;
                    self.scroll_offset = 0;
                }
            }
            "s" => {
                self.mode_label = if self.mode_label == "VIEW" {
                    "STREAMING (Auto)"
                } else {
                    "VIEW"
                };
            }
            "h" | "H" | "?" => self.show_help = true,
            _ => {}
        }
    }

    fn render(&mut self) {
        let current_file = self.current_file;
        let current_hunk = self.current_hunk;
        let scroll_offset = self.scroll_offset;
        let show_help = self.show_help;
        let mode_label = self.mode_label;
        let files = &self.files;
        let file_count = files.len();

        self.terminal
            .draw(|frame| {
                let area = frame.size();

                let main_chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([
                        Constraint::Length(3), // header
                        Constraint::Min(0),    // content
                    ])
                    .split(area);

                // ── Header ───────────────────────────────────────────────
                draw_header(frame, main_chunks[0], mode_label, file_count);

                if show_help {
                    draw_help(frame, main_chunks[1]);
                    return;
                }

                if files.is_empty() {
                    let msg = Paragraph::new("No changes detected.")
                        .style(Style::default().fg(Color::DarkGray))
                        .block(Block::default().borders(Borders::ALL));
                    frame.render_widget(msg, main_chunks[1]);
                    return;
                }

                // ── Split: file list + diff view ─────────────────────────
                let content_chunks = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([
                        Constraint::Length(28),
                        Constraint::Min(0),
                    ])
                    .split(main_chunks[1]);

                draw_file_list(frame, content_chunks[0], files, current_file);
                draw_diff_view(
                    frame,
                    content_chunks[1],
                    files,
                    current_file,
                    current_hunk,
                    scroll_offset,
                );
            })
            .unwrap();
    }
}

fn draw_header(frame: &mut ratatui::Frame, area: Rect, mode_label: &str, file_count: usize) {
    let available = area.width.saturating_sub(2) as usize;
    let right_text = format!("{} file(s) changed", file_count);

    let left = format!(" hunky  {}  ", mode_label);
    let pad = available.saturating_sub(left.len() + right_text.len());

    let header_line = Line::from(vec![
        Span::styled(" hunky ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        Span::raw(" "),
        Span::styled(
            mode_label,
            Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
        ),
        Span::raw(" ".repeat(pad)),
        Span::styled(right_text, Style::default().fg(Color::DarkGray)),
    ]);

    let header = Paragraph::new(header_line)
        .block(Block::default().borders(Borders::ALL).title(" hunky "));
    frame.render_widget(header, area);
}

fn draw_file_list(
    frame: &mut ratatui::Frame,
    area: Rect,
    files: &[FileChange],
    current_file: usize,
) {
    let items: Vec<ListItem> = files
        .iter()
        .enumerate()
        .map(|(i, f)| {
            let status_color = match f.status.as_str() {
                "Modified" => Color::Yellow,
                "Added" => Color::Green,
                "Deleted" => Color::Red,
                _ => Color::Gray,
            };
            let prefix = if i == current_file { "▸ " } else { "  " };
            let status_char = match f.status.as_str() {
                "Modified" => "M",
                "Added" => "A",
                "Deleted" => "D",
                _ => "?",
            };

            let line = Line::from(vec![
                Span::styled(
                    prefix,
                    if i == current_file {
                        Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default()
                    },
                ),
                Span::styled(
                    format!("{} ", status_char),
                    Style::default().fg(status_color),
                ),
                Span::styled(
                    short_path(&f.path),
                    if i == current_file {
                        Style::default().fg(Color::White).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(Color::Gray)
                    },
                ),
            ]);
            ListItem::new(line)
        })
        .collect();

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(" Files "));
    frame.render_widget(list, area);
}

fn draw_diff_view(
    frame: &mut ratatui::Frame,
    area: Rect,
    files: &[FileChange],
    current_file: usize,
    current_hunk: usize,
    _scroll_offset: u16,
) {
    let file = &files[current_file];
    let title = format!(" {} — {} ", file.path, file.status);

    if file.hunks.is_empty() {
        let msg = Paragraph::new("No hunks in this file.")
            .style(Style::default().fg(Color::DarkGray))
            .block(Block::default().borders(Borders::ALL).title(title));
        frame.render_widget(msg, area);
        return;
    }

    let hunk_nav = format!(
        " Hunk {}/{} ",
        current_hunk + 1,
        file.hunks.len()
    );

    let inner_area = Rect {
        x: area.x + 1,
        y: area.y + 1,
        width: area.width.saturating_sub(2),
        height: area.height.saturating_sub(2),
    };

    // Draw border
    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .title_position(ratatui::widgets::block::Position::Top);
    frame.render_widget(block, area);

    // Draw hunk nav bar
    if inner_area.height < 2 {
        return;
    }
    let nav_line = Line::from(vec![
        Span::styled(hunk_nav, Style::default().fg(Color::Cyan)),
        Span::raw("  "),
        Span::styled("j/k", Style::default().fg(Color::Green)),
        Span::styled(": hunks  ", Style::default().fg(Color::DarkGray)),
        Span::styled("J/K", Style::default().fg(Color::Green)),
        Span::styled(": files  ", Style::default().fg(Color::DarkGray)),
        Span::styled("H", Style::default().fg(Color::Green)),
        Span::styled(": help", Style::default().fg(Color::DarkGray)),
    ]);
    let nav_bar = Paragraph::new(nav_line);
    frame.render_widget(
        nav_bar,
        Rect {
            x: inner_area.x,
            y: inner_area.y,
            width: inner_area.width,
            height: 1,
        },
    );

    // Draw diff lines
    let diff_area = Rect {
        x: inner_area.x,
        y: inner_area.y + 1,
        width: inner_area.width,
        height: inner_area.height.saturating_sub(1),
    };

    let hunk = &file.hunks[current_hunk];
    let diff_lines: Vec<Line> = hunk
        .lines
        .iter()
        .map(|line| {
            let trimmed = line.trim_end_matches('\n');
            if trimmed.starts_with('+') {
                Line::from(Span::styled(
                    trimmed.to_string(),
                    Style::default().fg(Color::Green),
                ))
            } else if trimmed.starts_with('-') {
                Line::from(Span::styled(
                    trimmed.to_string(),
                    Style::default().fg(Color::Red),
                ))
            } else {
                Line::from(Span::styled(
                    trimmed.to_string(),
                    Style::default().fg(Color::DarkGray),
                ))
            }
        })
        .collect();

    let diff_widget = Paragraph::new(diff_lines).wrap(Wrap { trim: false });
    frame.render_widget(diff_widget, diff_area);
}

fn draw_help(frame: &mut ratatui::Frame, area: Rect) {
    let help_lines = vec![
        Line::from(Span::styled(
            " Keyboard Shortcuts",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled("  j / ↓   ", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
            Span::raw("Next hunk"),
        ]),
        Line::from(vec![
            Span::styled("  k / ↑   ", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
            Span::raw("Previous hunk"),
        ]),
        Line::from(vec![
            Span::styled("  J       ", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
            Span::raw("Next file"),
        ]),
        Line::from(vec![
            Span::styled("  K       ", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
            Span::raw("Previous file"),
        ]),
        Line::from(vec![
            Span::styled("  Tab     ", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
            Span::raw("Cycle through files"),
        ]),
        Line::from(vec![
            Span::styled("  s       ", Style::default().fg(Color::Blue).add_modifier(Modifier::BOLD)),
            Span::raw("Toggle streaming mode"),
        ]),
        Line::from(vec![
            Span::styled("  H / ?   ", Style::default().fg(Color::Blue).add_modifier(Modifier::BOLD)),
            Span::raw("Toggle help"),
        ]),
        Line::from(vec![
            Span::styled("  q / Esc ", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
            Span::raw("Quit"),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            " This is a web demo powered by tui2web.",
            Style::default().fg(Color::DarkGray),
        )),
        Line::from(Span::styled(
            " The real hunky app streams live git changes.",
            Style::default().fg(Color::DarkGray),
        )),
    ];

    let help = Paragraph::new(help_lines)
        .block(Block::default().borders(Borders::ALL).title(" Help "))
        .wrap(Wrap { trim: false });
    frame.render_widget(help, area);
}

/// Shorten a file path for display in the file list.
fn short_path(path: &str) -> String {
    if path.len() <= 22 {
        path.to_string()
    } else {
        let parts: Vec<&str> = path.split('/').collect();
        if parts.len() > 1 {
            format!("…/{}", parts.last().unwrap_or(&path))
        } else {
            path.to_string()
        }
    }
}
