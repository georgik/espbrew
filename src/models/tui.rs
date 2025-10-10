//! TUI-specific data models

use chrono::{DateTime, Local};
use ratatui::style::Color;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;

/// Which pane is currently focused in the TUI
#[derive(Debug, PartialEq)]
pub enum FocusedPane {
    BoardList,
    ComponentList,
    LogPane,
}

/// Build status with visual indicators
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

/// Board configuration for TUI display
#[derive(Debug, Clone)]
pub struct BoardConfig {
    pub name: String,
    pub config_file: PathBuf,
    pub build_dir: PathBuf,
    pub status: BuildStatus,
    pub log_lines: Vec<String>,
    pub build_time: Option<Duration>,
    pub last_updated: DateTime<Local>,
    pub target: Option<String>,
    pub project_type: Option<crate::projects::ProjectType>,
}

/// Component configuration for TUI display
#[derive(Debug, Clone)]
pub struct ComponentConfig {
    pub name: String,
    pub path: PathBuf,
    pub is_managed: bool, // true if in managed_components, false if in components
    pub action_status: Option<String>, // Current action being performed (e.g., "Cloning...")
}

/// Board actions available in TUI
#[derive(Debug, Clone, PartialEq)]
pub enum BoardAction {
    Build,
    Flash,
    FlashAppOnly,
    Monitor,
    Clean,
    Purge,
    GenerateBinary,
    RemoteFlash,
    RemoteMonitor,
}

impl BoardAction {
    pub fn name(&self) -> &'static str {
        match self {
            BoardAction::Build => "Build",
            BoardAction::Flash => "Flash",
            BoardAction::FlashAppOnly => "Flash App Only",
            BoardAction::Monitor => "Monitor",
            BoardAction::Clean => "Clean",
            BoardAction::Purge => "Purge (Delete build dir)",
            BoardAction::GenerateBinary => "Generate Binary",
            BoardAction::RemoteFlash => "Remote Flash",
            BoardAction::RemoteMonitor => "Remote Monitor",
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            BoardAction::Build => "Build the project for this board",
            BoardAction::Flash => "Flash all partitions (bootloader, app, data)",
            BoardAction::FlashAppOnly => "Flash only the application partition (faster)",
            BoardAction::Monitor => "Flash and start serial monitor",
            BoardAction::Clean => "Clean build files (idf.py clean)",
            BoardAction::Purge => "Force delete build directory",
            BoardAction::GenerateBinary => "Create single binary file for distribution",
            BoardAction::RemoteFlash => "Flash to remote board via ESPBrew server",
            BoardAction::RemoteMonitor => "Monitor remote board via ESPBrew server",
        }
    }
}

/// Component actions available in TUI
#[derive(Debug, Clone, PartialEq)]
pub enum ComponentAction {
    MoveToComponents,
    CloneFromRepository,
    Remove,
    OpenInEditor,
}

/// Remote board information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteBoard {
    pub id: String,
    pub logical_name: Option<String>,
    pub mac_address: String,
    pub unique_id: String,
    pub chip_type: String,
    pub port: String,
    pub status: String,
    pub board_type_id: Option<String>,
    pub device_description: String,
    pub last_updated: String,
}

/// Local board information (connected via USB/serial)
#[derive(Debug, Clone)]
pub struct LocalBoard {
    pub port: String,
    pub chip_type: String,
    pub device_description: String,
    pub mac_address: String,
    pub unique_id: String,
}

/// Discovered server information
#[derive(Debug, Clone)]
pub struct DiscoveredServer {
    pub name: String,
    pub ip: std::net::IpAddr,
    pub port: u16,
    pub hostname: String,
    pub version: String,
    pub description: String,
    pub board_count: u32,
    pub boards_list: String,
}

/// Remote action type
#[derive(Debug, Clone, PartialEq)]
pub enum RemoteActionType {
    Flash,
    Monitor,
}
