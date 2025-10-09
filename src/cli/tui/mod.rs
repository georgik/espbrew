//! Terminal User Interface components

pub mod app;
pub mod components;
pub mod event_loop;
pub mod events;
pub mod main_app;
pub mod ui;

#[cfg(test)]
mod tests;

use crate::cli::args::Cli;
use anyhow::Result;

/// Run the Terminal User Interface
pub async fn run_tui(cli: Cli) -> Result<()> {
    let mut app = app::TuiApp::new(cli)?;
    app.run().await
}
