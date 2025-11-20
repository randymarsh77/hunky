mod app;
mod git;
mod ui;
mod watcher;
mod diff;
mod syntax;

use anyhow::Result;
use app::App;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize the application
    let mut app = App::new().await?;
    
    // Run the application
    app.run().await?;
    
    Ok(())
}
