//! Monitor-related data models

use chrono::{DateTime, Local};
use ratatui::style::Color;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

/// Remote monitoring session request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitorRequest {
    /// Board ID to monitor
    pub board_id: String,
    /// Baud rate for serial monitoring (default: 115200)
    pub baud_rate: Option<u32>,
    /// Optional filter patterns for log lines
    pub filters: Option<Vec<String>>,
    /// Maximum monitoring duration in seconds (0 = infinite)
    pub timeout: Option<u64>,
    /// Success pattern - monitoring exits with success when this regex pattern is found
    pub success_pattern: Option<String>,
    /// Failure pattern - monitoring exits with error when this regex pattern is found
    pub failure_pattern: Option<String>,
    /// Log format type (serial, defmt)
    pub log_format: Option<String>,
    /// Whether to reset the board before monitoring
    pub reset: Option<bool>,
    /// Non-interactive mode flag
    pub non_interactive: Option<bool>,
}

/// Remote monitoring session response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitorResponse {
    pub success: bool,
    pub message: String,
    /// WebSocket URL for receiving logs
    pub websocket_url: Option<String>,
    /// Session ID for this monitoring session
    pub session_id: Option<String>,
}

/// Monitoring session state
#[derive(Debug)]
pub struct MonitoringSession {
    /// Unique session ID
    pub id: String,
    /// Board being monitored
    pub board_id: String,
    /// Serial port path
    pub port: String,
    /// Baud rate
    pub baud_rate: u32,
    /// Session start time
    pub started_at: DateTime<Local>,
    /// Last activity timestamp for keep-alive tracking
    pub last_activity: DateTime<Local>,
    /// WebSocket broadcast sender for this session
    pub sender: broadcast::Sender<String>,
    /// Task handle for the monitoring process
    pub task_handle: Option<tokio::task::JoinHandle<()>>,
}

/// WebSocket log message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogMessage {
    /// Session ID
    pub session_id: String,
    /// Board ID
    pub board_id: String,
    /// Log line content
    pub content: String,
    /// Timestamp when log was received
    pub timestamp: DateTime<Local>,
    /// Log level if detectable (INFO, ERROR, WARNING, etc.)
    pub level: Option<String>,
}

/// Stop monitoring request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StopMonitorRequest {
    /// Session ID to stop
    pub session_id: String,
}

/// Stop monitoring response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StopMonitorResponse {
    pub success: bool,
    pub message: String,
}

/// Keep-alive monitoring request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeepAliveRequest {
    /// Session ID to keep alive
    pub session_id: String,
}

/// Keep-alive monitoring response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeepAliveResponse {
    pub success: bool,
    pub message: String,
}

/// WebSocket message structure
#[derive(Debug, Deserialize)]
pub struct WebSocketMessage {
    #[serde(rename = "type")]
    pub message_type: String,
    pub session_id: Option<String>,
    pub content: Option<String>,
    pub timestamp: Option<String>,
    pub message: Option<String>,
    pub error: Option<String>,
}

/// Remote monitor status for TUI
#[derive(Debug, Clone)]
pub enum RemoteMonitorStatus {
    Connecting,
    Connected,
    Monitoring,
    Disconnected,
    Failed(String),
}

impl RemoteMonitorStatus {
    pub fn color(&self) -> Color {
        match self {
            RemoteMonitorStatus::Connecting => Color::Yellow,
            RemoteMonitorStatus::Connected => Color::Green,
            RemoteMonitorStatus::Monitoring => Color::Blue,
            RemoteMonitorStatus::Disconnected => Color::Gray,
            RemoteMonitorStatus::Failed(_) => Color::Red,
        }
    }

    pub fn symbol(&self) -> &'static str {
        match self {
            RemoteMonitorStatus::Connecting => "🔗",
            RemoteMonitorStatus::Connected => "✅",
            RemoteMonitorStatus::Monitoring => "📺",
            RemoteMonitorStatus::Disconnected => "⚫",
            RemoteMonitorStatus::Failed(_) => "❌",
        }
    }

    pub fn description(&self) -> String {
        match self {
            RemoteMonitorStatus::Connecting => "Connecting to board...".to_string(),
            RemoteMonitorStatus::Connected => "Connected to board".to_string(),
            RemoteMonitorStatus::Monitoring => "Monitoring board logs".to_string(),
            RemoteMonitorStatus::Disconnected => "Disconnected from board".to_string(),
            RemoteMonitorStatus::Failed(e) => format!("Monitor failed: {}", e),
        }
    }
}
