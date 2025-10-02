use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::any::Any;
use std::path::{Path, PathBuf};
use tokio::sync::mpsc;

use crate::AppEvent;

/// Represents different types of projects that espbrew can handle
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ProjectType {
    EspIdf,
    RustNoStd,
    Arduino, // Future support
}

impl ProjectType {
    pub fn name(&self) -> &'static str {
        match self {
            ProjectType::EspIdf => "ESP-IDF",
            ProjectType::RustNoStd => "Rust no_std",
            ProjectType::Arduino => "Arduino",
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            ProjectType::EspIdf => "ESP-IDF project with CMake build system",
            ProjectType::RustNoStd => "Embedded Rust project with Cargo",
            ProjectType::Arduino => "Arduino project with Arduino IDE",
        }
    }
}

/// Configuration for a specific board/target within a project
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoardConfig {
    pub name: String,
    pub config_file: PathBuf,
    pub build_dir: PathBuf,
    pub target: Option<String>, // ESP32, ESP32-S3, etc.
    pub project_type: ProjectType,
}

/// Information about build artifacts (binaries, ELF files, etc.)
#[derive(Debug, Clone)]
pub struct BuildArtifact {
    pub name: String,
    pub file_path: PathBuf,
    pub artifact_type: ArtifactType,
    pub offset: Option<u32>, // Flash offset for ESP-IDF binaries
}

#[derive(Debug, Clone, PartialEq)]
pub enum ArtifactType {
    Application,
    Bootloader,
    PartitionTable,
    Binary,
    Elf,
}

/// Represents the status of a build operation
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BuildStatus {
    Pending,
    Building,
    Success,
    Failed,
    Flashing,
    Flashed,
}

impl BuildStatus {
    pub fn color(&self) -> ratatui::style::Color {
        match self {
            BuildStatus::Pending => ratatui::style::Color::Gray,
            BuildStatus::Building => ratatui::style::Color::Yellow,
            BuildStatus::Success => ratatui::style::Color::Green,
            BuildStatus::Failed => ratatui::style::Color::Red,
            BuildStatus::Flashing => ratatui::style::Color::Cyan,
            BuildStatus::Flashed => ratatui::style::Color::Blue,
        }
    }

    pub fn symbol(&self) -> &'static str {
        match self {
            BuildStatus::Pending => "⏳",
            BuildStatus::Building => "⚙️ ",
            BuildStatus::Success => "✅",
            BuildStatus::Failed => "❌",
            BuildStatus::Flashing => "📡",
            BuildStatus::Flashed => "🔥",
        }
    }
}

/// Common operations that all project types must support
#[async_trait]
pub trait ProjectHandler: Send + Sync {
    /// Enable downcasting to concrete types
    fn as_any(&self) -> &dyn std::any::Any;

    /// Get the project type
    fn project_type(&self) -> ProjectType;

    /// Detect if this handler can handle the given project directory
    fn can_handle(&self, project_dir: &Path) -> bool;

    /// Discover available boards/targets in the project
    fn discover_boards(&self, project_dir: &Path) -> Result<Vec<BoardConfig>>;

    /// Build a specific board configuration
    async fn build_board(
        &self,
        project_dir: &Path,
        board_config: &BoardConfig,
        tx: mpsc::UnboundedSender<AppEvent>,
    ) -> Result<Vec<BuildArtifact>>;

    /// Flash build artifacts to a device
    async fn flash_board(
        &self,
        project_dir: &Path,
        board_config: &BoardConfig,
        artifacts: &[BuildArtifact],
        port: Option<&str>,
        tx: mpsc::UnboundedSender<AppEvent>,
    ) -> Result<()>;

    /// Monitor serial output from a device
    async fn monitor_board(
        &self,
        project_dir: &Path,
        board_config: &BoardConfig,
        port: Option<&str>,
        baud_rate: u32,
        tx: mpsc::UnboundedSender<AppEvent>,
    ) -> Result<()>;

    /// Clean build artifacts
    async fn clean_board(
        &self,
        project_dir: &Path,
        board_config: &BoardConfig,
        tx: mpsc::UnboundedSender<AppEvent>,
    ) -> Result<()>;

    /// Get the build command for display purposes
    fn get_build_command(&self, project_dir: &Path, board_config: &BoardConfig) -> String;

    /// Get the flash command for display purposes
    fn get_flash_command(
        &self,
        project_dir: &Path,
        board_config: &BoardConfig,
        port: Option<&str>,
    ) -> String;

    /// Check if the required tools for this project type are available
    fn check_tools_available(&self) -> Result<(), String>;

    /// Get a user-friendly warning message when tools are missing
    fn get_missing_tools_message(&self) -> String;
}

/// Detects the project type in a given directory
pub struct ProjectDetector;

impl ProjectDetector {
    /// Detect the project type and return an appropriate handler
    pub fn detect_project(project_dir: &Path) -> Option<Box<dyn ProjectHandler>> {
        // Check for Rust no_std project (Cargo.toml with embedded dependencies)
        if Self::is_rust_nostd_project(project_dir) {
            return Some(Box::new(super::rust_nostd::RustNoStdHandler));
        }

        // Check for ESP-IDF project (CMakeLists.txt and sdkconfig files)
        if Self::is_esp_idf_project(project_dir) {
            return Some(Box::new(super::esp_idf::EspIdfHandler));
        }

        // TODO: Add Arduino detection
        // if Self::is_arduino_project(project_dir) {
        //     return Some(Box::new(super::arduino::ArduinoHandler));
        // }

        None
    }

    /// Detect all available project handlers for a directory
    pub fn detect_all_handlers(project_dir: &Path) -> Vec<Box<dyn ProjectHandler>> {
        let mut handlers = Vec::new();

        let all_handlers: Vec<Box<dyn ProjectHandler>> = vec![
            Box::new(super::rust_nostd::RustNoStdHandler),
            Box::new(super::esp_idf::EspIdfHandler),
            // TODO: Add Arduino handler when implemented
        ];

        for handler in all_handlers {
            if handler.can_handle(project_dir) {
                handlers.push(handler);
            }
        }

        handlers
    }

    fn is_rust_nostd_project(project_dir: &Path) -> bool {
        let cargo_toml = project_dir.join("Cargo.toml");
        if !cargo_toml.exists() {
            return false;
        }

        // Check if it's an embedded Rust project
        if let Ok(content) = std::fs::read_to_string(&cargo_toml) {
            // Look for common embedded Rust dependencies
            content.contains("esp-hal")
                || content.contains("esp-backtrace")
                || content.contains("esp-println")
                || content.contains("embedded-hal")
                || (content.contains("no_std")
                    && (content.contains("esp32") || content.contains("esp")))
        } else {
            false
        }
    }

    fn is_esp_idf_project(project_dir: &Path) -> bool {
        let cmake_file = project_dir.join("CMakeLists.txt");
        let sdkconfig_exists = project_dir.join("sdkconfig").exists()
            || project_dir
                .read_dir()
                .map(|mut entries| {
                    entries.any(|entry| {
                        entry
                            .map(|e| {
                                e.file_name()
                                    .to_string_lossy()
                                    .starts_with("sdkconfig.defaults")
                            })
                            .unwrap_or(false)
                    })
                })
                .unwrap_or(false);

        cmake_file.exists() && sdkconfig_exists
    }

    fn is_arduino_project(project_dir: &Path) -> bool {
        // Look for .ino files (main Arduino sketch files)
        if let Ok(entries) = project_dir.read_dir() {
            for entry in entries.flatten() {
                if let Some(extension) = entry.path().extension() {
                    if extension == "ino" {
                        return true;
                    }
                }
            }
        }

        // Look for common Arduino files
        project_dir.join("arduino_secrets.h").exists()
            || project_dir.join("libraries").exists()
            || project_dir.join("sketches").exists()
    }
}

/// Registry of all available project handlers
pub struct ProjectRegistry {
    handlers: Vec<Box<dyn ProjectHandler>>,
}

impl ProjectRegistry {
    pub fn new() -> Self {
        Self {
            handlers: vec![
                Box::new(super::rust_nostd::RustNoStdHandler),
                Box::new(super::esp_idf::EspIdfHandler),
                // TODO: Add Arduino handler
            ],
        }
    }

    pub fn detect_project(&self, project_dir: &Path) -> Option<Box<dyn ProjectHandler>> {
        if super::rust_nostd::RustNoStdHandler.can_handle(project_dir) {
            return Some(Box::new(super::rust_nostd::RustNoStdHandler));
        }
        if super::esp_idf::EspIdfHandler.can_handle(project_dir) {
            return Some(Box::new(super::esp_idf::EspIdfHandler));
        }
        None
    }

    pub fn get_handler(&self, project_type: &ProjectType) -> Option<&dyn ProjectHandler> {
        self.handlers
            .iter()
            .find(|handler| handler.project_type() == *project_type)
            .map(|handler| handler.as_ref())
    }

    pub fn get_handler_by_type(
        &self,
        project_type: &ProjectType,
    ) -> Option<Box<dyn ProjectHandler>> {
        match project_type {
            ProjectType::RustNoStd => Some(Box::new(super::rust_nostd::RustNoStdHandler)),
            ProjectType::EspIdf => Some(Box::new(super::esp_idf::EspIdfHandler)),
            ProjectType::Arduino => None, // TODO: implement when Arduino handler is ready
        }
    }

    pub fn list_supported_types(&self) -> Vec<ProjectType> {
        self.handlers
            .iter()
            .map(|handler| handler.project_type())
            .collect()
    }
}

impl Default for ProjectRegistry {
    fn default() -> Self {
        Self::new()
    }
}
