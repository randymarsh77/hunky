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
