//! Custom error types for ESPBrew

use std::fmt;

/// Main error type for ESPBrew operations
#[derive(Debug)]
pub enum ESPBrewError {
    /// Configuration related errors
    Config(String),
    /// Project detection/handling errors
    Project(String),
    /// Board communication errors
    Board(String),
    /// Flash operation errors
    Flash(String),
    /// Monitor operation errors
    Monitor(String),
    /// Network/remote operation errors
    Remote(String),
    /// File system errors
    FileSystem(String),
    /// Build system errors
    Build(String),
    /// TUI related errors
    Tui(String),
    /// General I/O errors
    Io(std::io::Error),
    /// Serialization errors
    Serialization(String),
}

impl fmt::Display for ESPBrewError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ESPBrewError::Config(msg) => write!(f, "Configuration error: {}", msg),
            ESPBrewError::Project(msg) => write!(f, "Project error: {}", msg),
            ESPBrewError::Board(msg) => write!(f, "Board error: {}", msg),
            ESPBrewError::Flash(msg) => write!(f, "Flash error: {}", msg),
            ESPBrewError::Monitor(msg) => write!(f, "Monitor error: {}", msg),
            ESPBrewError::Remote(msg) => write!(f, "Remote operation error: {}", msg),
            ESPBrewError::FileSystem(msg) => write!(f, "File system error: {}", msg),
            ESPBrewError::Build(msg) => write!(f, "Build error: {}", msg),
            ESPBrewError::Tui(msg) => write!(f, "TUI error: {}", msg),
            ESPBrewError::Io(err) => write!(f, "I/O error: {}", err),
            ESPBrewError::Serialization(msg) => write!(f, "Serialization error: {}", msg),
        }
    }
}

impl std::error::Error for ESPBrewError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ESPBrewError::Io(err) => Some(err),
            _ => None,
        }
    }
}

impl From<std::io::Error> for ESPBrewError {
    fn from(err: std::io::Error) -> Self {
        ESPBrewError::Io(err)
    }
}

impl From<serde_json::Error> for ESPBrewError {
    fn from(err: serde_json::Error) -> Self {
        ESPBrewError::Serialization(err.to_string())
    }
}

impl From<serde_yaml::Error> for ESPBrewError {
    fn from(err: serde_yaml::Error) -> Self {
        ESPBrewError::Serialization(err.to_string())
    }
}

/// Result type alias for ESPBrew operations
pub type Result<T> = std::result::Result<T, ESPBrewError>;
