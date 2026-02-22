use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
    Frame,
};

use crate::app::{App, FocusPane, Mode, StreamSpeed, StreamingType};
use crate::syntax::SyntaxHighlighter;

/// Fade a color by reducing its brightness (for context lines)
fn fade_color(color: Color) -> Color {
    match color {
        Color::Rgb(r, g, b) => {
            // Reduce brightness by about 60%
            let factor = 0.4;
            Color::Rgb(
                (r as f32 * factor) as u8,
                (g as f32 * factor) as u8,
                (b as f32 * factor) as u8,
            )
        }
        _ => Color::DarkGray,
    }
}

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

    pub fn draw(&self, frame: &mut Frame) -> (u16, u16, u16) {
        // Always use compact layout (no footer)
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Header
                Constraint::Min(0),    // Main content
            ])
            .split(frame.area());

        self.draw_header(frame, chunks[0]);
        let (diff_height, help_height, file_list_height) = self.draw_main_content(frame, chunks[1]);

        // Return viewport heights for clamping scroll offsets
        // file_list_height is unused but kept for API compatibility
        (diff_height, help_height, file_list_height)
    }

    fn draw_header(&self, frame: &mut Frame, area: Rect) {
        let available_width = area.width.saturating_sub(2) as usize; // Subtract borders
        let help_text = "H: Help";
        let help_width = help_text.len();

        // Determine which layout to use based on available width
        // Wide: > 80, Medium: > 50, Compact: > 40, Mini: <= 40
        let (mode_label, mode_text, title_text) = if available_width > 80 {
            // Full layout
            let mode_text = match self.app.mode() {
                Mode::View => "VIEW",
                Mode::Review => "REVIEW",
                Mode::Streaming(StreamingType::Buffered) => "STREAMING (Buffered)",
                Mode::Streaming(StreamingType::Auto(StreamSpeed::Fast)) => {
                    "STREAMING (Auto - Fast)"
                }
                Mode::Streaming(StreamingType::Auto(StreamSpeed::Medium)) => {
                    "STREAMING (Auto - Medium)"
                }
                Mode::Streaming(StreamingType::Auto(StreamSpeed::Slow)) => {
                    "STREAMING (Auto - Slow)"
                }
            };
            ("Mode: ", mode_text, "Hunky")
        } else if available_width > 50 {
            // Medium layout
            let mode_text = match self.app.mode() {
                Mode::View => "VIEW",
                Mode::Review => "REVIEW",
                Mode::Streaming(StreamingType::Buffered) => "STREAM (Buff)",
                Mode::Streaming(StreamingType::Auto(StreamSpeed::Fast)) => "STREAM (Fast)",
                Mode::Streaming(StreamingType::Auto(StreamSpeed::Medium)) => "STREAM (Med)",
                Mode::Streaming(StreamingType::Auto(StreamSpeed::Slow)) => "STREAM (Slow)",
            };
            ("M: ", mode_text, "Hunky")
        } else if available_width > 40 {
            // Compact layout
            let mode_text = match self.app.mode() {
                Mode::View => "VIEW",
                Mode::Review => "REV",
                Mode::Streaming(StreamingType::Buffered) => "STM:B",
                Mode::Streaming(StreamingType::Auto(StreamSpeed::Fast)) => "STM:F",
                Mode::Streaming(StreamingType::Auto(StreamSpeed::Medium)) => "STM:M",
                Mode::Streaming(StreamingType::Auto(StreamSpeed::Slow)) => "STM:S",
            };
            ("M:", mode_text, "Hunky")
        } else {
            // Mini layout - minimal info
            let mode_text = match self.app.mode() {
                Mode::View => "V",
                Mode::Review => "R",
                Mode::Streaming(StreamingType::Buffered) => "B",
                Mode::Streaming(StreamingType::Auto(StreamSpeed::Fast)) => "F",
                Mode::Streaming(StreamingType::Auto(StreamSpeed::Medium)) => "M",
                Mode::Streaming(StreamingType::Auto(StreamSpeed::Slow)) => "S",
            };
            ("", mode_text, "Hunky")
        };

        // Build title with help hint on the right side
        let mut title_left = vec![
            Span::styled(
                title_text,
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" | "),
        ];

        if !mode_label.is_empty() {
            title_left.push(Span::raw(mode_label));
        }
        title_left.push(Span::styled(mode_text, Style::default().fg(Color::Yellow)));

        // Show status message if present
        if let Some(msg) = self.app.status_message() {
            title_left.push(Span::raw(" | "));
            title_left.push(Span::styled(
                msg,
                Style::default().fg(Color::Green),
            ));
        }

        // Calculate padding to right-align help hint
        let left_width = title_left.iter().map(|s| s.content.len()).sum::<usize>();
        let padding_width = available_width.saturating_sub(left_width + help_width);

        let mut title_line = title_left;
        if padding_width > 0 {
            title_line.push(Span::raw(" ".repeat(padding_width)));
            title_line.push(Span::styled(help_text, Style::default().fg(Color::Gray)));
        }

        let header =
            Paragraph::new(Line::from(title_line)).block(Block::default().borders(Borders::ALL));

        frame.render_widget(header, area);
    }

    fn draw_main_content(&self, frame: &mut Frame, area: Rect) -> (u16, u16, u16) {
        // Check if commit picker overlay should be shown
        if self.app.review_selecting_commit() {
            self.draw_commit_picker(frame, area);
            return (0, 0, 0);
        }

        // Check if extended help view should be shown
        if self.app.show_extended_help() {
            let help_height = self.draw_extended_help(frame, area);
            return (0, help_height, 0);
        }

        // Check if help sidebar should be shown
        if self.app.show_help() {
            // Split into 3 columns: file list, diff, help
            let chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Percentage(25), // File list
                    Constraint::Min(0),         // Diff content (takes remaining space)
                    Constraint::Length(20),     // Help sidebar
                ])
                .split(area);

            self.draw_file_list(frame, chunks[0]);
            let diff_height = self.draw_diff_content(frame, chunks[1]);
            let help_height = self.draw_help_sidebar(frame, chunks[2]);
            (diff_height, help_height, 0)
        } else {
            // No help shown, just file list and diff
            let chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Percentage(25), // File list
                    Constraint::Percentage(75), // Diff content
                ])
                .split(area);

            self.draw_file_list(frame, chunks[0]);
            let diff_height = self.draw_diff_content(frame, chunks[1]);
            (diff_height, 0, 0)
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

        let is_review_mode = self.app.mode() == Mode::Review;

        let items: Vec<ListItem> = snapshot
            .files
            .iter()
            .enumerate()
            .map(|(idx, file)| {
                let file_name = file
                    .path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("unknown");

                let is_selected = idx == self.app.current_file_index();
                let name_style = if is_selected {
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };

                let hunk_count = file.hunks.len();

                let count_text = if is_review_mode {
                    let accepted_count = file.hunks.iter().filter(|h| h.accepted).count();
                    if accepted_count > 0 {
                        format!(" ({}) [{}✓]", hunk_count, accepted_count)
                    } else {
                        format!(" ({})", hunk_count)
                    }
                } else {
                    let staged_count = file.hunks.iter().filter(|h| h.staged).count();

                    // Count partially staged hunks
                    let partial_count = file
                        .hunks
                        .iter()
                        .filter(|h| {
                            let total_change_lines = h
                                .lines
                                .iter()
                                .filter(|line| {
                                    (line.starts_with('+') && !line.starts_with("+++"))
                                        || (line.starts_with('-') && !line.starts_with("---"))
                                })
                                .count();
                            let staged_lines = h.staged_line_indices.len();
                            staged_lines > 0 && staged_lines < total_change_lines
                        })
                        .count();

                    if staged_count > 0 || partial_count > 0 {
                        if partial_count > 0 {
                            format!(" ({}) [{}✓ {}⚠]", hunk_count, staged_count, partial_count)
                        } else {
                            format!(" ({}) [{}✓]", hunk_count, staged_count)
                        }
                    } else {
                        format!(" ({})", hunk_count)
                    }
                };

                let content = Line::from(vec![
                    Span::styled(file_name, name_style),
                    Span::styled(count_text, Style::default().fg(Color::DarkGray)),
                ]);

                ListItem::new(content)
            })
            .collect();

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

        let list = List::new(items).block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
                .border_style(border_style),
        );

        // Use stateful widget to handle scrolling automatically
        let mut state = ratatui::widgets::ListState::default();
        state.select(Some(self.app.current_file_index()));
        frame.render_stateful_widget(list, area, &mut state);
    }

    fn draw_diff_content(&self, frame: &mut Frame, area: Rect) -> u16 {
        // Return viewport height for clamping
        let viewport_height = area.height.saturating_sub(2); // Subtract borders

        let file = match self.app.current_file() {
            Some(f) => f,
            None => {
                let empty = Paragraph::new("No file selected")
                    .block(Block::default().borders(Borders::ALL).title("Diff"));
                frame.render_widget(empty, area);
                return viewport_height;
            }
        };

        if self.app.show_filenames_only() {
            let content = format!(
                "File: {}\nStatus: {}\nHunks: {}",
                file.path.display(),
                file.status,
                file.hunks.len()
            );
            let file_info_title = "File Info".to_string();
            let paragraph = Paragraph::new(content)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(file_info_title),
                )
                .wrap(Wrap { trim: true });
            frame.render_widget(paragraph, area);
            return viewport_height;
        }

        // Get only the current hunk (one hunk at a time UX)
        let current_hunk = file.hunks.get(self.app.current_hunk_index());

        if current_hunk.is_none() {
            let file_title = file.path.to_string_lossy().to_string();
            let empty = Paragraph::new("No hunks to display yet")
                .block(Block::default().borders(Borders::ALL).title(file_title));
            frame.render_widget(empty, area);
            return viewport_height;
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

        // Add hunk header with seen, staged, and accepted indicators
        // Check if partially staged
        let total_change_lines = hunk
            .lines
            .iter()
            .filter(|line| {
                (line.starts_with('+') && !line.starts_with("+++"))
                    || (line.starts_with('-') && !line.starts_with("---"))
            })
            .count();
        let staged_lines_count = hunk.staged_line_indices.len();
        let is_partially_staged = staged_lines_count > 0 && staged_lines_count < total_change_lines;
        let is_review_mode = self.app.mode() == Mode::Review;

        let base_header = format!(
            "@@ -{},{} +{},{} @@",
            hunk.old_start,
            hunk.lines.len(),
            hunk.new_start,
            hunk.lines.len()
        );

        let hunk_header = if is_review_mode {
            // In review mode, show accepted state
            if hunk.accepted {
                format!("{} [ACCEPTED ✓]", base_header)
            } else {
                base_header
            }
        } else if is_partially_staged {
            match hunk.seen {
                true => format!("{} [PARTIAL ⚠] [SEEN]", base_header),
                false => format!("{} [PARTIAL ⚠]", base_header),
            }
        } else {
            match (hunk.staged, hunk.seen) {
                (true, true) => format!("{} [STAGED ✓] [SEEN]", base_header),
                (true, false) => format!("{} [STAGED ✓]", base_header),
                (false, true) => format!("{} [SEEN]", base_header),
                (false, false) => base_header,
            }
        };

        let header_style = if is_review_mode && hunk.accepted {
            Style::default().fg(Color::Green)
        } else if is_partially_staged {
            Style::default().fg(Color::Yellow)
        } else if hunk.staged {
            Style::default().fg(Color::Green)
        } else if hunk.seen {
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

        for (idx, line) in hunk.lines.iter().enumerate() {
            if line.starts_with('+') || line.starts_with('-') {
                in_changes = true;
                changes.push((idx, line.clone()));
            } else if !in_changes {
                context_before.push(line.clone());
            } else {
                context_after.push(line.clone());
            }
        }

        // Create syntax highlighter for this file if enabled
        let mut file_highlighter = if self.app.syntax_highlighting() {
            Some(self.highlighter.create_highlighter(&file.path))
        } else {
            None
        };

        // Show up to 5 lines of context before
        let context_before_start = if context_before.len() > 5 {
            context_before.len() - 5
        } else {
            0
        };

        for line in &context_before[context_before_start..] {
            let content = line.strip_prefix(' ').unwrap_or(line);
            if let Some(ref mut highlighter) = file_highlighter {
                // Apply syntax highlighting with faded colors
                let highlighted = highlighter.highlight_line(content);
                let mut spans = vec![Span::raw("      ")]; // 6 spaces: 4 for indicators + 1 for +/- + 1 space
                for (color, text) in highlighted {
                    // Make syntax colors darker/faded for context
                    let faded_color = fade_color(color);
                    spans.push(Span::styled(text, Style::default().fg(faded_color)));
                }
                lines.push(Line::from(spans));
            } else {
                lines.push(Line::from(Span::styled(
                    format!("      {}", content), // 6 spaces: 4 for indicators + 1 for +/- + 1 space
                    Style::default().fg(Color::DarkGray),
                )));
            }
        }

        // Show changes with background colors for better visibility
        // Using very subtle colors: 233 (near-black with slight tint), 234 for contrast
        // Green additions: bg 22 → 236 (darker gray-green), prefix 28 → 34 (softer green)
        // Red additions: bg 52 → 235 (darker gray-red), prefix 88 → 124 (softer red)
        let line_selection_mode = self.app.line_selection_mode();
        let selected_line = self.app.selected_line_index();

        for (original_idx, line) in &changes {
            let is_selected = line_selection_mode && *original_idx == selected_line;
            let is_staged = hunk.staged_line_indices.contains(original_idx);

            // Build 4-character indicator prefix: [selection (2)][staged (2)]
            let selection_marker = if is_selected { "► " } else { "  " };
            let staged_marker = if is_staged { "✓ " } else { "  " };
            let indicator_prefix = format!("{}{}", selection_marker, staged_marker);

            if line.starts_with('+') {
                let content = line.strip_prefix('+').unwrap_or(line);
                if let Some(ref mut highlighter) = file_highlighter {
                    // Apply syntax highlighting with very subtle green background
                    let highlighted = highlighter.highlight_line(content);
                    let bg_color = if is_selected {
                        Color::Indexed(28)
                    } else {
                        Color::Indexed(236)
                    };
                    let fg_color = if is_selected {
                        Color::Indexed(46)
                    } else {
                        Color::Indexed(34)
                    };
                    let mut spans = vec![Span::styled(
                        format!("{}+ ", indicator_prefix),
                        Style::default().fg(fg_color).bg(bg_color),
                    )];
                    for (color, text) in highlighted {
                        // Apply syntax colors with subtle green-tinted background
                        spans.push(Span::styled(text, Style::default().fg(color).bg(bg_color)));
                    }
                    lines.push(Line::from(spans));
                } else {
                    let bg_color = if is_selected {
                        Color::Indexed(28)
                    } else {
                        Color::Indexed(236)
                    };
                    let fg_color = if is_selected {
                        Color::Indexed(46)
                    } else {
                        Color::Indexed(34)
                    };
                    lines.push(Line::from(Span::styled(
                        format!("{}+ {}", indicator_prefix, content),
                        Style::default().fg(fg_color).bg(bg_color),
                    )));
                }
            } else if line.starts_with('-') {
                let content = line.strip_prefix('-').unwrap_or(line);
                if let Some(ref mut highlighter) = file_highlighter {
                    // Apply syntax highlighting with very subtle red background
                    let highlighted = highlighter.highlight_line(content);
                    let bg_color = if is_selected {
                        Color::Indexed(52)
                    } else {
                        Color::Indexed(235)
                    };
                    let fg_color = if is_selected {
                        Color::Indexed(196)
                    } else {
                        Color::Indexed(124)
                    };
                    let mut spans = vec![Span::styled(
                        format!("{}- ", indicator_prefix),
                        Style::default().fg(fg_color).bg(bg_color),
                    )];
                    for (color, text) in highlighted {
                        // Apply syntax colors with subtle red-tinted background
                        spans.push(Span::styled(text, Style::default().fg(color).bg(bg_color)));
                    }
                    lines.push(Line::from(spans));
                } else {
                    let bg_color = if is_selected {
                        Color::Indexed(52)
                    } else {
                        Color::Indexed(235)
                    };
                    let fg_color = if is_selected {
                        Color::Indexed(196)
                    } else {
                        Color::Indexed(124)
                    };
                    lines.push(Line::from(Span::styled(
                        format!("{}- {}", indicator_prefix, content),
                        Style::default().fg(fg_color).bg(bg_color),
                    )));
                }
            }
        }

        // Show up to 5 lines of context after
        let context_after_end = context_after.len().min(5);

        for line in &context_after[..context_after_end] {
            let content = line.strip_prefix(' ').unwrap_or(line);
            if let Some(ref mut highlighter) = file_highlighter {
                // Apply syntax highlighting with faded colors
                let highlighted = highlighter.highlight_line(content);
                let mut spans = vec![Span::raw("      ")]; // 6 spaces: 4 for indicators + 1 for +/- + 1 space
                for (color, text) in highlighted {
                    // Make syntax colors darker/faded for context
                    let faded_color = fade_color(color);
                    spans.push(Span::styled(text, Style::default().fg(faded_color)));
                }
                lines.push(Line::from(spans));
            } else {
                lines.push(Line::from(Span::styled(
                    format!("      {}", content), // 6 spaces: 4 for indicators + 1 for +/- + 1 space
                    Style::default().fg(Color::DarkGray),
                )));
            }
        }

        let text = Text::from(lines);

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
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(format!(
                        "{} (Hunk {}/{}{})",
                        file.path.to_string_lossy(),
                        self.app.current_hunk_index() + 1,
                        file.hunks.len(),
                        title_focus
                    ))
                    .border_style(border_style),
            )
            .scroll((self.app.scroll_offset(), 0));

        // Apply wrapping if enabled
        if self.app.wrap_lines() {
            paragraph = paragraph.wrap(Wrap { trim: false });
        }

        frame.render_widget(paragraph, area);
        viewport_height
    }

    fn draw_help_sidebar(&self, frame: &mut Frame, area: Rect) -> u16 {
        // Return viewport height for clamping
        let viewport_height = area.height.saturating_sub(2); // Subtract borders

        let help_lines = vec![
            Line::from(Span::styled(
                "Navigation",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from("Q: Quit"),
            Line::from("Tab/Shift+Tab: Focus"),
            Line::from("Space: Next Hunk"),
            Line::from("B: Prev Hunk"),
            Line::from("J/K: Scroll/Nav"),
            Line::from("N/P: Next/Prev File"),
            Line::from(""),
            Line::from(Span::styled(
                "Modes",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from("M: Cycle Mode"),
            Line::from("  View → Streaming"),
            Line::from("  (Buffered/Auto)"),
            Line::from(""),
            Line::from(Span::styled(
                "Display",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from("W: Toggle Wrap"),
            Line::from("Y: Toggle Syntax"),
            Line::from("F: Filenames Only"),
            Line::from("H: Toggle Help"),
            Line::from("Shift+H: Extended Help"),
            Line::from(""),
            Line::from(Span::styled(
                "Staging",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from("L: Line Mode"),
            Line::from("S: Stage/Unstage"),
            Line::from(""),
            Line::from(Span::styled(
                "Review",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from("R: Review Commit"),
            Line::from("S: Accept (in review)"),
            Line::from("ESC: Exit Review"),
            Line::from(""),
            Line::from(Span::styled(
                "Other",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from("C: Commit (Open Editor)"),
            Line::from("Ctrl+Y: Copy to Clipboard"),
            Line::from("ESC: Reset to Defaults"),
        ];

        let is_focused = self.app.focus() == FocusPane::HelpSidebar;
        let border_color = if is_focused {
            Color::Cyan
        } else {
            Color::White
        };
        let title = if is_focused { "Keys [FOCUSED]" } else { "Keys" };

        let help = Paragraph::new(help_lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(border_color))
                    .title(title),
            )
            .style(Style::default().fg(Color::Gray))
            .scroll((self.app.help_scroll_offset(), 0));

        frame.render_widget(help, area);
        viewport_height
    }

    fn draw_commit_picker(&self, frame: &mut Frame, area: Rect) {
        let commits = self.app.review_commits();
        let cursor = self.app.review_commit_cursor();

        let items: Vec<ListItem> = commits
            .iter()
            .enumerate()
            .map(|(idx, commit)| {
                let is_selected = idx == cursor;
                let style = if is_selected {
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };

                let content = Line::from(vec![
                    Span::styled(
                        format!("{} ", commit.short_sha),
                        Style::default().fg(if is_selected {
                            Color::Cyan
                        } else {
                            Color::DarkGray
                        }),
                    ),
                    Span::styled(&commit.summary, style),
                    Span::styled(
                        format!(" ({})", commit.author),
                        Style::default().fg(Color::DarkGray),
                    ),
                ]);

                ListItem::new(content)
            })
            .collect();

        let list = List::new(items).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan))
                .title(
                    "Select a commit to review (↑/↓ to navigate, Enter to select, Esc to cancel)",
                ),
        );

        let mut state = ratatui::widgets::ListState::default();
        state.select(Some(cursor));
        frame.render_stateful_widget(list, area, &mut state);
    }

    fn draw_extended_help(&self, frame: &mut Frame, area: Rect) -> u16 {
        // Return viewport height for clamping
        let viewport_height = area.height.saturating_sub(2); // Subtract borders

        let help_content = vec![
            Line::from(Span::styled(
                "HUNKY - Extended Help",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "═══════════════════════════════════════════════════════════",
                Style::default().fg(Color::DarkGray),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "OVERVIEW",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from("Hunky is a terminal UI for reviewing and staging git changes at the hunk"),
            Line::from("or line level. It provides two main modes for different workflows:"),
            Line::from(""),
            Line::from(Span::styled(
                "MODES",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(vec![
                Span::styled(
                    "View Mode",
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(" - Browse all current changes"),
            ]),
            Line::from("  • Shows all changes from HEAD to working directory"),
            Line::from("  • Full navigation with Space (next) and Shift+Space (previous)"),
            Line::from("  • Ideal for reviewing existing changes before committing"),
            Line::from("  • Default mode when starting Hunky"),
            Line::from(""),
            Line::from(vec![
                Span::styled(
                    "Streaming Mode",
                    Style::default()
                        .fg(Color::Magenta)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(" - Watch new changes as they appear"),
            ]),
            Line::from("  • Only shows hunks that appear after entering this mode"),
            Line::from("  • Two sub-modes:"),
            Line::from("    - Buffered: Manual advance with Space key"),
            Line::from("    - Auto (Fast/Medium/Slow): Automatic advancement with timing"),
            Line::from("  • Perfect for TDD workflows or watching build output changes"),
            Line::from("  • Press M to cycle through streaming options"),
            Line::from(""),
            Line::from(Span::styled(
                "NAVIGATION",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from("  Space           Next hunk (all modes)"),
            Line::from("  B               Previous hunk (View & Buffered modes)"),
            Line::from("  J/K or ↓/↑      Scroll hunk view or navigate in line mode"),
            Line::from("  N/P             Next/Previous file"),
            Line::from("  Tab             Cycle focus forward (File → Hunk → Help)"),
            Line::from("  Shift+Tab       Cycle focus backward"),
            Line::from(""),
            Line::from(Span::styled(
                "STAGING",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from("  S               Smart stage/unstage toggle"),
            Line::from("  L               Toggle Line Mode for line-level staging"),
            Line::from(""),
            Line::from("Smart Toggle Behavior (Hunk Mode):"),
            Line::from("  • Unstaged → Press S → Fully staged"),
            Line::from("  • Partially staged → Press S → Fully staged"),
            Line::from("  • Fully staged → Press S → Fully unstaged"),
            Line::from(""),
            Line::from("In Line Mode:"),
            Line::from("  • Use J/K to navigate between changed lines (+ or -)"),
            Line::from("  • Press S to toggle staging for the selected line"),
            Line::from("  • Staged lines show a ✓ indicator"),
            Line::from("  • External changes (e.g., git add -p) are detected automatically"),
            Line::from(""),
            Line::from(Span::styled(
                "DISPLAY OPTIONS",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from("  H               Toggle help sidebar"),
            Line::from("  Shift+H         Toggle this extended help view"),
            Line::from("  F               Toggle filenames-only mode (hide diffs)"),
            Line::from("  W               Toggle line wrapping"),
            Line::from("  Y               Toggle syntax highlighting"),
            Line::from(""),
            Line::from(Span::styled(
                "MODE SWITCHING",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from("  M               Cycle through modes:"),
            Line::from("                    View → Streaming (Buffered) → Streaming (Auto Fast)"),
            Line::from(
                "                    → Streaming (Auto Medium) → Streaming (Auto Slow) → View",
            ),
            Line::from(""),
            Line::from("When switching to Streaming mode, Hunky captures the current state and"),
            Line::from("will only show new hunks that appear after the switch."),
            Line::from(""),
            Line::from(Span::styled(
                "CLIPBOARD",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from("  Ctrl+Y          Copy to system clipboard (via OSC 52)"),
            Line::from(""),
            Line::from("  In normal mode, copies the entire current hunk."),
            Line::from("  In Line Mode (L), copies only the selected line."),
            Line::from("  Line content is copied without the diff prefix (+/-/ )."),
            Line::from(""),
            Line::from(Span::styled(
                "RESET TO DEFAULTS",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from("  ESC             Reset everything to defaults:"),
            Line::from("                    • Exit extended help view"),
            Line::from("                    • Set mode to View"),
            Line::from("                    • Exit line mode"),
            Line::from("                    • Focus hunk view"),
            Line::from("                    • Hide help sidebar"),
            Line::from(""),
            Line::from(Span::styled(
                "WORKFLOWS",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(vec![
                Span::styled("Code Review", Style::default().fg(Color::Green)),
                Span::raw(" - Use View mode to browse all changes, stage what you want"),
            ]),
            Line::from("to commit, then press C to open your configured git editor."),
            Line::from(""),
            Line::from(vec![
                Span::styled("TDD Workflow", Style::default().fg(Color::Magenta)),
                Span::raw(" - Switch to Streaming (Auto) mode, run tests in"),
            ]),
            Line::from("another terminal, and watch test changes flow through Hunky as you"),
            Line::from("iterate on your code."),
            Line::from(""),
            Line::from(vec![
                Span::styled("Partial Staging", Style::default().fg(Color::Cyan)),
                Span::raw(" - Enable Line Mode (L) to stage specific lines"),
            ]),
            Line::from(
                "within a hunk. Great for separating formatting changes from logic changes.",
            ),
            Line::from(""),
            Line::from(Span::styled(
                "═══════════════════════════════════════════════════════════",
                Style::default().fg(Color::DarkGray),
            )),
            Line::from(""),
            Line::from("Press ESC to exit this help view and return to normal operation."),
            Line::from("Press J/K to scroll through this help."),
        ];

        let help = Paragraph::new(help_content)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Cyan))
                    .title("Extended Help (Shift+H to close, ESC to reset)"),
            )
            .style(Style::default().fg(Color::White))
            .wrap(Wrap { trim: false })
            .scroll((self.app.extended_help_scroll_offset(), 0));

        frame.render_widget(help, area);
        viewport_height
    }
}

#[cfg(test)]
#[path = "../tests/ui.rs"]
mod tests;
