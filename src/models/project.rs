//! Project-related data models

use ratatui::style::Color;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Build strategy for multiple boards
#[derive(Debug, Clone, PartialEq, clap::ValueEnum)]
pub enum BuildStrategy {
    /// Build boards sequentially (avoids component manager conflicts, recommended)
    Sequential,
    /// Build boards in parallel (faster but may cause component manager conflicts)
    Parallel,
    /// Use professional idf-build-apps tool (recommended for production, zero conflicts)
    IdfBuildApps,
}

/// Build status for TUI display
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BuildStatus {
    Pending,
    Building,
    Success,
    Failed,
    Flashing,
    Flashed,
    Monitoring,
}

impl BuildStatus {
    pub fn color(&self) -> Color {
        match self {
            BuildStatus::Pending => Color::Gray,
            BuildStatus::Building => Color::Yellow,
            BuildStatus::Success => Color::Green,
            BuildStatus::Failed => Color::Red,
            BuildStatus::Flashing => Color::Cyan,
            BuildStatus::Flashed => Color::Blue,
            BuildStatus::Monitoring => Color::Magenta,
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
            BuildStatus::Monitoring => "📺",
        }
    }
}

/// Component configuration for TUI
#[derive(Debug, Clone)]
pub struct ComponentConfig {
    pub name: String,
    pub path: PathBuf,
    pub is_managed: bool, // true if in managed_components, false if in components
    pub action_status: Option<String>, // Current action being performed (e.g., "Cloning...")
}

/// Component actions available in TUI
#[derive(Debug, Clone, PartialEq)]
pub enum ComponentAction {
    MoveToComponents,
    CloneFromRepository,
    Remove,
    OpenInEditor,
    Update,
}

impl ComponentAction {
    pub fn name(&self) -> &'static str {
        match self {
            ComponentAction::MoveToComponents => "Move to Components",
            ComponentAction::CloneFromRepository => "Clone from Repository",
            ComponentAction::Remove => "Remove",
            ComponentAction::OpenInEditor => "Open in Editor",
            ComponentAction::Update => "Update",
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            ComponentAction::MoveToComponents => "Move from managed_components to components",
            ComponentAction::CloneFromRepository => {
                "Clone from Git repository to components (supports wrapper components)"
            }
            ComponentAction::Remove => "Delete the component directory",
            ComponentAction::OpenInEditor => "Open component directory in default editor",
            ComponentAction::Update => "Update component to latest version",
        }
    }

    pub fn is_available_for(&self, component: &ComponentConfig) -> bool {
        match self {
            ComponentAction::MoveToComponents => component.is_managed,
            ComponentAction::CloneFromRepository => {
                component.is_managed && Self::has_manifest_file(component)
            }
            ComponentAction::Remove => true,
            ComponentAction::OpenInEditor => true,
            ComponentAction::Update => Self::has_manifest_file(component),
        }
    }

    fn has_manifest_file(component: &ComponentConfig) -> bool {
        component.path.join("idf_component.yml").exists()
    }
}

/// Component manifest structure
#[derive(Debug, Deserialize)]
pub struct ComponentManifest {
    pub url: Option<String>,
    pub git: Option<String>,
    pub repository: Option<String>,
}

/// Build artifacts information
#[derive(Debug, Clone)]
pub struct BuildArtifact {
    pub name: String,
    pub file_path: PathBuf,
    pub artifact_type: ArtifactType,
    pub offset: Option<u32>, // Flash offset for ESP-IDF binaries
}

/// Types of build artifacts
#[derive(Debug, Clone, PartialEq)]
pub enum ArtifactType {
    Application,
    Bootloader,
    PartitionTable,
    Binary,
    Elf,
    Python,
}

/// Project types supported by ESPBrew
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ProjectType {
    EspIdf,
    RustNoStd,
    Arduino,
    PlatformIO,
    MicroPython,
    CircuitPython,
    Zephyr,
    NuttX,
    TinyGo,
    Jaculus,
}

impl ProjectType {
    pub fn name(&self) -> &'static str {
        match self {
            ProjectType::EspIdf => "ESP-IDF",
            ProjectType::RustNoStd => "Rust no_std",
            ProjectType::Arduino => "Arduino",
            ProjectType::PlatformIO => "PlatformIO",
            ProjectType::MicroPython => "MicroPython",
            ProjectType::CircuitPython => "CircuitPython",
            ProjectType::Zephyr => "Zephyr RTOS",
            ProjectType::NuttX => "NuttX RTOS",
            ProjectType::TinyGo => "TinyGo",
            ProjectType::Jaculus => "Jaculus",
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            ProjectType::EspIdf => "ESP-IDF project with CMake build system",
            ProjectType::RustNoStd => "Embedded Rust project with Cargo",
            ProjectType::Arduino => "Arduino project with Arduino IDE",
            ProjectType::PlatformIO => "PlatformIO universal IoT platform",
            ProjectType::MicroPython => "MicroPython embedded Python",
            ProjectType::CircuitPython => "CircuitPython embedded Python",
            ProjectType::Zephyr => "Zephyr real-time operating system",
            ProjectType::NuttX => "NuttX real-time operating system",
            ProjectType::TinyGo => "TinyGo embedded Go",
            ProjectType::Jaculus => "JavaScript runtime for ESP32 devices",
        }
    }
}

/// Project board configuration (different from TUI BoardConfig)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectBoardConfig {
    pub name: String,
    pub config_file: PathBuf,
    pub build_dir: PathBuf,
    pub target: Option<String>, // ESP32, ESP32-S3, etc.
    pub project_type: ProjectType,
}
