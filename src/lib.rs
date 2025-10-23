//! ESPBrew - Multi-Platform ESP32 Build Manager
//!
//! ESPBrew is a comprehensive build manager for ESP32 projects supporting multiple
//! frameworks including ESP-IDF, Rust no_std, Arduino, MicroPython, CircuitPython,
//! TinyGo, Zephyr, NuttX, and PlatformIO.

pub mod cli;
pub mod config;
pub mod errors;
pub mod espflash_local;
pub mod models;
pub mod platform;
pub mod projects;
pub mod remote;
pub mod security;
pub mod server;
pub mod services;
pub mod ui;
pub mod utils;

// Re-export commonly used types
pub use errors::*;
pub use models::*;
pub use projects::{ProjectHandler, ProjectRegistry, ProjectType};

/// ESPBrew version
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// ESPBrew application name
pub const APP_NAME: &str = "espbrew";
