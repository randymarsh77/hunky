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
