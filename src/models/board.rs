//! Board-related data models

use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Current status of a board
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BoardStatus {
    Available,
    Flashing,
    Monitoring,
    Error(String),
}

impl std::fmt::Display for BoardStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BoardStatus::Available => write!(f, "Available"),
            BoardStatus::Flashing => write!(f, "Flashing"),
            BoardStatus::Monitoring => write!(f, "Monitoring"),
            BoardStatus::Error(msg) => write!(f, "Error: {}", msg),
        }
    }
}

/// Board-specific flash progress information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoardFlashProgress {
    /// Current segment being flashed (1-based)
    pub current_segment: u32,
    /// Total number of segments to flash
    pub total_segments: u32,
    /// Current segment name
    pub current_segment_name: String,
    /// Overall progress percentage (0-100)
    pub overall_progress: f32,
    /// Current segment progress percentage (0-100)
    pub segment_progress: f32,
    /// Bytes written so far
    pub bytes_written: u64,
    /// Total bytes to write
    pub total_bytes: u64,
    /// Current operation ("Connecting", "Erasing", "Writing", "Verifying")
    pub current_operation: String,
    /// Flash start time
    pub started_at: DateTime<Local>,
    /// Estimated completion time (optional)
    pub estimated_completion: Option<DateTime<Local>>,
}

/// Connected ESP32 board information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectedBoard {
    /// Board identifier (assigned by server)
    pub id: String,
    /// Physical serial port
    pub port: String,
    /// Detected chip type (esp32s3, esp32p4, etc.)
    pub chip_type: String,
    /// Crystal frequency
    pub crystal_frequency: String,
    /// Flash size
    pub flash_size: String,
    /// Chip features (WiFi, BLE, etc.)
    pub features: String,
    /// MAC address (partially redacted for privacy)
    pub mac_address: String,
    /// USB device description
    pub device_description: String,
    /// Current status
    pub status: BoardStatus,
    /// Last activity timestamp
    pub last_updated: DateTime<Local>,
    /// User-assigned logical name (optional)
    pub logical_name: Option<String>,
    /// Unique identifier combining multiple chip characteristics
    pub unique_id: String,
    /// Chip revision (e.g., "v0.2")
    pub chip_revision: Option<String>,
    /// Chip ID from security info
    pub chip_id: Option<u32>,
    /// Flash manufacturer ID
    pub flash_manufacturer: Option<String>,
    /// Flash device ID
    pub flash_device_id: Option<String>,
    /// Assigned board type ID (if any)
    pub assigned_board_type_id: Option<String>,
    /// Assigned board type information
    pub assigned_board_type: Option<BoardType>,
    /// Flash progress information (only present when flashing)
    pub flash_progress: Option<BoardFlashProgress>,
}

/// Board type definition from sdkconfig files
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BoardType {
    /// Unique identifier for this board type
    pub id: String,
    /// Human-readable name
    pub name: String,
    /// Description
    pub description: String,
    /// Target chip type this board supports
    pub chip_type: String,
    /// Path to sdkconfig.defaults file (relative or absolute)
    pub sdkconfig_path: Option<PathBuf>,
    /// Additional metadata
    pub metadata: HashMap<String, String>,
}

/// Board assignment - maps physical boards to board types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoardAssignment {
    /// Physical board unique ID (MAC-based or hardware ID)
    pub board_unique_id: String,
    /// Assigned board type ID
    pub board_type_id: String,
    /// User-assigned logical name (optional)
    pub logical_name: Option<String>,
    /// Chip type override (for detection issues)
    pub chip_type_override: Option<String>,
    /// Assignment timestamp
    pub assigned_at: DateTime<Local>,
    /// Additional notes
    pub notes: Option<String>,
}

/// Enhanced board information cache entry
#[derive(Debug, Clone)]
pub struct EnhancedBoardInfo {
    /// Detailed chip information from native espflash
    pub chip_type: String,
    /// Crystal frequency
    pub crystal_frequency: String,
    /// Flash size
    pub flash_size: String,
    /// Chip features
    pub features: String,
    /// MAC address (may be masked for security)
    pub mac_address: String,
    /// Chip revision
    pub chip_revision: Option<String>,
    /// Chip ID
    pub chip_id: Option<u32>,
    /// Unique identifier
    pub unique_id: String,
    /// Cache timestamp
    pub cached_at: DateTime<Local>,
}

/// Board configuration for TUI
#[derive(Debug, Clone)]
pub struct BoardConfig {
    pub name: String,
    pub config_file: PathBuf,
    pub build_dir: PathBuf,
    pub status: crate::models::project::BuildStatus,
    pub log_lines: Vec<String>,
    pub build_time: Option<std::time::Duration>,
    pub last_updated: DateTime<Local>,
    pub target: Option<String>, // ESP32, ESP32-S3, etc.
    pub project_type: crate::models::project::ProjectType,
}

/// Remote board representation
#[derive(Debug, Clone, Deserialize, Serialize)]
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

/// Board reset request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResetRequest {
    /// Board ID to reset
    pub board_id: String,
}

/// Board reset response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResetResponse {
    pub success: bool,
    pub message: String,
}
