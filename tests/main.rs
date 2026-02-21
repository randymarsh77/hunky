use super::*;
use clap::CommandFactory;

#[test]
fn parses_default_repo_argument() {
    let args = Args::try_parse_from(["hunky"]).expect("args should parse");
    assert_eq!(args.repo, ".");
}

#[test]
fn parses_explicit_repo_argument() {
    let args = Args::try_parse_from(["hunky", "--repo", "/tmp/custom"]).expect("args should parse");
    assert_eq!(args.repo, "/tmp/custom");
}

#[test]
fn parses_short_repo_argument() {
    let args = Args::try_parse_from(["hunky", "-r", "/tmp/short"]).expect("args should parse");
    assert_eq!(args.repo, "/tmp/short");
}

#[test]
fn help_text_mentions_tui_description() {
    let mut help = Vec::new();
    Args::command()
        .write_long_help(&mut help)
        .expect("help should render");
    let help = String::from_utf8(help).expect("help should be utf-8");
    assert!(help.contains("A TUI for streaming git changes in real-time"));
}

#[test]
fn unknown_argument_returns_error() {
    assert!(Args::try_parse_from(["hunky", "--unknown"]).is_err());
}
