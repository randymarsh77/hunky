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
fn syntect_color_to_ratatui(color: syntect::highlighting::Color) -> Color {
    Color::Rgb(color.r, color.g, color.b)
}

impl Default for SyntaxHighlighter {
    fn default() -> Self {
        Self::new()
    }
}
