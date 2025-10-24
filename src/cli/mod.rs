//! Command Line Interface module
//!
//! This module contains the CLI argument parsing, command implementations,
//! and the Terminal User Interface (TUI) components.

pub mod args;
pub mod commands;
pub mod tui;
pub mod url_handler;

pub use args::*;

use anyhow::Result;

/// Main CLI application runner
pub async fn run() -> Result<()> {
    let cli = Cli::parse_args();

    match &cli.command {
        Some(command) => {
            // Run specific command
            commands::execute_command(command.clone(), &cli).await
        }
        None => {
            // Default behavior - run TUI or CLI based on flags
            if cli.cli {
                commands::list::execute_list_command(&cli).await
            } else {
                tui::run_tui(cli).await
            }
        }
    }
}
