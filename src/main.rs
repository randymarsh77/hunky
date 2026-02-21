mod app;
mod diff;
mod git;
mod logger;
mod syntax;
mod ui;
mod watcher;

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
    logger::init();

    // Initialize the application with the specified repository
    let mut app = App::new(&args.repo).await?;

    // Run the application
    app.run().await?;

    Ok(())
}

#[cfg(test)]
#[path = "../tests/main.rs"]
mod tests;
