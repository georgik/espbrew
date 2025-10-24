//! Project handler registry and trait definitions

use anyhow::Result;
use async_trait::async_trait;
use std::path::Path;
use tokio::sync::mpsc;

use crate::models::{AppEvent, BuildArtifact, ProjectBoardConfig, ProjectType};

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
    fn discover_boards(&self, project_dir: &Path) -> Result<Vec<ProjectBoardConfig>>;

    /// Check if build artifacts exist for this board configuration
    fn check_artifacts_exist(&self, project_dir: &Path, board_config: &ProjectBoardConfig) -> bool;

    /// Build a specific board configuration
    async fn build_board(
        &self,
        project_dir: &Path,
        board_config: &ProjectBoardConfig,
        tx: mpsc::UnboundedSender<AppEvent>,
    ) -> Result<Vec<BuildArtifact>>;

    /// Flash build artifacts to a device
    async fn flash_board(
        &self,
        project_dir: &Path,
        board_config: &ProjectBoardConfig,
        artifacts: &[BuildArtifact],
        port: Option<&str>,
        tx: mpsc::UnboundedSender<AppEvent>,
    ) -> Result<()>;

    /// Monitor serial output from a device
    async fn monitor_board(
        &self,
        project_dir: &Path,
        board_config: &ProjectBoardConfig,
        port: Option<&str>,
        baud_rate: u32,
        tx: mpsc::UnboundedSender<AppEvent>,
    ) -> Result<()>;

    /// Clean build artifacts
    async fn clean_board(
        &self,
        project_dir: &Path,
        board_config: &ProjectBoardConfig,
        tx: mpsc::UnboundedSender<AppEvent>,
    ) -> Result<()>;

    /// Get the build command for display purposes
    fn get_build_command(&self, project_dir: &Path, board_config: &ProjectBoardConfig) -> String;

    /// Get the flash command for display purposes
    fn get_flash_command(
        &self,
        project_dir: &Path,
        board_config: &ProjectBoardConfig,
        port: Option<&str>,
    ) -> String;

    /// Check if the required tools for this project type are available
    fn check_tools_available(&self) -> Result<(), String>;

    /// Get a user-friendly warning message when tools are missing
    fn get_missing_tools_message(&self) -> String;
}

/// Registry for managing project handlers
pub struct ProjectRegistry {
    handlers: Vec<Box<dyn ProjectHandler>>,
}

impl ProjectRegistry {
    /// Create a new registry with all supported project handlers
    pub fn new() -> Self {
        let mut handlers: Vec<Box<dyn ProjectHandler>> = Vec::new();

        // Add all project handler implementations
        handlers.push(Box::new(crate::projects::handlers::arduino::ArduinoHandler));
        handlers.push(Box::new(crate::projects::handlers::esp_idf::EspIdfHandler));
        handlers.push(Box::new(
            crate::projects::handlers::rust_nostd::RustNoStdHandler,
        ));
        handlers.push(Box::new(
            crate::projects::handlers::platformio::PlatformIOHandler,
        ));
        handlers.push(Box::new(
            crate::projects::handlers::micropython::MicroPythonHandler,
        ));
        handlers.push(Box::new(
            crate::projects::handlers::circuitpython::CircuitPythonHandler,
        ));
        handlers.push(Box::new(crate::projects::handlers::zephyr::ZephyrHandler));
        handlers.push(Box::new(crate::projects::handlers::nuttx::NuttXHandler));
        handlers.push(Box::new(crate::projects::handlers::tinygo::TinyGoHandler));
        handlers.push(Box::new(crate::projects::handlers::jaculus::JaculusHandler));

        Self { handlers }
    }

    /// Detect the appropriate project handler for a directory
    pub fn detect_project(&self, project_dir: &Path) -> Option<&dyn ProjectHandler> {
        self.handlers
            .iter()
            .find(|handler| handler.can_handle(project_dir))
            .map(|handler| handler.as_ref())
    }

    /// Detect the appropriate project handler and return a new boxed instance
    pub fn detect_project_boxed(&self, project_dir: &Path) -> Option<Box<dyn ProjectHandler>> {
        // Find the handler type and create a new instance
        if self.handlers.iter().any(|h| {
            h.as_any().type_id() == std::any::TypeId::of::<crate::projects::handlers::arduino::ArduinoHandler>() && h.can_handle(project_dir)
        }) {
            Some(Box::new(crate::projects::handlers::arduino::ArduinoHandler))
        } else if self.handlers.iter().any(|h| {
            h.as_any().type_id() == std::any::TypeId::of::<crate::projects::handlers::esp_idf::EspIdfHandler>() && h.can_handle(project_dir)
        }) {
            Some(Box::new(crate::projects::handlers::esp_idf::EspIdfHandler))
        } else if self.handlers.iter().any(|h| {
            h.as_any().type_id() == std::any::TypeId::of::<crate::projects::handlers::rust_nostd::RustNoStdHandler>() && h.can_handle(project_dir)
        }) {
            Some(Box::new(crate::projects::handlers::rust_nostd::RustNoStdHandler))
        } else if self.handlers.iter().any(|h| {
            h.as_any().type_id() == std::any::TypeId::of::<crate::projects::handlers::platformio::PlatformIOHandler>() && h.can_handle(project_dir)
        }) {
            Some(Box::new(crate::projects::handlers::platformio::PlatformIOHandler))
        } else if self.handlers.iter().any(|h| {
            h.as_any().type_id() == std::any::TypeId::of::<crate::projects::handlers::micropython::MicroPythonHandler>() && h.can_handle(project_dir)
        }) {
            Some(Box::new(crate::projects::handlers::micropython::MicroPythonHandler))
        } else if self.handlers.iter().any(|h| {
            h.as_any().type_id() == std::any::TypeId::of::<crate::projects::handlers::circuitpython::CircuitPythonHandler>() && h.can_handle(project_dir)
        }) {
            Some(Box::new(crate::projects::handlers::circuitpython::CircuitPythonHandler))
        } else if self.handlers.iter().any(|h| {
            h.as_any().type_id() == std::any::TypeId::of::<crate::projects::handlers::zephyr::ZephyrHandler>() && h.can_handle(project_dir)
        }) {
            Some(Box::new(crate::projects::handlers::zephyr::ZephyrHandler))
        } else if self.handlers.iter().any(|h| {
            h.as_any().type_id() == std::any::TypeId::of::<crate::projects::handlers::nuttx::NuttXHandler>() && h.can_handle(project_dir)
        }) {
            Some(Box::new(crate::projects::handlers::nuttx::NuttXHandler))
        } else if self.handlers.iter().any(|h| {
            h.as_any().type_id() == std::any::TypeId::of::<crate::projects::handlers::tinygo::TinyGoHandler>() && h.can_handle(project_dir)
        }) {
            Some(Box::new(crate::projects::handlers::tinygo::TinyGoHandler))
        } else if self.handlers.iter().any(|h| {
            h.as_any().type_id() == std::any::TypeId::of::<crate::projects::handlers::jaculus::JaculusHandler>() && h.can_handle(project_dir)
        }) {
            Some(Box::new(crate::projects::handlers::jaculus::JaculusHandler))
        } else {
            None
        }
    }

    /// Get all registered handlers
    pub fn get_all_handlers(&self) -> &[Box<dyn ProjectHandler>] {
        &self.handlers
    }
}

impl Default for ProjectRegistry {
    fn default() -> Self {
        Self::new()
    }
}
