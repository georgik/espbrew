//! GUI mode for ESPBrew using Slint
//!
//! This module provides a modern desktop GUI alternative to the TUI,
//! reusing all existing business logic while providing enhanced user experience.

pub mod adapters;
pub mod components;
pub mod event_handler;
pub mod main_window;

use crate::cli::tui::main_app::App;
use anyhow::Result;

/// Run the application in GUI mode
pub async fn run_gui_mode(app: App) -> Result<()> {
    main_window::run_main_window(app).await
}
