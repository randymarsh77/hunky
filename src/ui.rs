use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
    Frame,
};

use crate::app::{App, FocusPane, StreamMode, StreamSpeed, ViewMode};
use crate::syntax::SyntaxHighlighter;

pub struct UI<'a> {
    app: &'a App,
    highlighter: SyntaxHighlighter,
}

impl<'a> UI<'a> {
    pub fn new(app: &'a App) -> Self {
        Self {
            app,
            highlighter: SyntaxHighlighter::new(),
        }
    }
    
    pub fn draw(&self, frame: &mut Frame) {
        // Always use compact layout (no footer)
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),   // Header
                Constraint::Min(0),      // Main content
            ])
            .split(frame.area());
        
        self.draw_header(frame, chunks[0]);
        self.draw_main_content(frame, chunks[1]);
    }
    
    fn draw_header(&self, frame: &mut Frame, area: Rect) {
        let available_width = area.width.saturating_sub(2) as usize; // Subtract borders
        let help_text = "H: Help";
        let help_width = help_text.len();
        
        // Determine which layout to use based on available width
        // Wide: > 80, Medium: > 50, Compact: > 40, Mini: <= 40
        let (mode_label, mode_text, speed_label, speed_text, view_mode_text, title_text, unseen_label) = 
            if available_width > 80 {
                // Full layout
                let mode_text = match self.app.mode() {
                    StreamMode::AutoStream => "AUTO-STREAM",
                    StreamMode::BufferedMore => "BUFFERED",
                };
                let speed_text = match self.app.speed() {
                    StreamSpeed::RealTime => "Real-time",
                    StreamSpeed::Slow => "Slow (5s)",
                    StreamSpeed::VerySlow => "Very Slow (10s)",
                };
                let view_mode_text = match self.app.view_mode() {
                    ViewMode::AllChanges => "All Changes",
                    ViewMode::NewChangesOnly => "New Only",
                };
                ("Mode: ", mode_text, "Speed: ", speed_text, view_mode_text, "Git Stream", "Unseen: ")
            } else if available_width > 50 {
                // Medium layout - abbreviate mode and speed labels
                let mode_text = match self.app.mode() {
                    StreamMode::AutoStream => "AUTO",
                    StreamMode::BufferedMore => "BUFF",
                };
                let speed_text = match self.app.speed() {
                    StreamSpeed::RealTime => "RT",
                    StreamSpeed::Slow => "5s",
                    StreamSpeed::VerySlow => "10s",
                };
                let view_mode_text = match self.app.view_mode() {
                    ViewMode::AllChanges => "All",
                    ViewMode::NewChangesOnly => "New",
                };
                ("M: ", mode_text, "S: ", speed_text, view_mode_text, "Git Stream", "U: ")
            } else if available_width > 40 {
                // Compact layout - single letters
                let mode_text = match self.app.mode() {
                    StreamMode::AutoStream => "A",
                    StreamMode::BufferedMore => "B",
                };
                let speed_text = match self.app.speed() {
                    StreamSpeed::RealTime => "R",
                    StreamSpeed::Slow => "S",
                    StreamSpeed::VerySlow => "V",
                };
                let view_mode_text = match self.app.view_mode() {
                    ViewMode::AllChanges => "All",
                    ViewMode::NewChangesOnly => "New",
                };
                ("M:", mode_text, "S:", speed_text, view_mode_text, "GS", "U:")
            } else {
                // Mini layout - minimal info
                let mode_text = match self.app.mode() {
                    StreamMode::AutoStream => "A",
                    StreamMode::BufferedMore => "B",
                };
                let speed_text = match self.app.speed() {
                    StreamSpeed::RealTime => "R",
                    StreamSpeed::Slow => "S",
                    StreamSpeed::VerySlow => "V",
                };
                let view_mode_text = match self.app.view_mode() {
                    ViewMode::AllChanges => "A",
                    ViewMode::NewChangesOnly => "N",
                };
                ("", mode_text, "", speed_text, view_mode_text, "GS", "U:")
            };
        
        let unseen_count = self.app.unseen_hunk_count();
        
        // Build title with help hint on the right side
        let mut title_left = vec![
            Span::styled(title_text, Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::raw(" | "),
            Span::styled(view_mode_text, Style::default().fg(Color::Magenta)),
        ];
        
        if !mode_label.is_empty() || available_width > 40 {
            title_left.push(Span::raw(" | "));
            if !mode_label.is_empty() {
                title_left.push(Span::raw(mode_label));
            }
            title_left.push(Span::styled(mode_text, Style::default().fg(Color::Yellow)));
        }
        
        if !speed_label.is_empty() || available_width > 40 {
            title_left.push(Span::raw(" | "));
            if !speed_label.is_empty() {
                title_left.push(Span::raw(speed_label));
            }
            title_left.push(Span::styled(speed_text, Style::default().fg(Color::Green)));
        }
        
        if available_width > 35 {
            title_left.push(Span::raw(" | "));
            title_left.push(Span::raw(unseen_label));
            title_left.push(Span::styled(format!("{}", unseen_count), Style::default().fg(Color::LightBlue)));
        }
        
        // Calculate padding to right-align help hint
        let left_width = title_left.iter().map(|s| s.content.len()).sum::<usize>();
        let padding_width = available_width.saturating_sub(left_width + help_width);
        
        let mut title_line = title_left;
        if padding_width > 0 {
            title_line.push(Span::raw(" ".repeat(padding_width)));
            title_line.push(Span::styled(help_text, Style::default().fg(Color::Gray)));
        }
        
        let header = Paragraph::new(Line::from(title_line))
            .block(Block::default().borders(Borders::ALL));
        
        frame.render_widget(header, area);
    }
    
    fn draw_main_content(&self, frame: &mut Frame, area: Rect) {
        // Check if help sidebar should be shown
        if self.app.show_help() {
            // Split into 3 columns: file list, diff, help
            let chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Percentage(25),  // File list
                    Constraint::Min(0),          // Diff content (takes remaining space)
                    Constraint::Length(20),      // Help sidebar
                ])
                .split(area);
            
            self.draw_file_list(frame, chunks[0]);
            self.draw_diff_content(frame, chunks[1]);
            self.draw_help_sidebar(frame, chunks[2]);
        } else {
            // No help shown, just file list and diff
            let chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Percentage(25),  // File list
                    Constraint::Percentage(75),  // Diff content
                ])
                .split(area);
            
            self.draw_file_list(frame, chunks[0]);
            self.draw_diff_content(frame, chunks[1]);
        }
    }
    
    fn draw_file_list(&self, frame: &mut Frame, area: Rect) {
        let snapshot = match self.app.current_snapshot() {
            Some(s) => s,
            None => {
                let empty = Paragraph::new("No changes")
                    .block(Block::default().borders(Borders::ALL).title("Files"));
                frame.render_widget(empty, area);
                return;
            }
        };
        
        let items: Vec<ListItem> = snapshot.files.iter().enumerate().map(|(idx, file)| {
            let file_name = file.path.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown");
            
            let style = if idx == self.app.current_file_index() {
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            
            let hunk_count = file.hunks.len();
            let unseen_count = file.hunks.iter().filter(|h| !h.seen).count();
            
            let count_text = if unseen_count < hunk_count {
                format!(" ({}/{})", unseen_count, hunk_count)
            } else {
                format!(" ({})", hunk_count)
            };
            
            let content = Line::from(vec![
                Span::styled(file_name, style),
                Span::styled(count_text, Style::default().fg(Color::DarkGray)),
            ]);
            
            ListItem::new(content)
        }).collect();
        
        let title = if self.app.focus() == FocusPane::FileList {
            "Files [FOCUSED]"
        } else {
            "Files"
        };
        
        let border_style = if self.app.focus() == FocusPane::FileList {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default()
        };
        
        let list = List::new(items)
            .block(Block::default().borders(Borders::ALL).title(title).border_style(border_style))
            .highlight_style(Style::default().bg(Color::DarkGray));
        
        frame.render_widget(list, area);
    }
    
    fn draw_diff_content(&self, frame: &mut Frame, area: Rect) {
        let file = match self.app.current_file() {
            Some(f) => f,
            None => {
                let empty = Paragraph::new("No file selected")
                    .block(Block::default().borders(Borders::ALL).title("Diff"));
                frame.render_widget(empty, area);
                return;
            }
        };
        
        if self.app.show_filenames_only() {
            let content = format!("File: {}\nStatus: {}\nHunks: {}", 
                file.path.display(), 
                file.status,
                file.hunks.len()
            );
            let file_info_title = "File Info".to_string();
            let paragraph = Paragraph::new(content)
                .block(Block::default().borders(Borders::ALL).title(file_info_title))
                .wrap(Wrap { trim: true });
            frame.render_widget(paragraph, area);
            return;
        }
        
        // Get only the current hunk (one hunk at a time UX)
        let current_hunk = file.hunks.get(self.app.current_hunk_index());
        
        if current_hunk.is_none() {
            let file_title = file.path.to_string_lossy().to_string();
            let empty = Paragraph::new("No hunks to display yet")
                .block(Block::default().borders(Borders::ALL).title(file_title));
            frame.render_widget(empty, area);
            return;
        }
        
        let hunk = current_hunk.unwrap();
        
        // Build the text with syntax highlighting
        let mut lines = Vec::new();
        
        // Add file header
        let file_path_str = file.path.to_string_lossy().to_string();
        lines.push(Line::from(vec![
            Span::styled("--- ", Style::default().fg(Color::Red)),
            Span::styled(file_path_str.clone(), Style::default().fg(Color::White)),
        ]));
        lines.push(Line::from(vec![
            Span::styled("+++ ", Style::default().fg(Color::Green)),
            Span::styled(file_path_str.clone(), Style::default().fg(Color::White)),
        ]));
        lines.push(Line::from(""));
        
        // Add hunk header with seen indicator
        let hunk_header = if hunk.seen {
            format!("@@ -{},{} +{},{} @@ [SEEN]", hunk.old_start, hunk.lines.len(), hunk.new_start, hunk.lines.len())
        } else {
            format!("@@ -{},{} +{},{} @@", hunk.old_start, hunk.lines.len(), hunk.new_start, hunk.lines.len())
        };
        
        let header_style = if hunk.seen {
            Style::default().fg(Color::DarkGray)
        } else {
            Style::default().fg(Color::Cyan)
        };
        
        lines.push(Line::from(Span::styled(hunk_header, header_style)));
        lines.push(Line::from("")); // Empty line for spacing
        
        // Separate lines into context before, changes, and context after
        let mut context_before = Vec::new();
        let mut changes = Vec::new();
        let mut context_after = Vec::new();
        
        let mut in_changes = false;
        
        for line in &hunk.lines {
            if line.starts_with('+') || line.starts_with('-') {
                in_changes = true;
                changes.push(line.clone());
            } else if !in_changes {
                context_before.push(line.clone());
            } else {
                context_after.push(line.clone());
            }
        }
        
        // Show up to 5 lines of context before
        let context_before_start = if context_before.len() > 5 {
            context_before.len() - 5
        } else {
            0
        };
        
        for line in &context_before[context_before_start..] {
            let content = line.strip_prefix(' ').unwrap_or(line);
            lines.push(Line::from(Span::styled(
                format!("  {}", content),
                Style::default().fg(Color::DarkGray)
            )));
        }
        
        // Show changes with background colors for better visibility
        for line in &changes {
            if line.starts_with('+') {
                let content = line.strip_prefix('+').unwrap_or(line);
                lines.push(Line::from(Span::styled(
                    format!("+ {}", content),
                    Style::default().fg(Color::Green).bg(Color::Rgb(0, 40, 0)) // Subtle green background
                )));
            } else if line.starts_with('-') {
                let content = line.strip_prefix('-').unwrap_or(line);
                lines.push(Line::from(Span::styled(
                    format!("- {}", content),
                    Style::default().fg(Color::Red).bg(Color::Rgb(40, 0, 0)) // Subtle red background
                )));
            }
        }
        
        // Show up to 5 lines of context after
        let context_after_end = context_after.len().min(5);
        
        for line in &context_after[..context_after_end] {
            let content = line.strip_prefix(' ').unwrap_or(line);
            lines.push(Line::from(Span::styled(
                format!("  {}", content),
                Style::default().fg(Color::DarkGray)
            )));
        }
        
        let text = Text::from(lines);
        
        let title_suffix = if self.app.reached_end() {
            " [END]"
        } else {
            ""
        };
        
        let title_focus = if self.app.focus() == FocusPane::HunkView {
            " [FOCUSED]"
        } else {
            ""
        };
        
        let border_style = if self.app.focus() == FocusPane::HunkView {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default()
        };
        
        let mut paragraph = Paragraph::new(text)
            .block(Block::default().borders(Borders::ALL).title(format!(
                "{} (Hunk {}/{}{}{})",
                file.path.to_string_lossy(),
                self.app.current_hunk_index() + 1,
                file.hunks.len(),
                title_suffix,
                title_focus
            )).border_style(border_style))
            .scroll((self.app.scroll_offset(), 0));
        
        // Apply wrapping if enabled
        if self.app.wrap_lines() {
            paragraph = paragraph.wrap(Wrap { trim: false });
        }
        
        frame.render_widget(paragraph, area);
    }
    
    fn draw_help_sidebar(&self, frame: &mut Frame, area: Rect) {
        let help_lines = vec![
            Line::from("Q: Quit"),
            Line::from("Tab: Focus"),
            Line::from("Space: Next"),
            Line::from("Shift+Space: Prev"),
            Line::from("J/K: Scroll/Nav"),
            Line::from("N/P: File"),
            Line::from("V: View"),
            Line::from("W: Wrap"),
            Line::from("H: Hide Help"),
            Line::from("C: Clear"),
            Line::from("F: Names"),
            Line::from("S: Speed"),
            Line::from("R: Refresh"),
            Line::from(""),
            Line::from("Enter/Esc:"),
            Line::from("  Toggle Mode"),
        ];
        
        let help = Paragraph::new(help_lines)
            .block(Block::default().borders(Borders::ALL).title("Keys"))
            .style(Style::default().fg(Color::Gray));
        
        frame.render_widget(help, area);
    }
}
