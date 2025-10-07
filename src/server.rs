#!/usr/bin/env rust
//! ESPBrew Server - Remote ESP32 Flashing Server
//!
//! A network-based server that manages connected ESP32 boards and provides
//! remote flashing capabilities. This enables ESPBrew to work with remote
//! test farms, CI/CD environments, and distributed development setups.

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use bytes::Buf;
use bytes::Bytes;
use chrono::{DateTime, Local};
use clap::{Parser, Subcommand};
use futures_util::{SinkExt, StreamExt};
use if_addrs::get_if_addrs;
use include_dir::{Dir, include_dir};
use mdns_sd::{ServiceDaemon, ServiceInfo};
use serde::{Deserialize, Serialize};
use tokio::sync::{RwLock, broadcast};
use uuid::Uuid;
use warp::Filter;
use warp::http::Response;
use warp::reply::Reply;
use warp::ws::{Message, WebSocket};

// Embed the static directory at compile time
static STATIC_DIR: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/static");

/// Resolve the path to static assets, supporting both development and deployment scenarios
fn resolve_static_assets_path() -> Result<PathBuf> {
    // Try different locations in order of preference:
    // 1. Current working directory (development mode)
    // 2. Relative to executable (deployment mode)
    // 3. Relative to cargo project root (development with different working dir)

    let cwd_static = std::env::current_dir()?.join("static");
    if cwd_static.exists() && cwd_static.is_dir() {
        return Ok(cwd_static);
    }

    // Try relative to executable location
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            let exe_static = exe_dir.join("static");
            if exe_static.exists() && exe_static.is_dir() {
                return Ok(exe_static);
            }

            // Try one level up from executable (in case exe is in target/release/)
            let exe_parent_static = exe_dir
                .parent()
                .and_then(|p| p.parent())
                .map(|p| p.join("static"));
            if let Some(path) = exe_parent_static {
                if path.exists() && path.is_dir() {
                    return Ok(path);
                }
            }
        }
    }

    // Try relative to CARGO_MANIFEST_DIR (if available during development)
    if let Ok(manifest_dir) = std::env::var("CARGO_MANIFEST_DIR") {
        let manifest_static = PathBuf::from(manifest_dir).join("static");
        if manifest_static.exists() && manifest_static.is_dir() {
            return Ok(manifest_static);
        }
    }

    // Fallback to current working directory + static (may not exist)
    println!("⚠️  Warning: Could not find static assets directory, falling back to ./static");
    println!("💡 Ensure static/ directory exists with index.html and other web assets");
    Ok(std::env::current_dir()?.join("static"))
}

/// Serve static files from embedded assets
fn serve_embedded_static(path: warp::path::Tail) -> Result<impl Reply, warp::Rejection> {
    let path_str = path.as_str();

    // Handle empty path or root - serve index.html
    let file_path = if path_str.is_empty() || path_str == "/" {
        "index.html"
    } else {
        // Remove leading slash if present
        path_str.strip_prefix('/').unwrap_or(path_str)
    };

    // Try to get the file from embedded assets
    if let Some(file) = STATIC_DIR.get_file(file_path) {
        let contents = file.contents();
        let mime_type = get_mime_type(file_path);

        let response = Response::builder()
            .header("content-type", mime_type)
            .header("cache-control", "public, max-age=3600") // Cache for 1 hour
            .body(Bytes::from(contents))
            .map_err(|_| warp::reject::not_found())?;

        Ok(response)
    } else {
        Err(warp::reject::not_found())
    }
}

/// Get MIME type based on file extension
fn get_mime_type(path: &str) -> &'static str {
    match path.split('.').last().unwrap_or("").to_lowercase().as_str() {
        "html" => "text/html; charset=utf-8",
        "css" => "text/css",
        "js" => "application/javascript",
        "json" => "application/json",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "svg" => "image/svg+xml",
        "ico" => "image/x-icon",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        _ => "application/octet-stream",
    }
}

/// Network protocol data structures
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
}

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlashConfig {
    /// Flash mode (e.g., "dio", "qio")
    pub flash_mode: String,
    /// Flash frequency (e.g., "80m", "40m")
    pub flash_freq: String,
    /// Flash size (e.g., "16MB", "8MB", "4MB")
    pub flash_size: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlashResponse {
    pub success: bool,
    pub message: String,
    pub duration_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoardListResponse {
    pub boards: Vec<ConnectedBoard>,
    pub server_info: ServerInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerInfo {
    pub version: String,
    pub hostname: String,
    pub last_scan: DateTime<Local>,
    pub total_boards: usize,
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

/// Remote monitoring session request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitorRequest {
    /// Board ID to monitor
    pub board_id: String,
    /// Baud rate for serial monitoring (default: 115200)
    pub baud_rate: Option<u32>,
    /// Optional filter patterns for log lines
    pub filters: Option<Vec<String>>,
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

/// Enhanced board information cache entry
#[derive(Debug, Clone)]
struct EnhancedBoardInfo {
    /// Detailed chip information from native espflash
    chip_type: String,
    /// Crystal frequency
    crystal_frequency: String,
    /// Flash size
    flash_size: String,
    /// Chip features
    features: String,
    /// MAC address (may be masked for security)
    mac_address: String,
    /// Chip revision
    chip_revision: Option<String>,
    /// Chip ID
    chip_id: Option<u32>,
    /// Unique identifier
    unique_id: String,
    /// Cache timestamp
    cached_at: DateTime<Local>,
}

/// Server state management
#[derive(Debug)]
pub struct ServerState {
    boards: HashMap<String, ConnectedBoard>,
    config: ServerConfig,
    last_scan: DateTime<Local>,
    /// Cache of enhanced board information by device path
    enhanced_info_cache: Arc<RwLock<HashMap<String, EnhancedBoardInfo>>>,
    /// Currently running background enhancement tasks by device path
    enhancement_tasks: Arc<RwLock<HashMap<String, tokio::task::JoinHandle<()>>>>,
    /// Persistent configuration (board types, assignments, etc.)
    persistent_config: PersistentConfig,
    /// Path to persistent configuration file
    config_path: PathBuf,
    /// Active monitoring sessions by session ID
    monitoring_sessions: Arc<RwLock<HashMap<String, MonitoringSession>>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    /// Server listening address
    pub bind_address: String,
    /// Server listening port
    pub port: u16,
    /// Board discovery interval in seconds
    pub scan_interval: u64,
    /// Board mappings (port -> logical_name)
    pub board_mappings: HashMap<String, String>,
    /// Maximum binary size for uploads (in MB)
    pub max_binary_size_mb: usize,
    /// Enable mDNS service announcement
    pub mdns_enabled: bool,
    /// mDNS service name (defaults to hostname)
    pub mdns_service_name: Option<String>,
    /// Server description for mDNS
    pub mdns_description: Option<String>,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            bind_address: "0.0.0.0".to_string(),
            port: 8080,
            scan_interval: 30,
            board_mappings: HashMap::new(),
            max_binary_size_mb: 50,
            mdns_enabled: true,
            mdns_service_name: None, // Will default to hostname
            mdns_description: Some("ESPBrew Remote Flashing Server".to_string()),
        }
    }
}

/// Get all available network interfaces and their IP addresses
fn get_network_interfaces() -> Vec<(String, std::net::IpAddr)> {
    match get_if_addrs() {
        Ok(interfaces) => {
            interfaces
                .into_iter()
                .filter_map(|iface| {
                    // Skip loopback interfaces for external access info
                    if iface.is_loopback() {
                        return None;
                    }

                    // Return interface name and IP address
                    Some((iface.name, iface.addr.ip()))
                })
                .collect()
        }
        Err(_) => vec![], // Return empty vec if we can't get interfaces
    }
}

/// Setup mDNS service announcement
fn setup_mdns_service(config: &ServerConfig, state: &ServerState) -> Result<Option<ServiceDaemon>> {
    if !config.mdns_enabled {
        println!("📻 mDNS service announcement disabled");
        return Ok(None);
    }

    println!("📻 Setting up mDNS service announcement...");

    let mdns = ServiceDaemon::new()?;

    // Get hostname for service name and mDNS registration
    let raw_hostname = hostname::get()
        .unwrap_or_else(|_| "espbrew-server".into())
        .to_string_lossy()
        .to_string();

    // Ensure hostname ends with .local. for mDNS
    let hostname = if raw_hostname.ends_with(".local.") {
        raw_hostname.clone()
    } else if raw_hostname.ends_with(".local") {
        format!("{}.", raw_hostname)
    } else {
        format!("{}.local.", raw_hostname)
    };

    // Use configured service name or default to raw hostname (without .local.)
    let service_name = config.mdns_service_name.as_ref().unwrap_or(&raw_hostname);

    // Get board information for TXT records
    let boards: Vec<String> = state
        .boards
        .iter()
        .map(|(id, board)| format!("{}:{}", id, board.chip_type))
        .collect();

    let board_count = state.boards.len();
    let description = config
        .mdns_description
        .as_deref()
        .unwrap_or("ESPBrew Remote Flashing Server");

    // Create TXT record properties as HashMap
    let mut txt_properties = HashMap::new();
    txt_properties.insert("version".to_string(), env!("CARGO_PKG_VERSION").to_string());
    txt_properties.insert("hostname".to_string(), raw_hostname.clone());
    txt_properties.insert("description".to_string(), description.to_string());
    txt_properties.insert("board_count".to_string(), board_count.to_string());

    // Add board information if not too large
    let boards_txt = boards.join(",");
    if boards_txt.len() < 200 {
        // Keep TXT record reasonable size
        txt_properties.insert("boards".to_string(), boards_txt);
    }

    // Create service info
    let service_type = "_espbrew._tcp.local.";
    let full_name = format!("{}.{}", service_name, service_type);

    // Try to get a non-loopback IPv4 address, fallback to unspecified for mDNS daemon to handle
    let interfaces = get_network_interfaces();
    let service_ip = interfaces
        .iter()
        .find(|(_, ip)| matches!(ip, std::net::IpAddr::V4(_) if !ip.is_loopback()))
        .map(|(_, ip)| *ip)
        .unwrap_or(std::net::IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED));

    println!("📻 mDNS using IP address: {}", service_ip);
    println!("📻 Available network interfaces:");
    for (name, ip) in &interfaces {
        println!("   • {} -> {}", name, ip);
    }

    // Debug: Print TXT record contents before creating service
    println!("📻 TXT records:");
    for (key, value) in &txt_properties {
        println!("   • {} = {}", key, value);
    }

    let service_info = ServiceInfo::new(
        service_type,
        service_name,
        &hostname,
        service_ip,
        config.port,
        txt_properties,
    )?;

    // Register the service
    mdns.register(service_info.clone())?;

    println!("📻 mDNS service announced as: {}", full_name);
    println!("   • Service type: {}", service_type);
    println!("   • Service name: {}", service_name);
    println!("   • Hostname: {}", hostname);
    println!("   • IP Address: {}", service_ip);
    println!("   • Port: {}", config.port);
    println!("   • Boards available: {}", board_count);

    Ok(Some(mdns))
}

/// Structure to hold enhanced unique identifiers
#[derive(Debug)]
struct EnhancedUniqueInfo {
    chip_id: Option<u32>,
    flash_manufacturer: Option<String>,
    flash_device_id: Option<String>,
    mac_address: Option<String>,
}

// Template structures for web UI
/*
#[derive(Template)]
#[template(path = "dashboard_test.html")]
struct DashboardTemplate {
    page: String,
    boards: Vec<ConnectedBoard>,
    server_info: ServerInfo,
}

#[derive(Template)]
#[template(path = "flash_test.html")]
struct FlashTemplate {
    page: String,
    boards: Vec<ConnectedBoard>,
    selected_board: Option<String>,
}
*/

impl ServerState {
    pub fn new(config: ServerConfig) -> Self {
        // Determine config directory
        let config_dir = Self::get_config_directory();
        let config_path = config_dir.join("espbrew-boards.ron");

        // Load or create persistent configuration
        let persistent_config = Self::load_persistent_config(&config_path).unwrap_or_else(|e| {
            println!(
                "⚠️ Failed to load persistent config from {}: {}",
                config_path.display(),
                e
            );
            println!("📁 Creating new configuration");
            Self::create_default_persistent_config()
        });

        println!(
            "💾 Loaded {} board types and {} assignments",
            persistent_config.board_types.len(),
            persistent_config.board_assignments.len()
        );

        Self {
            boards: HashMap::new(),
            config,
            last_scan: Local::now(),
            enhanced_info_cache: Arc::new(RwLock::new(HashMap::new())),
            enhancement_tasks: Arc::new(RwLock::new(HashMap::new())),
            persistent_config,
            config_path,
            monitoring_sessions: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Get the configuration directory (create if doesn't exist)
    fn get_config_directory() -> PathBuf {
        let config_dir = if let Some(config_dir) = dirs::config_dir() {
            config_dir.join("espbrew")
        } else {
            // Fallback to home directory
            PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| ".".to_string()))
                .join(".config")
                .join("espbrew")
        };

        // Create directory if it doesn't exist
        if let Err(e) = fs::create_dir_all(&config_dir) {
            eprintln!(
                "⚠️ Failed to create config directory {}: {}",
                config_dir.display(),
                e
            );
        }

        config_dir
    }

    /// Load persistent configuration from RON file
    fn load_persistent_config(config_path: &PathBuf) -> Result<PersistentConfig> {
        let content = fs::read_to_string(config_path)?;
        let config: PersistentConfig = ron::from_str(&content)?;
        Ok(config)
    }

    /// Save persistent configuration to RON file
    fn save_persistent_config(&self) -> Result<()> {
        let mut config = self.persistent_config.clone();
        config.last_updated = Local::now();

        let ron_string = ron::ser::to_string_pretty(&config, ron::ser::PrettyConfig::default())?;
        fs::write(&self.config_path, ron_string)?;

        println!("💾 Saved configuration to {}", self.config_path.display());
        Ok(())
    }

    /// Create default persistent configuration with board types from snow directory
    fn create_default_persistent_config() -> PersistentConfig {
        let mut config = PersistentConfig::default();
        config.config_version = 1;
        config.last_updated = Local::now();

        // Discover board types from snow directory
        config.board_types = Self::discover_board_types_from_snow();

        config
    }

    /// Discover board types from snow directory sdkconfig.defaults.* files
    fn discover_board_types_from_snow() -> Vec<BoardType> {
        let mut board_types = Vec::new();

        let snow_path = PathBuf::from("../snow");
        if !snow_path.exists() {
            println!("📂 Snow directory not found at ../snow, creating minimal board types");
            return Self::create_minimal_board_types();
        }

        // Look for sdkconfig.defaults.* files
        if let Ok(entries) = fs::read_dir(&snow_path) {
            for entry in entries.flatten() {
                let file_name = entry.file_name();
                let file_name_str = file_name.to_string_lossy();

                if file_name_str.starts_with("sdkconfig.defaults.") {
                    let board_id = file_name_str.strip_prefix("sdkconfig.defaults.").unwrap();

                    // Try to determine chip type from board ID
                    let chip_type = Self::infer_chip_type_from_board_id(board_id);

                    let board_type = BoardType {
                        id: board_id.to_string(),
                        name: Self::format_board_name(board_id),
                        description: format!("Board configuration for {}", board_id),
                        chip_type,
                        sdkconfig_path: Some(snow_path.join(&file_name)),
                        metadata: HashMap::new(),
                    };

                    board_types.push(board_type);
                }
            }
        }

        println!(
            "🔍 Discovered {} board types from snow directory",
            board_types.len()
        );
        board_types
    }

    /// Create minimal board types if snow directory not available
    fn create_minimal_board_types() -> Vec<BoardType> {
        vec![
            BoardType {
                id: "generic_esp32".to_string(),
                name: "Generic ESP32".to_string(),
                description: "Generic ESP32 development board".to_string(),
                chip_type: "esp32".to_string(),
                sdkconfig_path: None,
                metadata: HashMap::new(),
            },
            BoardType {
                id: "generic_esp32s3".to_string(),
                name: "Generic ESP32-S3".to_string(),
                description: "Generic ESP32-S3 development board".to_string(),
                chip_type: "esp32s3".to_string(),
                sdkconfig_path: None,
                metadata: HashMap::new(),
            },
            BoardType {
                id: "generic_esp32c6".to_string(),
                name: "Generic ESP32-C6".to_string(),
                description: "Generic ESP32-C6 development board".to_string(),
                chip_type: "esp32c6".to_string(),
                sdkconfig_path: None,
                metadata: HashMap::new(),
            },
        ]
    }

    /// Infer chip type from board ID
    fn infer_chip_type_from_board_id(board_id: &str) -> String {
        if board_id.contains("s3") {
            "esp32s3".to_string()
        } else if board_id.contains("c3") {
            "esp32c3".to_string()
        } else if board_id.contains("c6") {
            "esp32c6".to_string()
        } else if board_id.contains("p4") {
            "esp32p4".to_string()
        } else if board_id.contains("h2") {
            "esp32h2".to_string()
        } else {
            "esp32".to_string()
        }
    }

    /// Format board ID into human-readable name
    fn format_board_name(board_id: &str) -> String {
        board_id
            .replace('_', " ")
            .split(' ')
            .map(|word| {
                let mut chars = word.chars();
                match chars.next() {
                    None => String::new(),
                    Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                }
            })
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// Get board assignment for a unique ID
    fn get_board_assignment(&self, unique_id: &str) -> Option<&BoardAssignment> {
        self.persistent_config
            .board_assignments
            .iter()
            .find(|assignment| assignment.board_unique_id == unique_id)
    }

    /// Get board type by ID
    fn get_board_type(&self, board_type_id: &str) -> Option<&BoardType> {
        self.persistent_config
            .board_types
            .iter()
            .find(|board_type| board_type.id == board_type_id)
    }

    /// Assign a board to a board type
    pub async fn assign_board_type(
        &mut self,
        unique_id: String,
        board_type_id: String,
        logical_name: Option<String>,
        chip_type_override: Option<String>,
    ) -> Result<()> {
        // Validate that the board type exists
        if !self
            .persistent_config
            .board_types
            .iter()
            .any(|bt| bt.id == board_type_id)
        {
            return Err(anyhow::anyhow!("Board type not found: {}", board_type_id));
        }

        // Remove existing assignment if any
        self.persistent_config
            .board_assignments
            .retain(|a| a.board_unique_id != unique_id);

        // Create new assignment
        let assignment = BoardAssignment {
            board_unique_id: unique_id.clone(),
            board_type_id,
            logical_name,
            chip_type_override,
            assigned_at: Local::now(),
            notes: None,
        };

        self.persistent_config.board_assignments.push(assignment);

        // Save configuration
        self.save_persistent_config()?;

        println!("📌 Assigned board type to board");
        Ok(())
    }

    /// Remove board assignment
    pub async fn unassign_board(&mut self, unique_id: String) -> Result<()> {
        let initial_len = self.persistent_config.board_assignments.len();
        self.persistent_config
            .board_assignments
            .retain(|a| a.board_unique_id != unique_id);

        if self.persistent_config.board_assignments.len() < initial_len {
            self.save_persistent_config()?;
            println!("📌 Removed board assignment for {}", unique_id);
            Ok(())
        } else {
            Err(anyhow::anyhow!(
                "Board assignment not found for unique ID: {}",
                unique_id
            ))
        }
    }

    /// Get all available board types
    pub fn get_available_board_types(&self) -> &[BoardType] {
        &self.persistent_config.board_types
    }

    /// Apply board assignment information to a connected board
    fn apply_board_assignment(&self, board: &mut ConnectedBoard) {
        if let Some(assignment) = self.get_board_assignment(&board.unique_id) {
            board.assigned_board_type_id = Some(assignment.board_type_id.clone());
            board.assigned_board_type = self.get_board_type(&assignment.board_type_id).cloned();

            // Override logical name if assigned
            if assignment.logical_name.is_some() {
                board.logical_name = assignment.logical_name.clone();
            }

            // Override chip type if specified (for detection issues)
            if let Some(ref chip_type_override) = assignment.chip_type_override {
                println!(
                    "🔧 Applying chip type override: {} -> {}",
                    board.chip_type, chip_type_override
                );
                board.chip_type = chip_type_override.clone();
            }
        }
    }

    /// Start background enhancement for a device if not already running
    async fn start_background_enhancement(&self, port: String) {
        // Check if we already have cached info that's recent (less than 1 hour old)
        {
            let cache = self.enhanced_info_cache.read().await;
            if let Some(cached_info) = cache.get(&port) {
                let age = Local::now() - cached_info.cached_at;
                if age.num_hours() < 1 {
                    println!(
                        "📋 Using cached enhanced info for {} (age: {}m)",
                        port,
                        age.num_minutes()
                    );
                    return;
                }
            }
        }

        // Check if enhancement task is already running for this port
        {
            let tasks = self.enhancement_tasks.read().await;
            if tasks.contains_key(&port) {
                println!("⚡ Enhancement already running for {}", port);
                return;
            }
        }

        let cache_clone = self.enhanced_info_cache.clone();
        let tasks_clone = self.enhancement_tasks.clone();
        let port_clone = port.clone();

        println!("🚀 Starting background enhancement for {}", port);

        // Spawn background task
        let task = tokio::spawn(async move {
            let port = port_clone;

            // Run native espflash identification
            match Self::identify_with_espflash_native(&port).await {
                Ok(Some(board_info)) => {
                    println!(
                        "✅ Background enhancement completed for {}: {}",
                        port, board_info.chip_type
                    );

                    // Cache the enhanced information
                    let enhanced_info = EnhancedBoardInfo {
                        chip_type: board_info.chip_type,
                        crystal_frequency: board_info.crystal_frequency,
                        flash_size: board_info.flash_size,
                        features: board_info.features,
                        mac_address: board_info.mac_address,
                        chip_revision: board_info.chip_revision,
                        chip_id: board_info.chip_id,
                        unique_id: board_info.unique_id,
                        cached_at: Local::now(),
                    };

                    // Store in cache
                    {
                        let mut cache = cache_clone.write().await;
                        cache.insert(port.clone(), enhanced_info);
                        println!("📋 Cached enhanced info for {}", port);
                    }
                }
                Ok(None) => {
                    println!("⚠️ Background enhancement found no ESP board on {}", port);
                }
                Err(e) => {
                    println!("⚠️ Background enhancement failed for {}: {}", port, e);
                }
            }

            // Remove task from tracking
            {
                let mut tasks = tasks_clone.write().await;
                tasks.remove(&port);
            }
        });

        // Track the running task
        {
            let mut tasks = self.enhancement_tasks.write().await;
            tasks.insert(port, task);
        }
    }

    /// Get enhanced board information from cache if available
    async fn get_cached_enhanced_info(&self, port: &str) -> Option<EnhancedBoardInfo> {
        let cache = self.enhanced_info_cache.read().await;
        cache.get(port).cloned()
    }

    /// Apply cached enhanced information to a board
    async fn apply_enhanced_info(&self, board: &mut ConnectedBoard) {
        if let Some(enhanced_info) = self.get_cached_enhanced_info(&board.port).await {
            // Update board with enhanced information
            board.chip_type = enhanced_info.chip_type;
            board.crystal_frequency = enhanced_info.crystal_frequency;
            board.flash_size = enhanced_info.flash_size;
            board.features = enhanced_info.features;

            // Only update MAC if it's not masked (contains real data)
            if !enhanced_info.mac_address.contains("*") && enhanced_info.mac_address != "Unknown" {
                board.mac_address = enhanced_info.mac_address;
            }

            board.chip_revision = enhanced_info.chip_revision;
            board.chip_id = enhanced_info.chip_id;
            board.unique_id = enhanced_info.unique_id.clone();
            board.device_description = format!("{} (enhanced)", board.device_description);

            println!(
                "✨ Applied enhanced info to board {}: {}",
                board.port, board.chip_type
            );
        }

        // Always apply board assignment information
        self.apply_board_assignment(board);
    }

    /// Discover and update connected boards
    pub async fn scan_boards(&mut self) -> Result<()> {
        self.scan_boards_with_cancellation(None).await
    }

    /// Discover and update connected boards with optional cancellation support
    pub async fn scan_boards_with_cancellation(
        &mut self,
        cancel_signal: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
    ) -> Result<()> {
        // Check if we should cancel early
        if let Some(ref cancel) = cancel_signal {
            if cancel.load(std::sync::atomic::Ordering::Relaxed) {
                println!("🛑 Board scan cancelled before starting");
                return Ok(());
            }
        }

        println!("🔍 Scanning for USB serial ports...");

        // Use serialport to discover serial ports
        let ports = serialport::available_ports()?;
        let mut discovered_boards = HashMap::new();

        // Filter for relevant USB ports on macOS and Linux
        let relevant_ports: Vec<_> = ports
            .into_iter()
            .filter(|port_info| {
                let port_name = &port_info.port_name;
                // On macOS, focus on USB modem and USB serial ports
                port_name.contains("/dev/cu.usbmodem")
                    || port_name.contains("/dev/cu.usbserial")
                    || port_name.contains("/dev/tty.usbmodem")
                    || port_name.contains("/dev/tty.usbserial")
                    // On Linux, ESP32 devices typically appear as ttyUSB* or ttyACM*
                    || port_name.contains("/dev/ttyUSB")
                    || port_name.contains("/dev/ttyACM")
            })
            .collect();

        println!("📡 Found {} USB serial ports", relevant_ports.len());
        for port_info in &relevant_ports {
            println!("  🔌 {}", port_info.port_name);
        }

        if relevant_ports.is_empty() {
            println!("⚠️  No USB serial ports found. Connect your development boards via USB.");
        }

        for (index, port_info) in relevant_ports.iter().enumerate() {
            // Check for cancellation before processing each port
            if let Some(ref cancel) = cancel_signal {
                if cancel.load(std::sync::atomic::Ordering::Relaxed) {
                    println!("🛑 Board scan cancelled during port enumeration");
                    return Ok(());
                }
            }

            println!(
                "🔍 [{}/{}] Adding port: {}",
                index + 1,
                relevant_ports.len(),
                port_info.port_name
            );

            // Try enhanced board identification, fall back to USB info if needed
            println!(
                "🔍 Attempting enhanced identification for {}",
                port_info.port_name
            );
            let board = match self
                .identify_board_with_cancellation(&port_info.port_name, cancel_signal.clone())
                .await
            {
                Ok(Some(enhanced_board)) => {
                    println!(
                        "✅ Enhanced identification successful: {} ({})",
                        enhanced_board.chip_type, enhanced_board.unique_id
                    );
                    enhanced_board
                }
                Ok(None) => {
                    println!("⚠️ Enhanced identification failed, using USB fallback");
                    self.create_usb_board_info(&port_info)
                }
                Err(e) => {
                    println!(
                        "⚠️ Enhanced identification error: {}, using USB fallback",
                        e
                    );
                    self.create_usb_board_info(&port_info)
                }
            };

            println!(
                "✅ Added board on {}: {} ({})",
                port_info.port_name, board.device_description, board.unique_id
            );
            let board_id = format!("board_{}", board.port.replace("/", "_").replace(".", "_"));

            // Apply logical name mapping if configured
            let logical_name = self.config.board_mappings.get(&board.port).cloned();

            let mut connected_board = ConnectedBoard {
                id: board_id.clone(),
                port: board.port.clone(),
                chip_type: board.chip_type.clone(),
                crystal_frequency: board.crystal_frequency.clone(),
                flash_size: board.flash_size.clone(),
                features: board.features.clone(),
                mac_address: board.mac_address.clone(),
                device_description: board.device_description.clone(),
                status: BoardStatus::Available,
                last_updated: Local::now(),
                logical_name,
                unique_id: board.unique_id.clone(),
                chip_revision: board.chip_revision.clone(),
                chip_id: board.chip_id,
                flash_manufacturer: board.flash_manufacturer.clone(),
                flash_device_id: board.flash_device_id.clone(),
                assigned_board_type_id: None,
                assigned_board_type: None,
            };

            // Apply any cached enhanced information immediately
            self.apply_enhanced_info(&mut connected_board).await;

            // Start background enhancement if needed
            self.start_background_enhancement(board.port.clone()).await;

            discovered_boards.insert(board_id, connected_board);
        }

        self.boards = discovered_boards;
        self.last_scan = Local::now();

        println!("✅ Scan complete. Found {} USB devices", self.boards.len());

        for board in self.boards.values() {
            let logical_name = board.logical_name.as_deref().unwrap_or("(unmapped)");
            println!(
                "  📱 {} [{}] - {} @ {} ({})",
                board.id, logical_name, board.chip_type, board.port, board.device_description
            );
        }

        Ok(())
    }

    /// Create a lightweight board info using only USB port information
    fn create_usb_board_info(&self, port_info: &serialport::SerialPortInfo) -> BoardInfo {
        use serialport::SerialPortType;

        // Determine likely chip type based on port name patterns
        let (chip_type, features) = if port_info.port_name.contains("usbmodem") {
            ("ESP32-S3/C3/C6/H2", "USB-OTG, WiFi, Bluetooth")
        } else if port_info.port_name.contains("usbserial") {
            ("ESP32/ESP8266", "WiFi, Bluetooth")
        } else if port_info.port_name.contains("/dev/ttyUSB") {
            // Linux: Most ESP32 boards with CP210x/FTDI appear as ttyUSB*
            ("ESP32/ESP8266", "WiFi, Bluetooth")
        } else if port_info.port_name.contains("/dev/ttyACM") {
            // Linux: ESP32-S3/C3/C6/H2 with native USB often appear as ttyACM*
            ("ESP32-S3/C3/C6/H2", "USB-OTG, WiFi, Bluetooth")
        } else {
            ("Unknown MCU", "Unknown")
        };

        // Get USB device description
        let device_description = match &port_info.port_type {
            SerialPortType::UsbPort(usb) => {
                format!(
                    "{} - {}",
                    usb.manufacturer
                        .as_deref()
                        .unwrap_or("Unknown Manufacturer"),
                    usb.product.as_deref().unwrap_or("USB Serial Device")
                )
            }
            SerialPortType::PciPort => "PCI Serial Port".to_string(),
            SerialPortType::BluetoothPort => "Bluetooth Serial Port".to_string(),
            SerialPortType::Unknown => "Unknown Serial Port".to_string(),
        };

        // Create a unique ID based on port name
        let unique_id = format!(
            "usb_port_{}",
            port_info.port_name.replace('/', "_").replace('.', "_")
        );

        BoardInfo {
            port: port_info.port_name.clone(),
            chip_type: chip_type.to_string(),
            crystal_frequency: "Unknown".to_string(),
            flash_size: "Unknown".to_string(),
            features: features.to_string(),
            mac_address: "Unknown".to_string(),
            device_description,
            chip_revision: None,
            chip_id: None,
            flash_manufacturer: None,
            flash_device_id: None,
            unique_id,
        }
    }

    /// Identify a board on the given port using probe-rs for accurate MCU detection
    async fn identify_board(&self, port: &str) -> Result<Option<BoardInfo>> {
        self.identify_board_with_cancellation(port, None).await
    }

    /// Identify a board on the given port with cancellation support
    async fn identify_board_with_cancellation(
        &self,
        port: &str,
        cancel_signal: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
    ) -> Result<Option<BoardInfo>> {
        use std::time::Duration;

        // Check for cancellation before starting identification
        if let Some(ref cancel) = cancel_signal {
            if cancel.load(std::sync::atomic::Ordering::Relaxed) {
                println!("🛑 Board identification cancelled for {}", port);
                return Ok(None);
            }
        }

        // Use 5-second timeout as requested (note: individual operations have their own timeouts)
        let _overall_timeout_dur = Duration::from_secs(5);
        let port_str = port.to_string();

        // Skip the strict port accessibility check for ESP32-P4 compatibility
        // ESP32-P4 boards may appear "busy" but still be accessible via espflash
        // This check was too restrictive for ESP32-P4
        // if !Self::is_port_accessible(&port_str).await {
        //     println!("⚠️ Port {} not accessible, skipping", port_str);
        //     return Ok(None);
        // }

        // Check if the port name contains indicators of board type
        let possible_board_type = if port_str.contains("usbmodem") {
            // Modern ESP32 dev kits often use usbmodem interface
            // This includes ESP32-S3, ESP32-C3, ESP32-C6, ESP32-P4
            Some("esp32-usb")
        } else if port_str.contains("usbserial") {
            // Traditional ESP32 boards with CP210x/FTDI often use usbserial
            Some("esp32-serial")
        } else if port_str.contains("/dev/ttyACM") {
            // Linux: ESP32-S3/C3/C6/H2 with native USB often appear as ttyACM*
            Some("esp32-usb")
        } else if port_str.contains("/dev/ttyUSB") {
            // Linux: Most ESP32 boards with CP210x/FTDI appear as ttyUSB*
            Some("esp32-serial")
        } else {
            None
        };

        if let Some(board_type) = possible_board_type {
            println!("🔍 Detected possible {} board on {}", board_type, port_str);
        }

        // Try USB-based detection first for quick identification
        println!(
            "🔍 Attempting to identify board on {} with USB detection",
            port_str
        );
        let result = tokio::time::timeout(
            Duration::from_millis(500),
            Self::identify_with_probe_rs(&port_str),
        )
        .await;

        match result {
            Ok(Ok(Some(board_info))) => {
                println!(
                    "✅ Successfully identified board with USB detection: {}",
                    board_info.chip_type
                );
                Ok(Some(board_info))
            }
            _ => {
                // Check for cancellation before trying espflash
                if let Some(ref cancel) = cancel_signal {
                    if cancel.load(std::sync::atomic::Ordering::Relaxed) {
                        println!(
                            "🛑 Board identification cancelled during USB detection for {}",
                            port
                        );
                        return Ok(None);
                    }
                }

                println!("ℹ️ USB detection inconclusive, trying native espflash first");

                // Try native espflash API first - extended timeout for ESP32-P4
                let native_result = tokio::time::timeout(
                    Duration::from_millis(4000), // Extended for ESP32-P4 compatibility
                    Self::identify_with_espflash_native(&port_str),
                )
                .await;

                match native_result {
                    Ok(Ok(Some(board_info))) => {
                        println!(
                            "✅ Successfully identified board with native espflash: {}",
                            board_info.chip_type
                        );
                        return Ok(Some(board_info));
                    }
                    Ok(Ok(None)) => {
                        println!(
                            "ℹ️ Native espflash found no ESP32, falling back to subprocess espflash"
                        );
                    }
                    Ok(Err(e)) => {
                        println!(
                            "⚠️ Native espflash error on {}: {}, falling back to subprocess",
                            port_str, e
                        );
                    }
                    Err(_) => {
                        println!(
                            "⏰ Native espflash timeout on {}, falling back to subprocess",
                            port_str
                        );
                    }
                }

                // Fallback to subprocess espflash with extended timeout for ESP32-P4
                let espflash_result = tokio::time::timeout(
                    Duration::from_millis(5000), // Further extended for ESP32-P4
                    Self::identify_with_espflash_subprocess(&port_str),
                )
                .await;

                match espflash_result {
                    Ok(Ok(Some(board_info))) => {
                        println!(
                            "✅ Successfully identified board with espflash: {}",
                            board_info.chip_type
                        );
                        Ok(Some(board_info))
                    }
                    Ok(Ok(None)) => {
                        println!("ℹ️ espflash found no ESP32, trying IDF tools as last resort");
                        // Try IDF tools as final fallback
                        let idf_result = tokio::time::timeout(
                            Duration::from_millis(1500),
                            Self::identify_with_idf_tools(&port_str),
                        )
                        .await;

                        match idf_result {
                            Ok(inner_result) => inner_result,
                            Err(_) => {
                                println!("⏰ IDF tools timeout on {}", port);
                                Ok(None)
                            }
                        }
                    }
                    Ok(Err(e)) => {
                        println!("⚠️ espflash error on {}: {}", port_str, e);
                        Ok(None)
                    }
                    Err(_) => {
                        println!("⏰ espflash timeout on {}, trying IDF tools fallback", port);
                        // Quick IDF tools fallback
                        let idf_result = tokio::time::timeout(
                            Duration::from_millis(1000),
                            Self::identify_with_idf_tools(&port_str),
                        )
                        .await;

                        match idf_result {
                            Ok(inner_result) => inner_result,
                            Err(_) => {
                                println!(
                                    "⏰ Complete timeout identifying board on {} (undetectable after 5s)",
                                    port
                                );
                                Ok(None)
                            }
                        }
                    }
                }
            }
        }
    }

    /// Quick port accessibility check without blocking terminal
    async fn is_port_accessible(port: &str) -> bool {
        use std::time::Duration;

        // Try to open port briefly with a very short timeout
        let result = tokio::time::timeout(
            Duration::from_millis(200),
            tokio::task::spawn_blocking({
                let port_str = port.to_string();
                move || {
                    match serialport::new(&port_str, 115200)
                        .timeout(Duration::from_millis(50))
                        .open()
                    {
                        Ok(port) => {
                            // Quick test: try to close it properly
                            drop(port);
                            true
                        }
                        Err(e) => {
                            // Log the specific error for debugging
                            println!("🔍 Port {} accessibility check failed: {}", port_str, e);
                            false
                        }
                    }
                }
            }),
        )
        .await;

        match result {
            Ok(Ok(accessible)) => accessible,
            Ok(Err(_)) => false, // Task panicked
            Err(_) => {
                println!("⏰ Port {} accessibility check timed out", port);
                false // Timeout
            }
        }
    }

    /// Identification using USB characteristics and enhanced espflash
    async fn identify_with_probe_rs(port: &str) -> Result<Option<BoardInfo>> {
        println!("🔍 Using USB-based detection for board on {}", port);

        // First, try to get USB device information based on the port
        let usb_info = Self::get_usb_device_info(port).await;

        // Use USB VID/PID to make educated guesses about ESP32 type
        if let Some((vid, pid, manufacturer, product)) = usb_info {
            println!(
                "🔍 USB Device: VID:0x{:04x}, PID:0x{:04x}, Mfg:{}, Product:{}",
                vid, pid, manufacturer, product
            );

            // Match known ESP32 development board USB identifiers
            let chip_guess = match (vid, pid) {
                // Espressif USB-JTAG/serial debug unit (ESP32-S3, ESP32-C3, ESP32-C6, etc.)
                (0x303A, _) => {
                    if product.to_lowercase().contains("esp32-s3") {
                        Some(("esp32s3", "WiFi, Bluetooth, USB-OTG", "40 MHz"))
                    } else if product.to_lowercase().contains("esp32-c6") {
                        Some(("esp32c6", "WiFi 6, Bluetooth 5, ZigBee 3.0", "40 MHz"))
                    } else if product.to_lowercase().contains("esp32-c3") {
                        Some(("esp32c3", "WiFi, Bluetooth 5", "40 MHz"))
                    } else if product.to_lowercase().contains("esp32-p4") {
                        Some(("esp32p4", "WiFi 6, High Performance AI", "40 MHz"))
                    } else if product.to_lowercase().contains("esp32-h2") {
                        Some(("esp32h2", "Bluetooth 5, ZigBee 3.0, Thread", "32 MHz"))
                    } else {
                        // Default for Espressif VID
                        Some(("esp32s3", "WiFi, Bluetooth, USB-OTG", "40 MHz"))
                    }
                }
                // Silicon Labs CP210x (common on ESP32 dev boards)
                (0x10C4, 0xEA60) => Some(("esp32", "WiFi, Bluetooth Classic", "40 MHz")),
                // FTDI (also used on some ESP32 boards)
                (0x0403, _) => Some(("esp32", "WiFi, Bluetooth Classic", "40 MHz")),
                // WCH CH340 (cheap USB-serial, often used on ESP32 clones)
                (0x1A86, _) => Some(("esp32", "WiFi, Bluetooth Classic", "40 MHz")),
                _ => None,
            };

            if let Some((chip_type, features, crystal_freq)) = chip_guess {
                println!("✅ USB-based identification suggests: {}", chip_type);

                // For USB-based detection, we'll need to get unique IDs later via enhanced detection
                let placeholder_unique_id = format!(
                    "USB-{:04x}:{:04x}-{}",
                    vid,
                    pid,
                    port.replace("/", "-").replace(".", "_")
                );

                return Ok(Some(BoardInfo {
                    port: port.to_string(),
                    chip_type: chip_type.to_string(),
                    crystal_frequency: crystal_freq.to_string(),
                    flash_size: "Unknown".to_string(),
                    features: features.to_string(),
                    mac_address: "**:**:**:**:**:**".to_string(),
                    device_description: format!("{} - {}", manufacturer, product),
                    chip_revision: None,
                    chip_id: None,
                    flash_manufacturer: None,
                    flash_device_id: None,
                    unique_id: placeholder_unique_id,
                }));
            }
        }

        println!("ℹ️ USB-based detection inconclusive, will fall back to espflash");
        Ok(None)
    }

    /// Get USB device information for a given serial port
    async fn get_usb_device_info(port: &str) -> Option<(u16, u16, String, String)> {
        use serialport::SerialPortType;

        // Get all available ports and find the one matching our port
        match serialport::available_ports() {
            Ok(ports) => {
                for port_info in ports {
                    if port_info.port_name == port {
                        if let SerialPortType::UsbPort(usb) = &port_info.port_type {
                            return Some((
                                usb.vid,
                                usb.pid,
                                usb.manufacturer
                                    .clone()
                                    .unwrap_or_else(|| "Unknown".to_string()),
                                usb.product.clone().unwrap_or_else(|| "Unknown".to_string()),
                            ));
                        }
                    }
                }
            }
            Err(_) => return None,
        }
        None
    }

    /// Identification using ESP-IDF tools as final fallback
    async fn identify_with_idf_tools(port: &str) -> Result<Option<BoardInfo>> {
        use std::process::Stdio;
        use tokio::process::Command;

        println!("🔍 Using ESP-IDF tools to identify board on {}", port);

        // Try using idf.py to detect the chip
        let result = async {
            let cmd = Command::new("python")
                .args([
                    "-c",
                    &format!(
                        "import esptool; esptool.main(['--port', '{}', 'chip_id'])",
                        port
                    ),
                ])
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .kill_on_drop(true)
                .spawn()?;

            let output = cmd.wait_with_output().await?;

            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);

                println!("ℹ️ esptool stdout: {}", stdout.trim());
                if !stderr.trim().is_empty() {
                    println!("ℹ️ esptool stderr: {}", stderr.trim());
                }

                // Parse esptool output for chip identification
                let combined_output = format!("{} {}", stdout, stderr);

                let (chip_type, features, crystal_freq) = if combined_output.contains("ESP32-S3") {
                    ("esp32s3", "WiFi, Bluetooth, USB-OTG", "40 MHz")
                } else if combined_output.contains("ESP32-P4") {
                    ("esp32p4", "WiFi 6, High Performance AI", "40 MHz")
                } else if combined_output.contains("ESP32-C6") {
                    ("esp32c6", "WiFi 6, Bluetooth 5, ZigBee 3.0", "40 MHz")
                } else if combined_output.contains("ESP32-C3") {
                    ("esp32c3", "WiFi, Bluetooth 5", "40 MHz")
                } else if combined_output.contains("ESP32-H2") {
                    ("esp32h2", "Bluetooth 5, ZigBee 3.0, Thread", "32 MHz")
                } else if combined_output.contains("ESP32") {
                    ("esp32", "WiFi, Bluetooth Classic", "40 MHz")
                } else {
                    println!("ℹ️ Could not determine chip type from esptool output");
                    return Ok(None);
                };

                println!("✅ IDF tools identified: {}", chip_type);

                // Generate a basic unique ID for IDF tools detection
                let unique_id = format!(
                    "{}-idf-{}",
                    chip_type,
                    port.replace("/", "-").replace(".", "_")
                );

                Ok(Some(BoardInfo {
                    port: port.to_string(),
                    chip_type: chip_type.to_string(),
                    crystal_frequency: crystal_freq.to_string(),
                    flash_size: "Unknown".to_string(),
                    features: features.to_string(),
                    mac_address: "**:**:**:**:**:**".to_string(),
                    device_description: "ESP Development Board (IDF detected)".to_string(),
                    chip_revision: None,
                    chip_id: None,
                    flash_manufacturer: None,
                    flash_device_id: None,
                    unique_id,
                }))
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                println!("ℹ️ esptool failed for {}: {}", port, stderr.trim());
                Ok(None)
            }
        }
        .await;

        result
    }

    /// Extract MAC address using multiple methods
    async fn get_mac_address(port: &str) -> Option<String> {
        use std::process::Stdio;
        use tokio::process::Command;

        println!("📟 Attempting to read MAC address for {}", port);

        // Method 1: Try espflash board-info (most comprehensive)
        if let Ok(result) = tokio::time::timeout(std::time::Duration::from_millis(2500), async {
            Command::new("espflash")
                .args(["board-info", "--port", port])
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .kill_on_drop(true)
                .output()
                .await
        })
        .await
        {
            if let Ok(output) = result {
                if output.status.success() {
                    let stdout = String::from_utf8_lossy(&output.stdout);

                    for line in stdout.lines() {
                        let line = line.trim();
                        if line.contains("MAC address:") || line.contains("MAC:") {
                            // Parse lines like "MAC address: AA:BB:CC:DD:EE:FF"
                            let remaining = line[line.find(':').unwrap() + 1..].trim();
                            // Look for MAC pattern AA:BB:CC:DD:EE:FF
                            if let Ok(mac_regex) = regex::Regex::new(
                                r"([0-9A-Fa-f]{2}:[0-9A-Fa-f]{2}:[0-9A-Fa-f]{2}:[0-9A-Fa-f]{2}:[0-9A-Fa-f]{2}:[0-9A-Fa-f]{2})",
                            ) {
                                if let Some(captures) = mac_regex.find(remaining) {
                                    let mac = captures.as_str().to_uppercase();
                                    println!("✅ Found MAC via espflash board-info: {}", mac);
                                    return Some(mac);
                                }
                            }
                        }
                    }
                }
            }
        }

        // Method 2: Try esptool read_mac command
        if let Ok(result) = tokio::time::timeout(std::time::Duration::from_millis(2000), async {
            Command::new("python")
                .args(["-m", "esptool", "--port", port, "read_mac"])
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .kill_on_drop(true)
                .output()
                .await
        })
        .await
        {
            if let Ok(output) = result {
                if output.status.success() {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    let combined = format!("{} {}", stdout, stderr);

                    // Look for MAC patterns in the output
                    if let Ok(mac_regex) = regex::Regex::new(
                        r"([0-9A-Fa-f]{2}:[0-9A-Fa-f]{2}:[0-9A-Fa-f]{2}:[0-9A-Fa-f]{2}:[0-9A-Fa-f]{2}:[0-9A-Fa-f]{2})",
                    ) {
                        if let Some(captures) = mac_regex.find(&combined) {
                            let mac = captures.as_str().to_uppercase();
                            println!("✅ Found MAC via esptool read_mac: {}", mac);
                            return Some(mac);
                        }

                        // Also look for patterns like "MAC: AA:BB:CC:DD:EE:FF"
                        for line in combined.lines() {
                            if (line.contains("MAC:") || line.contains("mac:"))
                                && line.contains(":")
                            {
                                if let Some(captures) = mac_regex.find(line) {
                                    let mac = captures.as_str().to_uppercase();
                                    println!("✅ Found MAC via esptool (line parse): {}", mac);
                                    return Some(mac);
                                }
                            }
                        }
                    }
                }
            }
        }

        // Method 3: Try espflash read-flash with MAC extraction (fallback)
        if let Ok(_result) = tokio::time::timeout(std::time::Duration::from_millis(1500), async {
            Command::new("espflash")
                .args([
                    "read-flash",
                    "--port",
                    port,
                    "0x1000",
                    "0x100",
                    "/tmp/mac_read.bin",
                ])
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .kill_on_drop(true)
                .output()
                .await
        })
        .await
        {
            // This method is more complex and might not always work, so we'll skip it for now
            // Could be implemented later if needed
        }

        println!("⚠️ No MAC address found for {}", port);
        None
    }

    /// Get enhanced unique identifiers using multiple detection methods
    async fn get_enhanced_unique_identifiers(port: &str) -> Option<EnhancedUniqueInfo> {
        use std::process::Stdio;
        use tokio::process::Command;

        let mut enhanced_info = EnhancedUniqueInfo {
            chip_id: None,
            flash_manufacturer: None,
            flash_device_id: None,
            mac_address: None,
        };

        println!("🔍 Gathering enhanced unique identifiers for {}", port);

        // Method 1: Try espflash for comprehensive board info (includes MAC)
        if let Ok(result) = tokio::time::timeout(std::time::Duration::from_millis(2500), async {
            Command::new("espflash")
                .args(["board-info", "--port", port])
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .kill_on_drop(true)
                .output()
                .await
        })
        .await
        {
            if let Ok(output) = result {
                if output.status.success() {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    println!("📋 espflash board-info output: {}", stdout.trim());

                    // Parse output for chip ID and other identifiers
                    for line in stdout.lines() {
                        let line = line.trim();
                        // Look for chip ID patterns in various formats
                        if line.contains("Chip ID:") || line.contains("chip id:") {
                            if let Some(id_part) = line.split(':').nth(1) {
                                // Parse hex values like "0x12345678" or just "12345678"
                                let id_str = id_part.trim().trim_start_matches("0x");
                                if let Ok(chip_id) = u32::from_str_radix(id_str, 16) {
                                    enhanced_info.chip_id = Some(chip_id);
                                    println!("✅ Found chip ID: 0x{:08X}", chip_id);
                                } else if let Ok(chip_id) = id_str.parse::<u32>() {
                                    enhanced_info.chip_id = Some(chip_id);
                                    println!("✅ Found chip ID: {}", chip_id);
                                }
                            }
                        }
                    }
                }
            }
        }

        // Method 2: Try esptool for chip_id command (more reliable for chip ID)
        if enhanced_info.chip_id.is_none() {
            if let Ok(result) =
                tokio::time::timeout(std::time::Duration::from_millis(2000), async {
                    Command::new("python")
                        .args(["-m", "esptool", "--port", port, "chip_id"])
                        .stdout(Stdio::piped())
                        .stderr(Stdio::piped())
                        .kill_on_drop(true)
                        .output()
                        .await
                })
                .await
            {
                if let Ok(output) = result {
                    if output.status.success() {
                        let stdout = String::from_utf8_lossy(&output.stdout);
                        let stderr = String::from_utf8_lossy(&output.stderr);
                        let combined = format!("{} {}", stdout, stderr);

                        // Look for chip ID patterns
                        for line in combined.lines() {
                            if line.contains("Chip ID:") || line.contains("chip id:") {
                                if let Some(id_part) = line.split(':').nth(1) {
                                    let id_str = id_part.trim().trim_start_matches("0x");
                                    if let Ok(chip_id) = u32::from_str_radix(id_str, 16) {
                                        enhanced_info.chip_id = Some(chip_id);
                                        println!("✅ Found chip ID via esptool: 0x{:08X}", chip_id);
                                        break;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // Method 3: Try to get flash ID information
        if let Ok(result) = tokio::time::timeout(std::time::Duration::from_millis(2000), async {
            Command::new("python")
                .args(["-m", "esptool", "--port", port, "flash_id"])
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .kill_on_drop(true)
                .output()
                .await
        })
        .await
        {
            if let Ok(output) = result {
                if output.status.success() {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    let combined = format!("{} {}", stdout, stderr);

                    for line in combined.lines() {
                        if line.contains("Manufacturer:") {
                            if let Some(mfg) = line.split(':').nth(1) {
                                let manufacturer = mfg.trim().to_string();
                                enhanced_info.flash_manufacturer = Some(manufacturer.clone());
                                println!("✅ Found flash manufacturer: {}", manufacturer);
                            }
                        } else if line.contains("Device:") {
                            if let Some(device) = line.split(':').nth(1) {
                                let device_id = device.trim().to_string();
                                enhanced_info.flash_device_id = Some(device_id.clone());
                                println!("✅ Found flash device: {}", device_id);
                            }
                        }
                    }
                }
            }
        }

        // Method 4: Extract MAC address using dedicated function
        enhanced_info.mac_address = Self::get_mac_address(port).await;

        // Return Some if we got at least one piece of unique info
        if enhanced_info.chip_id.is_some()
            || enhanced_info.flash_manufacturer.is_some()
            || enhanced_info.mac_address.is_some()
        {
            println!(
                "✅ Enhanced identifiers found: chip_id={:?}, flash_mfg={:?}, flash_dev={:?}, mac={:?}",
                enhanced_info.chip_id,
                enhanced_info.flash_manufacturer,
                enhanced_info.flash_device_id,
                enhanced_info.mac_address
            );
            Some(enhanced_info)
        } else {
            println!("⚠️ No enhanced identifiers found for {}", port);
            None
        }
    }

    /// Generate a comprehensive unique ID from all available information
    fn generate_comprehensive_unique_id(board_info: &BoardInfo) -> String {
        let mut id_parts = Vec::new();

        // Priority order for uniqueness:
        // 1. MAC address (most unique)
        // 2. Chip ID + chip type (hardware unique)
        // 3. Flash identifiers + chip type (somewhat unique)
        // 4. Port-based fallback (not unique across reconnections)

        // Use MAC address if available and not masked
        let has_real_mac = !board_info.mac_address.contains("*")
            && !board_info.mac_address.contains("Unknown")
            && board_info.mac_address.len() >= 17
            && board_info.mac_address.contains(":");

        if has_real_mac {
            // MAC address is the most reliable unique identifier
            let mac_clean = board_info.mac_address.replace(":", "").to_uppercase();
            id_parts.push(format!("MAC{}", mac_clean));
            println!("✅ Using MAC-based unique ID: MAC{}", mac_clean);
        } else {
            // Fallback to chip-based identification
            id_parts.push(board_info.chip_type.clone().to_uppercase());

            // Add chip revision if available
            if let Some(ref revision) = board_info.chip_revision {
                id_parts.push(format!("REV{}", revision));
            }

            // Add chip ID if available (hardware-specific)
            if let Some(chip_id) = board_info.chip_id {
                id_parts.push(format!("CID{:08X}", chip_id));
                println!("✅ Using chip ID-based unique ID with CID{:08X}", chip_id);
            } else {
                // Add flash identifiers as secondary uniqueness
                if let Some(ref flash_mfg) = board_info.flash_manufacturer {
                    let mfg_clean = flash_mfg.replace(" ", "").replace("(", "").replace(")", "");
                    id_parts.push(format!("FLH{}", mfg_clean.to_uppercase()));
                }
                if let Some(ref flash_dev) = board_info.flash_device_id {
                    let dev_clean = flash_dev.replace(" ", "").replace("(", "").replace(")", "");
                    id_parts.push(format!("DEV{}", dev_clean.to_uppercase()));
                }

                // If we still don't have enough uniqueness, add port info
                if id_parts.len() < 2 {
                    let port_clean = board_info
                        .port
                        .replace("/", "-")
                        .replace(".", "_")
                        .to_uppercase();
                    id_parts.push(format!("PORT{}", port_clean));
                    println!(
                        "⚠️ Using port-based unique ID (less reliable): PORT{}",
                        port_clean
                    );
                }
            }
        }

        let unique_id = id_parts.join("-");
        println!("🏷️ Generated unique ID: {}", unique_id);
        unique_id
    }

    /// Native espflash identification using the espflash crate API directly
    async fn identify_with_espflash_native(port: &str) -> Result<Option<BoardInfo>> {
        use espflash::connection::{Connection, ResetAfterOperation, ResetBeforeOperation};
        use espflash::flasher::Flasher;
        use serialport::SerialPortType;
        use std::time::Duration;

        println!("🔍 Running native espflash identification for {}", port);

        // Get port info for creating connection
        let ports = match serialport::available_ports() {
            Ok(ports) => ports,
            Err(e) => {
                println!("⚠️ Failed to enumerate ports: {}", e);
                return Ok(None);
            }
        };

        let port_info = match ports.iter().find(|p| p.port_name == port) {
            Some(info) => info.clone(),
            None => {
                println!("⚠️ Port {} not found in available ports", port);
                return Ok(None);
            }
        };

        let usb_info = match &port_info.port_type {
            SerialPortType::UsbPort(info) => info.clone(),
            _ => {
                // For non-USB ports, create a dummy UsbPortInfo
                serialport::UsbPortInfo {
                    vid: 0,
                    pid: 0,
                    serial_number: None,
                    manufacturer: None,
                    product: None,
                    interface: None,
                }
            }
        };

        // Create serial port with longer timeout for ESP32-P4 compatibility
        let serial_port = match serialport::new(port, 115200)
            .timeout(Duration::from_millis(3000)) // Increased timeout for ESP32-P4
            .open_native()
        {
            Ok(port) => port,
            Err(e) => {
                println!("⚠️ Failed to open serial port {}: {}", port, e);
                return Ok(None);
            }
        };

        // Create connection with ESP32-P4 compatible settings
        let connection = Connection::new(
            *Box::new(serial_port),
            usb_info,
            ResetAfterOperation::HardReset,
            ResetBeforeOperation::DefaultReset,
            115200,
        );

        // Create flasher and connect in blocking task with extended timeout for ESP32-P4
        let flasher_result = tokio::task::spawn_blocking(move || {
            // Enable stub, verify_chip_id, and RAM download for better ESP32-P4 compatibility
            Flasher::connect(connection, true, true, true, None, None)
        })
        .await;

        let mut flasher = match flasher_result {
            Ok(Ok(flasher)) => flasher,
            Ok(Err(e)) => {
                println!("⚠️ Failed to connect to flasher on {}: {}", port, e);
                return Ok(None);
            }
            Err(e) => {
                println!("⚠️ Task error connecting to flasher on {}: {}", port, e);
                return Ok(None);
            }
        };

        // Get device info which includes MAC address and other details
        let device_info_result = tokio::task::spawn_blocking(move || flasher.device_info()).await;

        let device_info = match device_info_result {
            Ok(Ok(info)) => info,
            Ok(Err(e)) => {
                println!("⚠️ Failed to get device info on {}: {}", port, e);
                return Ok(None);
            }
            Err(e) => {
                println!("⚠️ Task error getting device info on {}: {}", port, e);
                return Ok(None);
            }
        };

        // Map espflash chip type to our board info format
        let chip_type = device_info.chip.to_string();
        let features = device_info.features.join(", ");
        let flash_size = device_info.flash_size.to_string();
        let crystal_frequency = device_info.crystal_frequency.to_string();
        let mac_address = device_info
            .mac_address
            .map(|mac| mac.to_string().to_uppercase())
            .unwrap_or_else(|| "Unknown".to_string());

        // Extract revision info
        let (chip_revision, chip_id) = match device_info.revision {
            Some((major, minor)) => {
                let revision_str = format!("{}.{}", major, minor);
                let chip_id = Some((major as u32) << 8 | (minor as u32));
                (Some(revision_str), chip_id)
            }
            None => (None, None),
        };

        // Generate comprehensive unique ID
        let unique_id = if !mac_address.contains("*") && !mac_address.contains("Unknown") {
            let mac_clean = mac_address.replace(":", "");
            format!("MAC{}", mac_clean)
        } else {
            format!(
                "{}:{}-{}",
                chip_type.to_uppercase(),
                chip_revision.as_deref().unwrap_or("unknown"),
                port.replace("/", "-").replace(".", "_")
            )
        };

        println!(
            "✅ Native espflash identified {} board: {} (rev: {})",
            chip_type,
            port,
            chip_revision.as_deref().unwrap_or("unknown")
        );

        let board_info = BoardInfo {
            port: port.to_string(),
            chip_type,
            crystal_frequency,
            flash_size,
            features,
            mac_address,
            device_description: "ESP Development Board (espflash native detected)".to_string(),
            chip_revision,
            chip_id,
            flash_manufacturer: None, // espflash DeviceInfo doesn't include flash manufacturer info
            flash_device_id: None,    // espflash DeviceInfo doesn't include flash device ID info
            unique_id,
        };

        Ok(Some(board_info))
    }

    /// Identification using espflash as subprocess with proper timeout and cancellation
    async fn identify_with_espflash_subprocess(port: &str) -> Result<Option<BoardInfo>> {
        use std::process::Stdio;
        use std::time::Duration;
        use tokio::process::Command;

        // Run espflash as separate subprocess to avoid terminal blocking
        // Increased timeout specifically for ESP32-P4 which can take longer to identify
        let timeout_dur = Duration::from_millis(3000);

        println!("🔍 Running espflash board-info for {}", port);

        let result = tokio::time::timeout(timeout_dur, async {
            let cmd = Command::new("espflash")
                .args(["board-info", "--port", port])
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .kill_on_drop(true) // Important: kill child if parent is cancelled
                .spawn()?;

            // Wait for the command to complete
            let output = cmd.wait_with_output().await?;

            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);

                println!("ℹ️ espflash stdout: {}", stdout.trim());
                if !stderr.trim().is_empty() {
                    println!("ℹ️ espflash stderr: {}", stderr.trim());
                }

                // Parse espflash output for comprehensive information
                let mut chip_type = "esp32".to_string();
                let mut flash_size = "Unknown".to_string();
                let mut features = "WiFi".to_string();
                let mut crystal_freq = "40 MHz".to_string();
                let mut chip_revision = None;
                let mut mac_address = "**:**:**:**:**:**".to_string();

                for line in stdout.lines() {
                    let line = line.trim();
                    if line.contains("Chip type:") {
                        if let Some(chip_info) = line.split(':').nth(1) {
                            let chip_info = chip_info.trim();
                            // Extract both chip type and revision if present
                            if let Some((chip, rev)) = chip_info.split_once(" (revision ") {
                                chip_type = chip.trim().to_lowercase().replace("-", "");
                                if let Some(revision) = rev.strip_suffix(")") {
                                    chip_revision = Some(revision.to_string());
                                }
                            } else {
                                chip_type = chip_info.to_lowercase().replace("-", "");
                            }
                        }
                    } else if line.contains("Crystal frequency:") {
                        if let Some(freq) = line.split(':').nth(1) {
                            crystal_freq = freq.trim().to_string();
                        }
                    } else if line.contains("Flash size:") {
                        if let Some(size) = line.split(':').nth(1) {
                            flash_size = size.trim().to_string();
                        }
                    } else if line.contains("Features:") {
                        if let Some(feat) = line.split(':').nth(1) {
                            features = feat.trim().to_string();
                        }
                    } else if line.contains("MAC address:") {
                        if let Some(mac) = line.split(':').nth(1) {
                            mac_address = mac.trim().to_string();
                        }
                    }
                }

                // Handle specific chip type conversions
                let normalized_chip_type = match chip_type.as_str() {
                    "esp32s3" | "esp32-s3" => "esp32s3",
                    "esp32p4" | "esp32-p4" => "esp32p4",
                    "esp32c3" | "esp32-c3" => "esp32c3",
                    "esp32c6" | "esp32-c6" => "esp32c6",
                    "esp32h2" | "esp32-h2" => "esp32h2",
                    _ => "esp32",
                };

                // Generate a preliminary unique ID from available info
                // We'll enhance this with additional unique identifiers later
                let preliminary_unique_id =
                    if mac_address != "**:**:**:**:**:**" && !mac_address.contains("*") {
                        mac_address.clone()
                    } else {
                        format!(
                            "{}:{}-{}",
                            normalized_chip_type,
                            chip_revision.as_deref().unwrap_or("unknown"),
                            port.replace("/", "-").replace(".", "_")
                        )
                    };

                println!(
                    "✅ Successfully identified {} board: {} (rev: {})",
                    normalized_chip_type,
                    port,
                    chip_revision.as_deref().unwrap_or("unknown")
                );

                let mut board_info = BoardInfo {
                    port: port.to_string(),
                    chip_type: normalized_chip_type.to_string(),
                    crystal_frequency: crystal_freq,
                    flash_size,
                    features,
                    mac_address: mac_address.clone(),
                    device_description: "ESP Development Board (espflash detected)".to_string(),
                    chip_revision,
                    chip_id: None,
                    flash_manufacturer: None,
                    flash_device_id: None,
                    unique_id: preliminary_unique_id,
                };

                // Try to get additional unique identifiers via esptool commands
                if let Some(enhanced_info) = Self::get_enhanced_unique_identifiers(port).await {
                    board_info.chip_id = enhanced_info.chip_id;
                    board_info.flash_manufacturer = enhanced_info.flash_manufacturer;
                    board_info.flash_device_id = enhanced_info.flash_device_id;

                    // Use enhanced MAC address if available
                    if let Some(enhanced_mac) = enhanced_info.mac_address {
                        board_info.mac_address = enhanced_mac;
                    }

                    // Create a more comprehensive unique ID
                    board_info.unique_id = Self::generate_comprehensive_unique_id(&board_info);
                }

                Ok(Some(board_info))
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                println!("ℹ️ espflash failed for {}: {}", port, stderr.trim());
                Ok(None)
            }
        })
        .await;

        match result {
            Ok(inner) => inner,
            Err(_) => {
                println!("⏰ espflash timed out for port {}", port);
                Ok(None) // Timeout
            }
        }
    }

    /// Flash a board with the provided binary
    pub async fn flash_board(&mut self, request: FlashRequest) -> Result<FlashResponse> {
        // Get board port before borrowing mutably
        let board_port = {
            let board = self
                .boards
                .get(&request.board_id)
                .ok_or_else(|| anyhow::anyhow!("Board not found: {}", request.board_id))?;
            board.port.clone()
        };

        // Update status to flashing
        if let Some(board) = self.boards.get_mut(&request.board_id) {
            board.status = BoardStatus::Flashing;
            board.last_updated = Local::now();
        }

        let start_time = std::time::Instant::now();

        // Perform the actual flashing using espflash
        // Determine what to flash based on request format
        let result = if let Some(flash_binaries) = &request.flash_binaries {
            // New multi-binary flash format - flash all binaries with proper offsets
            println!(
                "📦 Multi-binary flash: {} binaries to flash",
                flash_binaries.len()
            );
            for binary in flash_binaries {
                println!(
                    "  - {} at 0x{:x} ({} bytes)",
                    binary.name,
                    binary.offset,
                    binary.data.len()
                );
            }
            Self::perform_multi_flash(&board_port, flash_binaries, &request.flash_config).await
        } else {
            // Legacy single binary flash (deprecated)
            println!(
                "⚠️ Using legacy single binary flash at offset 0x{:x}",
                request.offset
            );
            Self::perform_flash(&board_port, &request.binary_data, request.offset).await
        };

        let duration_ms = start_time.elapsed().as_millis() as u64;

        // Update board status based on result
        if let Some(board) = self.boards.get_mut(&request.board_id) {
            match result {
                Ok(_) => {
                    board.status = BoardStatus::Available;
                    board.last_updated = Local::now();
                    Ok(FlashResponse {
                        success: true,
                        message: format!(
                            "Successfully flashed {} ({} bytes)",
                            board.id,
                            request.binary_data.len()
                        ),
                        duration_ms: Some(duration_ms),
                    })
                }
                Err(e) => {
                    board.status = BoardStatus::Error(e.to_string());
                    board.last_updated = Local::now();
                    Ok(FlashResponse {
                        success: false,
                        message: format!("Flash failed: {}", e),
                        duration_ms: Some(duration_ms),
                    })
                }
            }
        } else {
            Err(anyhow::anyhow!(
                "Board not found during status update: {}",
                request.board_id
            ))
        }
    }

    /// Progress callback for flash operations
    fn progress_callback(current: usize, total: usize) {
        let percentage = (current as f32 / total as f32) * 100.0;
        println!(
            "💾 Flash progress: {} / {} bytes ({:.1}%)",
            current, total, percentage
        );
    }

    /// Perform multi-binary flash operation (bootloader + partition table + application)
    async fn perform_multi_flash(
        port: &str,
        flash_binaries: &[FlashBinary],
        flash_config: &Option<FlashConfig>,
    ) -> Result<()> {
        println!(
            "🔥 Starting multi-binary flash operation on port {}: {} binaries",
            port,
            flash_binaries.len()
        );

        // Try native espflash first
        match Self::perform_multi_flash_native(port, flash_binaries, flash_config).await {
            Ok(()) => {
                println!("✨ Native multi-flash completed successfully");
                Ok(())
            }
            Err(e) => {
                println!(
                    "⚠️ Native multi-flash failed: {}. Falling back to esptool...",
                    e
                );
                Self::perform_multi_flash_esptool(port, flash_binaries, flash_config).await
            }
        }
    }

    /// Native multi-binary flash using espflash library
    async fn perform_multi_flash_native(
        _port: &str,
        flash_binaries: &[FlashBinary],
        _flash_config: &Option<FlashConfig>,
    ) -> Result<()> {
        // For now, return an error to fall back to esptool
        // This is where we would implement native espflash multi-binary support
        Err(anyhow::anyhow!(
            "Native multi-flash not yet implemented for {} binaries. Falling back to esptool.",
            flash_binaries.len()
        ))
    }

    /// Multi-binary flash using esptool (reliable fallback)
    async fn perform_multi_flash_esptool(
        port: &str,
        flash_binaries: &[FlashBinary],
        flash_config: &Option<FlashConfig>,
    ) -> Result<()> {
        use std::process::Stdio;
        use tokio::fs;
        use tokio::process::Command;

        println!(
            "🔥 Multi-flash using esptool: {} binaries on port {}",
            flash_binaries.len(),
            port
        );

        // Create temporary files for each binary
        let temp_dir = std::env::temp_dir();
        let mut temp_files = Vec::new();
        let mut args = vec![
            "--port".to_string(),
            port.to_string(),
            "--baud".to_string(),
            "460800".to_string(),
            "write-flash".to_string(),
        ];

        // Add flash configuration if provided (after write-flash command)
        if let Some(config) = flash_config {
            args.extend_from_slice(&[
                "--flash-mode".to_string(),
                config.flash_mode.clone(),
                "--flash-freq".to_string(),
                config.flash_freq.clone(),
                "--flash-size".to_string(),
                config.flash_size.clone(),
            ]);
        }

        // Create temp files and add to args
        for (i, binary) in flash_binaries.iter().enumerate() {
            let temp_file = temp_dir.join(format!(
                "espbrew_flash_{}_{}.bin",
                uuid::Uuid::new_v4().simple(),
                i
            ));

            // Write binary data to temp file
            fs::write(&temp_file, &binary.data).await?;
            temp_files.push(temp_file.clone());

            println!(
                "💾 [{}] {} -> {} ({} bytes)",
                binary.name,
                format!("0x{:x}", binary.offset),
                temp_file.display(),
                binary.data.len()
            );

            // Add offset and file to args
            args.push(format!("0x{:x}", binary.offset));
            args.push(temp_file.to_str().unwrap().to_string());
        }

        println!("🚀 Running esptool multi-flash command...");

        let cmd = Command::new("esptool")
            .args(&args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()?;

        // Wait for completion with extended timeout for multi-flash
        let timeout_dur = std::time::Duration::from_secs(300); // 5 minute timeout for multi-flash
        let result = tokio::time::timeout(timeout_dur, cmd.wait_with_output()).await;

        // Clean up temp files
        for temp_file in &temp_files {
            let _ = fs::remove_file(temp_file).await;
        }

        match result {
            Ok(Ok(output)) => {
                if output.status.success() {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    let stderr = String::from_utf8_lossy(&output.stderr);

                    // Print output for debugging
                    if !stdout.trim().is_empty() {
                        println!("📝 esptool stdout: {}", stdout.trim());
                    }
                    if !stderr.trim().is_empty() {
                        println!("📝 esptool stderr: {}", stderr.trim());
                    }

                    println!("✅ Multi-flash operation completed successfully");
                    Ok(())
                } else {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    Err(anyhow::anyhow!(
                        "Multi-flash command failed (exit code: {}): {} {}",
                        output.status.code().unwrap_or(-1),
                        stderr.trim(),
                        stdout.trim()
                    ))
                }
            }
            Ok(Err(e)) => Err(anyhow::anyhow!("Failed to run multi-flash command: {}", e)),
            Err(_) => Err(anyhow::anyhow!(
                "Multi-flash operation timed out after 5 minutes"
            )),
        }
    }

    /// Perform the actual flashing operation using native espflash library (more efficient)
    async fn perform_flash(port: &str, binary_data: &[u8], offset: u32) -> Result<()> {
        use espflash::connection::{Connection, ResetAfterOperation, ResetBeforeOperation};
        use espflash::flasher::Flasher;
        use serialport::SerialPortType;
        use std::time::Duration;

        println!(
            "🔥 Starting native flash operation: port={}, offset=0x{:x}, size={} bytes",
            port,
            offset,
            binary_data.len()
        );

        // Validate binary data
        if binary_data.is_empty() {
            return Err(anyhow::anyhow!("Binary data is empty"));
        }

        // Get port info for creating connection
        let ports = serialport::available_ports()
            .map_err(|e| anyhow::anyhow!("Failed to enumerate serial ports: {}", e))?;

        let port_info = ports
            .iter()
            .find(|p| p.port_name == port)
            .ok_or_else(|| anyhow::anyhow!("Port {} not found in available ports", port))?
            .clone();

        let usb_info = match &port_info.port_type {
            SerialPortType::UsbPort(info) => info.clone(),
            _ => {
                // For non-USB ports, create a dummy UsbPortInfo
                serialport::UsbPortInfo {
                    vid: 0,
                    pid: 0,
                    serial_number: None,
                    manufacturer: None,
                    product: None,
                    interface: None,
                }
            }
        };

        // Create serial port with appropriate timeout and baud rate for flashing
        let serial_port = serialport::new(port, 460800)
            .timeout(Duration::from_millis(2000))
            .open_native()
            .map_err(|e| anyhow::anyhow!("Failed to open serial port {}: {}", port, e))?;

        // Create connection with hard reset after flashing
        let connection = Connection::new(
            *Box::new(serial_port),
            usb_info,
            ResetAfterOperation::HardReset,
            ResetBeforeOperation::DefaultReset,
            460800,
        );

        println!("🔗 Establishing connection to ESP32 device...");

        // Create flasher and connect in blocking task (espflash is not async)
        let mut flasher = tokio::task::spawn_blocking(move || {
            Flasher::connect(connection, true, true, true, None, None)
        })
        .await
        .map_err(|e| anyhow::anyhow!("Task join error: {}", e))?
        .map_err(|e| anyhow::anyhow!("Failed to connect to ESP32 device on {}: {}", port, e))?;

        println!("🚀 Connected to ESP32 device, starting flash operation...");

        // Clone binary data to avoid lifetime issues with spawn_blocking
        let binary_data_owned = binary_data.to_vec();

        // Perform the actual flashing operation - use a more basic approach
        let flash_result = tokio::task::spawn_blocking(move || -> Result<()> {
            // Get device info for validation
            let device_info = flasher.device_info()
                .map_err(|e| anyhow::anyhow!("Failed to get device info: {}", e))?;

            println!("📎 Detected chip: {} with features: {:?}", device_info.chip, device_info.features);

            // For now, let's return an error to indicate we need to implement the correct flash API
            // This is a placeholder - we need to find the correct espflash API for writing flash data
            Err(anyhow::anyhow!(
                "Native espflash flashing not yet fully implemented. Binary size: {} bytes, offset: 0x{:x}",
                binary_data_owned.len(),
                offset
            ))
        })
        .await;

        match flash_result {
            Ok(Ok(())) => {
                println!("✨ Native flash operation completed successfully");
                Ok(())
            }
            Ok(Err(e)) => {
                println!("⚠️ Falling back to esptool due to: {}", e);
                // Fall back to the original esptool implementation
                Self::perform_flash_esptool(port, binary_data, offset).await
            }
            Err(e) => {
                println!("⚠️ Falling back to esptool due to task error: {}", e);
                // Fall back to the original esptool implementation
                Self::perform_flash_esptool(port, binary_data, offset).await
            }
        }
    }

    /// Fallback flash implementation using esptool command (original implementation)
    async fn perform_flash_esptool(port: &str, binary_data: &[u8], offset: u32) -> Result<()> {
        use std::process::Stdio;
        use tokio::fs;
        use tokio::process::Command;

        println!(
            "🔥 Fallback flash using esptool: port={}, offset=0x{:x}, size={} bytes",
            port,
            offset,
            binary_data.len()
        );

        // Create temporary file for binary data
        let temp_dir = std::env::temp_dir();
        let temp_file = temp_dir.join(format!(
            "espbrew_flash_{}.bin",
            uuid::Uuid::new_v4().simple()
        ));

        // Write binary data to temp file
        fs::write(&temp_file, binary_data).await?;
        println!(
            "💾 Wrote {} bytes to temp file: {}",
            binary_data.len(),
            temp_file.display()
        );

        // Use esptool for reliable flashing - this is what ESP-IDF uses
        let cmd = Command::new("esptool")
            .args([
                "--port",
                port,
                "--baud",
                "460800",
                "write_flash",
                &format!("0x{:x}", offset),
                temp_file.to_str().unwrap(),
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()?;

        println!("🚀 Running esptool command...");

        // Wait for completion with timeout
        let timeout_dur = std::time::Duration::from_secs(120); // 2 minute timeout for flashing
        let result = tokio::time::timeout(timeout_dur, cmd.wait_with_output()).await;

        // Clean up temp file
        let _ = fs::remove_file(&temp_file).await;

        match result {
            Ok(Ok(output)) => {
                if output.status.success() {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    let stderr = String::from_utf8_lossy(&output.stderr);

                    // Print output for debugging
                    if !stdout.trim().is_empty() {
                        println!("📝 esptool stdout: {}", stdout.trim());
                    }
                    if !stderr.trim().is_empty() {
                        println!("📝 esptool stderr: {}", stderr.trim());
                    }

                    println!("✅ Flash operation completed successfully");
                    Ok(())
                } else {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    Err(anyhow::anyhow!(
                        "Flash command failed (exit code: {}): {} {}",
                        output.status.code().unwrap_or(-1),
                        stderr.trim(),
                        stdout.trim()
                    ))
                }
            }
            Ok(Err(e)) => Err(anyhow::anyhow!("Failed to run flash command: {}", e)),
            Err(_) => Err(anyhow::anyhow!("Flash operation timed out after 2 minutes")),
        }
    }

    /// Start a new monitoring session for a board
    pub async fn start_monitoring_session(
        &mut self,
        request: MonitorRequest,
    ) -> Result<MonitorResponse> {
        // Find the board
        let board = self
            .boards
            .get(&request.board_id)
            .ok_or_else(|| anyhow::anyhow!("Board not found: {}", request.board_id))?
            .clone();

        // Check if board is already being monitored
        if matches!(board.status, BoardStatus::Monitoring) {
            return Err(anyhow::anyhow!("Board is already being monitored"));
        }

        // Generate session ID
        let session_id = Uuid::new_v4().to_string();
        let baud_rate = request.baud_rate.unwrap_or(115200);
        let port = board.port.clone();
        let board_id = request.board_id.clone();

        // Create broadcast channel for log streaming
        let (sender, _receiver) = broadcast::channel(1000);

        // Update board status
        if let Some(board) = self.boards.get_mut(&request.board_id) {
            board.status = BoardStatus::Monitoring;
            board.last_updated = Local::now();
        }

        // Create monitoring session
        let now = Local::now();
        let session = MonitoringSession {
            id: session_id.clone(),
            board_id: board_id.clone(),
            port: port.clone(),
            baud_rate,
            started_at: now,
            last_activity: now,
            sender: sender.clone(),
            task_handle: None,
        };

        // Start monitoring task
        let task_sender = sender.clone();
        let task_port = port.clone();
        let task_handle = tokio::spawn(async move {
            Self::monitoring_task(task_port, baud_rate, task_sender).await;
        });

        // Store session with task handle
        let mut final_session = session;
        final_session.task_handle = Some(task_handle);

        self.monitoring_sessions
            .write()
            .await
            .insert(session_id.clone(), final_session);

        println!(
            "📺 Started monitoring session: {} for board: {}",
            session_id, board_id
        );

        Ok(MonitorResponse {
            success: true,
            message: "Monitoring session started successfully".to_string(),
            websocket_url: Some(format!("/ws/monitor/{}", session_id)),
            session_id: Some(session_id),
        })
    }

    /// Stop a monitoring session
    pub async fn stop_monitoring_session(&mut self, session_id: &str) -> Result<()> {
        let session = {
            let mut sessions = self.monitoring_sessions.write().await;
            sessions.remove(session_id)
        };

        if let Some(session) = session {
            // Stop the monitoring task
            if let Some(task_handle) = session.task_handle {
                task_handle.abort();
            }

            // Update board status back to available
            if let Some(board) = self.boards.get_mut(&session.board_id) {
                board.status = BoardStatus::Available;
                board.last_updated = Local::now();
            }

            println!(
                "🛑 Stopped monitoring session: {} for board: {}",
                session_id, session.board_id
            );
            Ok(())
        } else {
            Err(anyhow::anyhow!(
                "Monitoring session not found: {}",
                session_id
            ))
        }
    }

    /// Keep a monitoring session alive by updating its last activity timestamp
    pub async fn keepalive_monitoring_session(&mut self, session_id: &str) -> Result<()> {
        let mut sessions = self.monitoring_sessions.write().await;

        if let Some(session) = sessions.get_mut(session_id) {
            session.last_activity = Local::now();
            println!(
                "💓 Keep-alive received for monitoring session: {}",
                session_id
            );
            Ok(())
        } else {
            Err(anyhow::anyhow!(
                "Monitoring session not found: {}",
                session_id
            ))
        }
    }

    /// Reset a board by toggling DTR/RTS lines
    pub async fn reset_board(&mut self, request: ResetRequest) -> Result<ResetResponse> {
        // Find the board
        let board = self
            .boards
            .get(&request.board_id)
            .ok_or_else(|| anyhow::anyhow!("Board not found: {}", request.board_id))?
            .clone();

        println!(
            "🔄 Resetting board: {} on port {}",
            request.board_id, board.port
        );

        // Check if there's an active monitoring session for this board
        let session_id_opt = {
            let monitoring_sessions = self.monitoring_sessions.read().await;
            monitoring_sessions
                .values()
                .find(|session| session.board_id == request.board_id)
                .map(|session| (session.id.clone(), session.sender.clone()))
        };

        if let Some((session_id, session_sender)) = session_id_opt {
            // There's an active monitoring session - we need to reset through the monitoring connection
            println!(
                "📡 Found active monitoring session for board {}, temporarily stopping for reset",
                request.board_id
            );

            // Send a reset notification through the broadcast channel
            let _ = session_sender.send("[RESET] Resetting board...".to_string());

            // Temporarily stop monitoring
            if let Err(e) = self.stop_monitoring_session(&session_id).await {
                println!("⚠️ Failed to stop monitoring for reset: {}", e);
            }

            // Small delay to ensure port is released
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;

            // Now perform the reset
            match self.reset_board_via_serial(&board.port).await {
                Ok(()) => {
                    println!("✅ Successfully reset board: {}", request.board_id);

                    // Restart monitoring session
                    let monitor_request = MonitorRequest {
                        board_id: request.board_id.clone(),
                        baud_rate: Some(115200),
                        filters: None,
                    };

                    if let Err(e) = self.start_monitoring_session(monitor_request).await {
                        println!("⚠️ Failed to restart monitoring after reset: {}", e);
                    }

                    Ok(ResetResponse {
                        success: true,
                        message: format!(
                            "Board {} reset successfully (monitoring restarted)",
                            request.board_id
                        ),
                    })
                }
                Err(e) => {
                    println!("❌ Failed to reset board {}: {}", request.board_id, e);

                    // Try to restart monitoring even if reset failed
                    let monitor_request = MonitorRequest {
                        board_id: request.board_id.clone(),
                        baud_rate: Some(115200),
                        filters: None,
                    };

                    if let Err(e) = self.start_monitoring_session(monitor_request).await {
                        println!("⚠️ Failed to restart monitoring after failed reset: {}", e);
                    }

                    Ok(ResetResponse {
                        success: false,
                        message: format!("Reset failed: {} (monitoring restarted)", e),
                    })
                }
            }
        } else {
            // No active monitoring session - proceed with normal reset
            match self.reset_board_via_serial(&board.port).await {
                Ok(()) => {
                    println!("✅ Successfully reset board: {}", request.board_id);
                    Ok(ResetResponse {
                        success: true,
                        message: format!("Board {} reset successfully", request.board_id),
                    })
                }
                Err(e) => {
                    println!("❌ Failed to reset board {}: {}", request.board_id, e);
                    Ok(ResetResponse {
                        success: false,
                        message: format!("Reset failed: {}", e),
                    })
                }
            }
        }
    }

    /// Reset board by toggling DTR/RTS lines
    async fn reset_board_via_serial(&self, port_path: &str) -> Result<()> {
        use std::time::Duration;

        // Try to open the serial port briefly to toggle DTR/RTS
        let mut port = serialport::new(port_path, 115200)
            .timeout(Duration::from_millis(100))
            .open()?;

        // Toggle DTR and RTS to reset ESP32 (standard reset sequence)
        // DTR=false, RTS=true -> ESP32 boot mode
        // DTR=true, RTS=false -> ESP32 reset
        port.write_data_terminal_ready(false)?;
        port.write_request_to_send(true)?;
        tokio::time::sleep(Duration::from_millis(10)).await;

        port.write_data_terminal_ready(true)?;
        port.write_request_to_send(false)?;
        tokio::time::sleep(Duration::from_millis(10)).await;

        // Return to normal state
        port.write_data_terminal_ready(false)?;
        port.write_request_to_send(false)?;

        Ok(())
    }

    /// Background task for reading serial port and broadcasting logs
    async fn monitoring_task(port: String, baud_rate: u32, sender: broadcast::Sender<String>) {
        use tokio::io::{AsyncBufReadExt, BufReader};
        use tokio_serial::SerialPortBuilderExt;

        println!(
            "📡 Starting async serial monitoring for port: {} at baud: {}",
            port, baud_rate
        );

        // Create async serial port
        match tokio_serial::new(&port, baud_rate)
            .data_bits(tokio_serial::DataBits::Eight)
            .flow_control(tokio_serial::FlowControl::None)
            .parity(tokio_serial::Parity::None)
            .stop_bits(tokio_serial::StopBits::One)
            .open_native_async()
        {
            Ok(serial_port) => {
                println!("✅ Successfully opened async serial port: {}", port);

                // Use BufReader for more reliable line reading
                let mut reader = BufReader::new(serial_port);
                let mut line_buffer = String::new();

                // Process each line as it arrives - this is fully async and event-driven
                loop {
                    line_buffer.clear();

                    match reader.read_line(&mut line_buffer).await {
                        Ok(0) => {
                            // EOF - serial port disconnected
                            println!("🔌 Serial port {} disconnected (EOF)", port);
                            break;
                        }
                        Ok(bytes_read) => {
                            let line = line_buffer.trim();
                            if !line.is_empty() {
                                println!("📝 [{}] {} bytes: {}", port, bytes_read, line); // Debug log
                                // Broadcast the log line to all connected WebSocket clients
                                if let Err(_) = sender.send(line.to_string()) {
                                    // No active receivers, but continue monitoring
                                    // This is normal when no WebSocket clients are connected
                                }
                            }
                        }
                        Err(e) => {
                            println!("⚠️ Error reading from serial port {}: {}", port, e);

                            // Handle different error types
                            match e.kind() {
                                std::io::ErrorKind::UnexpectedEof
                                | std::io::ErrorKind::BrokenPipe => {
                                    println!("🔌 Serial port {} disconnected", port);
                                    break;
                                }
                                std::io::ErrorKind::TimedOut => {
                                    // Timeout is normal, continue reading
                                    continue;
                                }
                                _ => {
                                    // For other errors, log and continue
                                    println!("⚠️ Continuing after error on {}: {}", port, e);
                                }
                            }
                        }
                    }
                }
            }
            Err(e) => {
                println!("❌ Failed to open async serial port {}: {}", port, e);
                let _ = sender.send(format!("❌ Failed to open serial port: {}", e));
            }
        }

        println!("🛑 Async monitoring task ended for port: {}", port);
    }
}

/// API request structures
#[derive(Debug, Serialize, Deserialize)]
pub struct AssignBoardRequest {
    /// Board unique ID
    pub board_unique_id: String,
    /// Board type ID to assign
    pub board_type_id: String,
    /// Optional logical name
    pub logical_name: Option<String>,
    /// Optional chip type override (for detection issues)
    pub chip_type_override: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BoardTypesResponse {
    pub board_types: Vec<BoardType>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AssignmentResponse {
    pub success: bool,
    pub message: String,
}

#[derive(Debug)]
struct BoardInfo {
    port: String,
    chip_type: String,
    crystal_frequency: String,
    flash_size: String,
    features: String,
    mac_address: String,
    device_description: String,
    chip_revision: Option<String>,
    chip_id: Option<u32>,
    flash_manufacturer: Option<String>,
    flash_device_id: Option<String>,
    unique_id: String, // Combined unique identifier
}

// API Handlers
pub async fn list_boards(
    state: Arc<RwLock<ServerState>>,
) -> Result<impl warp::Reply, warp::Rejection> {
    let state = state.read().await;

    // Get boards with latest enhanced information applied
    let mut boards = Vec::new();
    for board in state.boards.values() {
        let mut enhanced_board = board.clone();
        state.apply_enhanced_info(&mut enhanced_board).await;
        boards.push(enhanced_board);
    }

    let response = BoardListResponse {
        boards,
        server_info: ServerInfo {
            version: env!("CARGO_PKG_VERSION").to_string(),
            hostname: hostname::get()
                .unwrap_or_else(|_| "unknown".into())
                .to_string_lossy()
                .to_string(),
            last_scan: state.last_scan,
            total_boards: state.boards.len(),
        },
    };
    Ok(warp::reply::json(&response))
}

pub async fn flash_board(
    request: FlashRequest,
    state: Arc<RwLock<ServerState>>,
) -> Result<impl warp::Reply, warp::Rejection> {
    let mut state = state.write().await;

    match state.flash_board(request).await {
        Ok(response) => Ok(warp::reply::json(&response)),
        Err(e) => {
            let error_response = FlashResponse {
                success: false,
                message: format!("Flash operation failed: {}", e),
                duration_ms: None,
            };
            Ok(warp::reply::json(&error_response))
        }
    }
}

/// Handle multipart form data flash requests (for web interface)
pub async fn flash_board_form(
    form: warp::multipart::FormData,
    state: Arc<RwLock<ServerState>>,
) -> Result<impl warp::Reply, warp::Rejection> {
    use futures::StreamExt;

    let mut board_id: Option<String> = None;
    let mut flash_address: Option<String> = None;
    let mut binary_data: Option<Vec<u8>> = None;

    let mut parts = form;

    // Process each part of the multipart form
    while let Some(part_result) = parts.next().await {
        let part = match part_result {
            Ok(part) => part,
            Err(e) => {
                let error_response = FlashResponse {
                    success: false,
                    message: format!("Failed to parse form data: {}", e),
                    duration_ms: None,
                };
                return Ok(warp::reply::json(&error_response));
            }
        };

        let name = part.name().to_string();

        // Read the data from the part using collect
        let mut all_bytes = Vec::new();
        let mut stream = part.stream();

        while let Some(chunk_result) = stream.next().await {
            match chunk_result {
                Ok(chunk) => {
                    all_bytes.extend_from_slice(chunk.chunk());
                }
                Err(e) => {
                    let error_response = FlashResponse {
                        success: false,
                        message: format!("Failed to read form part '{}': {}", name, e),
                        duration_ms: None,
                    };
                    return Ok(warp::reply::json(&error_response));
                }
            }
        }

        let bytes = all_bytes;

        match name.as_str() {
            "board_id" => {
                board_id = Some(String::from_utf8_lossy(&bytes).trim().to_string());
            }
            "flash_address" => {
                flash_address = Some(String::from_utf8_lossy(&bytes).trim().to_string());
            }
            "binary_file" => {
                if !bytes.is_empty() {
                    binary_data = Some(bytes);
                }
            }
            _ => {
                // Ignore other fields
                continue;
            }
        }
    }

    // Validate required fields
    let board_id = match board_id {
        Some(id) if !id.is_empty() => id,
        _ => {
            let error_response = FlashResponse {
                success: false,
                message: "Missing or empty board_id field".to_string(),
                duration_ms: None,
            };
            return Ok(warp::reply::json(&error_response));
        }
    };

    let flash_address = flash_address.unwrap_or_else(|| "0x10000".to_string());

    let binary_data = match binary_data {
        Some(data) if !data.is_empty() => data,
        _ => {
            let error_response = FlashResponse {
                success: false,
                message: "Missing or empty binary file".to_string(),
                duration_ms: None,
            };
            return Ok(warp::reply::json(&error_response));
        }
    };

    // Parse flash address (support both hex and decimal)
    let offset = if flash_address.starts_with("0x") || flash_address.starts_with("0X") {
        u32::from_str_radix(&flash_address[2..], 16).unwrap_or(0x10000)
    } else {
        flash_address.parse::<u32>().unwrap_or(0x10000)
    };

    println!(
        "📤 Flash request: board_id={}, offset=0x{:x}, size={} bytes",
        board_id,
        offset,
        binary_data.len()
    );

    // Create FlashRequest from form data (legacy single-binary format)
    let flash_binary = FlashBinary {
        offset,
        data: binary_data.clone(),
        name: "application".to_string(),
        file_name: "legacy.bin".to_string(),
    };

    let request = FlashRequest {
        board_id,
        binary_data,
        offset,
        flash_binaries: Some(vec![flash_binary]),
        flash_config: None,
        chip_type: None, // Auto-detect
        verify: true,
    };

    // Call existing flash_board logic
    let mut state = state.write().await;
    match state.flash_board(request).await {
        Ok(response) => Ok(warp::reply::json(&response)),
        Err(e) => {
            let error_response = FlashResponse {
                success: false,
                message: format!("Flash operation failed: {}", e),
                duration_ms: None,
            };
            Ok(warp::reply::json(&error_response))
        }
    }
}

/// Handle multipart form data flash requests with multi-binary support (for ESP-IDF builds)
pub async fn flash_board_multi_form(
    form: warp::multipart::FormData,
    state: Arc<RwLock<ServerState>>,
) -> Result<impl warp::Reply, warp::Rejection> {
    use futures::StreamExt;

    let mut board_id: Option<String> = None;
    let mut flash_mode: Option<String> = None;
    let mut flash_freq: Option<String> = None;
    let mut flash_size: Option<String> = None;
    let mut binary_count: Option<usize> = None;
    let mut binaries = Vec::new();
    let mut binary_metadata = std::collections::HashMap::new();

    let mut parts = form;

    // First pass - collect all form data
    while let Some(part_result) = parts.next().await {
        let part = match part_result {
            Ok(part) => part,
            Err(e) => {
                let error_response = FlashResponse {
                    success: false,
                    message: format!("Failed to parse form data: {}", e),
                    duration_ms: None,
                };
                return Ok(warp::reply::json(&error_response));
            }
        };

        let name = part.name().to_string();

        // Read the data from the part
        let mut all_bytes = Vec::new();
        let mut stream = part.stream();

        while let Some(chunk_result) = stream.next().await {
            match chunk_result {
                Ok(chunk) => {
                    all_bytes.extend_from_slice(chunk.chunk());
                }
                Err(e) => {
                    let error_response = FlashResponse {
                        success: false,
                        message: format!("Failed to read form part '{}': {}", name, e),
                        duration_ms: None,
                    };
                    return Ok(warp::reply::json(&error_response));
                }
            }
        }

        let bytes = all_bytes;
        let text = String::from_utf8_lossy(&bytes).trim().to_string();

        match name.as_str() {
            "board_id" => board_id = Some(text),
            "flash_mode" => flash_mode = Some(text),
            "flash_freq" => flash_freq = Some(text),
            "flash_size" => flash_size = Some(text),
            "binary_count" => binary_count = text.parse().ok(),
            name if name.starts_with("binary_") => {
                if name.ends_with("_offset")
                    || name.ends_with("_name")
                    || name.ends_with("_filename")
                {
                    // Store metadata - convert name to String for consistent HashMap key type
                    binary_metadata.insert(name.to_string(), text);
                } else if let Some(binary_idx_str) = name.strip_prefix("binary_") {
                    // This is binary data
                    if let Ok(index) = binary_idx_str.parse::<usize>() {
                        binaries.push((index, bytes));
                    }
                }
            }
            _ => {
                // Ignore other fields
                continue;
            }
        }
    }

    // Validate required fields
    let board_id = match board_id {
        Some(id) if !id.is_empty() => id,
        _ => {
            let error_response = FlashResponse {
                success: false,
                message: "Missing or empty board_id field".to_string(),
                duration_ms: None,
            };
            return Ok(warp::reply::json(&error_response));
        }
    };

    let binary_count = binary_count.unwrap_or(0);
    if binary_count == 0 {
        let error_response = FlashResponse {
            success: false,
            message: "No binaries specified".to_string(),
            duration_ms: None,
        };
        return Ok(warp::reply::json(&error_response));
    }

    // Build FlashBinary structs from collected data
    let mut flash_binaries = Vec::new();
    for (index, data) in binaries {
        let offset_key = format!("binary_{}_offset", index);
        let name_key = format!("binary_{}_name", index);
        let filename_key = format!("binary_{}_filename", index);

        let offset_str = match binary_metadata.get(&offset_key) {
            Some(val) => val,
            None => {
                let error_response = FlashResponse {
                    success: false,
                    message: format!("Missing offset for binary {}", index),
                    duration_ms: None,
                };
                return Ok(warp::reply::json(&error_response));
            }
        };

        let offset = if offset_str.starts_with("0x") || offset_str.starts_with("0X") {
            match u32::from_str_radix(&offset_str[2..], 16) {
                Ok(val) => val,
                Err(e) => {
                    let error_response = FlashResponse {
                        success: false,
                        message: format!("Invalid hex offset {}: {}", offset_str, e),
                        duration_ms: None,
                    };
                    return Ok(warp::reply::json(&error_response));
                }
            }
        } else {
            match offset_str.parse::<u32>() {
                Ok(val) => val,
                Err(e) => {
                    let error_response = FlashResponse {
                        success: false,
                        message: format!("Invalid offset {}: {}", offset_str, e),
                        duration_ms: None,
                    };
                    return Ok(warp::reply::json(&error_response));
                }
            }
        };

        let name = binary_metadata
            .get(&name_key)
            .cloned()
            .unwrap_or_else(|| format!("binary_{}", index));

        let file_name = binary_metadata
            .get(&filename_key)
            .cloned()
            .unwrap_or_else(|| format!("binary_{}.bin", index));

        flash_binaries.push(FlashBinary {
            offset,
            data,
            name,
            file_name,
        });
    }

    // Sort binaries by offset for consistent flashing order
    flash_binaries.sort_by_key(|b| b.offset);

    println!(
        "📤 Multi-binary flash request: board_id={}, {} binaries",
        board_id,
        flash_binaries.len()
    );

    for binary in &flash_binaries {
        println!(
            "  - {} ({}) at 0x{:x}: {} bytes",
            binary.name,
            binary.file_name,
            binary.offset,
            binary.data.len()
        );
    }

    // Create flash config if provided
    let flash_config = if flash_mode.is_some() || flash_freq.is_some() || flash_size.is_some() {
        Some(FlashConfig {
            flash_mode: flash_mode.unwrap_or_else(|| "dio".to_string()),
            flash_freq: flash_freq.unwrap_or_else(|| "40m".to_string()),
            flash_size: flash_size.unwrap_or_else(|| "detect".to_string()),
        })
    } else {
        None
    };

    // Create FlashRequest - use the first binary for legacy fields
    let first_binary = flash_binaries.first().unwrap();
    let request = FlashRequest {
        board_id,
        binary_data: first_binary.data.clone(),
        offset: first_binary.offset,
        flash_binaries: Some(flash_binaries),
        flash_config,
        chip_type: None,
        verify: true,
    };

    // Call existing flash_board logic
    let mut state = state.write().await;
    match state.flash_board(request).await {
        Ok(response) => Ok(warp::reply::json(&response)),
        Err(e) => {
            let error_response = FlashResponse {
                success: false,
                message: format!("Flash operation failed: {}", e),
                duration_ms: None,
            };
            Ok(warp::reply::json(&error_response))
        }
    }
}

pub async fn get_board_info(
    board_id: String,
    state: Arc<RwLock<ServerState>>,
) -> Result<impl warp::Reply, warp::Rejection> {
    let state = state.read().await;

    match state.boards.get(&board_id) {
        Some(board) => {
            // Apply latest enhanced information
            let mut enhanced_board = board.clone();
            state.apply_enhanced_info(&mut enhanced_board).await;
            Ok(warp::reply::json(&enhanced_board))
        }
        None => {
            let error = serde_json::json!({
                "error": format!("Board not found: {}", board_id)
            });
            Ok(warp::reply::json(&error))
        }
    }
}

// Web Interface Handlers
/*
pub async fn dashboard(
    state: Arc<RwLock<ServerState>>,
) -> Result<impl warp::Reply, warp::Rejection> {
    let state = state.read().await;
    let template = DashboardTemplate {
        page: "dashboard".to_string(),
        boards: state.boards.values().cloned().collect(),
        server_info: ServerInfo {
            version: env!("CARGO_PKG_VERSION").to_string(),
            hostname: hostname::get()
                .unwrap_or_else(|_| "unknown".into())
                .to_string_lossy()
                .to_string(),
            last_scan: state.last_scan,
            total_boards: state.boards.len(),
        },
    };

    match template.render() {
        Ok(html) => Ok(warp::reply::html(html)),
        Err(e) => {
            eprintln!("Template rendering error: {}", e);
            Ok(warp::reply::html(format!("<h1>Error</h1><p>Failed to render template: {}</p>", e)))
        }
    }
}


pub async fn flash_board_form(
    query: HashMap<String, String>,
    state: Arc<RwLock<ServerState>>,
) -> Result<impl warp::Reply, warp::Rejection> {
    let state = state.read().await;
    let template = FlashTemplate {
        page: "flash".to_string(),
        boards: state.boards.values().cloned().collect(),
        selected_board: query.get("board").map(|s| s.clone()),
    };

    match template.render() {
        Ok(html) => Ok(warp::reply::html(html)),
        Err(e) => {
            eprintln!("Template rendering error: {}", e);
            Ok(warp::reply::html(format!("<h1>Error</h1><p>Failed to render template: {}</p>", e)))
        }
    }
}
*/

#[derive(Parser)]
#[command(name = "espbrew-server")]
#[command(about = "ESPBrew Remote Flashing Server")]
struct ServerCli {
    /// Server configuration file
    #[arg(short, long, default_value = "espbrew-server.toml")]
    config: PathBuf,

    /// Bind address
    #[arg(short, long, default_value = "0.0.0.0")]
    bind: String,

    /// Port to listen on
    #[arg(short, long, default_value = "8080")]
    port: u16,

    /// Board scan interval in seconds
    #[arg(long, default_value = "30")]
    scan_interval: u64,

    /// Disable mDNS service announcement
    #[arg(long)]
    no_mdns: bool,

    /// mDNS service name (defaults to hostname)
    #[arg(long)]
    mdns_name: Option<String>,

    #[command(subcommand)]
    command: Option<ServerCommands>,
}

#[derive(Subcommand)]
enum ServerCommands {
    /// Start the server
    Start,
    /// Scan for boards and exit
    Scan,
    /// Generate default configuration
    Config,
}

/// Get available board types
pub async fn get_board_types(
    state: Arc<RwLock<ServerState>>,
) -> Result<impl warp::Reply, warp::Rejection> {
    let state = state.read().await;
    let response = BoardTypesResponse {
        board_types: state.get_available_board_types().to_vec(),
    };
    Ok(warp::reply::json(&response))
}

/// Assign a board to a board type
pub async fn assign_board_type(
    request: AssignBoardRequest,
    state: Arc<RwLock<ServerState>>,
) -> Result<impl warp::Reply, warp::Rejection> {
    let mut state = state.write().await;

    match state
        .assign_board_type(
            request.board_unique_id,
            request.board_type_id,
            request.logical_name,
            request.chip_type_override,
        )
        .await
    {
        Ok(()) => {
            let response = AssignmentResponse {
                success: true,
                message: "Board type assigned successfully".to_string(),
            };
            Ok(warp::reply::json(&response))
        }
        Err(e) => {
            let response = AssignmentResponse {
                success: false,
                message: format!("Failed to assign board type: {}", e),
            };
            Ok(warp::reply::json(&response))
        }
    }
}

/// Unassign a board from its board type
pub async fn unassign_board(
    unique_id: String,
    state: Arc<RwLock<ServerState>>,
) -> Result<impl warp::Reply, warp::Rejection> {
    let mut state = state.write().await;

    match state.unassign_board(unique_id).await {
        Ok(()) => {
            let response = AssignmentResponse {
                success: true,
                message: "Board unassigned successfully".to_string(),
            };
            Ok(warp::reply::json(&response))
        }
        Err(e) => {
            let response = AssignmentResponse {
                success: false,
                message: format!("Failed to unassign board: {}", e),
            };
            Ok(warp::reply::json(&response))
        }
    }
}

/// Start monitoring a board
pub async fn start_monitoring(
    request: MonitorRequest,
    state: Arc<RwLock<ServerState>>,
) -> Result<impl warp::Reply, warp::Rejection> {
    let mut state = state.write().await;

    match state.start_monitoring_session(request).await {
        Ok(response) => Ok(warp::reply::json(&response)),
        Err(e) => {
            let error_response = MonitorResponse {
                success: false,
                message: format!("Failed to start monitoring: {}", e),
                websocket_url: None,
                session_id: None,
            };
            Ok(warp::reply::json(&error_response))
        }
    }
}

/// Stop monitoring a session
pub async fn stop_monitoring(
    request: StopMonitorRequest,
    state: Arc<RwLock<ServerState>>,
) -> Result<impl warp::Reply, warp::Rejection> {
    let mut state = state.write().await;

    match state.stop_monitoring_session(&request.session_id).await {
        Ok(()) => {
            let response = StopMonitorResponse {
                success: true,
                message: "Monitoring session stopped successfully".to_string(),
            };
            Ok(warp::reply::json(&response))
        }
        Err(e) => {
            let response = StopMonitorResponse {
                success: false,
                message: format!("Failed to stop monitoring: {}", e),
            };
            Ok(warp::reply::json(&response))
        }
    }
}

/// List active monitoring sessions
pub async fn list_monitoring_sessions(
    state: Arc<RwLock<ServerState>>,
) -> Result<impl warp::Reply, warp::Rejection> {
    let state = state.read().await;
    let sessions = state.monitoring_sessions.read().await;

    let session_info: Vec<serde_json::Value> = sessions
        .values()
        .map(|session| {
            serde_json::json!({
                "session_id": session.id,
                "board_id": session.board_id,
                "port": session.port,
                "baud_rate": session.baud_rate,
                "started_at": session.started_at,
                "last_activity": session.last_activity
            })
        })
        .collect();

    let response = serde_json::json!({
        "success": true,
        "sessions": session_info
    });

    Ok(warp::reply::json(&response))
}

/// Keep a monitoring session alive
pub async fn keepalive_monitoring_session(
    request: KeepAliveRequest,
    state: Arc<RwLock<ServerState>>,
) -> Result<impl warp::Reply, warp::Rejection> {
    let mut state = state.write().await;

    match state
        .keepalive_monitoring_session(&request.session_id)
        .await
    {
        Ok(()) => {
            let response = KeepAliveResponse {
                success: true,
                message: "Monitoring session keep-alive updated".to_string(),
            };
            Ok(warp::reply::json(&response))
        }
        Err(e) => {
            let response = KeepAliveResponse {
                success: false,
                message: format!("Failed to update keep-alive: {}", e),
            };
            Ok(warp::reply::json(&response))
        }
    }
}

/// Reset a board
pub async fn reset_board_handler(
    request: ResetRequest,
    state: Arc<RwLock<ServerState>>,
) -> Result<impl warp::Reply, warp::Rejection> {
    let mut state = state.write().await;

    match state.reset_board(request).await {
        Ok(response) => Ok(warp::reply::json(&response)),
        Err(e) => {
            let error_response = ResetResponse {
                success: false,
                message: format!("Reset operation failed: {}", e),
            };
            Ok(warp::reply::json(&error_response))
        }
    }
}

/// WebSocket handler for monitoring logs
pub async fn monitor_websocket_handler(
    websocket: WebSocket,
    session_id: String,
    state: Arc<RwLock<ServerState>>,
) {
    let (mut ws_sender, mut ws_receiver) = websocket.split();

    // Get the broadcast receiver for this session
    let receiver = {
        let state = state.read().await;
        let sessions = state.monitoring_sessions.read().await;

        match sessions.get(&session_id) {
            Some(session) => session.sender.subscribe(),
            None => {
                let _ = ws_sender
                    .send(Message::text(
                        serde_json::json!({
                            "error": "Session not found",
                            "session_id": session_id
                        })
                        .to_string(),
                    ))
                    .await;
                return;
            }
        }
    };

    // Send initial connection message
    let _ = ws_sender
        .send(Message::text(
            serde_json::json!({
                "type": "connection",
                "message": "Connected to monitoring session",
                "session_id": session_id
            })
            .to_string(),
        ))
        .await;

    let mut receiver = receiver;

    // Handle incoming messages and forward logs
    tokio::select! {
        // Forward logs from the monitoring session
        _ = async {
            while let Ok(log_line) = receiver.recv().await {
                let message = serde_json::json!({
                    "type": "log",
                    "session_id": session_id,
                    "content": log_line,
                    "timestamp": chrono::Local::now()
                });

                if ws_sender.send(Message::text(message.to_string())).await.is_err() {
                    break;
                }
            }
        } => {}
        // Handle client disconnection
        _ = async {
            while let Some(msg) = ws_receiver.next().await {
                match msg {
                    Ok(msg) => {
                        if msg.is_close() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        } => {}
    }

    println!("📡 WebSocket connection closed for session: {}", session_id);

    // Automatically stop the monitoring session when WebSocket disconnects
    {
        let mut state = state.write().await;
        if let Err(e) = state.stop_monitoring_session(&session_id).await {
            eprintln!(
                "❌ Failed to auto-stop session {} after WebSocket disconnect: {}",
                session_id, e
            );
        } else {
            println!(
                "✅ Automatically stopped monitoring session: {}",
                session_id
            );
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = ServerCli::parse();

    // Load or create configuration
    let mut config = if cli.config.exists() {
        let config_str = tokio::fs::read_to_string(&cli.config).await?;
        toml::from_str(&config_str)?
    } else {
        ServerConfig::default()
    };

    // Override with CLI arguments
    config.bind_address = cli.bind.clone();
    config.port = cli.port;
    config.scan_interval = cli.scan_interval;
    config.mdns_enabled = !cli.no_mdns;
    if let Some(name) = cli.mdns_name {
        config.mdns_service_name = Some(name);
    }

    match cli.command.unwrap_or(ServerCommands::Start) {
        ServerCommands::Config => {
            let config_str = toml::to_string_pretty(&config)?;
            tokio::fs::write(&cli.config, config_str).await?;
            println!("📝 Configuration written to {}", cli.config.display());
            return Ok(());
        }
        ServerCommands::Scan => {
            // Set up signal handling for graceful shutdown on CTRL+C
            let ctrl_c = async {
                tokio::signal::ctrl_c()
                    .await
                    .expect("Failed to install CTRL+C signal handler")
            };

            let mut state = ServerState::new(config);

            // Run scan with signal handling and timeout
            tokio::select! {
                result = state.scan_boards() => {
                    match result {
                        Ok(_) => println!("✅ Scan completed successfully"),
                        Err(e) => {
                            eprintln!("❌ Scan failed: {}", e);
                            return Err(e);
                        }
                    }
                }
                _ = ctrl_c => {
                    println!("\n🛑 Scan interrupted by user (CTRL+C)");
                    return Ok(());
                }
                _ = tokio::time::sleep(tokio::time::Duration::from_secs(60)) => {
                    println!("🕒 Scan timed out after 60 seconds");
                    return Ok(());
                }
            }

            return Ok(());
        }
        ServerCommands::Start => {
            // Continue to start the server
        }
    }

    println!("🍺 ESPBrew Server v{}", env!("CARGO_PKG_VERSION"));

    // Enhanced startup logging showing actual available addresses
    if config.bind_address == "0.0.0.0" {
        println!(
            "🌐 Binding to 0.0.0.0:{} (listening on all interfaces)",
            config.port
        );

        let interfaces = get_network_interfaces();
        if !interfaces.is_empty() {
            println!("📡 Server accessible on:");

            // Always show localhost
            println!("   • http://localhost:{}", config.port);

            // Show all network interfaces
            for (name, ip) in interfaces {
                println!("   • http://{}:{} ({})", ip, config.port, name);
            }
        } else {
            println!("   • http://localhost:{}", config.port);
            println!("   ⚠️  Could not detect network interfaces");
        }
    } else {
        println!(
            "🌐 Starting server on {}:{}",
            config.bind_address, config.port
        );
    }

    // Initialize server state
    let state = Arc::new(RwLock::new(ServerState::new(config.clone())));

    // Perform initial board scan
    {
        let mut state_lock = state.write().await;
        state_lock.scan_boards().await?;
    }

    // Setup mDNS service announcement
    let _mdns_service = {
        let state_lock = state.read().await;
        match setup_mdns_service(&config, &*state_lock) {
            Ok(service) => service,
            Err(e) => {
                eprintln!("⚠️ Failed to setup mDNS service: {}", e);
                eprintln!("📻 Server will continue without mDNS announcement");
                None
            }
        }
    };

    // Setup shutdown notify for background tasks
    let shutdown_notify = Arc::new(tokio::sync::Notify::new());

    // Start periodic board scanning
    let scan_state = state.clone();
    let scan_interval = config.scan_interval;
    let scan_shutdown = shutdown_notify.clone();
    let scanner_handle = tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(scan_interval));
        let scan_cancel_signal = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    if let Ok(mut state) = scan_state.try_write() {
                        // Reset the cancel signal for this scan iteration
                        scan_cancel_signal.store(false, std::sync::atomic::Ordering::Relaxed);

                        if let Err(e) = state.scan_boards_with_cancellation(Some(scan_cancel_signal.clone())).await {
                            eprintln!("❌ Board scan failed: {}", e);
                        }
                    }
                }
                _ = scan_shutdown.notified() => {
                    println!("🛑 Stopping scanner task...");
                    // Signal any ongoing scan to cancel
                    scan_cancel_signal.store(true, std::sync::atomic::Ordering::Relaxed);
                    break;
                }
            }
        }
        println!("🛑 Scanner task stopped");
    });

    // Start monitoring session cleanup task
    let cleanup_state = state.clone();
    let cleanup_shutdown = shutdown_notify.clone();
    let _cleanup_handle = tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(30)); // Check every 30 seconds
        const KEEPALIVE_TIMEOUT_SECS: i64 = 120; // 2 minutes timeout

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    if let Ok(mut state) = cleanup_state.try_write() {
                        let now = Local::now();
                        let mut expired_sessions = Vec::new();

                        // Check for expired sessions
                        {
                            let sessions = state.monitoring_sessions.read().await;
                            for (session_id, session) in sessions.iter() {
                                let duration = now.signed_duration_since(session.last_activity);
                                if duration.num_seconds() > KEEPALIVE_TIMEOUT_SECS {
                                    expired_sessions.push(session_id.clone());
                                }
                            }
                        }

                        // Remove expired sessions
                        for session_id in expired_sessions {
                            println!("⏰ Cleaning up expired monitoring session: {}", session_id);
                            if let Err(e) = state.stop_monitoring_session(&session_id).await {
                                eprintln!("❌ Failed to stop expired session {}: {}", session_id, e);
                            }
                        }
                    }
                }
                _ = cleanup_shutdown.notified() => {
                    println!("🛑 Stopping monitoring cleanup task...");
                    break;
                }
            }
        }
        println!("🛑 Monitoring cleanup task stopped");
    });

    // Define routes
    let state_filter = warp::any().map(move || state.clone());

    // Web Interface routes (temporarily disabled)
    /*
    // GET / - Dashboard
    let web_dashboard = warp::path::end()
        .and(warp::get())
        .and(state_filter.clone())
        .and_then(dashboard);

    // GET /flash - Flash page
    let web_flash = warp::path("flash")
        .and(warp::path::end())
        .and(warp::get())
        .and(warp::query::<HashMap<String, String>>())
        .and(state_filter.clone())
        .and_then(flash_page);
    */

    let api = warp::path("api").and(warp::path("v1"));

    // GET /api/v1/boards - List all boards
    let boards = api
        .and(warp::path("boards"))
        .and(warp::path::end())
        .and(warp::get())
        .and(state_filter.clone())
        .and_then(list_boards);

    // GET /api/v1/boards/{id} - Get board info
    let board_info = api
        .and(warp::path("boards"))
        .and(warp::path::param())
        .and(warp::path::end())
        .and(warp::get())
        .and(state_filter.clone())
        .and_then(get_board_info);

    // POST /api/v1/flash - Flash a board (JSON API)
    let flash_json = api
        .and(warp::path("flash"))
        .and(warp::path::end())
        .and(warp::post())
        .and(warp::body::json())
        .and(state_filter.clone())
        .and_then(flash_board);

    // POST /api/v1/flash - Flash a board (Multipart form for web interface - legacy single binary)
    let flash_form_legacy = api
        .and(warp::path("flash"))
        .and(warp::path::end())
        .and(warp::post())
        .and(warp::multipart::form().max_length(100 * 1024 * 1024)) // 100MB max file size
        .and(state_filter.clone())
        .and_then(flash_board_form);

    // POST /api/v1/flash - Flash a board (Multipart form for ESP-IDF multi-binary builds)
    let flash_form_multi = api
        .and(warp::path("flash"))
        .and(warp::path::end())
        .and(warp::post())
        .and(warp::multipart::form().max_length(500 * 1024 * 1024)) // 500MB max for multi-binary
        .and(state_filter.clone())
        .and_then(flash_board_multi_form);

    // Combine all flash endpoints - try multi-binary first, fall back to legacy
    let flash = flash_json.or(flash_form_multi).or(flash_form_legacy);

    // POST /api/v1/reset - Reset a board
    let reset_board = api
        .and(warp::path("reset"))
        .and(warp::path::end())
        .and(warp::post())
        .and(warp::body::json())
        .and(state_filter.clone())
        .and_then(reset_board_handler);

    // POST /api/v1/monitor/start - Start monitoring a board
    let monitor_start = api
        .and(warp::path("monitor"))
        .and(warp::path("start"))
        .and(warp::path::end())
        .and(warp::post())
        .and(warp::body::json())
        .and(state_filter.clone())
        .and_then(start_monitoring);

    // POST /api/v1/monitor/stop - Stop monitoring a session
    let monitor_stop = api
        .and(warp::path("monitor"))
        .and(warp::path("stop"))
        .and(warp::path::end())
        .and(warp::post())
        .and(warp::body::json())
        .and(state_filter.clone())
        .and_then(stop_monitoring);

    // GET /api/v1/monitor/sessions - List active monitoring sessions
    let monitor_sessions = api
        .and(warp::path("monitor"))
        .and(warp::path("sessions"))
        .and(warp::path::end())
        .and(warp::get())
        .and(state_filter.clone())
        .and_then(list_monitoring_sessions);

    // POST /api/v1/monitor/keepalive - Keep a monitoring session alive
    let monitor_keepalive = api
        .and(warp::path("monitor"))
        .and(warp::path("keepalive"))
        .and(warp::path::end())
        .and(warp::post())
        .and(warp::body::json())
        .and(state_filter.clone())
        .and_then(keepalive_monitoring_session);

    // WebSocket endpoint for receiving logs
    let monitor_ws = warp::path("ws")
        .and(warp::path("monitor"))
        .and(warp::path::param()) // session_id
        .and(warp::ws())
        .and(state_filter.clone())
        .map(
            |session_id: String, ws: warp::ws::Ws, state: Arc<RwLock<ServerState>>| {
                ws.on_upgrade(move |socket| monitor_websocket_handler(socket, session_id, state))
            },
        );

    // GET /api/v1/board-types - Get available board types
    let board_types_route = api
        .and(warp::path("board-types"))
        .and(warp::path::end())
        .and(warp::get())
        .and(state_filter.clone())
        .and_then(get_board_types);

    // POST /api/v1/assign-board - Assign a board to a board type
    let assign_board_route = api
        .and(warp::path("assign-board"))
        .and(warp::path::end())
        .and(warp::post())
        .and(warp::body::json())
        .and(state_filter.clone())
        .and_then(assign_board_type);

    // DELETE /api/v1/assign-board/{unique_id} - Unassign a board
    let unassign_board_route = api
        .and(warp::path("assign-board"))
        .and(warp::path::param())
        .and(warp::path::end())
        .and(warp::delete())
        .and(state_filter.clone())
        .and_then(unassign_board);

    // Health check endpoint
    let health = warp::path("health").and(warp::get()).map(|| {
        warp::reply::json(&serde_json::json!({
            "status": "healthy",
            "version": env!("CARGO_PKG_VERSION")
        }))
    });

    // Static file serving - use embedded assets first, fallback to filesystem
    let static_files = warp::path("static")
        .and(warp::path::tail())
        .and_then(|path: warp::path::Tail| async move { serve_embedded_static(path) })
        .or({
            // Fallback to filesystem for development
            let asset_path = resolve_static_assets_path()?;
            println!(
                "📁 Static assets fallback directory: {}",
                asset_path.display()
            );
            warp::path("static").and(warp::fs::dir(asset_path))
        });

    println!("📦 Using embedded static assets (with filesystem fallback for development)");

    // Root redirect to dashboard
    let root_redirect = warp::path::end()
        .and(warp::get())
        .map(|| warp::redirect(warp::http::Uri::from_static("/static/index.html")));

    let routes = root_redirect
        .or(static_files)
        .or(boards)
        .or(board_info)
        .or(board_types_route)
        .or(assign_board_route)
        .or(unassign_board_route)
        .or(flash)
        .or(reset_board)
        .or(monitor_start)
        .or(monitor_stop)
        .or(monitor_sessions)
        .or(monitor_keepalive)
        .or(monitor_ws)
        .or(health)
        .with(warp::cors().allow_any_origin());
    // Removed warp::log middleware as it can cause shutdown delays

    // Start the server - show confirmation message
    if config.bind_address == "0.0.0.0" {
        println!("🚀 Server is now running and ready to accept connections!");
    } else {
        println!(
            "🚀 Server running at http://{}:{}",
            config.bind_address, config.port
        );
    }
    println!("🌐 Web Interface:");
    println!("   GET  /                    - Dashboard (redirects to /static/index.html)");
    println!("   GET  /static/index.html   - ESP32 board dashboard");
    println!("   GET  /static/flash.html   - Flash firmware interface");
    println!("📡 API endpoints:");
    println!("   GET    /api/v1/boards         - List all connected boards");
    println!("   GET    /api/v1/boards/{{id}}   - Get board information");
    println!("   GET    /api/v1/board-types    - Get available board types");
    println!("   POST   /api/v1/assign-board   - Assign a board to a board type");
    println!("   DELETE /api/v1/assign-board/{{id}} - Unassign a board");
    println!("   POST   /api/v1/flash          - Flash a board");
    println!("   POST   /api/v1/reset          - Reset a board");
    println!("   POST   /api/v1/monitor/start  - Start monitoring a board");
    println!("   POST   /api/v1/monitor/stop   - Stop monitoring a session");
    println!("   POST   /api/v1/monitor/keepalive - Keep a monitoring session alive");
    println!("   GET    /api/v1/monitor/sessions - List active monitoring sessions");
    println!("   WS     /ws/monitor/{{session_id}} - WebSocket for receiving logs");
    println!("   GET    /health                - Health check");
    println!();
    println!("Press Ctrl+C to stop the server");

    // Simplified shutdown signal handling - just Ctrl+C
    async fn shutdown_signal() {
        let _ = tokio::signal::ctrl_c().await;
        println!("\n🛑 Shutdown signal received. Stopping HTTP server...");
    }

    let addr: std::net::IpAddr = config
        .bind_address
        .parse()
        .unwrap_or_else(|_| std::net::IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED));

    let (_server, server_fut) = {
        let shutdown_notify = shutdown_notify.clone();
        let (addr, server) =
            warp::serve(routes).bind_with_graceful_shutdown((addr, config.port), async move {
                shutdown_signal().await;
                shutdown_notify.notify_waiters();
            });
        (addr, server)
    };

    // Create a task to handle forced shutdown after timeout
    let server_handle = tokio::spawn(server_fut);

    // Wait for Ctrl+C
    tokio::signal::ctrl_c()
        .await
        .expect("Failed to listen for ctrl-c");
    println!("\n🛑 Shutdown signal received. Stopping HTTP server...");

    // Now we apply aggressive timeouts during shutdown
    let server_shutdown_timeout = tokio::time::Duration::from_secs(3);
    let server_shutdown_result = tokio::time::timeout(server_shutdown_timeout, server_handle).await;

    match server_shutdown_result {
        Ok(Ok(_)) => println!("✅ HTTP server shut down gracefully"),
        Ok(Err(e)) => println!("⚠️ HTTP server task error: {}", e),
        Err(_) => {
            println!(
                "⚠️ HTTP server shutdown timed out after 3 seconds (likely due to hanging connections)"
            );
            println!("ℹ️ This is normal if browser tabs were open to the server");
        }
    }

    // Now shut down the scanner task with timeout
    let scanner_timeout = tokio::time::Duration::from_secs(1);
    let scanner_result = tokio::time::timeout(scanner_timeout, async {
        // Wait for scanner task to finish
        if let Err(e) = scanner_handle.await {
            eprintln!("⚠️ Scanner task join error: {}", e);
        }
    })
    .await;

    match scanner_result {
        Ok(_) => println!("✅ Scanner task stopped cleanly"),
        Err(_) => {
            println!("⚠️ Scanner task shutdown timed out after 1 second");
        }
    }

    println!("✅ Server stopped cleanly");

    Ok(())
}
