use std::path::Path;
use ratatui::style::Color;
use syntect::easy::HighlightLines;
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSet;

pub struct SyntaxHighlighter {
    syntax_set: SyntaxSet,
    theme_set: ThemeSet,
}

impl SyntaxHighlighter {
    pub fn new() -> Self {
        Self {
            syntax_set: SyntaxSet::load_defaults_newlines(),
            theme_set: ThemeSet::load_defaults(),
        }
    }
    
    /// Get a highlighter for a specific file that can be used to highlight multiple lines sequentially
    pub fn create_highlighter(&self, file_path: &Path) -> FileHighlighter<'_> {
        let syntax = self.syntax_set
            .find_syntax_for_file(file_path)
            .ok()
            .flatten()
            .unwrap_or_else(|| self.syntax_set.find_syntax_plain_text());
        
        let theme = &self.theme_set.themes["base16-ocean.dark"];
        
        FileHighlighter {
            highlighter: HighlightLines::new(syntax, theme),
            syntax_set: &self.syntax_set,
        }
    }
    
    #[allow(dead_code)]
    pub fn detect_language(&self, file_path: &Path) -> Option<String> {
        self.syntax_set
            .find_syntax_for_file(file_path)
            .ok()
            .flatten()
            .map(|s| s.name.clone())
    }
}

pub struct FileHighlighter<'a> {
    highlighter: HighlightLines<'a>,
    syntax_set: &'a SyntaxSet,
}

impl<'a> FileHighlighter<'a> {
    /// Highlight a single line (must be called sequentially for proper context)
    pub fn highlight_line(&mut self, line: &str) -> Vec<(Color, String)> {
        let mut result = Vec::new();
        
        if let Ok(ranges) = self.highlighter.highlight_line(line, self.syntax_set) {
            for (style, text) in ranges {
                let color = syntect_color_to_ratatui(style.foreground);
                result.push((color, text.to_string()));
            }
        }
        
        result
    }
}

/// Convert syntect color to ratatui color
/// Uses 256-color palette for better terminal compatibility (macOS Terminal.app)
fn syntect_color_to_ratatui(color: syntect::highlighting::Color) -> Color {
    rgb_to_ansi256(color.r, color.g, color.b)
}

/// Convert RGB color to nearest ANSI 256-color palette index
/// This provides better compatibility with terminals that don't support 24-bit true color
fn rgb_to_ansi256(r: u8, g: u8, b: u8) -> Color {
    // Check if it's a grayscale color
    let max_diff = r.abs_diff(g).max(g.abs_diff(b)).max(r.abs_diff(b));
    
    if max_diff < 10 {
        // It's grayscale - use the 24 grayscale colors (232-255)
        // Map 0-255 to 0-23
        let gray_index = ((r as u16 * 23) / 255) as u8;
        return Color::Indexed(232 + gray_index);
    }
    
    // Map to 6x6x6 color cube (16-231)
    // Each component is mapped to 0-5
    let r_index = (r as u16 * 5 / 255) as u8;
    let g_index = (g as u16 * 5 / 255) as u8;
    let b_index = (b as u16 * 5 / 255) as u8;
    
    let index = 16 + 36 * r_index + 6 * g_index + b_index;
    Color::Indexed(index)
}

impl Default for SyntaxHighlighter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn detect_language_for_rust_file() {
        let highlighter = SyntaxHighlighter::new();
        let language = highlighter.detect_language(Path::new("example.rs"));
        assert_eq!(language.as_deref(), Some("Rust"));
    }

    #[test]
    fn highlight_line_returns_colored_segments() {
        let highlighter = SyntaxHighlighter::new();
        let mut file_highlighter = highlighter.create_highlighter(Path::new("example.rs"));
        let highlighted = file_highlighter.highlight_line("fn main() {}\n");
        assert!(!highlighted.is_empty());
    }

    #[test]
    fn rgb_to_ansi256_maps_grayscale_and_color_cube() {
        assert_eq!(rgb_to_ansi256(128, 128, 128), Color::Indexed(243));
        assert_eq!(rgb_to_ansi256(255, 0, 0), Color::Indexed(196));
    }

    #[test]
    fn default_constructor_and_plain_text_fallback_work() {
        let highlighter = SyntaxHighlighter::default();
        let mut file_highlighter = highlighter.create_highlighter(Path::new("unknown.customext"));
        let highlighted = file_highlighter.highlight_line("plain text\n");
        assert!(!highlighted.is_empty());
    }

    #[test]
    fn highlight_line_handles_invalid_scope_without_panicking() {
        let highlighter = SyntaxHighlighter::new();
        let mut file_highlighter = highlighter.create_highlighter(Path::new("example.rs"));
        let highlighted = file_highlighter.highlight_line("\u{0000}\n");
        assert!(highlighted.iter().all(|(_, segment)| !segment.is_empty()));
    }
}
