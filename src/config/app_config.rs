//! Application configuration management

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Main application configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    /// Default project directory
    pub default_project_dir: Option<PathBuf>,
    /// Default server URL for remote operations
    pub default_server_url: String,
    /// Build configuration
    pub build: BuildConfig,
    /// UI configuration
    pub ui: UiConfig,
}

/// Build-related configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildConfig {
    /// Default build strategy
    pub default_strategy: String,
    /// Parallel build job count
    pub parallel_jobs: Option<usize>,
    /// Build timeout in seconds
    pub timeout_seconds: u64,
}

/// UI-related configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiConfig {
    /// Enable TUI by default
    pub enable_tui: bool,
    /// Log level
    pub log_level: String,
    /// Color scheme
    pub color_scheme: String,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            default_project_dir: None,
            default_server_url: "http://localhost:8080".to_string(),
            build: BuildConfig::default(),
            ui: UiConfig::default(),
        }
    }
}

impl Default for BuildConfig {
    fn default() -> Self {
        Self {
            default_strategy: "idf-build-apps".to_string(),
            parallel_jobs: None,
            timeout_seconds: 300,
        }
    }
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            enable_tui: true,
            log_level: "info".to_string(),
            color_scheme: "default".to_string(),
        }
    }
}
