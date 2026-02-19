mod app;
mod git;
mod ui;
mod watcher;
mod diff;
mod syntax;

use anyhow::Result;
use app::App;
use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "hunky")]
#[command(about = "A TUI for streaming git changes in real-time", long_about = None)]
struct Args {
    /// Path to the git repository to watch
    #[arg(short, long, default_value = ".")]
    repo: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    
    // Initialize the application with the specified repository
    let mut app = App::new(&args.repo).await?;
    
    // Run the application
    app.run().await?;
    
    Ok(())
}

#[cfg(test)]
mod tests {
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
}
