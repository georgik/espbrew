//! Flash-related data models

use ratatui::style::Color;
use serde::{Deserialize, Serialize};

/// Flash operation request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlashRequest {
    /// Board ID to flash
    pub board_id: String,
    /// Binary data to flash (base64 encoded) - DEPRECATED: use flash_binaries instead
    pub binary_data: Vec<u8>,
    /// Flash offset (usually 0x0 for merged binaries) - DEPRECATED: use flash_binaries instead
    pub offset: u32,
    /// Optional chip type override
    pub chip_type: Option<String>,
    /// Flash after completion
    pub verify: bool,
    /// Multiple binaries to flash with their offsets (NEW: proper ESP32 multi-partition flashing)
    pub flash_binaries: Option<Vec<FlashBinary>>,
    /// Flash configuration parameters
    pub flash_config: Option<FlashConfig>,
}

/// Individual binary to flash
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlashBinary {
    /// Flash offset in hex (e.g., 0x0 for bootloader, 0x8000 for partition table, 0x10000 for app)
    pub offset: u32,
    /// Binary data
    pub data: Vec<u8>,
    /// Description/name (e.g., "bootloader", "partition_table", "application")
    pub name: String,
    /// File path/name for reference
    pub file_name: String,
}

/// Flash configuration parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlashConfig {
    /// Flash mode (e.g., "dio", "qio")
    pub flash_mode: String,
    /// Flash frequency (e.g., "80m", "40m")
    pub flash_freq: String,
    /// Flash size (e.g., "16MB", "8MB", "4MB")
    pub flash_size: String,
}

/// Flash operation progress tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlashProgress {
    pub phase: String, // "connecting", "flashing", "verifying", "completed", "error"
    pub current_segment: Option<usize>,
    pub total_segments: usize,
    pub segment_name: Option<String>,
    pub segment_bytes_written: usize,
    pub segment_total_bytes: usize,
    pub total_bytes_written: usize,
    pub total_bytes: usize,
    pub overall_percent: u32,
    pub message: String,
}

/// Flash operation response
#[derive(Debug, Serialize, Deserialize)]
pub struct FlashResponse {
    pub message: String,
    pub flash_id: Option<String>,
    pub success: bool,
    pub duration_ms: Option<u64>,
    /// Progress information (if operation is in progress)
    pub progress: Option<FlashProgress>,
}

/// Flash binary information for client operations
#[derive(Debug, Clone)]
pub struct FlashBinaryInfo {
    pub name: String,
    pub file_name: String,
    pub file_path: std::path::PathBuf,
    pub offset: u32,
}

/// Remote flash status for TUI
#[derive(Debug, Clone)]
pub enum RemoteFlashStatus {
    Uploading,
    Queued,
    Flashing,
    Success,
    Failed(String),
}

impl RemoteFlashStatus {
    pub fn color(&self) -> Color {
        match self {
            RemoteFlashStatus::Uploading => Color::Yellow,
            RemoteFlashStatus::Queued => Color::Cyan,
            RemoteFlashStatus::Flashing => Color::Blue,
            RemoteFlashStatus::Success => Color::Green,
            RemoteFlashStatus::Failed(_) => Color::Red,
        }
    }

    pub fn symbol(&self) -> &'static str {
        match self {
            RemoteFlashStatus::Uploading => "📤",
            RemoteFlashStatus::Queued => "⏳",
            RemoteFlashStatus::Flashing => "📡",
            RemoteFlashStatus::Success => "✅",
            RemoteFlashStatus::Failed(_) => "❌",
        }
    }

    pub fn description(&self) -> String {
        match self {
            RemoteFlashStatus::Uploading => "Uploading binary to server...".to_string(),
            RemoteFlashStatus::Queued => "Flash job queued on server".to_string(),
            RemoteFlashStatus::Flashing => "Flashing binary to board...".to_string(),
            RemoteFlashStatus::Success => "Flash operation completed successfully".to_string(),
            RemoteFlashStatus::Failed(e) => format!("Flash operation failed: {}", e),
        }
    }
}
