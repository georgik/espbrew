//! Data models and types used throughout ESPBrew

pub mod board;
pub mod esp_idf_config;
pub mod events;
pub mod flash;
pub mod monitor;
pub mod project;
pub mod responses;
pub mod server;
pub mod tui;

// Re-export commonly used types
pub use board::*;
pub use events::*;
pub use flash::*;
pub use monitor::*;
pub use project::*;
pub use responses::*;
pub use server::*;

// Only export TUI-specific types that don't conflict
pub use tui::FocusedPane;
