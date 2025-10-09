//! API response models and server information structures

use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

use super::board::{BoardAssignment, BoardType, ConnectedBoard};

/// Board list API response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoardListResponse {
    pub boards: Vec<ConnectedBoard>,
    pub server_info: ServerInfo,
}

/// Server information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerInfo {
    pub version: String,
    pub hostname: String,
    pub last_scan: DateTime<Local>,
    pub total_boards: usize,
}

/// Remote boards response for client operations
#[derive(Debug, Deserialize)]
pub struct RemoteBoardsResponse {
    pub boards: Vec<super::board::RemoteBoard>,
    pub server_info: serde_json::Value,
}

/// Board types response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoardTypesResponse {
    pub board_types: Vec<BoardType>,
}

/// Assignment request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssignBoardRequest {
    pub board_unique_id: String,
    pub board_type_id: String,
    pub logical_name: Option<String>,
    pub chip_type_override: Option<String>,
}

/// Assignment response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssignmentResponse {
    pub success: bool,
    pub message: String,
}

/// Persistent configuration stored in RON format
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PersistentConfig {
    /// Available board types
    pub board_types: Vec<BoardType>,
    /// Board assignments (physical board -> board type)
    pub board_assignments: Vec<BoardAssignment>,
    /// Server configuration overrides
    pub server_overrides: HashMap<String, String>,
    /// Configuration version for compatibility
    pub config_version: u32,
    /// Last updated timestamp
    pub last_updated: DateTime<Local>,
}
