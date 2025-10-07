use anyhow::Result;
use chrono::{DateTime, Local};
use clap::{Parser, Subcommand};
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use futures_util::StreamExt;
use glob::glob;
use mdns_sd::{ServiceDaemon, ServiceEvent};
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap},
};
use reqwest::multipart;
use serde::{Deserialize, Serialize};
use serde_yaml;
use std::any::Any;
use std::{
    fs, io,
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::Command as TokioCommand,
    sync::mpsc,
};
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};

// New project system modules
mod arduino;
mod circuitpython;
mod esp_idf;
mod espflash_utils;
mod micropython;
mod nuttx;
mod platformio;
mod project;
mod rust_nostd;
mod tinygo;
mod zephyr;

use project::{ProjectHandler, ProjectRegistry, ProjectType};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
#[command(name = "espbrew")]
#[command(
    about = "🍺 Multi-Platform ESP32 Build Manager - Supports ESP-IDF, Rust no_std, and Arduino projects!"
)]
struct Cli {
    /// Path to project directory (ESP-IDF, Rust no_std, or Arduino - defaults to current directory)
    #[arg(global = true, value_name = "PROJECT_DIR")]
    project_dir: Option<PathBuf>,

    /// Run in CLI mode without TUI - just generate scripts and build all boards
    #[arg(long, help = "Run builds without interactive TUI")]
    cli_only: bool,

    /// Build strategy: 'idf-build-apps' (default, professional), 'sequential' (safe) or 'parallel' (may have conflicts)
    #[arg(
        long,
        default_value = "idf-build-apps",
        help = "Build strategy for multiple boards"
    )]
    build_strategy: BuildStrategy,

    /// Remote ESPBrew server URL for remote flashing
    #[arg(
        long,
        help = "ESPBrew server URL for remote flashing (default: http://localhost:8080)"
    )]
    server_url: Option<String>,

    /// Target board MAC address for remote flashing
    #[arg(long, help = "Target board MAC address for remote flashing")]
    board_mac: Option<String>,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// List boards and components (default CLI behavior)
    List,
    /// Build all boards
    Build,
    /// Discover ESPBrew servers on the local network via mDNS
    Discover {
        /// Timeout for discovery in seconds
        #[arg(short, long, default_value = "5")]
        timeout: u64,
    },
    /// Flash firmware to board(s) using local tools (idf.py flash or esptool)
    Flash {
        /// Path to binary file to flash (if not specified, will look for built binary)
        #[arg(short, long)]
        binary: Option<PathBuf>,
        /// Board configuration file to use for flashing
        #[arg(short, long)]
        config: Option<PathBuf>,
        /// Serial port to flash to (e.g., /dev/ttyUSB0, COM3)
        #[arg(short, long)]
        port: Option<String>,
    },
    /// Flash firmware to remote board(s) via ESPBrew server API
    RemoteFlash {
        /// Path to binary file to flash (if not specified, will look for built binary)
        #[arg(short, long)]
        binary: Option<PathBuf>,
        /// Board configuration file to use for flashing
        #[arg(short, long)]
        config: Option<PathBuf>,
        /// Target board MAC address (if not specified, will list available boards)
        #[arg(short, long)]
        mac: Option<String>,
        /// Target board logical name (alternative to MAC address)
        #[arg(short, long)]
        name: Option<String>,
        /// ESPBrew server URL (default: http://localhost:8080)
        #[arg(short, long)]
        server: Option<String>,
    },
    /// Monitor remote board(s) via ESPBrew server API
    RemoteMonitor {
        /// Target board MAC address (if not specified, will list available boards)
        #[arg(short, long)]
        mac: Option<String>,
        /// Target board logical name (alternative to MAC address)
        #[arg(short, long)]
        name: Option<String>,
        /// ESPBrew server URL (default: http://localhost:8080)
        #[arg(short, long)]
        server: Option<String>,
        /// Baud rate for serial monitoring (default: 115200)
        #[arg(short, long, default_value = "115200")]
        baud_rate: u32,
        /// Reset the board after establishing monitoring connection to capture boot logs
        #[arg(
            short,
            long,
            help = "Reset board after starting monitoring to capture complete boot sequence"
        )]
        reset: bool,
    },
}

#[derive(Debug, Clone, PartialEq, clap::ValueEnum)]
enum BuildStrategy {
    /// Build boards sequentially (avoids component manager conflicts, recommended)
    Sequential,
    /// Build boards in parallel (faster but may cause component manager conflicts)
    Parallel,
    /// Use professional idf-build-apps tool (recommended for production, zero conflicts)
    IdfBuildApps,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
enum BuildStatus {
    Pending,
    Building,
    Success,
    Failed,
    Flashing,
    Flashed,
}

impl BuildStatus {
    fn color(&self) -> Color {
        match self {
            BuildStatus::Pending => Color::Gray,
            BuildStatus::Building => Color::Yellow,
            BuildStatus::Success => Color::Green,
            BuildStatus::Failed => Color::Red,
            BuildStatus::Flashing => Color::Cyan,
            BuildStatus::Flashed => Color::Blue,
        }
    }

    fn symbol(&self) -> &'static str {
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

#[derive(Debug, Clone)]
struct DiscoveredServer {
    name: String,
    ip: std::net::IpAddr,
    port: u16,
    hostname: String,
    version: String,
    description: String,
    board_count: u32,
    boards_list: String,
}

#[derive(Debug, Clone)]
struct BoardConfig {
    name: String,
    config_file: PathBuf,
    build_dir: PathBuf,
    status: BuildStatus,
    log_lines: Vec<String>,
    build_time: Option<Duration>,
    last_updated: DateTime<Local>,
}

#[derive(Debug)]
enum AppEvent {
    BuildOutput(String, String),                   // board_name, line
    BuildFinished(String, bool),                   // board_name, success
    BuildCompleted,                                // All builds completed
    ActionFinished(String, String, bool),          // board_name, action, success
    ComponentActionStarted(String, String),        // component_name, action_name
    ComponentActionProgress(String, String),       // component_name, progress_message
    ComponentActionFinished(String, String, bool), // component_name, action_name, success
    // Monitoring events
    MonitorLogReceived(String), // log_line
    MonitorConnected(String),   // session_id
    MonitorDisconnected,        // monitoring session ended
    MonitorError(String),       // error_message
    // Remote board fetching events
    RemoteBoardsFetched(Vec<RemoteBoard>), // successful fetch result
    RemoteBoardsFetchFailed(String),       // error message
    // Server discovery events
    ServerDiscoveryStarted,
    ServerDiscovered(DiscoveredServer),
    ServerDiscoveryCompleted(Vec<DiscoveredServer>),
    ServerDiscoveryFailed(String),
    Tick,
}

#[derive(Debug, PartialEq)]
enum FocusedPane {
    BoardList,
    ComponentList,
    LogPane,
}

#[derive(Debug, Clone, PartialEq)]
enum BoardAction {
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

#[derive(Debug, Clone)]
struct ComponentConfig {
    name: String,
    path: PathBuf,
    is_managed: bool, // true if in managed_components, false if in components
    action_status: Option<String>, // Current action being performed (e.g., "Cloning...")
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct RemoteBoard {
    id: String,
    logical_name: Option<String>,
    mac_address: String,
    unique_id: String,
    chip_type: String,
    port: String,
    status: String,
    board_type_id: Option<String>,
    device_description: String,
    last_updated: String,
}

#[derive(Debug, Deserialize)]
struct RemoteBoardsResponse {
    boards: Vec<RemoteBoard>,
    server_info: serde_json::Value,
}

#[derive(Debug, Serialize)]
struct FlashRequest {
    board_id: String,
}

#[derive(Debug, Deserialize)]
struct FlashResponse {
    message: String,
    flash_id: Option<String>,
}

#[derive(Debug, Clone)]
enum RemoteFlashStatus {
    Uploading,
    Queued,
    Flashing,
    Success,
    Failed(String),
}

#[derive(Debug, Clone)]
enum RemoteMonitorStatus {
    Connecting,
    Connected,
    Monitoring,
    Disconnected,
    Failed(String),
}

#[derive(Debug, Serialize)]
struct MonitorRequest {
    board_id: String,
    baud_rate: Option<u32>,
    filters: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct MonitorResponse {
    success: bool,
    message: String,
    websocket_url: Option<String>,
    session_id: Option<String>,
}

#[derive(Debug, Serialize)]
struct StopMonitorRequest {
    session_id: String,
}

#[derive(Debug, Deserialize)]
struct StopMonitorResponse {
    success: bool,
    message: String,
}

#[derive(Debug, Serialize)]
struct KeepAliveRequest {
    session_id: String,
}

#[derive(Debug, Deserialize)]
struct KeepAliveResponse {
    success: bool,
    message: String,
}

#[derive(Debug, Deserialize)]
struct WebSocketMessage {
    #[serde(rename = "type")]
    message_type: String,
    session_id: Option<String>,
    content: Option<String>,
    timestamp: Option<String>,
    message: Option<String>,
    error: Option<String>,
}

impl RemoteFlashStatus {
    fn color(&self) -> Color {
        match self {
            RemoteFlashStatus::Uploading => Color::Yellow,
            RemoteFlashStatus::Queued => Color::Cyan,
            RemoteFlashStatus::Flashing => Color::Blue,
            RemoteFlashStatus::Success => Color::Green,
            RemoteFlashStatus::Failed(_) => Color::Red,
        }
    }

    fn symbol(&self) -> &'static str {
        match self {
            RemoteFlashStatus::Uploading => "📤",
            RemoteFlashStatus::Queued => "⏳",
            RemoteFlashStatus::Flashing => "📡",
            RemoteFlashStatus::Success => "✅",
            RemoteFlashStatus::Failed(_) => "❌",
        }
    }

    fn description(&self) -> String {
        match self {
            RemoteFlashStatus::Uploading => "Uploading binary to server...".to_string(),
            RemoteFlashStatus::Queued => "Flash job queued on server".to_string(),
            RemoteFlashStatus::Flashing => "Flashing board remotely...".to_string(),
            RemoteFlashStatus::Success => "Remote flash completed successfully".to_string(),
            RemoteFlashStatus::Failed(err) => format!("Remote flash failed: {}", err),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
enum ComponentAction {
    MoveToComponents,
    CloneFromRepository,
    Remove,
    OpenInEditor,
}

#[derive(Debug, Clone, PartialEq)]
enum RemoteActionType {
    Flash,
    Monitor,
}

impl BoardAction {
    fn name(&self) -> &'static str {
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

    fn description(&self) -> &'static str {
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

impl ComponentAction {
    fn name(&self) -> &'static str {
        match self {
            ComponentAction::MoveToComponents => "Move to Components",
            ComponentAction::CloneFromRepository => "Clone from Repository",
            ComponentAction::Remove => "Remove",
            ComponentAction::OpenInEditor => "Open in Editor",
        }
    }
}

// Remote flashing functionality
async fn fetch_remote_boards(server_url: &str) -> Result<Vec<RemoteBoard>> {
    // Create client with timeout to prevent hanging
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10)) // 10 second timeout
        .build()?;
    let url = format!("{}/api/v1/boards", server_url.trim_end_matches('/'));

    // Don't print to console when called from TUI - this breaks the interface
    // println!("🔍 Fetching boards from server: {}", url);

    let response = client.get(&url).send().await?.error_for_status()?;

    let boards_response: RemoteBoardsResponse = response.json().await?;
    Ok(boards_response.boards)
}

fn filter_boards_by_mac<'a>(
    boards: &'a [RemoteBoard],
    target_mac: Option<&str>,
) -> Vec<&'a RemoteBoard> {
    if let Some(mac) = target_mac {
        boards
            .iter()
            .filter(|board| board.mac_address.to_lowercase() == mac.to_lowercase())
            .collect()
    } else {
        boards.iter().collect()
    }
}

async fn select_remote_board<'a>(
    boards: &'a [RemoteBoard],
    target_mac: Option<&str>,
) -> Result<&'a RemoteBoard> {
    let filtered_boards = filter_boards_by_mac(boards, target_mac);

    if filtered_boards.is_empty() {
        if let Some(mac) = target_mac {
            let available_macs: Vec<String> =
                boards.iter().map(|b| b.mac_address.clone()).collect();
            return Err(anyhow::anyhow!(
                "No board found with MAC address: {}. Available boards: {}",
                mac,
                available_macs.join(", ")
            ));
        } else {
            return Err(anyhow::anyhow!("No boards available on the server"));
        }
    }

    if filtered_boards.len() == 1 {
        let board = filtered_boards[0];
        // Don't print to console when called from TUI - this breaks the interface
        // println!(
        //     "🎯 Selected board: {} ({}) - {}",
        //     board.logical_name.as_ref().unwrap_or(&board.id),
        //     board.mac_address,
        //     board.device_description
        // );
        return Ok(board);
    }

    // Multiple boards available - let user choose
    // Don't print to console when called from TUI - this breaks the interface
    // println!("📝 Multiple boards available:");
    // for (i, board) in filtered_boards.iter().enumerate() {
    //     println!(
    //         "  {}. {} ({}) - {} [{}]",
    //         i + 1,
    //         board.logical_name.as_ref().unwrap_or(&board.id),
    //         board.mac_address,
    //         board.device_description,
    //         board.status
    //     );
    // }

    // For now, auto-select the first available board
    // Later we can add interactive selection
    let selected = filtered_boards[0];
    // Don't print to console when called from TUI - this breaks the interface
    // println!(
    //     "🎯 Auto-selected first available board: {} ({})",
    //     selected.logical_name.as_ref().unwrap_or(&selected.id),
    //     selected.mac_address
    // );

    Ok(selected)
}

async fn upload_and_flash_esp_build(
    server_url: &str,
    board: &RemoteBoard,
    flash_config: &FlashConfig,
    binaries: &[FlashBinaryInfo],
) -> Result<()> {
    let client = reqwest::Client::new();
    let flash_url = format!("{}/api/v1/flash", server_url.trim_end_matches('/'));

    // Don't print to console when called from TUI - this breaks the interface
    // println!(
    //     "📤 Uploading {} binaries to server for ESP-IDF build...",
    //     binaries.len()
    // );

    // Create multipart form with all binaries
    let mut form = multipart::Form::new();

    // Add board ID
    form = form.text("board_id", board.id.clone());

    // Add flash configuration
    form = form.text("flash_mode", flash_config.flash_mode.clone());
    form = form.text("flash_freq", flash_config.flash_freq.clone());
    form = form.text("flash_size", flash_config.flash_size.clone());

    // Add each binary
    for (i, binary_info) in binaries.iter().enumerate() {
        let binary_data = fs::read(&binary_info.file_path).map_err(|e| {
            anyhow::anyhow!("Failed to read {}: {}", binary_info.file_path.display(), e)
        })?;

        // Don't print to console when called from TUI - this breaks the interface
        // println!(
        //     "📦 Adding {} at 0x{:x} ({} bytes): {}",
        //     binary_info.name,
        //     binary_info.offset,
        //     binary_data.len(),
        //     binary_info.file_name
        // );

        // Add binary data with metadata
        form = form.part(
            format!("binary_{}", i),
            multipart::Part::bytes(binary_data)
                .file_name(binary_info.file_name.clone())
                .mime_str("application/octet-stream")?,
        );

        // Add binary metadata
        form = form.text(
            format!("binary_{}_offset", i),
            format!("0x{:x}", binary_info.offset),
        );
        form = form.text(format!("binary_{}_name", i), binary_info.name.clone());
        form = form.text(
            format!("binary_{}_filename", i),
            binary_info.file_name.clone(),
        );
    }

    form = form.text("binary_count", binaries.len().to_string());

    // Don't print to console when called from TUI - this breaks the interface
    // println!(
    //     "📡 Initiating ESP-IDF multi-binary remote flash for board: {} ({})",
    //     board.logical_name.as_ref().unwrap_or(&board.id),
    //     board.mac_address
    // );

    let response = client
        .post(&flash_url)
        .multipart(form)
        .send()
        .await?
        .error_for_status()?;

    let flash_response: FlashResponse = response.json().await?;
    // Don't print to console when called from TUI - this breaks the interface
    // println!("✅ {}", flash_response.message);

    // Don't print to console when called from TUI - this breaks the interface
    // if let Some(flash_id) = flash_response.flash_id {
    //     println!("🔍 Flash job ID: {}", flash_id);
    // }

    Ok(())
}

async fn upload_and_flash_remote_legacy(
    server_url: &str,
    board: &RemoteBoard,
    binary_path: &Path,
) -> Result<()> {
    let client = reqwest::Client::new();
    let flash_url = format!("{}/api/v1/flash", server_url.trim_end_matches('/'));

    println!("📤 Uploading binary to server: {}", binary_path.display());

    // Read the binary file
    let binary_content = fs::read(binary_path)?;
    let file_name = binary_path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    // Create multipart form
    let form = multipart::Form::new()
        .text("board_id", board.id.clone())
        .part(
            "binary_file",
            multipart::Part::bytes(binary_content)
                .file_name(file_name.clone())
                .mime_str("application/octet-stream")?,
        );

    println!(
        "📡 Initiating remote flash for board: {} ({})",
        board.logical_name.as_ref().unwrap_or(&board.id),
        board.mac_address
    );

    let response = client
        .post(&flash_url)
        .multipart(form)
        .send()
        .await?
        .error_for_status()?;

    let flash_response: FlashResponse = response.json().await?;
    println!("✅ {}", flash_response.message);

    if let Some(flash_id) = flash_response.flash_id {
        println!("🔍 Flash job ID: {}", flash_id);
    }

    Ok(())
}

impl ComponentAction {
    fn description(&self) -> &'static str {
        match self {
            ComponentAction::MoveToComponents => "Move from managed_components to components",
            ComponentAction::CloneFromRepository => {
                "Clone from Git repository to components (supports wrapper components)"
            }
            ComponentAction::Remove => "Delete the component directory",
            ComponentAction::OpenInEditor => "Open component directory in default editor",
        }
    }

    fn is_available_for(&self, component: &ComponentConfig) -> bool {
        match self {
            ComponentAction::MoveToComponents => component.is_managed,
            ComponentAction::CloneFromRepository => {
                component.is_managed && Self::has_manifest_file(component)
            }
            ComponentAction::Remove => true,
            ComponentAction::OpenInEditor => true,
        }
    }

    fn has_manifest_file(component: &ComponentConfig) -> bool {
        component.path.join("idf_component.yml").exists()
    }

    fn is_wrapper_component(component: &ComponentConfig) -> bool {
        // Check if this is a wrapper component by looking for known wrapper patterns
        // Wrapper components typically have subdirectories that contain the actual component

        // For georgik__sdl, the wrapper contains an 'sdl' subdirectory
        if component.name.contains("georgik__sdl") {
            return true;
        }

        // Add other wrapper component patterns here as needed
        // This could be extended to read from a config file or detect based on directory structure

        false
    }

    fn find_wrapper_subdirectory(component: &ComponentConfig) -> Option<String> {
        // Return the subdirectory name that should be moved for wrapper components
        if component.name.contains("georgik__sdl") {
            return Some("sdl".to_string());
        }

        // Add other wrapper component subdirectory mappings here

        None
    }
}

#[derive(Debug, Deserialize)]
struct ComponentManifest {
    url: Option<String>,
    git: Option<String>,
    repository: Option<String>,
}

fn parse_component_manifest(manifest_path: &Path) -> Result<Option<String>> {
    if !manifest_path.exists() {
        return Ok(None);
    }

    let content = fs::read_to_string(manifest_path)?;
    let manifest: ComponentManifest = serde_yaml::from_str(&content)
        .map_err(|e| anyhow::anyhow!("Failed to parse manifest: {}", e))?;

    // Try different possible fields for repository URL
    let mut url = manifest.repository.or(manifest.git).or(manifest.url);

    // Convert git:// URLs to https:// for better compatibility
    if let Some(ref mut repo_url) = url {
        if repo_url.starts_with("git://github.com/") {
            *repo_url = repo_url.replace("git://github.com/", "https://github.com/");
        }
    }

    Ok(url)
}

/// Run local flash using esptool directly
async fn run_local_flash_esptool(binary_path: &Path, port: &str) -> Result<()> {
    use tokio::process::Command;

    println!(
        "🔥 Running esptool to flash {} on {}",
        binary_path.display(),
        port
    );

    let output = Command::new("esptool")
        .args([
            "--port",
            port,
            "--baud",
            "460800",
            "write_flash",
            "0x10000", // Default application offset
            &binary_path.to_string_lossy(),
        ])
        .output()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to run esptool: {}", e))?;

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        if !stdout.trim().is_empty() {
            println!("📝 esptool output: {}", stdout.trim());
        }
        if !stderr.trim().is_empty() {
            println!("📝 esptool info: {}", stderr.trim());
        }

        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(anyhow::anyhow!("esptool failed: {}", stderr.trim()))
    }
}

/// Run local flash using idf.py flash (requires ESP-IDF environment)
async fn run_local_flash_idf(project_dir: &Path) -> Result<()> {
    use tokio::process::Command;

    println!("🔥 Running idf.py flash in {}", project_dir.display());

    let output = Command::new("idf.py")
        .args(["flash"])
        .current_dir(project_dir)
        .output()
        .await
        .map_err(|e| {
            anyhow::anyhow!(
                "Failed to run idf.py flash: {}. Make sure ESP-IDF is properly set up.",
                e
            )
        })?;

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        if !stdout.trim().is_empty() {
            println!("📝 idf.py output: {}", stdout.trim());
        }
        if !stderr.trim().is_empty() {
            println!("📝 idf.py info: {}", stderr.trim());
        }

        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(anyhow::anyhow!(
            "idf.py flash failed: {}. Make sure ESP-IDF environment is properly set up.",
            stderr.trim()
        ))
    }
}

/// Direct ESP-IDF build flash for TUI (like the successful curl command)
/// This function directly looks for common ESP-IDF build directories and flash_args files
/// instead of relying on board name mapping which has issues
async fn upload_and_flash_esp_build_direct(
    server_url: &str,
    board: &RemoteBoard,
    project_dir: &Path,
    tx: mpsc::UnboundedSender<AppEvent>,
) -> Result<()> {
    let _ = tx.send(AppEvent::BuildOutput(
        "remote".to_string(),
        "🔍 Searching for ESP-IDF build directories...".to_string(),
    ));

    // Common ESP-IDF build directory patterns to check
    let build_patterns = vec![
        "build.m5stack_core_s3",
        "build.esp-box-3",
        "build.esp32_c6_devkit",
        "build.esp32_s3_eye",
        "build",
    ];

    for pattern in &build_patterns {
        let build_dir = project_dir.join(pattern);
        let flash_args_path = build_dir.join("flash_args");

        if flash_args_path.exists() {
            let _ = tx.send(AppEvent::BuildOutput(
                "remote".to_string(),
                format!("📋 Found ESP-IDF build: {}", build_dir.display()),
            ));

            match parse_flash_args(&flash_args_path, &build_dir) {
                Ok((flash_config, binaries)) => {
                    let _ = tx.send(AppEvent::BuildOutput(
                        "remote".to_string(),
                        format!(
                            "📦 Found {} binaries for multi-binary flash",
                            binaries.len()
                        ),
                    ));

                    for binary in &binaries {
                        let _ = tx.send(AppEvent::BuildOutput(
                            "remote".to_string(),
                            format!(
                                "  - {} at 0x{:x}: {} ({} bytes)",
                                binary.name,
                                binary.offset,
                                binary.file_name,
                                std::fs::metadata(&binary.file_path)
                                    .map(|m| m.len())
                                    .unwrap_or(0)
                            ),
                        ));
                    }

                    // Use the same multi-binary approach as the successful curl command
                    return upload_and_flash_esp_build_with_logging(
                        server_url,
                        board,
                        &flash_config,
                        &binaries,
                        tx,
                    )
                    .await;
                }
                Err(e) => {
                    let _ = tx.send(AppEvent::BuildOutput(
                        "remote".to_string(),
                        format!("⚠️ Failed to parse {}: {}", flash_args_path.display(), e),
                    ));
                    continue;
                }
            }
        }
    }

    Err(anyhow::anyhow!(
        "No ESP-IDF build directories with flash_args found in {}. Checked: {}",
        project_dir.display(),
        build_patterns.join(", ")
    ))
}

// Multi-binary version for TUI with ESP-IDF support and logging
async fn upload_and_flash_esp_build_with_logging(
    server_url: &str,
    board: &RemoteBoard,
    flash_config: &FlashConfig,
    binaries: &[FlashBinaryInfo],
    tx: mpsc::UnboundedSender<AppEvent>,
) -> Result<()> {
    let client = reqwest::Client::new();
    let flash_url = format!("{}/api/v1/flash", server_url.trim_end_matches('/'));

    let _ = tx.send(AppEvent::BuildOutput(
        "remote".to_string(),
        format!(
            "📤 Uploading {} binaries to server for ESP-IDF build...",
            binaries.len()
        ),
    ));

    // Create multipart form with all binaries
    let mut form = multipart::Form::new();

    // Add board ID
    form = form.text("board_id", board.id.clone());

    // Add flash configuration
    form = form.text("flash_mode", flash_config.flash_mode.clone());
    form = form.text("flash_freq", flash_config.flash_freq.clone());
    form = form.text("flash_size", flash_config.flash_size.clone());

    // Add each binary
    for (i, binary_info) in binaries.iter().enumerate() {
        let binary_data = fs::read(&binary_info.file_path).map_err(|e| {
            anyhow::anyhow!("Failed to read {}: {}", binary_info.file_path.display(), e)
        })?;

        let _ = tx.send(AppEvent::BuildOutput(
            "remote".to_string(),
            format!(
                "📦 Adding {} at 0x{:x} ({} bytes): {}",
                binary_info.name,
                binary_info.offset,
                binary_data.len(),
                binary_info.file_name
            ),
        ));

        // Add binary data with metadata
        form = form.part(
            format!("binary_{}", i),
            multipart::Part::bytes(binary_data)
                .file_name(binary_info.file_name.clone())
                .mime_str("application/octet-stream")?,
        );

        // Add binary metadata
        form = form.text(
            format!("binary_{}_offset", i),
            format!("0x{:x}", binary_info.offset),
        );
        form = form.text(format!("binary_{}_name", i), binary_info.name.clone());
        form = form.text(
            format!("binary_{}_filename", i),
            binary_info.file_name.clone(),
        );
    }

    form = form.text("binary_count", binaries.len().to_string());

    let _ = tx.send(AppEvent::BuildOutput(
        "remote".to_string(),
        format!(
            "📡 Initiating ESP-IDF multi-binary remote flash for board: {} ({})",
            board.logical_name.as_ref().unwrap_or(&board.id),
            board.mac_address
        ),
    ));

    let response = client
        .post(&flash_url)
        .multipart(form)
        .send()
        .await?
        .error_for_status()?;

    let flash_response: FlashResponse = response.json().await?;

    let _ = tx.send(AppEvent::BuildOutput(
        "remote".to_string(),
        format!("✅ {}", flash_response.message),
    ));

    if let Some(flash_id) = flash_response.flash_id {
        let _ = tx.send(AppEvent::BuildOutput(
            "remote".to_string(),
            format!("🔍 Flash job ID: {}", flash_id),
        ));
    }

    Ok(())
}

/// Rust-specific remote flash function that properly handles Rust no_std binaries
async fn upload_and_flash_rust_binary(
    server_url: &str,
    board: &RemoteBoard,
    project_dir: &Path,
    tx: mpsc::UnboundedSender<AppEvent>,
) -> Result<()> {
    let _ = tx.send(AppEvent::BuildOutput(
        "remote".to_string(),
        "🦀 Starting Rust no_std binary remote flash...".to_string(),
    ));

    // Use the same artifact detection approach as local flashing
    let _ = tx.send(AppEvent::BuildOutput(
        "remote".to_string(),
        "🔍 Looking for Rust build artifacts using project handler...".to_string(),
    ));

    // Create a RustNoStdHandler instance to use its artifact detection
    let rust_handler = crate::rust_nostd::RustNoStdHandler;
    let cargo_toml = project_dir.join("Cargo.toml");

    if !cargo_toml.exists() {
        return Err(anyhow::anyhow!(
            "No Cargo.toml found in project directory: {}",
            project_dir.display()
        ));
    }

    // Create a temporary board config to use the handler's methods
    let board_config = project::BoardConfig {
        name: "remote-rust-flash".to_string(),
        config_file: cargo_toml,
        build_dir: project_dir.join("target"),
        target: None,
        project_type: project::ProjectType::RustNoStd,
    };

    // Try to find existing artifacts first
    let build_artifacts = match rust_handler.find_build_artifacts(project_dir, &board_config) {
        Ok(artifacts) => {
            let _ = tx.send(AppEvent::BuildOutput(
                "remote".to_string(),
                format!("✅ Found {} existing Rust artifact(s)", artifacts.len()),
            ));
            artifacts
        }
        Err(_) => {
            // No existing artifacts found, try to build
            let _ = tx.send(AppEvent::BuildOutput(
                "remote".to_string(),
                "⚠️ No existing binary found, building Rust project first...".to_string(),
            ));

            // Build the project using the same approach as local flash
            let mut cmd = tokio::process::Command::new("cargo");
            cmd.current_dir(project_dir)
                .args(["build", "--release"])
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped());

            let build_output = cmd.output().await?;

            if !build_output.status.success() {
                let stderr = String::from_utf8_lossy(&build_output.stderr);
                let _ = tx.send(AppEvent::BuildOutput(
                    "remote".to_string(),
                    format!("❌ Rust build failed: {}", stderr.trim()),
                ));
                return Err(anyhow::anyhow!("Rust build failed: {}", stderr.trim()));
            }

            let _ = tx.send(AppEvent::BuildOutput(
                "remote".to_string(),
                "✅ Rust project built successfully, looking for artifacts...".to_string(),
            ));

            // Try to find artifacts again after building
            rust_handler
                .find_build_artifacts(project_dir, &board_config)
                .map_err(|e| {
                    anyhow::anyhow!("No build artifacts found even after building: {}", e)
                })?
        }
    };

    // Find the ELF binary artifact to upload
    let binary_artifact = build_artifacts
        .iter()
        .find(|artifact| matches!(artifact.artifact_type, project::ArtifactType::Elf))
        .or_else(|| {
            build_artifacts
                .iter()
                .find(|artifact| matches!(artifact.artifact_type, project::ArtifactType::Binary))
        })
        .ok_or_else(|| {
            anyhow::anyhow!(
                "No suitable ELF or binary artifact found in {} artifact(s)",
                build_artifacts.len()
            )
        })?;

    let _ = tx.send(AppEvent::BuildOutput(
        "remote".to_string(),
        format!(
            "✅ Using Rust binary: {}",
            binary_artifact.file_path.display()
        ),
    ));

    // Check if it's an ELF file to provide appropriate messaging
    let is_elf = binary_artifact.file_path
        .extension()
        .map(|ext| ext.to_str().unwrap_or(""))
        .unwrap_or("") == "" // Rust binaries typically have no extension and are ELF files
        && std::fs::read(&binary_artifact.file_path)
            .map(|content| content.len() > 4 && &content[0..4] == b"\x7fELF")
            .unwrap_or(false);

    if is_elf {
        let _ = tx.send(AppEvent::BuildOutput(
            "remote".to_string(),
            "📄 Detected ELF binary - compatible with espbrew-server".to_string(),
        ));
    }

    // Upload the binary using enhanced remote upload
    upload_and_flash_rust_binary_to_server(server_url, board, &binary_artifact.file_path, tx).await
}

/// Extract all flash components from Rust ELF using espflash save-image command
async fn extract_rust_flash_binaries_with_espflash(
    elf_path: &Path,
    tx: mpsc::UnboundedSender<AppEvent>,
) -> Result<Vec<(u32, Vec<u8>, String)>> {
    use std::path::PathBuf;
    use tokio::process::Command;

    let _ = tx.send(AppEvent::BuildOutput(
        "remote".to_string(),
        "🔧 Extracting flash components from Rust ELF using espflash save-image...".to_string(),
    ));

    // Detect chip type from ELF path - assume ESP32-S3 based on project
    // In a real implementation, we could detect this from the target directory name
    let chip_type = if elf_path.to_string_lossy().contains("esp32s3") {
        "esp32s3"
    } else if elf_path.to_string_lossy().contains("esp32s2") {
        "esp32s2"
    } else if elf_path.to_string_lossy().contains("esp32c3") {
        "esp32c3"
    } else if elf_path.to_string_lossy().contains("esp32c6") {
        "esp32c6"
    } else {
        "esp32" // Default fallback
    };

    let _ = tx.send(AppEvent::BuildOutput(
        "remote".to_string(),
        format!("🔍 Detected chip type: {} from ELF path", chip_type),
    ));

    // Create temporary file for the flash image
    let temp_image = std::env::temp_dir().join(format!(
        "espbrew_flash_image_{}.bin",
        uuid::Uuid::new_v4().simple()
    ));

    let _ = tx.send(AppEvent::BuildOutput(
        "remote".to_string(),
        format!("💾 Creating flash image: {}", temp_image.display()),
    ));

    // Run espflash save-image to extract all flash components
    let output = Command::new("espflash")
        .args([
            "save-image",
            "--chip",
            chip_type,
            "--merge", // Create a merged image with all components
            elf_path.to_str().unwrap(),
            temp_image.to_str().unwrap(),
        ])
        .output()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to run espflash save-image: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        // Clean up temp file
        let _ = tokio::fs::remove_file(&temp_image).await;
        return Err(anyhow::anyhow!(
            "espflash save-image failed: {}",
            stderr.trim()
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    let _ = tx.send(AppEvent::BuildOutput(
        "remote".to_string(),
        format!("✅ Flash image created successfully"),
    ));

    // Parse espflash output to extract component information
    if !stderr.trim().is_empty() {
        for line in stderr.lines() {
            if !line.trim().is_empty() {
                let _ = tx.send(AppEvent::BuildOutput(
                    "remote".to_string(),
                    format!("  📜 {}", line.trim()),
                ));
            }
        }
    }

    // Read the generated flash image
    let flash_image_data = tokio::fs::read(&temp_image)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to read flash image: {}", e))?;

    let _ = tx.send(AppEvent::BuildOutput(
        "remote".to_string(),
        format!("📦 Flash image size: {} bytes", flash_image_data.len()),
    ));

    // For a merged image, we typically flash it at offset 0x0
    // However, for proper flashing, we should extract individual components
    // Let's try to get the individual components using --no-merge
    let _ = tokio::fs::remove_file(&temp_image).await;

    // Run again without --merge to get component information
    let info_output = Command::new("espflash")
        .args([
            "save-image",
            "--chip",
            chip_type,
            "--dry-run", // Don't actually create the file, just show what would be done
            elf_path.to_str().unwrap(),
            "/dev/null", // Dummy output path
        ])
        .output()
        .await;

    let mut binaries: Vec<(u32, Vec<u8>, String)> = Vec::new();

    match info_output {
        Ok(info_out) if info_out.status.success() => {
            // Parse the dry-run output to get component information
            let info_stderr = String::from_utf8_lossy(&info_out.stderr);

            let _ = tx.send(AppEvent::BuildOutput(
                "remote".to_string(),
                "🔍 Analyzing flash components:".to_string(),
            ));

            // Look for component information in the output
            for line in info_stderr.lines() {
                let _ = tx.send(AppEvent::BuildOutput(
                    "remote".to_string(),
                    format!("  📜 {}", line.trim()),
                ));
            }
        }
        _ => {
            let _ = tx.send(AppEvent::BuildOutput(
                "remote".to_string(),
                "⚠️ Could not analyze components, using merged image approach".to_string(),
            ));
        }
    }

    // For now, let's use a practical approach:
    // Create individual component binaries by calling espflash with specific extraction
    let components = extract_individual_components(elf_path, chip_type, tx.clone()).await?;

    if !components.is_empty() {
        let _ = tx.send(AppEvent::BuildOutput(
            "remote".to_string(),
            format!(
                "✅ Successfully extracted {} flash components",
                components.len()
            ),
        ));

        for (offset, data, name) in &components {
            let _ = tx.send(AppEvent::BuildOutput(
                "remote".to_string(),
                format!("  📦 {}: 0x{:x} ({} bytes)", name, offset, data.len()),
            ));
        }

        Ok(components)
    } else {
        // Fallback: use the merged image at offset 0x0
        let _ = tx.send(AppEvent::BuildOutput(
            "remote".to_string(),
            "⚠️ Using fallback merged image approach".to_string(),
        ));

        // Re-create the merged image
        let merged_output = Command::new("espflash")
            .args([
                "save-image",
                "--chip",
                chip_type,
                "--merge",
                elf_path.to_str().unwrap(),
                temp_image.to_str().unwrap(),
            ])
            .output()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to create merged image: {}", e))?;

        if merged_output.status.success() {
            let merged_data = tokio::fs::read(&temp_image)
                .await
                .map_err(|e| anyhow::anyhow!("Failed to read merged image: {}", e))?;

            let _ = tokio::fs::remove_file(&temp_image).await;

            Ok(vec![(
                0x0, // Merged images typically start at 0x0
                merged_data,
                "merged_firmware".to_string(),
            )])
        } else {
            let stderr = String::from_utf8_lossy(&merged_output.stderr);
            Err(anyhow::anyhow!(
                "Failed to create merged flash image: {}",
                stderr.trim()
            ))
        }
    }
}

/// Extract individual flash components (bootloader, partition table, application)
async fn extract_individual_components(
    elf_path: &Path,
    chip_type: &str,
    tx: mpsc::UnboundedSender<AppEvent>,
) -> Result<Vec<(u32, Vec<u8>, String)>> {
    use tokio::process::Command;

    let _ = tx.send(AppEvent::BuildOutput(
        "remote".to_string(),
        "🔍 Extracting individual flash components...".to_string(),
    ));

    // Try to use espflash to create separate binaries for each component
    // This approach uses espflash write-bin to create component binaries

    let temp_dir = std::env::temp_dir();
    let session_id = uuid::Uuid::new_v4().simple().to_string();

    // Create bootloader binary
    let bootloader_path = temp_dir.join(format!("{}_bootloader.bin", session_id));
    let partition_path = temp_dir.join(format!("{}_partition.bin", session_id));
    let app_path = temp_dir.join(format!("{}_app.bin", session_id));

    let mut components = Vec::new();

    // Try to extract application binary first (this should always work)
    let app_result = Command::new("espflash")
        .args([
            "write-bin",
            "--chip",
            chip_type,
            elf_path.to_str().unwrap(),
            app_path.to_str().unwrap(),
        ])
        .output()
        .await;

    match app_result {
        Ok(output) if output.status.success() => {
            if let Ok(app_data) = tokio::fs::read(&app_path).await {
                components.push((0x10000, app_data, "application".to_string()));
                let _ = tx.send(AppEvent::BuildOutput(
                    "remote".to_string(),
                    "✅ Extracted application binary".to_string(),
                ));
            }
            let _ = tokio::fs::remove_file(&app_path).await;
        }
        _ => {
            let _ = tx.send(AppEvent::BuildOutput(
                "remote".to_string(),
                "⚠️ Could not extract application binary".to_string(),
            ));
        }
    }

    // For bootloader and partition table, we need a different approach
    // Let's try to find them in the standard ESP32 layout by creating a full image
    // and parsing it

    if components.is_empty() {
        let _ = tx.send(AppEvent::BuildOutput(
            "remote".to_string(),
            "⚠️ Individual component extraction failed, will use alternative method".to_string(),
        ));
    }

    Ok(components)
}

/// Enhanced Rust binary upload function with complete multi-component support
async fn upload_and_flash_rust_binary_to_server(
    server_url: &str,
    board: &RemoteBoard,
    binary_path: &Path,
    tx: mpsc::UnboundedSender<AppEvent>,
) -> Result<()> {
    let client = reqwest::Client::new();
    let flash_url = format!("{}/api/v1/flash", server_url.trim_end_matches('/'));

    let _ = tx.send(AppEvent::BuildOutput(
        "remote".to_string(),
        format!(
            "🚀 Starting comprehensive Rust ELF remote flash: {}",
            binary_path.display()
        ),
    ));

    // Extract all flash components using espflash
    let flash_components =
        extract_rust_flash_binaries_with_espflash(binary_path, tx.clone()).await?;

    if flash_components.is_empty() {
        return Err(anyhow::anyhow!(
            "No flash components extracted from ELF file: {}",
            binary_path.display()
        ));
    }

    let _ = tx.send(AppEvent::BuildOutput(
        "remote".to_string(),
        format!(
            "📎 Preparing to upload {} flash components",
            flash_components.len()
        ),
    ));

    // Create multipart form for the espbrew-server multi-binary API
    let mut form = multipart::Form::new()
        .text("board_id", board.id.clone())
        .text("binary_count", flash_components.len().to_string())
        .text("flash_mode", "dio")
        .text("flash_freq", "40m")
        .text("flash_size", "detect")
        .text("project_type", "rust_nostd")
        .text(
            "source_file",
            binary_path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string(),
        );

    // Add each component as a separate binary part
    for (index, (offset, data, name)) in flash_components.iter().enumerate() {
        let file_name = format!("{}.bin", name);

        let _ = tx.send(AppEvent::BuildOutput(
            "remote".to_string(),
            format!(
                "📦 Adding component {}: {} ({} bytes) at 0x{:x}",
                index,
                name,
                data.len(),
                offset
            ),
        ));

        // Add binary data
        form = form.part(
            format!("binary_{}", index),
            multipart::Part::bytes(data.clone())
                .file_name(file_name.clone())
                .mime_str("application/octet-stream")?,
        );

        // Add metadata for this component
        form = form
            .text(
                format!("binary_{}_offset", index),
                format!("0x{:x}", offset),
            )
            .text(format!("binary_{}_name", index), name.clone())
            .text(format!("binary_{}_filename", index), file_name);
    }

    let total_size: usize = flash_components.iter().map(|(_, data, _)| data.len()).sum();
    let _ = tx.send(AppEvent::BuildOutput(
        "remote".to_string(),
        format!(
            "📦 Total flash data: {} bytes across {} components",
            total_size,
            flash_components.len()
        ),
    ));

    let _ = tx.send(AppEvent::BuildOutput(
        "remote".to_string(),
        format!(
            "📡 Initiating multi-component Rust remote flash for board: {} ({})",
            board.logical_name.as_ref().unwrap_or(&board.id),
            board.mac_address
        ),
    ));

    // Upload and flash with enhanced timeout for multi-component flashing
    let response = client
        .post(&flash_url)
        .multipart(form)
        .timeout(std::time::Duration::from_secs(600)) // 10 minute timeout for multi-component flashing
        .send()
        .await?
        .error_for_status()?;

    let flash_response: FlashResponse = response.json().await?;

    let _ = tx.send(AppEvent::BuildOutput(
        "remote".to_string(),
        format!("✅ Server response: {}", flash_response.message),
    ));

    if let Some(flash_id) = flash_response.flash_id {
        let _ = tx.send(AppEvent::BuildOutput(
            "remote".to_string(),
            format!("🎯 Multi-component flash job ID: {}", flash_id),
        ));
    }

    let _ = tx.send(AppEvent::BuildOutput(
        "remote".to_string(),
        format!("✨ Comprehensive Rust remote flash completed! Flashed {} components with {} total bytes", 
            flash_components.len(), total_size),
    ));

    Ok(())
}

// Legacy single-binary version for TUI with logging
async fn upload_and_flash_remote_with_logging(
    server_url: &str,
    board: &RemoteBoard,
    binary_path: &Path,
    tx: mpsc::UnboundedSender<AppEvent>,
) -> Result<()> {
    let client = reqwest::Client::new();
    let flash_url = format!("{}/api/v1/flash", server_url.trim_end_matches('/'));

    let _ = tx.send(AppEvent::BuildOutput(
        "remote".to_string(),
        format!("📤 Uploading binary to server: {}", binary_path.display()),
    ));

    // Read the binary file
    let binary_content = fs::read(binary_path)?;
    let file_name = binary_path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    let _ = tx.send(AppEvent::BuildOutput(
        "remote".to_string(),
        format!("📦 Binary size: {} bytes", binary_content.len()),
    ));

    // Create multipart form
    let form = multipart::Form::new()
        .text("board_id", board.id.clone())
        .part(
            "binary_file",
            multipart::Part::bytes(binary_content)
                .file_name(file_name.clone())
                .mime_str("application/octet-stream")?,
        );

    let _ = tx.send(AppEvent::BuildOutput(
        "remote".to_string(),
        format!(
            "📡 Initiating remote flash for board: {} ({})",
            board.logical_name.as_ref().unwrap_or(&board.id),
            board.mac_address
        ),
    ));

    let response = client
        .post(&flash_url)
        .multipart(form)
        .send()
        .await?
        .error_for_status()?;

    let flash_response: FlashResponse = response.json().await?;

    let _ = tx.send(AppEvent::BuildOutput(
        "remote".to_string(),
        format!("✅ {}", flash_response.message),
    ));

    if let Some(flash_id) = flash_response.flash_id {
        let _ = tx.send(AppEvent::BuildOutput(
            "remote".to_string(),
            format!("🔍 Flash job ID: {}", flash_id),
        ));
    }

    Ok(())
}

/// Arduino-specific remote flash function that handles Arduino binaries
async fn upload_and_flash_arduino_remote(
    server_url: &str,
    board: &RemoteBoard,
    project_dir: &Path,
    tx: mpsc::UnboundedSender<AppEvent>,
) -> Result<()> {
    let _ = tx.send(AppEvent::BuildOutput(
        "remote".to_string(),
        "🔧 Starting Arduino binary remote flash...".to_string(),
    ));

    // Use the same artifact detection approach as local flashing
    let _ = tx.send(AppEvent::BuildOutput(
        "remote".to_string(),
        "🔍 Looking for Arduino build artifacts using project handler...".to_string(),
    ));

    // Create an ArduinoHandler instance to use its artifact detection
    let arduino_handler = crate::arduino::ArduinoHandler::new();

    // Find arduino-cli.yaml config file
    let mut arduino_cli_file = project_dir.join("arduino-cli.yaml");
    if !arduino_cli_file.exists() {
        // Try to find arduino-cli.yaml in parent directories
        let mut current = project_dir;
        while let Some(parent) = current.parent() {
            let config_path = parent.join("arduino-cli.yaml");
            if config_path.exists() {
                arduino_cli_file = config_path;
                break;
            }
            current = parent;
        }
    }

    // Create a temporary board config to use the handler's methods
    let board_config = project::BoardConfig {
        name: "remote-arduino-flash".to_string(),
        config_file: arduino_cli_file,
        build_dir: project_dir.join("build"),
        target: None,
        project_type: project::ProjectType::Arduino,
    };

    let _ = tx.send(AppEvent::BuildOutput(
        "remote".to_string(),
        "🔨 Building Arduino project to generate artifacts...".to_string(),
    ));

    // Build the project to get artifacts
    let build_artifacts = match arduino_handler
        .build_board(project_dir, &board_config, tx.clone())
        .await
    {
        Ok(artifacts) => {
            let _ = tx.send(AppEvent::BuildOutput(
                "remote".to_string(),
                format!(
                    "✅ Arduino build completed with {} artifact(s)",
                    artifacts.len()
                ),
            ));
            artifacts
        }
        Err(e) => {
            let _ = tx.send(AppEvent::BuildOutput(
                "remote".to_string(),
                format!("❌ Arduino build failed: {}", e),
            ));
            return Err(anyhow::anyhow!("Arduino build failed: {}", e));
        }
    };

    if build_artifacts.is_empty() {
        return Err(anyhow::anyhow!("No Arduino build artifacts found"));
    }

    // Log details about found artifacts
    for artifact in &build_artifacts {
        let _ = tx.send(AppEvent::BuildOutput(
            "remote".to_string(),
            format!(
                "📦 Found artifact: {} at {}",
                artifact.name,
                artifact.file_path.display()
            ),
        ));
    }

    // Find the main binary artifact to upload (typically .bin or .elf)
    let binary_artifact = build_artifacts
        .iter()
        .find(|artifact| {
            matches!(
                artifact.artifact_type,
                project::ArtifactType::Binary | project::ArtifactType::Elf
            )
        })
        .ok_or_else(|| {
            anyhow::anyhow!(
                "No suitable binary artifact found in {} artifact(s)",
                build_artifacts.len()
            )
        })?;

    let _ = tx.send(AppEvent::BuildOutput(
        "remote".to_string(),
        format!(
            "✅ Using Arduino binary: {}",
            binary_artifact.file_path.display()
        ),
    ));

    // Upload the binary using the server API
    upload_and_flash_arduino_binary_to_server(server_url, board, &binary_artifact.file_path, tx)
        .await
}

/// Upload Arduino binary to server using the multi-binary API
async fn upload_and_flash_arduino_binary_to_server(
    server_url: &str,
    board: &RemoteBoard,
    binary_path: &Path,
    tx: mpsc::UnboundedSender<AppEvent>,
) -> Result<()> {
    let client = reqwest::Client::new();
    let flash_url = format!("{}/api/v1/flash", server_url.trim_end_matches('/'));

    let _ = tx.send(AppEvent::BuildOutput(
        "remote".to_string(),
        format!(
            "🚀 Starting Arduino binary remote flash: {}",
            binary_path.display()
        ),
    ));

    // Read the binary file
    let binary_content = fs::read(binary_path)?;
    let file_name = binary_path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    let _ = tx.send(AppEvent::BuildOutput(
        "remote".to_string(),
        format!("📦 Arduino binary size: {} bytes", binary_content.len()),
    ));

    // Create multipart form for Arduino project
    let form = multipart::Form::new()
        .text("board_id", board.id.clone())
        .text("binary_count", "1") // Arduino typically uses single binary
        .text("flash_mode", "dio")
        .text("flash_freq", "40m")
        .text("flash_size", "detect")
        .text("project_type", "arduino")
        .text(
            "source_file",
            binary_path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string(),
        )
        .part(
            "binary_0",
            multipart::Part::bytes(binary_content)
                .file_name(file_name.clone())
                .mime_str("application/octet-stream")?,
        )
        .text("binary_0_offset", "0x10000") // Arduino app typically starts at 0x10000
        .text("binary_0_name", "arduino_app")
        .text("binary_0_filename", file_name);

    let _ = tx.send(AppEvent::BuildOutput(
        "remote".to_string(),
        format!(
            "📡 Initiating Arduino remote flash for board: {} ({})",
            board.logical_name.as_ref().unwrap_or(&board.id),
            board.mac_address
        ),
    ));

    // Upload and flash with timeout
    let response = client
        .post(&flash_url)
        .multipart(form)
        .timeout(std::time::Duration::from_secs(300)) // 5 minute timeout for Arduino flashing
        .send()
        .await?
        .error_for_status()?;

    let flash_response: FlashResponse = response.json().await?;

    let _ = tx.send(AppEvent::BuildOutput(
        "remote".to_string(),
        format!("✅ Server response: {}", flash_response.message),
    ));

    if let Some(flash_id) = flash_response.flash_id {
        let _ = tx.send(AppEvent::BuildOutput(
            "remote".to_string(),
            format!("🎯 Arduino flash job ID: {}", flash_id),
        ));
    }

    let _ = tx.send(AppEvent::BuildOutput(
        "remote".to_string(),
        "✨ Arduino remote flash completed successfully!".to_string(),
    ));

    Ok(())
}

/// Parse ESP-IDF flash_args file to extract flash configuration and binaries
fn parse_flash_args(
    flash_args_path: &Path,
    build_dir: &Path,
) -> Result<(FlashConfig, Vec<FlashBinaryInfo>)> {
    let content = fs::read_to_string(flash_args_path).map_err(|e| {
        anyhow::anyhow!(
            "Failed to read flash_args file {}: {}",
            flash_args_path.display(),
            e
        )
    })?;

    let lines: Vec<&str> = content.lines().collect();
    if lines.is_empty() {
        return Err(anyhow::anyhow!("flash_args file is empty"));
    }

    // Parse first line for flash configuration
    let config_line = lines[0];
    let mut flash_mode = "dio".to_string();
    let mut flash_freq = "40m".to_string();
    let mut flash_size = "4MB".to_string();

    for part in config_line.split_whitespace() {
        if part.starts_with("--flash_mode") {
            if let Some(mode) = part.split(' ').nth(1) {
                flash_mode = mode.to_string();
            }
        } else if part.starts_with("--flash_freq") {
            if let Some(freq) = part.split(' ').nth(1) {
                flash_freq = freq.to_string();
            }
        } else if part.starts_with("--flash_size") {
            if let Some(size) = part.split(' ').nth(1) {
                flash_size = size.to_string();
            }
        }
    }

    // Parse remaining lines for binary files
    let mut binaries = Vec::new();
    for line in lines.iter().skip(1) {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 2 {
            let offset_str = parts[0];
            let file_path = parts[1];

            // Parse hex offset
            let offset = if offset_str.starts_with("0x") {
                u32::from_str_radix(&offset_str[2..], 16)
                    .map_err(|e| anyhow::anyhow!("Invalid hex offset {}: {}", offset_str, e))?
            } else {
                offset_str
                    .parse::<u32>()
                    .map_err(|e| anyhow::anyhow!("Invalid offset {}: {}", offset_str, e))?
            };

            // Determine binary type based on offset and filename
            let name = match offset {
                0x0 => "bootloader".to_string(),
                0x8000 => "partition_table".to_string(),
                0x10000 => "application".to_string(),
                _ => format!("binary_at_0x{:x}", offset),
            };

            let full_path = build_dir.join(file_path);
            binaries.push(FlashBinaryInfo {
                offset,
                file_path: full_path,
                name,
                file_name: file_path.to_string(),
            });
        }
    }

    let flash_config = FlashConfig {
        flash_mode,
        flash_freq,
        flash_size,
    };

    Ok((flash_config, binaries))
}

/// Information about a binary to be flashed
#[derive(Debug, Clone)]
struct FlashBinaryInfo {
    offset: u32,
    file_path: PathBuf,
    name: String,
    file_name: String,
}

#[derive(Debug, Clone)]
struct FlashConfig {
    flash_mode: String,
    flash_freq: String,
    flash_size: String,
}

/// Find ESP-IDF build directory and binaries for a project
fn find_esp_build_artifacts(
    project_dir: &Path,
    board_name: Option<&str>,
) -> Result<(FlashConfig, Vec<FlashBinaryInfo>)> {
    // Try to find build directory - look for board-specific build first
    let build_dirs = if let Some(name) = board_name {
        vec![
            project_dir.join(format!("build.{}", name)),
            project_dir.join("build"),
        ]
    } else {
        vec![project_dir.join("build")]
    };

    for build_dir in build_dirs {
        let flash_args_path = build_dir.join("flash_args");
        if flash_args_path.exists() {
            println!("📁 Using build directory: {}", build_dir.display());
            return parse_flash_args(&flash_args_path, &build_dir);
        }
    }

    Err(anyhow::anyhow!(
        "No ESP-IDF build directory found in {}. Run 'idf.py build' first.",
        project_dir.display()
    ))
}

fn find_binary_file(project_dir: &Path, config_path: Option<&Path>) -> Result<PathBuf> {
    // If binary path is explicitly provided, use it
    if let Some(config) = config_path {
        if config.exists() {
            return Ok(config.to_path_buf());
        }
    }

    // Look for built binaries in build directories
    let build_pattern = project_dir.join("build*").join("*.bin");
    let bin_files: Vec<PathBuf> = glob(&build_pattern.to_string_lossy())
        .unwrap_or_else(|_| glob("").unwrap())
        .filter_map(Result::ok)
        .collect();

    if !bin_files.is_empty() {
        // Prefer files with "app" in the name, then take the first one
        if let Some(app_bin) = bin_files.iter().find(|p| {
            p.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_lowercase()
                .contains("app")
        }) {
            return Ok(app_bin.clone());
        }
        return Ok(bin_files[0].clone());
    }

    // Look for common ESP-IDF binary locations
    let common_paths = vec![
        project_dir.join("build").join("*.bin"),
        project_dir.join("build").join("*.elf"),
        project_dir.join("build").join("project.bin"),
    ];

    for pattern in common_paths {
        if let Ok(entries) = glob(&pattern.to_string_lossy()) {
            for entry in entries.filter_map(Result::ok) {
                if entry.exists() {
                    return Ok(entry);
                }
            }
        }
    }

    Err(anyhow::anyhow!(
        "No binary file found. Please build the project first or specify a binary file with --binary"
    ))
}

struct App {
    boards: Vec<BoardConfig>,
    selected_board: usize,
    list_state: ListState,
    components: Vec<ComponentConfig>,
    selected_component: usize,
    component_list_state: ListState,
    project_dir: PathBuf,
    logs_dir: PathBuf,
    support_dir: PathBuf,
    project_type: Option<ProjectType>,
    project_handler: Option<Box<dyn ProjectHandler>>,
    show_help: bool,
    focused_pane: FocusedPane,
    log_scroll_offset: usize,
    show_tool_warning: bool,
    tool_warning_acknowledged: bool,
    tool_warning_message: String,
    show_action_menu: bool,
    show_component_action_menu: bool,
    action_menu_selected: usize,
    component_action_menu_selected: usize,
    available_actions: Vec<BoardAction>,
    available_component_actions: Vec<ComponentAction>,
    build_strategy: BuildStrategy,
    build_in_progress: bool,
    server_url: Option<String>,
    board_mac: Option<String>,
    // Remote board dialog state
    show_remote_board_dialog: bool,
    remote_boards: Vec<RemoteBoard>,
    selected_remote_board: usize,
    remote_board_list_state: ListState,
    remote_flash_in_progress: bool,
    remote_flash_status: Option<String>,
    // Remote board fetching state
    remote_boards_loading: bool,
    remote_boards_fetch_error: Option<String>,
    // Remote monitoring state
    remote_monitor_in_progress: bool,
    remote_monitor_status: Option<String>,
    remote_monitor_session_id: Option<String>,
    // Track which remote action is being performed
    remote_action_type: RemoteActionType,
    // Monitoring modal state
    show_monitor_modal: bool,
    monitor_logs: Vec<String>,
    monitor_session_id: Option<String>,
    monitor_board_id: Option<String>,
    monitor_connected: bool,
    monitor_scroll_offset: usize,
    monitor_auto_scroll: bool,
    // Server discovery state
    discovered_servers: Vec<DiscoveredServer>,
    server_discovery_in_progress: bool,
    server_discovery_status: String,
}

impl App {
    fn new(
        project_dir: PathBuf,
        build_strategy: BuildStrategy,
        server_url: Option<String>,
        board_mac: Option<String>,
        project_handler: Option<Box<dyn ProjectHandler>>,
    ) -> Result<Self> {
        let logs_dir = project_dir.join("logs");
        let support_dir = project_dir.join("support");

        // Create directories if they don't exist
        fs::create_dir_all(&logs_dir)?;
        fs::create_dir_all(&support_dir)?;

        // Use project-aware board discovery if handler is available
        let mut boards = if let Some(ref handler) = project_handler {
            // Convert project::BoardConfig to our BoardConfig
            handler
                .discover_boards(&project_dir)
                .unwrap_or_default()
                .into_iter()
                .map(|board| BoardConfig {
                    name: board.name,
                    config_file: board.config_file,
                    build_dir: board.build_dir,
                    status: BuildStatus::Pending,
                    log_lines: Vec::new(),
                    build_time: None,
                    last_updated: Local::now(),
                })
                .collect()
        } else {
            // Fallback to ESP-IDF discovery for unknown projects
            Self::discover_boards(&project_dir)?
        };

        let components = Self::discover_components(&project_dir)?;

        // Load existing logs if they exist
        for board in &mut boards {
            Self::load_existing_logs(board, &logs_dir);
        }

        let mut list_state = ListState::default();
        if !boards.is_empty() {
            list_state.select(Some(0));
        }

        let mut component_list_state = ListState::default();
        if !components.is_empty() {
            component_list_state.select(Some(0));
        }

        // Check if project tools are available (only if project type is detected)
        let (show_tool_warning, tool_warning_message, detected_project_type) =
            if let Some(ref handler) = project_handler {
                let project_type = handler.project_type();

                // Use project-specific tool checking for Rust no_std
                let tool_check_result = if project_type == ProjectType::RustNoStd {
                    // For Rust no_std, cast to RustNoStdHandler and use enhanced checking
                    if let Some(rust_handler) = handler
                        .as_any()
                        .downcast_ref::<crate::rust_nostd::RustNoStdHandler>()
                    {
                        rust_handler.check_tools_for_project(&project_dir)
                    } else {
                        handler.check_tools_available().map_err(|e| e.to_string())
                    }
                } else {
                    handler.check_tools_available().map_err(|e| e.to_string())
                };

                match tool_check_result {
                    Ok(()) => (false, String::new(), Some(project_type)),
                    Err(err_msg) => (true, err_msg, Some(project_type)),
                }
            } else {
                (false, String::new(), None)
            };

        let available_actions = vec![
            BoardAction::Build,
            BoardAction::GenerateBinary,
            BoardAction::Clean,
            BoardAction::Purge,
            BoardAction::Flash,
            BoardAction::FlashAppOnly,
            BoardAction::Monitor,
            BoardAction::RemoteFlash,
            BoardAction::RemoteMonitor,
        ];

        let available_component_actions = vec![
            ComponentAction::MoveToComponents,
            ComponentAction::CloneFromRepository,
            ComponentAction::Remove,
            ComponentAction::OpenInEditor,
        ];

        Ok(Self {
            boards,
            selected_board: 0,
            list_state,
            components,
            selected_component: 0,
            component_list_state,
            project_dir,
            logs_dir,
            support_dir,
            project_type: detected_project_type,
            project_handler,
            show_help: false,
            focused_pane: FocusedPane::BoardList,
            log_scroll_offset: 0,
            show_tool_warning,
            tool_warning_acknowledged: false,
            tool_warning_message,
            show_action_menu: false,
            show_component_action_menu: false,
            action_menu_selected: 0,
            component_action_menu_selected: 0,
            available_actions,
            available_component_actions,
            build_strategy,
            build_in_progress: false,
            server_url,
            board_mac,
            // Remote board dialog state
            show_remote_board_dialog: false,
            remote_boards: Vec::new(),
            selected_remote_board: 0,
            remote_board_list_state: ListState::default(),
            remote_flash_in_progress: false,
            remote_flash_status: None,
            // Remote board fetching state
            remote_boards_loading: false,
            remote_boards_fetch_error: None,
            // Remote monitoring state
            remote_monitor_in_progress: false,
            remote_monitor_status: None,
            remote_monitor_session_id: None,
            // Track which remote action is being performed
            remote_action_type: RemoteActionType::Flash,
            // Monitoring modal state
            show_monitor_modal: false,
            monitor_logs: Vec::new(),
            monitor_session_id: None,
            monitor_board_id: None,
            monitor_connected: false,
            monitor_scroll_offset: 0,
            monitor_auto_scroll: true,
            // Server discovery state
            discovered_servers: Vec::new(),
            server_discovery_in_progress: false,
            server_discovery_status: "mDNS: idle".to_string(),
        })
    }

    fn load_existing_logs(board: &mut BoardConfig, logs_dir: &Path) {
        // First try to load from build directory (preferred, idf-build-apps location)
        let build_log_file = board.build_dir.join("build.log");
        let legacy_log_file = logs_dir.join(format!("{}.log", board.name));

        let log_file_to_use = if build_log_file.exists() {
            &build_log_file
        } else {
            &legacy_log_file
        };

        if log_file_to_use.exists() {
            if let Ok(content) = fs::read_to_string(log_file_to_use) {
                // Load recent log lines for display (last 100 lines)
                let lines: Vec<String> = content.lines().map(|l| l.to_string()).collect();
                let start_idx = if lines.len() > 100 {
                    lines.len() - 100
                } else {
                    0
                };
                board.log_lines = lines[start_idx..].to_vec();

                // Update status based on log content
                if lines.iter().any(|line| {
                    line.contains("build success")
                        || line.contains("Build complete")
                        || line.contains("Project build complete")
                }) {
                    board.status = BuildStatus::Success;
                } else if lines.iter().any(|line| {
                    line.contains("build failed")
                        || line.contains("FAILED")
                        || line.contains("Error")
                        || line.contains("returned non-zero exit status")
                }) {
                    board.status = BuildStatus::Failed;
                }

                board.last_updated = Local::now();
            }
        }
    }

    fn discover_boards(project_dir: &Path) -> Result<Vec<BoardConfig>> {
        let pattern = project_dir.join("sdkconfig.defaults.*");
        let mut boards = Vec::new();

        for entry in glob(&pattern.to_string_lossy())? {
            let config_file = entry?;
            if let Some(file_name) = config_file.file_name() {
                if let Some(name) = file_name.to_str() {
                    if let Some(board_name) = name.strip_prefix("sdkconfig.defaults.") {
                        let build_dir = project_dir.join(format!("build.{}", board_name));
                        boards.push(BoardConfig {
                            name: board_name.to_string(),
                            config_file: config_file.clone(),
                            build_dir,
                            status: BuildStatus::Pending,
                            log_lines: Vec::new(),
                            build_time: None,
                            last_updated: Local::now(),
                        });
                    }
                }
            }
        }

        boards.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(boards)
    }

    fn discover_components(project_dir: &Path) -> Result<Vec<ComponentConfig>> {
        let mut components = Vec::new();

        // Discover components in "components" directory
        let components_dir = project_dir.join("components");
        if components_dir.exists() && components_dir.is_dir() {
            if let Ok(entries) = fs::read_dir(&components_dir) {
                for entry in entries.flatten() {
                    if entry.path().is_dir() {
                        if let Some(name) = entry.file_name().to_str() {
                            components.push(ComponentConfig {
                                name: name.to_string(),
                                path: entry.path(),
                                is_managed: false,
                                action_status: None,
                            });
                        }
                    }
                }
            }
        }

        // Discover components in "managed_components" directory
        let managed_components_dir = project_dir.join("managed_components");
        if managed_components_dir.exists() && managed_components_dir.is_dir() {
            if let Ok(entries) = fs::read_dir(&managed_components_dir) {
                for entry in entries.flatten() {
                    if entry.path().is_dir() {
                        if let Some(name) = entry.file_name().to_str() {
                            components.push(ComponentConfig {
                                name: name.to_string(),
                                path: entry.path(),
                                is_managed: true,
                                action_status: None,
                            });
                        }
                    }
                }
            }
        }

        components.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(components)
    }

    fn generate_support_scripts(&self) -> Result<()> {
        for board in &self.boards {
            self.generate_build_script(board)?;
            self.generate_flash_script(board)?;
            self.generate_app_flash_script(board)?;
        }
        // Generate idf-build-apps script for efficient multi-board building
        self.generate_idf_build_apps_script()?;
        Ok(())
    }

    fn generate_build_script(&self, board: &BoardConfig) -> Result<()> {
        let script_path = self.support_dir.join(format!("build_{}.sh", board.name));
        let content = format!(
            r#"#!/bin/bash
# ESPBrew generated build script for {}
# Generated at {}

set -e

echo "🍺 ESPBrew: Building {} board..."
echo "Project: {}"
echo "Config: {}"
echo "Build dir: {}"

cd "{}"

# Set target based on board configuration
BOARD_CONFIG="{}"
if grep -q "esp32p4" "$BOARD_CONFIG"; then
    TARGET="esp32p4"
elif grep -q "esp32c6" "$BOARD_CONFIG"; then
    TARGET="esp32c6"
elif grep -q "esp32c3" "$BOARD_CONFIG"; then
    TARGET="esp32c3"
else
    TARGET="esp32s3"
fi

echo "Target: $TARGET"

# Build with board-specific configuration
# Use board-specific sdkconfig file to avoid conflicts when building multiple boards in parallel
SDKCONFIG_FILE="{}/sdkconfig"

# Set target and build with board-specific defaults and sdkconfig
SDKCONFIG_DEFAULTS="{}" idf.py -D SDKCONFIG="$SDKCONFIG_FILE" -B "{}" set-target $TARGET
SDKCONFIG_DEFAULTS="{}" idf.py -D SDKCONFIG="$SDKCONFIG_FILE" -B "{}" build

echo "✅ Build completed for {}"
"#,
            board.name,
            Local::now().format("%Y-%m-%d %H:%M:%S"),
            board.name,
            self.project_dir.display(),
            board.config_file.display(),
            board.build_dir.display(),
            self.project_dir.display(),
            board.config_file.display(),
            board.build_dir.display(),
            board.config_file.display(),
            board.build_dir.display(),
            board.config_file.display(),
            board.build_dir.display(),
            board.name,
        );

        fs::write(&script_path, content)?;

        // Make script executable on Unix systems
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&script_path)?.permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&script_path, perms)?;
        }

        Ok(())
    }

    fn generate_flash_script(&self, board: &BoardConfig) -> Result<()> {
        let script_path = self.support_dir.join(format!("flash_{}.sh", board.name));
        let content = format!(
            r#"#!/bin/bash
# ESPBrew generated flash script for {}
# Generated at {}

set -e

echo "🔥 ESPBrew: Flashing {} board..."
echo "Build dir: {}"

cd "{}"

if [ ! -d "{}" ]; then
    echo "❌ Build directory does not exist. Please build first."
    exit 1
fi

# Flash the board with board-specific sdkconfig
SDKCONFIG_FILE="{}/sdkconfig"
idf.py -D SDKCONFIG="$SDKCONFIG_FILE" -B "{}" flash monitor

echo "🔥 Flash completed for {}"
"#,
            board.name,
            Local::now().format("%Y-%m-%d %H:%M:%S"),
            board.name,
            board.build_dir.display(),
            self.project_dir.display(),
            board.build_dir.display(),
            board.build_dir.display(),
            board.build_dir.display(),
            board.name,
        );

        fs::write(&script_path, content)?;

        // Make script executable on Unix systems
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&script_path)?.permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&script_path, perms)?;
        }

        Ok(())
    }

    fn generate_app_flash_script(&self, board: &BoardConfig) -> Result<()> {
        let script_path = self
            .support_dir
            .join(format!("app-flash_{}.sh", board.name));
        let content = format!(
            r#"#!/bin/bash
# ESPBrew generated app-flash script for {}
# Generated at {}

set -e

echo "⚡ ESPBrew: App-flashing {} board..."
echo "Build dir: {}"

cd "{}"

if [ ! -d "{}" ]; then
    echo "❌ Build directory does not exist. Please build first."
    exit 1
fi

# Flash only the app partition with board-specific sdkconfig
SDKCONFIG_FILE="{}/sdkconfig"
idf.py -D SDKCONFIG="$SDKCONFIG_FILE" -B "{}" app-flash

echo "⚡ App-flash completed for {}"
"#,
            board.name,
            Local::now().format("%Y-%m-%d %H:%M:%S"),
            board.name,
            board.build_dir.display(),
            self.project_dir.display(),
            board.build_dir.display(),
            board.build_dir.display(),
            board.build_dir.display(),
            board.name,
        );

        fs::write(&script_path, content)?;

        // Make script executable on Unix systems
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&script_path)?.permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&script_path, perms)?;
        }

        Ok(())
    }

    fn generate_idf_build_apps_script(&self) -> Result<()> {
        let script_path = self.support_dir.join("build-all-idf-build-apps.sh");

        // Determine unique targets from all board configurations
        let mut targets = std::collections::HashSet::new();
        for board in &self.boards {
            let target = Self::determine_target(&board.config_file)
                .unwrap_or_else(|_| "esp32s3".to_string());
            targets.insert(target);
        }
        let targets_str = targets.into_iter().collect::<Vec<_>>().join(" ");

        let content = format!(
            r#"#!/bin/bash
# ESPBrew generated idf-build-apps script
# Generated at {}
# 
# This script uses the professional ESP-IDF idf-build-apps tool for efficient multi-board building.
# It automatically handles component manager conflicts and provides advanced build features.

set -e

echo "🍺 ESPBrew: Building all boards using idf-build-apps (professional ESP-IDF multi-build tool)"
echo "Project: {}"
echo "Detected {} boards: {}"
echo "Targets: {}"
echo

# Check if idf-build-apps is available
if ! command -v idf-build-apps &> /dev/null; then
    echo "⚠️  idf-build-apps not found. Installing..."
    echo "Installing idf-build-apps via pip..."
    pip install idf-build-apps
    echo "✅ idf-build-apps installed successfully"
    echo
fi

cd "{}"

# Find all buildable applications with our sdkconfig pattern
echo "🔍 Finding buildable applications..."
idf-build-apps find \
    --paths . \
    --target all \
    --config-rules "sdkconfig.defaults.*" \
    --build-dir "build.@w" \
    --recursive

echo
echo "🔨 Building all applications..."

# Build all applications using idf-build-apps
# Features:
# - Automatic component manager conflict resolution
# - Parallel builds with proper job management
# - Build directory isolation (build.{{board_name}})
# - Comprehensive error handling and logging
# - Professional CI/CD support
idf-build-apps build \
    --paths . \
    --target all \
    --config-rules "sdkconfig.defaults.*" \
    --build-dir "build.@w" \
    --build-log-filename "build.log" \
    --keep-going \
    --recursive

BUILD_EXIT_CODE=$?

echo
if [ $BUILD_EXIT_CODE -eq 0 ]; then
    echo "🎉 All boards built successfully using idf-build-apps!"
    echo "Build directories: {}"
    echo "Individual board scripts are also available in ./support/ for targeted builds."
else
    echo "❌ Some builds failed. Check individual build logs in build directories."
    echo "Exit code: $BUILD_EXIT_CODE"
fi

echo "Build logs are available in: build.*/build.log"
echo "Flash scripts are available in: ./support/flash_*.sh"

exit $BUILD_EXIT_CODE
"#,
            Local::now().format("%Y-%m-%d %H:%M:%S"),
            self.project_dir.display(),
            self.boards.len(),
            self.boards
                .iter()
                .map(|b| b.name.as_str())
                .collect::<Vec<_>>()
                .join(", "),
            targets_str,
            self.project_dir.display(),
            self.boards
                .iter()
                .map(|b| format!("build.{}", b.name))
                .collect::<Vec<_>>()
                .join(", "),
        );

        fs::write(&script_path, content)?;

        // Make script executable on Unix systems
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&script_path)?.permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&script_path, perms)?;
        }

        Ok(())
    }

    async fn build_all_boards(&mut self, tx: mpsc::UnboundedSender<AppEvent>) -> Result<()> {
        // Set build in progress
        self.build_in_progress = true;

        let result = match self.build_strategy {
            BuildStrategy::Sequential => self.build_all_boards_sequential(tx.clone()).await,
            BuildStrategy::Parallel => self.build_all_boards_parallel(tx.clone()).await,
            BuildStrategy::IdfBuildApps => self.build_all_boards_idf_build_apps(tx.clone()).await,
        };

        // Send build completion event
        let _ = tx.send(AppEvent::BuildCompleted);

        result
    }

    async fn build_all_boards_sequential(
        &mut self,
        tx: mpsc::UnboundedSender<AppEvent>,
    ) -> Result<()> {
        let _ = tx.send(AppEvent::BuildOutput(
            "system".to_string(),
            "🍺 Starting sequential build of all boards to avoid component manager conflicts"
                .to_string(),
        ));

        // Clone the data we need before iterating
        let boards_data: Vec<_> = self
            .boards
            .iter()
            .enumerate()
            .map(|(index, board)| {
                (
                    index,
                    board.name.clone(),
                    board.config_file.clone(),
                    board.build_dir.clone(),
                )
            })
            .collect();

        let project_dir = self.project_dir.clone();
        let logs_dir = self.logs_dir.clone();
        let mut successful_builds = 0;
        let total_boards = boards_data.len();

        // Build each board sequentially to avoid component manager lock conflicts
        for (index, board_name, config_file, build_dir) in boards_data {
            let _ = tx.send(AppEvent::BuildOutput(
                "system".to_string(),
                format!(
                    "🔨 Building board {} ({}/{}) - {}",
                    board_name,
                    index + 1,
                    total_boards,
                    board_name
                ),
            ));

            // Update status to building
            self.boards[index].status = BuildStatus::Building;
            self.boards[index].last_updated = Local::now();

            // Clear previous logs for this board
            self.boards[index].log_lines.clear();

            let log_file = logs_dir.join(format!("{}.log", board_name));
            let result = if let Some(ref handler) = self.project_handler {
                Self::build_board_with_handler(
                    handler.as_ref(),
                    &board_name,
                    &project_dir,
                    &config_file,
                    &build_dir,
                    &log_file,
                    tx.clone(),
                )
                .await
            } else {
                // Fallback to ESP-IDF build for unknown projects
                Self::build_board(
                    &board_name,
                    &project_dir,
                    &config_file,
                    &build_dir,
                    &log_file,
                    tx.clone(),
                )
                .await
            };

            // Update board status based on result
            if result.is_ok() {
                self.boards[index].status = BuildStatus::Success;
                successful_builds += 1;
                let _ = tx.send(AppEvent::BuildOutput(
                    "system".to_string(),
                    format!(
                        "✅ Board {} completed successfully ({}/{})",
                        board_name, successful_builds, total_boards
                    ),
                ));
            } else {
                self.boards[index].status = BuildStatus::Failed;
                let _ = tx.send(AppEvent::BuildOutput(
                    "system".to_string(),
                    format!(
                        "❌ Board {} failed ({} successful, {} failed)",
                        board_name,
                        successful_builds,
                        index + 1 - successful_builds
                    ),
                ));
            }
            self.boards[index].last_updated = Local::now();

            // Send build finished event for this board
            let _ = tx.send(AppEvent::BuildFinished(board_name, result.is_ok()));
        }

        // Send final summary
        let failed_builds = total_boards - successful_builds;
        if failed_builds == 0 {
            let _ = tx.send(AppEvent::BuildOutput(
                "system".to_string(),
                format!("🎉 All {} boards built successfully!", total_boards),
            ));
        } else {
            let _ = tx.send(AppEvent::BuildOutput(
                "system".to_string(),
                format!(
                    "⚠️ Build completed: {} successful, {} failed out of {} total boards",
                    successful_builds, failed_builds, total_boards
                ),
            ));
        }

        Ok(())
    }

    async fn build_all_boards_parallel(
        &mut self,
        tx: mpsc::UnboundedSender<AppEvent>,
    ) -> Result<()> {
        let _ = tx.send(AppEvent::BuildOutput(
            "system".to_string(),
            "⚠️ Starting parallel build of all boards (may cause component manager conflicts)"
                .to_string(),
        ));

        // Clone the data we need before iterating
        let boards_data: Vec<_> = self
            .boards
            .iter()
            .enumerate()
            .map(|(index, board)| {
                (
                    index,
                    board.name.clone(),
                    board.config_file.clone(),
                    board.build_dir.clone(),
                )
            })
            .collect();

        let project_dir = self.project_dir.clone();
        let logs_dir = self.logs_dir.clone();

        for (index, board_name, config_file, build_dir) in boards_data {
            let tx_clone = tx.clone();
            let project_dir_clone = project_dir.clone();
            let logs_dir_clone = logs_dir.clone();

            // Update status to building
            self.boards[index].status = BuildStatus::Building;
            self.boards[index].last_updated = Local::now();

            tokio::spawn(async move {
                let log_file = logs_dir_clone.join(format!("{}.log", board_name));
                // For parallel builds, fall back to the original ESP-IDF build method for now
                // TODO: Implement proper handler cloning for parallel execution
                let result = Self::build_board(
                    &board_name,
                    &project_dir_clone,
                    &config_file,
                    &build_dir,
                    &log_file,
                    tx_clone.clone(),
                )
                .await;

                let _ = tx_clone.send(AppEvent::BuildFinished(board_name, result.is_ok()));
            });
        }
        Ok(())
    }

    async fn build_all_boards_idf_build_apps(
        &mut self,
        tx: mpsc::UnboundedSender<AppEvent>,
    ) -> Result<()> {
        let _ = tx.send(AppEvent::BuildOutput(
            "system".to_string(),
            "🍺 Starting professional idf-build-apps multi-board build (zero conflicts, optimal performance)".to_string(),
        ));

        let project_dir = self.project_dir.clone();
        let total_boards = self.boards.len();

        // Set all boards to building status
        for board in &mut self.boards {
            board.status = BuildStatus::Building;
            board.last_updated = Local::now();
            board.log_lines.clear();
        }

        let _ = tx.send(AppEvent::BuildOutput(
            "system".to_string(),
            format!(
                "🔍 Running idf-build-apps for {} boards: {}",
                total_boards,
                self.boards
                    .iter()
                    .map(|b| b.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        ));

        // Execute idf-build-apps build command
        let mut cmd = TokioCommand::new("idf-build-apps");
        cmd.current_dir(&project_dir)
            .args([
                "build",
                "--paths",
                ".",
                "--target",
                "all",
                "--config-rules",
                "sdkconfig.defaults.*",
                "--build-dir",
                "build.@w",
                "--build-log-filename",
                "build.log",
                "--keep-going",
                "--recursive",
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = cmd.spawn()?;
        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();

        let tx_stdout = tx.clone();
        let tx_stderr = tx.clone();

        // Handle stdout with real-time parsing
        tokio::spawn(async move {
            let mut reader = BufReader::new(stdout);
            let mut buffer = String::new();

            while reader.read_line(&mut buffer).await.unwrap_or(0) > 0 {
                let line = buffer.trim().to_string();

                // Parse idf-build-apps output to extract board-specific information
                if line.contains("build success") {
                    if let Some(board_name) = Self::extract_board_name_from_build_output(&line) {
                        let _ = tx_stdout.send(AppEvent::BuildFinished(board_name, true));
                    }
                } else if line.contains("build failed") {
                    if let Some(board_name) = Self::extract_board_name_from_build_output(&line) {
                        let _ = tx_stdout.send(AppEvent::BuildFinished(board_name, false));
                    }
                }

                // Send all output as system messages
                let _ = tx_stdout.send(AppEvent::BuildOutput("idf-build-apps".to_string(), line));
                buffer.clear();
            }
        });

        // Handle stderr
        tokio::spawn(async move {
            let mut reader = BufReader::new(stderr);
            let mut buffer = String::new();

            while reader.read_line(&mut buffer).await.unwrap_or(0) > 0 {
                let line = buffer.trim().to_string();
                let _ = tx_stderr.send(AppEvent::BuildOutput(
                    "idf-build-apps-err".to_string(),
                    line,
                ));
                buffer.clear();
            }
        });

        let status = child.wait().await?;

        // Wait a bit for output processing to complete
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

        // Update board statuses from build logs
        self.update_board_statuses_from_build_logs().await?;

        // Send final summary
        let successful = self
            .boards
            .iter()
            .filter(|b| matches!(b.status, BuildStatus::Success))
            .count();
        let failed = self
            .boards
            .iter()
            .filter(|b| matches!(b.status, BuildStatus::Failed))
            .count();

        if status.success() && failed == 0 {
            let _ = tx.send(AppEvent::BuildOutput(
                "system".to_string(),
                format!(
                    "🎉 All {} boards built successfully using idf-build-apps!",
                    total_boards
                ),
            ));
        } else {
            let _ = tx.send(AppEvent::BuildOutput(
                "system".to_string(),
                format!(
                    "📋 idf-build-apps completed: {} successful, {} failed. Check build.*/build.log for details.",
                    successful, failed
                ),
            ));
        }

        if status.success() {
            Ok(())
        } else {
            Err(anyhow::anyhow!("idf-build-apps build failed"))
        }
    }

    fn extract_board_name_from_build_output(line: &str) -> Option<String> {
        // Parse lines like: "(cmake) App ., target esp32s3, sdkconfig /path/sdkconfig.defaults.board_name, build in ./build.board_name, build success in 31.978582s"
        if let Some(build_dir_start) = line.find("build in ./build.") {
            let remaining = &line[build_dir_start + "build in ./build.".len()..];
            if let Some(comma_pos) = remaining.find(',') {
                return Some(remaining[..comma_pos].to_string());
            }
        }
        None
    }

    async fn update_board_statuses_from_build_logs(&mut self) -> Result<()> {
        for board in &mut self.boards {
            let build_log_path = board.build_dir.join("build.log");

            if build_log_path.exists() {
                // Board has a build log, check if build was successful
                let log_content = fs::read_to_string(&build_log_path)?;

                if log_content.contains("build success")
                    || log_content.contains("Project build complete")
                {
                    board.status = BuildStatus::Success;
                } else if log_content.contains("build failed") || log_content.contains("FAILED") {
                    board.status = BuildStatus::Failed;
                }

                // Load recent log lines for display (last 50 lines)
                let lines: Vec<String> = log_content.lines().map(|l| l.to_string()).collect();
                let start_idx = if lines.len() > 50 {
                    lines.len() - 50
                } else {
                    0
                };
                board.log_lines = lines[start_idx..].to_vec();
            } else {
                // No build log found, board might not have been built
                board.status = BuildStatus::Pending;
            }

            board.last_updated = Local::now();
        }
        Ok(())
    }

    async fn build_board_with_handler(
        project_handler: &dyn ProjectHandler,
        board_name: &str,
        project_dir: &Path,
        config_file: &Path,
        build_dir: &Path,
        log_file: &Path,
        tx: mpsc::UnboundedSender<AppEvent>,
    ) -> Result<()> {
        // Create a BoardConfig from the individual parameters
        let board_config = project::BoardConfig {
            name: board_name.to_string(),
            config_file: config_file.to_path_buf(),
            build_dir: build_dir.to_path_buf(),
            target: None, // Will be auto-detected
            project_type: project_handler.project_type(),
        };

        // Call the project handler's build method
        match project_handler
            .build_board(project_dir, &board_config, tx)
            .await
        {
            Ok(_artifacts) => Ok(()),
            Err(e) => Err(e),
        }
    }

    async fn flash_board_with_handler(
        project_handler: &dyn ProjectHandler,
        board_name: &str,
        project_dir: &Path,
        config_file: &Path,
        build_dir: &Path,
        log_file: &Path,
        tx: mpsc::UnboundedSender<AppEvent>,
    ) -> Result<()> {
        // Create a BoardConfig from the individual parameters
        let board_config = project::BoardConfig {
            name: board_name.to_string(),
            config_file: config_file.to_path_buf(),
            build_dir: build_dir.to_path_buf(),
            target: None, // Will be auto-detected
            project_type: project_handler.project_type(),
        };

        // First build to get artifacts
        let artifacts = project_handler
            .build_board(project_dir, &board_config, tx.clone())
            .await?;
        // Then flash the artifacts
        project_handler
            .flash_board(project_dir, &board_config, &artifacts, None, tx)
            .await
    }

    async fn monitor_board_with_handler(
        project_handler: &dyn ProjectHandler,
        board_name: &str,
        project_dir: &Path,
        config_file: &Path,
        build_dir: &Path,
        log_file: &Path,
        tx: mpsc::UnboundedSender<AppEvent>,
    ) -> Result<()> {
        // Create a BoardConfig from the individual parameters
        let board_config = project::BoardConfig {
            name: board_name.to_string(),
            config_file: config_file.to_path_buf(),
            build_dir: build_dir.to_path_buf(),
            target: None, // Will be auto-detected
            project_type: project_handler.project_type(),
        };

        // Monitor the board
        project_handler
            .monitor_board(project_dir, &board_config, None, 115200, tx)
            .await
    }

    async fn clean_board_with_handler(
        project_handler: &dyn ProjectHandler,
        board_name: &str,
        project_dir: &Path,
        config_file: &Path,
        build_dir: &Path,
        log_file: &Path,
        tx: mpsc::UnboundedSender<AppEvent>,
    ) -> Result<()> {
        // Create a BoardConfig from the individual parameters
        let board_config = project::BoardConfig {
            name: board_name.to_string(),
            config_file: config_file.to_path_buf(),
            build_dir: build_dir.to_path_buf(),
            target: None, // Will be auto-detected
            project_type: project_handler.project_type(),
        };

        // Clean the board
        project_handler
            .clean_board(project_dir, &board_config, tx)
            .await
    }

    async fn build_board(
        board_name: &str,
        project_dir: &Path,
        config_file: &Path,
        build_dir: &Path,
        log_file: &Path,
        tx: mpsc::UnboundedSender<AppEvent>,
    ) -> Result<()> {
        let config_path = config_file.to_string_lossy();

        // First determine target
        let target = Self::determine_target(config_file)?;

        // Use board-specific sdkconfig file to avoid conflicts when building multiple boards in parallel
        let sdkconfig_path = build_dir.join("sdkconfig");
        let _ = tx.send(AppEvent::BuildOutput(
            board_name.to_string(),
            format!(
                "📋 Using board-specific sdkconfig: {}",
                sdkconfig_path.display()
            ),
        ));

        // Get current working directory to check if cd is needed
        let current_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let needs_cd = current_dir != *project_dir;

        // Log the set-target command
        let set_target_cmd = if needs_cd {
            format!(
                "cd {} && SDKCONFIG_DEFAULTS='{}' idf.py -D SDKCONFIG='{}' -B '{}' set-target {}",
                project_dir.display(),
                config_path,
                sdkconfig_path.display(),
                build_dir.display(),
                target
            )
        } else {
            format!(
                "SDKCONFIG_DEFAULTS='{}' idf.py -D SDKCONFIG='{}' -B '{}' set-target {}",
                config_path,
                sdkconfig_path.display(),
                build_dir.display(),
                target
            )
        };
        let _ = tx.send(AppEvent::BuildOutput(
            board_name.to_string(),
            format!("🔨 Executing: {}", set_target_cmd),
        ));

        // Set target command
        let mut cmd = TokioCommand::new("idf.py");
        cmd.current_dir(project_dir)
            .env("SDKCONFIG_DEFAULTS", &*config_path)
            .args([
                "-D",
                &format!("SDKCONFIG={}", sdkconfig_path.display()),
                "-B",
                &build_dir.to_string_lossy(),
                "set-target",
                &target,
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let output = cmd.output().await?;
        let stdout_str = String::from_utf8_lossy(&output.stdout);
        let stderr_str = String::from_utf8_lossy(&output.stderr);

        let set_target_log = format!(
            "🔨 COMMAND: {}\nSET TARGET OUTPUT:\n{}\n{}\n",
            set_target_cmd, stdout_str, stderr_str
        );

        fs::write(log_file, &set_target_log)?;

        // Send set-target output to TUI
        if !stdout_str.trim().is_empty() {
            for line in stdout_str.lines() {
                let _ = tx.send(AppEvent::BuildOutput(
                    board_name.to_string(),
                    format!("[tgt] {}", line),
                ));
            }
        }
        if !stderr_str.trim().is_empty() {
            for line in stderr_str.lines() {
                let _ = tx.send(AppEvent::BuildOutput(
                    board_name.to_string(),
                    format!("[tgt!] {}", line),
                ));
            }
        }

        if !output.status.success() {
            let _ = tx.send(AppEvent::BuildOutput(
                board_name.to_string(),
                format!(
                    "❌ Failed to set target (exit code: {})",
                    output.status.code().unwrap_or(-1)
                ),
            ));
            return Err(anyhow::anyhow!("Failed to set target"));
        }

        // Log the build command
        let build_cmd = if needs_cd {
            format!(
                "cd {} && SDKCONFIG_DEFAULTS='{}' idf.py -D SDKCONFIG='{}' -B '{}' build",
                project_dir.display(),
                config_path,
                sdkconfig_path.display(),
                build_dir.display()
            )
        } else {
            format!(
                "SDKCONFIG_DEFAULTS='{}' idf.py -D SDKCONFIG='{}' -B '{}' build",
                config_path,
                sdkconfig_path.display(),
                build_dir.display()
            )
        };
        let _ = tx.send(AppEvent::BuildOutput(
            board_name.to_string(),
            format!("🔨 Executing: {}", build_cmd),
        ));

        // Build command with unbuffered output for real-time streaming
        let mut cmd = TokioCommand::new("idf.py");
        cmd.current_dir(project_dir)
            .env("SDKCONFIG_DEFAULTS", &*config_path)
            .env("PYTHONUNBUFFERED", "1") // Force Python to not buffer output
            .args([
                "-D",
                &format!("SDKCONFIG={}", sdkconfig_path.display()),
                "-B",
                &build_dir.to_string_lossy(),
                "build",
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = cmd.spawn()?;
        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();

        let tx_stdout = tx.clone();
        let tx_stderr = tx.clone();
        let board_name_stdout = board_name.to_string();
        let board_name_stderr = board_name.to_string();
        let log_file_clone = log_file.to_path_buf();

        // Handle stdout
        tokio::spawn(async move {
            let mut reader = BufReader::new(stdout);
            let mut log_content = format!(
                "{}\n🔨 BUILD COMMAND: {}\n",
                set_target_log.clone(),
                build_cmd
            );
            let mut buffer = String::new();

            while reader.read_line(&mut buffer).await.unwrap_or(0) > 0 {
                let line = buffer.trim().to_string();
                log_content.push_str(&format!("{}\n", line));
                let _ = fs::write(&log_file_clone, &log_content);
                let _ = tx_stdout.send(AppEvent::BuildOutput(board_name_stdout.clone(), line));
                buffer.clear();
            }
        });

        // Handle stderr
        tokio::spawn(async move {
            let mut reader = BufReader::new(stderr);
            let mut buffer = String::new();

            while reader.read_line(&mut buffer).await.unwrap_or(0) > 0 {
                let line = buffer.trim().to_string();
                let _ = tx_stderr.send(AppEvent::BuildOutput(board_name_stderr.clone(), line));
                buffer.clear();
            }
        });

        let status = child.wait().await?;
        if status.success() {
            Ok(())
        } else {
            Err(anyhow::anyhow!("Build failed"))
        }
    }

    fn determine_target(config_file: &Path) -> Result<String> {
        let content = fs::read_to_string(config_file)?;

        if content.contains("esp32p4") || content.contains("CONFIG_IDF_TARGET=\"esp32p4\"") {
            Ok("esp32p4".to_string())
        } else if content.contains("esp32c6") || content.contains("CONFIG_IDF_TARGET=\"esp32c6\"") {
            Ok("esp32c6".to_string())
        } else if content.contains("esp32c3") || content.contains("CONFIG_IDF_TARGET=\"esp32c3\"") {
            Ok("esp32c3".to_string())
        } else {
            Ok("esp32s3".to_string()) // default
        }
    }

    fn update_board_status(&mut self, board_name: &str, status: BuildStatus) {
        if let Some(board) = self.boards.iter_mut().find(|b| b.name == board_name) {
            board.status = status;
            board.last_updated = Local::now();
        }
    }

    fn add_log_line(&mut self, board_name: &str, line: String) {
        if let Some(board) = self.boards.iter_mut().find(|b| b.name == board_name) {
            board.log_lines.push(line);
            // Keep only last 1000 lines to prevent memory issues
            if board.log_lines.len() > 1000 {
                board.log_lines.drain(0..100);
            }
            // Auto-scroll to bottom for the selected board when new content arrives
            if board_name == self.boards[self.selected_board].name {
                self.auto_scroll_to_bottom();
            }
        }
    }

    fn auto_scroll_to_bottom(&mut self) {
        if let Some(selected_board) = self.boards.get(self.selected_board) {
            if !selected_board.log_lines.is_empty() {
                // Set scroll to a high value - the UI will auto-adjust to show latest content
                let total_lines = selected_board.log_lines.len();
                self.log_scroll_offset = total_lines; // UI will clamp this to valid range
            }
        }
    }

    fn scroll_to_top(&mut self) {
        self.log_scroll_offset = 0;
    }

    fn scroll_to_bottom(&mut self) {
        if let Some(selected_board) = self.boards.get(self.selected_board) {
            let total_lines = selected_board.log_lines.len();
            if total_lines > 0 {
                // Scroll to the very end
                self.log_scroll_offset = total_lines.saturating_sub(1);
            }
        }
    }

    fn colorize_log_line(line: &str) -> Line<'_> {
        let line_lower = line.to_lowercase();

        // Error patterns (red)
        if line_lower.contains("error:")
            || line_lower.contains("failed")
            || line_lower.contains("❌")
            || line_lower.contains("fatal error")
            || line_lower.contains("compilation failed")
        {
            return Line::from(Span::styled(line, Style::default().fg(Color::Red)));
        }

        // Warning patterns (yellow)
        if line_lower.contains("warning:")
            || line_lower.contains("#warning")
            || line_lower.contains("deprecated")
            || line_lower.contains("[-w")
        {
            return Line::from(Span::styled(line, Style::default().fg(Color::Yellow)));
        }

        // Build progress patterns (cyan/bright blue)
        if line.contains("[")
            && line.contains("/")
            && line.contains("]")
            && (line.contains("Building") || line.contains("Linking") || line.contains("Compiling"))
        {
            return Line::from(Span::styled(line, Style::default().fg(Color::Cyan)));
        }

        // Success patterns (green)
        if line_lower.contains("✅")
            || line_lower.contains("completed successfully")
            || line_lower.contains("build complete")
        {
            return Line::from(Span::styled(line, Style::default().fg(Color::Green)));
        }

        // Command execution patterns (bright white/bold)
        if line.contains("🔨 Executing:")
            || line.contains("🧡 Executing:")
            || line.contains("🔥 Executing:")
            || line.contains("📺 Executing:")
        {
            return Line::from(Span::styled(
                line,
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ));
        }

        // File paths (dim white)
        if line.contains(".c:")
            || line.contains(".cpp:")
            || line.contains(".h:")
            || line.contains(".obj")
            || line.contains(".a")
            || line.starts_with("/")
                && (line.contains("components") || line.contains("managed_components"))
        {
            return Line::from(Span::styled(line, Style::default().fg(Color::Gray)));
        }

        // Prefixes with specific colors
        if line.starts_with("[tgt]") {
            return Line::from(vec![
                Span::styled("[tgt]", Style::default().fg(Color::Blue)),
                Span::raw(&line[5..]),
            ]);
        }

        if line.starts_with("[tgt!]") {
            return Line::from(vec![
                Span::styled("[tgt!]", Style::default().fg(Color::Red)),
                Span::styled(&line[6..], Style::default().fg(Color::Red)),
            ]);
        }

        // Default: normal text
        Line::from(line)
    }

    fn next_board(&mut self) {
        if !self.boards.is_empty() {
            self.selected_board = (self.selected_board + 1) % self.boards.len();
            self.list_state.select(Some(self.selected_board));
        }
    }

    fn previous_board(&mut self) {
        if !self.boards.is_empty() {
            self.selected_board = if self.selected_board == 0 {
                self.boards.len() - 1
            } else {
                self.selected_board - 1
            };
            self.list_state.select(Some(self.selected_board));
        }
    }

    fn next_component(&mut self) {
        if !self.components.is_empty() {
            self.selected_component = (self.selected_component + 1) % self.components.len();
            self.component_list_state
                .select(Some(self.selected_component));
        }
    }

    fn previous_component(&mut self) {
        if !self.components.is_empty() {
            self.selected_component = if self.selected_component == 0 {
                self.components.len() - 1
            } else {
                self.selected_component - 1
            };
            self.component_list_state
                .select(Some(self.selected_component));
        }
    }

    fn toggle_focused_pane(&mut self) {
        self.focused_pane = match self.focused_pane {
            FocusedPane::BoardList => FocusedPane::ComponentList,
            FocusedPane::ComponentList => FocusedPane::LogPane,
            FocusedPane::LogPane => FocusedPane::BoardList,
        };
        // Reset log scroll when switching away from log pane
        if self.focused_pane != FocusedPane::LogPane {
            self.log_scroll_offset = 0;
        }
    }

    fn scroll_log_up(&mut self) {
        if self.log_scroll_offset > 0 {
            self.log_scroll_offset -= 1;
        }
    }

    fn scroll_log_down(&mut self) {
        if let Some(selected_board) = self.boards.get(self.selected_board) {
            let max_scroll = selected_board.log_lines.len().saturating_sub(1);
            if self.log_scroll_offset < max_scroll {
                self.log_scroll_offset += 1;
            }
        }
    }

    fn scroll_log_page_up(&mut self) {
        self.log_scroll_offset = self.log_scroll_offset.saturating_sub(10);
    }

    fn scroll_log_page_down(&mut self) {
        if let Some(selected_board) = self.boards.get(self.selected_board) {
            let max_scroll = selected_board.log_lines.len().saturating_sub(1);
            self.log_scroll_offset = (self.log_scroll_offset + 10).min(max_scroll);
        }
    }

    fn reset_log_scroll(&mut self) {
        self.log_scroll_offset = 0;
    }

    fn acknowledge_tool_warning(&mut self) {
        self.tool_warning_acknowledged = true;
        self.show_tool_warning = false;
    }

    fn show_action_menu(&mut self) {
        self.show_action_menu = true;
        self.action_menu_selected = 0;
    }

    fn hide_action_menu(&mut self) {
        self.show_action_menu = false;
        self.action_menu_selected = 0;
    }

    fn show_component_action_menu(&mut self) {
        self.show_component_action_menu = true;
        self.component_action_menu_selected = 0;
    }

    fn hide_component_action_menu(&mut self) {
        self.show_component_action_menu = false;
        self.component_action_menu_selected = 0;
    }

    fn next_action(&mut self) {
        if !self.available_actions.is_empty() {
            self.action_menu_selected =
                (self.action_menu_selected + 1) % self.available_actions.len();
        }
    }

    fn previous_action(&mut self) {
        if !self.available_actions.is_empty() {
            self.action_menu_selected = if self.action_menu_selected == 0 {
                self.available_actions.len() - 1
            } else {
                self.action_menu_selected - 1
            };
        }
    }

    fn start_all_boards_build(&mut self, tx: mpsc::UnboundedSender<AppEvent>) {
        if self.build_in_progress {
            return; // Prevent multiple concurrent builds
        }

        // Set build in progress immediately to prevent additional builds
        self.build_in_progress = true;

        // Clone necessary data for the spawned task
        let project_dir = self.project_dir.clone();
        let build_strategy = self.build_strategy.clone();
        let boards_data: Vec<_> = self
            .boards
            .iter()
            .map(|b| (b.name.clone(), b.config_file.clone(), b.build_dir.clone()))
            .collect();
        let logs_dir = self.logs_dir.clone();

        // Set all boards to building status
        for board in &mut self.boards {
            board.status = BuildStatus::Building;
            board.last_updated = Local::now();
            board.log_lines.clear();
        }

        tokio::spawn(async move {
            let _result = Self::execute_build_all_boards(
                project_dir,
                build_strategy,
                boards_data,
                logs_dir,
                tx.clone(),
            )
            .await;

            // Send build completion event
            let _ = tx.send(AppEvent::BuildCompleted);
        });
    }

    fn start_single_board_build(&mut self, tx: mpsc::UnboundedSender<AppEvent>) {
        if self.build_in_progress || self.selected_board >= self.boards.len() {
            return;
        }

        // Set build in progress immediately
        self.build_in_progress = true;

        let board_index = self.selected_board;
        let board = &self.boards[board_index];
        let board_name = board.name.clone();
        let config_file = board.config_file.clone();
        let build_dir = board.build_dir.clone();
        let project_dir = self.project_dir.clone();
        let logs_dir = self.logs_dir.clone();
        let project_type = self.project_handler.as_ref().map(|h| h.project_type());

        // Update status to building
        self.boards[board_index].status = BuildStatus::Building;
        self.boards[board_index].last_updated = Local::now();
        self.boards[board_index].log_lines.clear();
        self.reset_log_scroll();

        tokio::spawn(async move {
            let log_file = logs_dir.join(format!("{}.log", board_name));
            let result = if let Some(proj_type) = project_type {
                // Use project handler for the detected project type
                let registry = ProjectRegistry::new();
                if let Some(new_handler) = registry.get_handler_by_type(&proj_type) {
                    Self::build_board_with_handler(
                        new_handler.as_ref(),
                        &board_name,
                        &project_dir,
                        &config_file,
                        &build_dir,
                        &log_file,
                        tx.clone(),
                    )
                    .await
                } else {
                    // Fallback to ESP-IDF build for unknown projects
                    Self::build_board(
                        &board_name,
                        &project_dir,
                        &config_file,
                        &build_dir,
                        &log_file,
                        tx.clone(),
                    )
                    .await
                }
            } else {
                // Fallback to ESP-IDF build for unknown projects
                Self::build_board(
                    &board_name,
                    &project_dir,
                    &config_file,
                    &build_dir,
                    &log_file,
                    tx.clone(),
                )
                .await
            };

            let _ = tx.send(AppEvent::BuildFinished(board_name.clone(), result.is_ok()));
            let _ = tx.send(AppEvent::BuildCompleted);
        });
    }

    async fn execute_build_all_boards(
        project_dir: PathBuf,
        build_strategy: BuildStrategy,
        boards_data: Vec<(String, PathBuf, PathBuf)>, // (name, config_file, build_dir)
        logs_dir: PathBuf,
        tx: mpsc::UnboundedSender<AppEvent>,
    ) -> Result<()> {
        match build_strategy {
            BuildStrategy::Sequential => {
                Self::execute_build_all_boards_sequential(project_dir, boards_data, logs_dir, tx)
                    .await
            }
            BuildStrategy::Parallel => {
                Self::execute_build_all_boards_parallel(project_dir, boards_data, logs_dir, tx)
                    .await
            }
            BuildStrategy::IdfBuildApps => {
                Self::execute_build_all_boards_idf_build_apps(project_dir, tx).await
            }
        }
    }

    async fn execute_build_all_boards_idf_build_apps(
        project_dir: PathBuf,
        tx: mpsc::UnboundedSender<AppEvent>,
    ) -> Result<()> {
        let _ = tx.send(AppEvent::BuildOutput(
            "system".to_string(),
            "🍺 Starting professional idf-build-apps multi-board build (zero conflicts, optimal performance)".to_string(),
        ));

        // Execute idf-build-apps build command
        let mut cmd = TokioCommand::new("idf-build-apps");
        cmd.current_dir(&project_dir)
            .args([
                "build",
                "--paths",
                ".",
                "--target",
                "all",
                "--config-rules",
                "sdkconfig.defaults.*",
                "--build-dir",
                "build.@w",
                "--build-log-filename",
                "build.log",
                "--keep-going",
                "--recursive",
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = cmd.spawn()?;
        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();

        let tx_stdout = tx.clone();
        let tx_stderr = tx.clone();

        // Handle stdout with real-time parsing
        tokio::spawn(async move {
            let mut reader = BufReader::new(stdout);
            let mut buffer = String::new();

            while reader.read_line(&mut buffer).await.unwrap_or(0) > 0 {
                let line = buffer.trim().to_string();

                // Parse idf-build-apps output to extract board-specific information
                if line.contains("build success") {
                    if let Some(board_name) = Self::extract_board_name_from_build_output(&line) {
                        let _ = tx_stdout.send(AppEvent::BuildFinished(board_name, true));
                    }
                } else if line.contains("build failed") {
                    if let Some(board_name) = Self::extract_board_name_from_build_output(&line) {
                        let _ = tx_stdout.send(AppEvent::BuildFinished(board_name, false));
                    }
                }

                // Send all output as system messages
                let _ = tx_stdout.send(AppEvent::BuildOutput("idf-build-apps".to_string(), line));
                buffer.clear();
            }
        });

        // Handle stderr
        tokio::spawn(async move {
            let mut reader = BufReader::new(stderr);
            let mut buffer = String::new();

            while reader.read_line(&mut buffer).await.unwrap_or(0) > 0 {
                let line = buffer.trim().to_string();
                let _ = tx_stderr.send(AppEvent::BuildOutput(
                    "idf-build-apps-err".to_string(),
                    line,
                ));
                buffer.clear();
            }
        });

        let status = child.wait().await?;

        if status.success() {
            Ok(())
        } else {
            Err(anyhow::anyhow!("idf-build-apps build failed"))
        }
    }

    async fn execute_build_all_boards_sequential(
        project_dir: PathBuf,
        boards_data: Vec<(String, PathBuf, PathBuf)>,
        logs_dir: PathBuf,
        tx: mpsc::UnboundedSender<AppEvent>,
    ) -> Result<()> {
        let _ = tx.send(AppEvent::BuildOutput(
            "system".to_string(),
            "🍺 Starting sequential build of all boards to avoid component manager conflicts"
                .to_string(),
        ));

        let total_boards = boards_data.len();
        let mut successful_builds = 0;

        for (index, (board_name, config_file, build_dir)) in boards_data.iter().enumerate() {
            let _ = tx.send(AppEvent::BuildOutput(
                "system".to_string(),
                format!(
                    "🔨 Building board {} ({}/{}) - {}",
                    board_name,
                    index + 1,
                    total_boards,
                    board_name
                ),
            ));

            let log_file = logs_dir.join(format!("{}.log", board_name));
            let result = Self::build_board(
                board_name,
                &project_dir,
                config_file,
                build_dir,
                &log_file,
                tx.clone(),
            )
            .await;

            let success = result.is_ok();
            if success {
                successful_builds += 1;
            }
            let _ = tx.send(AppEvent::BuildFinished(board_name.clone(), success));
        }

        // Send final summary
        let failed_builds = total_boards - successful_builds;
        if failed_builds == 0 {
            let _ = tx.send(AppEvent::BuildOutput(
                "system".to_string(),
                format!("🎉 All {} boards built successfully!", total_boards),
            ));
        } else {
            let _ = tx.send(AppEvent::BuildOutput(
                "system".to_string(),
                format!(
                    "⚠️ Build completed: {} successful, {} failed out of {} total boards",
                    successful_builds, failed_builds, total_boards
                ),
            ));
        }

        Ok(())
    }

    async fn execute_build_all_boards_parallel(
        project_dir: PathBuf,
        boards_data: Vec<(String, PathBuf, PathBuf)>,
        logs_dir: PathBuf,
        tx: mpsc::UnboundedSender<AppEvent>,
    ) -> Result<()> {
        let _ = tx.send(AppEvent::BuildOutput(
            "system".to_string(),
            "⚠️ Starting parallel build of all boards (may cause component manager conflicts)"
                .to_string(),
        ));

        for (board_name, config_file, build_dir) in boards_data {
            let tx_clone = tx.clone();
            let project_dir_clone = project_dir.clone();
            let logs_dir_clone = logs_dir.clone();

            tokio::spawn(async move {
                let log_file = logs_dir_clone.join(format!("{}.log", board_name));
                let result = Self::build_board(
                    &board_name,
                    &project_dir_clone,
                    &config_file,
                    &build_dir,
                    &log_file,
                    tx_clone.clone(),
                )
                .await;

                let _ = tx_clone.send(AppEvent::BuildFinished(board_name, result.is_ok()));
            });
        }
        Ok(())
    }

    fn next_component_action(&mut self) {
        if !self.available_component_actions.is_empty() {
            self.component_action_menu_selected =
                (self.component_action_menu_selected + 1) % self.available_component_actions.len();
        }
    }

    fn previous_component_action(&mut self) {
        if !self.available_component_actions.is_empty() {
            self.component_action_menu_selected = if self.component_action_menu_selected == 0 {
                self.available_component_actions.len() - 1
            } else {
                self.component_action_menu_selected - 1
            };
        }
    }

    // Remote board dialog methods
    fn start_fetching_remote_boards(&mut self, tx: mpsc::UnboundedSender<AppEvent>) {
        // Use discovered server URL if available, otherwise fallback
        let server_url = self.get_server_url();

        // Set loading state
        self.remote_boards_loading = true;
        self.remote_boards_fetch_error = None;
        self.show_remote_board_dialog = true; // Show dialog with loading state

        // Log the connection attempt
        if self.selected_board < self.boards.len() {
            self.boards[self.selected_board].log_lines.push(format!(
                "🔍 Connecting to remote ESPBrew server: {}",
                server_url
            ));
        }

        // Spawn async task to fetch boards
        tokio::spawn(async move {
            match fetch_remote_boards(&server_url).await {
                Ok(remote_boards) => {
                    let _ = tx.send(AppEvent::RemoteBoardsFetched(remote_boards));
                }
                Err(e) => {
                    let error_msg = if e.to_string().contains("Connection refused") {
                        format!("Server not running at {}", server_url)
                    } else if e.to_string().contains("timeout") {
                        format!("Connection timeout to {}", server_url)
                    } else {
                        format!("Network error: {}", e)
                    };
                    let _ = tx.send(AppEvent::RemoteBoardsFetchFailed(error_msg));
                }
            }
        });
    }

    fn handle_remote_boards_fetched(&mut self, remote_boards: Vec<RemoteBoard>) {
        // Clear loading state
        self.remote_boards_loading = false;
        self.remote_boards_fetch_error = None;

        // Log successful connection
        if self.selected_board < self.boards.len() {
            self.boards[self.selected_board].log_lines.push(format!(
                "📈 Found {} board(s) on server",
                remote_boards.len()
            ));

            // Log details of each found board
            for (i, board) in remote_boards.iter().enumerate() {
                self.boards[self.selected_board].log_lines.push(format!(
                    "   {}. {} ({}) - {}",
                    i + 1,
                    board.logical_name.as_ref().unwrap_or(&board.id),
                    board.chip_type,
                    board.status
                ));
            }
        }

        self.remote_boards = remote_boards;
        self.selected_remote_board = 0;
        if !self.remote_boards.is_empty() {
            self.remote_board_list_state.select(Some(0));
        }
        self.remote_flash_status = None; // Clear any previous errors
        self.remote_monitor_status = None; // Clear any previous monitor errors
    }

    fn handle_remote_boards_fetch_failed(&mut self, error_msg: String) {
        // Clear loading state
        self.remote_boards_loading = false;
        self.remote_boards_fetch_error = Some(error_msg.clone());

        // Log connection failure with more specific error
        if self.selected_board < self.boards.len() {
            self.boards[self.selected_board]
                .log_lines
                .push(format!("❌ Server connection failed: {}", error_msg));
        }

        // Clear remote boards and show error in dialog
        self.remote_boards.clear();
        self.selected_remote_board = 0;
        self.remote_board_list_state = ListState::default();
        self.remote_flash_status = Some(error_msg.clone());
        self.remote_monitor_status = Some(error_msg);
    }

    fn hide_remote_board_dialog(&mut self) {
        self.show_remote_board_dialog = false;
        self.remote_boards.clear();
        self.selected_remote_board = 0;
        self.remote_board_list_state = ListState::default();
        self.remote_flash_status = None;
    }

    fn next_remote_board(&mut self) {
        if !self.remote_boards.is_empty() {
            self.selected_remote_board =
                (self.selected_remote_board + 1) % self.remote_boards.len();
            self.remote_board_list_state
                .select(Some(self.selected_remote_board));
        }
    }

    fn previous_remote_board(&mut self) {
        if !self.remote_boards.is_empty() {
            self.selected_remote_board = if self.selected_remote_board == 0 {
                self.remote_boards.len() - 1
            } else {
                self.selected_remote_board - 1
            };
            self.remote_board_list_state
                .select(Some(self.selected_remote_board));
        }
    }

    async fn execute_remote_flash(&mut self, tx: mpsc::UnboundedSender<AppEvent>) -> Result<()> {
        if self.selected_remote_board >= self.remote_boards.len() {
            return Err(anyhow::anyhow!("No remote board selected"));
        }

        let selected_board = &self.remote_boards[self.selected_remote_board];
        let server_url = self.get_server_url();
        let selected_board_clone = selected_board.clone();
        let project_dir = self.project_dir.clone();
        let project_type = self.project_handler.as_ref().map(|h| h.project_type());

        // Update status
        if self.selected_board < self.boards.len() {
            self.boards[self.selected_board].status = BuildStatus::Flashing;
        }

        self.remote_flash_in_progress = true;
        self.remote_flash_status = Some("Preparing remote flash...".to_string());

        let tx_clone = tx.clone();
        tokio::spawn(async move {
            let result = async {
                // Detect project type and use appropriate flash method
                let _ = tx_clone.send(AppEvent::BuildOutput(
                    "remote".to_string(),
                    "🔍 Detecting project type and build artifacts...".to_string(),
                ));

                // Check if this is a Rust no_std project
                if let Some(ProjectType::RustNoStd) = project_type {
                    let _ = tx_clone.send(AppEvent::BuildOutput(
                        "remote".to_string(),
                        "🦀 Detected Rust no_std project, using Rust binary flash method".to_string(),
                    ));

                    // Use Rust-specific remote flash method
                    upload_and_flash_rust_binary(
                        &server_url,
                        &selected_board_clone,
                        &project_dir,
                        tx_clone.clone(),
                    )
                    .await
                } else if let Some(ProjectType::Arduino) = project_type {
                    let _ = tx_clone.send(AppEvent::BuildOutput(
                        "remote".to_string(),
                        "🔧 Detected Arduino project, using Arduino binary flash method".to_string(),
                    ));

                    // Use Arduino-specific remote flash method
                    upload_and_flash_arduino_remote(
                        &server_url,
                        &selected_board_clone,
                        &project_dir,
                        tx_clone.clone(),
                    )
                    .await
                } else {
                    let _ = tx_clone.send(AppEvent::BuildOutput(
                        "remote".to_string(),
                        "🏗️ Detected ESP-IDF or unknown project type, using ESP-IDF multi-binary flash method".to_string(),
                    ));

                    // Extract board name for ESP-IDF build detection
                    let board_name = selected_board_clone
                        .board_type_id
                        .as_ref()
                        .or(selected_board_clone.logical_name.as_ref())
                        .map(|s| s.as_str());

                    // Try direct ESP-IDF build directory approach first (like the successful curl command)
                    match upload_and_flash_esp_build_direct(
                        &server_url,
                        &selected_board_clone,
                        &project_dir,
                        tx_clone.clone(),
                    )
                    .await
                    {
                        Ok(()) => {
                            let _ = tx_clone.send(AppEvent::BuildOutput(
                                "remote".to_string(),
                                "✅ ESP-IDF multi-binary remote flash completed successfully!"
                                    .to_string(),
                            ));
                            Ok(())
                        }
                        Err(e) => {
                            let _ = tx_clone.send(AppEvent::BuildOutput(
                                "remote".to_string(),
                                format!("⚠️ Multi-binary flash failed, trying fallback: {}", e),
                            ));

                            // Fallback to old detection method
                            match find_esp_build_artifacts(&project_dir, board_name) {
                                Ok((flash_config, binaries)) => {
                                    let _ = tx_clone.send(AppEvent::BuildOutput(
                                        "remote".to_string(),
                                        format!(
                                            "📦 Found ESP-IDF build artifacts: {} binaries",
                                            binaries.len()
                                        ),
                                    ));

                                    for binary in &binaries {
                                        let _ = tx_clone.send(AppEvent::BuildOutput(
                                            "remote".to_string(),
                                            format!(
                                                "  - {} at 0x{:x}: {}",
                                                binary.name,
                                                binary.offset,
                                                binary.file_path.display()
                                            ),
                                        ));
                                    }

                                    // Upload and flash with multi-binary support
                                    upload_and_flash_esp_build_with_logging(
                                        &server_url,
                                        &selected_board_clone,
                                        &flash_config,
                                        &binaries,
                                        tx_clone.clone(),
                                    )
                                    .await
                                }
                                Err(_) => {
                                    // Fall back to single binary flash
                                    let _ = tx_clone.send(AppEvent::BuildOutput(
                                        "remote".to_string(),
                                        "⚠️ No ESP-IDF build detected, using legacy single-binary flash"
                                            .to_string(),
                                    ));

                                    let _ = tx_clone.send(AppEvent::BuildOutput(
                                        "remote".to_string(),
                                        "🔍 Looking for binary file to flash...".to_string(),
                                    ));

                                    let binary_path = find_binary_file(&project_dir, None)?;

                                    let _ = tx_clone.send(AppEvent::BuildOutput(
                                        "remote".to_string(),
                                        format!("📦 Found binary: {}", binary_path.display()),
                                    ));

                                    // Upload and flash with legacy method
                                    upload_and_flash_remote_with_logging(
                                        &server_url,
                                        &selected_board_clone,
                                        &binary_path,
                                        tx_clone.clone(),
                                    )
                                    .await
                                }
                            }
                        }
                    }
                }
            }
            .await;

            let success = result.is_ok();
            let message = if success {
                "✅ Remote flash completed successfully!".to_string()
            } else {
                format!("❌ Remote flash failed: {}", result.unwrap_err())
            };

            let _ = tx_clone.send(AppEvent::BuildOutput("remote".to_string(), message));

            let _ = tx_clone.send(AppEvent::ActionFinished(
                "remote".to_string(),
                "Remote Flash".to_string(),
                success,
            ));
        });

        Ok(())
    }

    async fn execute_remote_monitor(&mut self, tx: mpsc::UnboundedSender<AppEvent>) -> Result<()> {
        if self.selected_remote_board >= self.remote_boards.len() {
            return Err(anyhow::anyhow!("No remote board selected"));
        }

        let selected_board = &self.remote_boards[self.selected_remote_board];
        let server_url = self.get_server_url();
        let selected_board_clone = selected_board.clone();

        // Update status
        if self.selected_board < self.boards.len() {
            self.boards[self.selected_board].status = BuildStatus::Building; // Use Building as "Monitoring" equivalent
        }

        self.remote_monitor_in_progress = true;
        self.remote_monitor_status = Some("📺 Starting remote monitoring session...".to_string());

        let tx_clone = tx.clone();
        tokio::spawn(async move {
            let result = async {
                // Start monitoring session
                let _ = tx_clone.send(AppEvent::BuildOutput(
                    "remote".to_string(),
                    "📺 Starting remote monitoring session...".to_string(),
                ));

                let monitor_request = MonitorRequest {
                    board_id: selected_board_clone.id.clone(),
                    baud_rate: Some(115200),
                    filters: None,
                };

                // Send monitor request to server
                let client = reqwest::Client::new();
                let url = format!("{}/api/v1/monitor/start", server_url.trim_end_matches('/'));

                match client.post(&url).json(&monitor_request).send().await {
                    Ok(response) => {
                        match response.json::<MonitorResponse>().await {
                            Ok(monitor_response) => {
                                if monitor_response.success {
                                    let _ = tx_clone.send(AppEvent::BuildOutput(
                                        "remote".to_string(),
                                        format!("✅ Remote monitoring started: {}", monitor_response.message),
                                    ));

                                    if let Some(session_id) = monitor_response.session_id {
                                        let _ = tx_clone.send(AppEvent::BuildOutput(
                                            "remote".to_string(),
                                            format!("🔗 Session ID: {}", session_id),
                                        ));

                                        // Connect to WebSocket and start log streaming
                                        let ws_url = format!("ws://{}/ws/monitor/{}", 
                                            server_url.trim_start_matches("http://").trim_start_matches("https://"),
                                            session_id
                                        );

                                        let _ = tx_clone.send(AppEvent::BuildOutput(
                                            "remote".to_string(),
                                            format!("🔗 Connecting to WebSocket: {}", ws_url),
                                        ));
                                        let _ = tx_clone.send(AppEvent::BuildOutput(
                                            "remote".to_string(),
                                            "📡 Real-time log streaming starting...".to_string(),
                                        ));

                                        // Start WebSocket connection and keep-alive tasks
                                        match Self::start_websocket_monitoring(
                                            ws_url,
                                            session_id.clone(),
                                            server_url.clone(),
                                            tx_clone.clone()
                                        ).await {
                                            Ok(_) => {
                                                let _ = tx_clone.send(AppEvent::BuildOutput(
                                                    "remote".to_string(),
                                                    "✅ WebSocket connection established - streaming logs...".to_string(),
                                                ));
                                                let _ = tx_clone.send(AppEvent::BuildOutput(
                                                    "remote".to_string(),
                                                    "🔥 Remote board logs will appear below in real-time".to_string(),
                                                ));
                                                let _ = tx_clone.send(AppEvent::BuildOutput(
                                                    "remote".to_string(),
                                                    "─".repeat(60),
                                                ));
                                            }
                                            Err(e) => {
                                                return Err(anyhow::anyhow!("WebSocket connection failed: {}", e));
                                            }
                                        }
                                    }

                                    Ok(())
                                } else {
                                    Err(anyhow::anyhow!("Server error: {}", monitor_response.message))
                                }
                            }
                            Err(e) => Err(anyhow::anyhow!("Failed to parse response: {}", e))
                        }
                    }
                    Err(e) => Err(anyhow::anyhow!("Failed to start monitoring: {}", e))
                }
            }.await;

            let success = result.is_ok();
            let message = if success {
                "✅ Remote monitoring session started successfully!".to_string()
            } else {
                format!("❌ Remote monitoring failed: {}", result.unwrap_err())
            };

            let _ = tx_clone.send(AppEvent::BuildOutput("remote".to_string(), message));

            let _ = tx_clone.send(AppEvent::ActionFinished(
                "remote".to_string(),
                "Remote Monitor".to_string(),
                success,
            ));

            // Update status based on result
            let final_status = if success {
                "✅ Remote monitoring session started - check logs for real-time output".to_string()
            } else {
                format!("❌ Remote monitoring failed: connection could not be established")
            };

            // Send final status update event
            let _ = tx_clone.send(AppEvent::BuildOutput("remote".to_string(), final_status));
        });

        Ok(())
    }

    async fn execute_remote_reset(&mut self, tx: mpsc::UnboundedSender<AppEvent>) -> Result<()> {
        if self.selected_remote_board >= self.remote_boards.len() {
            return Err(anyhow::anyhow!("No remote board selected"));
        }

        let selected_board = &self.remote_boards[self.selected_remote_board];
        let server_url = self
            .server_url
            .as_deref()
            .unwrap_or("http://localhost:8080")
            .to_string();
        let selected_board_clone = selected_board.clone();

        // Show reset confirmation in logs
        if self.selected_board < self.boards.len() {
            self.boards[self.selected_board].log_lines.push(format!(
                "🔄 Sending reset command to board: {} ({})",
                selected_board
                    .logical_name
                    .as_ref()
                    .unwrap_or(&selected_board.id),
                selected_board.chip_type
            ));
        }

        let tx_clone = tx.clone();
        tokio::spawn(async move {
            let result = async {
                // Send reset command to server
                let client = reqwest::Client::new();
                let url = format!("{}/api/v1/reset", server_url.trim_end_matches('/'));

                let reset_request = serde_json::json!({
                    "board_id": selected_board_clone.id
                });

                let _ = tx_clone.send(AppEvent::BuildOutput(
                    "remote".to_string(),
                    "🔄 Sending reset command to remote board...".to_string(),
                ));

                match client.post(&url).json(&reset_request).send().await {
                    Ok(response) => {
                        if response.status().is_success() {
                            let _ = tx_clone.send(AppEvent::BuildOutput(
                                "remote".to_string(),
                                "✅ Reset command sent successfully!".to_string(),
                            ));
                            let _ = tx_clone.send(AppEvent::BuildOutput(
                                "remote".to_string(),
                                "📡 Board should restart momentarily...".to_string(),
                            ));
                            Ok(())
                        } else {
                            let error_msg =
                                format!("Reset failed with status: {}", response.status());
                            Err(anyhow::anyhow!(error_msg))
                        }
                    }
                    Err(e) => {
                        if e.to_string().contains("404") {
                            let _ = tx_clone.send(AppEvent::BuildOutput(
                                "remote".to_string(),
                                "⚠️ Reset API not available on this server version".to_string(),
                            ));
                            let _ = tx_clone.send(AppEvent::BuildOutput(
                                "remote".to_string(),
                                "💡 Try using DTR/RTS reset or manual reset button".to_string(),
                            ));
                            Ok(()) // Don't fail for unsupported feature
                        } else {
                            Err(anyhow::anyhow!("Failed to send reset command: {}", e))
                        }
                    }
                }
            }
            .await;

            let success = result.is_ok();
            if !success {
                let error_msg = format!("❌ Remote reset failed: {}", result.unwrap_err());
                let _ = tx_clone.send(AppEvent::BuildOutput("remote".to_string(), error_msg));
            }
        });

        Ok(())
    }

    /// Start WebSocket connection for monitoring with keep-alive
    async fn start_websocket_monitoring(
        ws_url: String,
        session_id: String,
        server_url: String,
        tx: tokio::sync::mpsc::UnboundedSender<AppEvent>,
    ) -> Result<()> {
        use futures_util::{SinkExt, StreamExt};
        use tokio_tungstenite::{connect_async, tungstenite::Message};

        // Connect to WebSocket
        let (ws_stream, _) = connect_async(&ws_url)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to connect to WebSocket: {}", e))?;

        let (mut ws_sender, mut ws_receiver) = ws_stream.split();

        // Send initial connection confirmation
        let _ = tx.send(AppEvent::BuildOutput(
            "remote".to_string(),
            "🔌 WebSocket connected successfully".to_string(),
        ));

        // Clone variables for the tasks
        let session_id_keepalive = session_id.clone();
        let server_url_keepalive = server_url.clone();
        let tx_keepalive = tx.clone();
        let tx_logs = tx.clone();

        // Spawn keep-alive task
        let keepalive_handle = tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(60));
            let client = reqwest::Client::new();

            loop {
                interval.tick().await;

                let keepalive_request = KeepAliveRequest {
                    session_id: session_id_keepalive.clone(),
                };

                let keepalive_url = format!(
                    "{}/api/v1/monitor/keepalive",
                    server_url_keepalive.trim_end_matches('/')
                );

                match client
                    .post(&keepalive_url)
                    .json(&keepalive_request)
                    .send()
                    .await
                {
                    Ok(response) => {
                        match response.json::<KeepAliveResponse>().await {
                            Ok(keepalive_response) => {
                                if keepalive_response.success {
                                    // Send a subtle keep-alive confirmation to logs
                                    let _ = tx_keepalive.send(AppEvent::BuildOutput(
                                        "remote".to_string(),
                                        "💓 Session keep-alive sent".to_string(),
                                    ));
                                } else {
                                    let _ = tx_keepalive.send(AppEvent::BuildOutput(
                                        "remote".to_string(),
                                        format!(
                                            "⚠️ Keep-alive failed: {}",
                                            keepalive_response.message
                                        ),
                                    ));
                                }
                            }
                            Err(e) => {
                                let _ = tx_keepalive.send(AppEvent::BuildOutput(
                                    "remote".to_string(),
                                    format!("⚠️ Keep-alive parse error: {}", e),
                                ));
                            }
                        }
                    }
                    Err(e) => {
                        let _ = tx_keepalive.send(AppEvent::BuildOutput(
                            "remote".to_string(),
                            format!("⚠️ Keep-alive request failed: {}", e),
                        ));
                    }
                }
            }
        });

        // Spawn WebSocket message handling task
        let ws_handle = tokio::spawn(async move {
            while let Some(msg) = ws_receiver.next().await {
                match msg {
                    Ok(Message::Text(text)) => {
                        // Parse the WebSocket message
                        match serde_json::from_str::<WebSocketMessage>(&text) {
                            Ok(ws_msg) => {
                                match ws_msg.message_type.as_str() {
                                    "log" => {
                                        if let Some(content) = ws_msg.content {
                                            // Send log content to TUI with remote indicator
                                            let formatted_log = if content.trim().is_empty() {
                                                content // Keep empty lines as-is
                                            } else if content.starts_with("[") {
                                                // ESP logs usually start with [timestamp], keep as-is
                                                content
                                            } else {
                                                // Add remote indicator for other logs
                                                format!("📡 {}", content)
                                            };
                                            let _ = tx_logs.send(AppEvent::BuildOutput(
                                                "remote".to_string(),
                                                formatted_log,
                                            ));
                                        }
                                    }
                                    "connection" => {
                                        if let Some(message) = ws_msg.message {
                                            let _ = tx_logs.send(AppEvent::BuildOutput(
                                                "remote".to_string(),
                                                format!("🔗 {}", message),
                                            ));
                                        }
                                    }
                                    "error" => {
                                        if let Some(error) = ws_msg.error {
                                            let _ = tx_logs.send(AppEvent::BuildOutput(
                                                "remote".to_string(),
                                                format!("❌ WebSocket error: {}", error),
                                            ));
                                        }
                                    }
                                    _ => {
                                        // Unknown message type, log as-is
                                        let _ = tx_logs.send(AppEvent::BuildOutput(
                                            "remote".to_string(),
                                            format!("📨 {}", text),
                                        ));
                                    }
                                }
                            }
                            Err(e) => {
                                // If we can't parse as JSON, treat as raw log line
                                let _ =
                                    tx_logs.send(AppEvent::BuildOutput("remote".to_string(), text));
                            }
                        }
                    }
                    Ok(Message::Close(_)) => {
                        let _ = tx_logs.send(AppEvent::BuildOutput(
                            "remote".to_string(),
                            "🔌 WebSocket connection closed by server".to_string(),
                        ));
                        break;
                    }
                    Err(e) => {
                        let _ = tx_logs.send(AppEvent::BuildOutput(
                            "remote".to_string(),
                            format!("❌ WebSocket error: {}", e),
                        ));
                        break;
                    }
                    _ => {
                        // Ignore other message types (Binary, Ping, Pong)
                    }
                }
            }

            // WebSocket closed, cancel keep-alive
            keepalive_handle.abort();

            let _ = tx_logs.send(AppEvent::BuildOutput("remote".to_string(), "─".repeat(60)));
            let _ = tx_logs.send(AppEvent::BuildOutput(
                "remote".to_string(),
                "📡 Remote monitoring session ended".to_string(),
            ));
            let _ = tx_logs.send(AppEvent::BuildOutput(
                "remote".to_string(),
                "✅ WebSocket connection closed gracefully".to_string(),
            ));
        });

        // Don't wait for the tasks to complete - they run in background
        // The function returns immediately after starting the tasks
        tokio::spawn(async move {
            let _ = ws_handle.await;
        });

        Ok(())
    }

    async fn execute_action(
        &mut self,
        action: BoardAction,
        tx: mpsc::UnboundedSender<AppEvent>,
    ) -> Result<()> {
        // Handle RemoteFlash specially - it opens the dialog
        if action == BoardAction::RemoteFlash {
            self.remote_action_type = RemoteActionType::Flash;
            self.start_fetching_remote_boards(tx.clone());
            return Ok(());
        }

        // Handle RemoteMonitor specially - it opens the board selection dialog
        if action == BoardAction::RemoteMonitor {
            self.remote_action_type = RemoteActionType::Monitor;

            // Show initial status message
            if self.selected_board < self.boards.len() {
                self.boards[self.selected_board]
                    .log_lines
                    .push("🔍 Attempting to connect to remote ESPBrew server...".to_string());
                self.boards[self.selected_board].status = BuildStatus::Building;
            }

            self.start_fetching_remote_boards(tx.clone());
            return Ok(());
        }

        if self.selected_board >= self.boards.len() {
            return Err(anyhow::anyhow!("No board selected"));
        }

        let board_index = self.selected_board;
        let board = &self.boards[board_index];
        let board_name = board.name.clone();
        let config_file = board.config_file.clone();
        let build_dir = board.build_dir.clone();
        let project_dir = self.project_dir.clone();
        let logs_dir = self.logs_dir.clone();

        // Update status
        self.boards[board_index].status = match action {
            BoardAction::Build => BuildStatus::Building,
            BoardAction::Flash => BuildStatus::Flashing,
            BoardAction::FlashAppOnly => BuildStatus::Flashing,
            BoardAction::GenerateBinary => BuildStatus::Building,
            _ => BuildStatus::Building, // For clean/purge/monitor/generate operations
        };
        self.boards[board_index].last_updated = chrono::Local::now();

        // Clear previous logs for this board
        self.boards[board_index].log_lines.clear();
        self.reset_log_scroll();

        let tx_clone = tx.clone();
        let action_name = action.name().to_string();
        let has_project_handler = self.project_handler.is_some();
        let project_type = self.project_handler.as_ref().map(|h| h.project_type());

        tokio::spawn(async move {
            let log_file = logs_dir.join(format!("{}.log", board_name));
            let result = match action {
                BoardAction::Build => {
                    if let Some(proj_type) = project_type {
                        let registry = ProjectRegistry::new();
                        if let Some(handler) = registry.get_handler_by_type(&proj_type) {
                            Self::build_board_with_handler(
                                handler.as_ref(),
                                &board_name,
                                &project_dir,
                                &config_file,
                                &build_dir,
                                &log_file,
                                tx_clone.clone(),
                            )
                            .await
                        } else {
                            // Fallback to ESP-IDF build for unknown projects
                            Self::build_board(
                                &board_name,
                                &project_dir,
                                &config_file,
                                &build_dir,
                                &log_file,
                                tx_clone.clone(),
                            )
                            .await
                        }
                    } else {
                        // Fallback to ESP-IDF build for unknown projects
                        Self::build_board(
                            &board_name,
                            &project_dir,
                            &config_file,
                            &build_dir,
                            &log_file,
                            tx_clone.clone(),
                        )
                        .await
                    }
                }
                BoardAction::Clean => {
                    if let Some(proj_type) = project_type {
                        let registry = ProjectRegistry::new();
                        if let Some(handler) = registry.get_handler_by_type(&proj_type) {
                            Self::clean_board_with_handler(
                                handler.as_ref(),
                                &board_name,
                                &project_dir,
                                &config_file,
                                &build_dir,
                                &log_file,
                                tx_clone.clone(),
                            )
                            .await
                        } else {
                            // Fallback to ESP-IDF clean for unknown projects
                            Self::clean_board(
                                &board_name,
                                &project_dir,
                                &build_dir,
                                &log_file,
                                tx_clone.clone(),
                            )
                            .await
                        }
                    } else {
                        // Fallback to ESP-IDF clean for unknown projects
                        Self::clean_board(
                            &board_name,
                            &project_dir,
                            &build_dir,
                            &log_file,
                            tx_clone.clone(),
                        )
                        .await
                    }
                }
                BoardAction::Purge => {
                    Self::purge_board(&board_name, &build_dir, &log_file, tx_clone.clone()).await
                }
                BoardAction::Flash => {
                    if let Some(proj_type) = project_type {
                        let registry = ProjectRegistry::new();
                        if let Some(handler) = registry.get_handler_by_type(&proj_type) {
                            Self::flash_board_with_handler(
                                handler.as_ref(),
                                &board_name,
                                &project_dir,
                                &config_file,
                                &build_dir,
                                &log_file,
                                tx_clone.clone(),
                            )
                            .await
                        } else {
                            // Fallback to ESP-IDF flash for unknown projects
                            Self::flash_board_action(
                                &board_name,
                                &project_dir,
                                &build_dir,
                                &log_file,
                                tx_clone.clone(),
                            )
                            .await
                        }
                    } else {
                        // Fallback to ESP-IDF flash for unknown projects
                        Self::flash_board_action(
                            &board_name,
                            &project_dir,
                            &build_dir,
                            &log_file,
                            tx_clone.clone(),
                        )
                        .await
                    }
                }
                BoardAction::FlashAppOnly => {
                    Self::flash_app_only_action(
                        &board_name,
                        &project_dir,
                        &build_dir,
                        &log_file,
                        tx_clone.clone(),
                    )
                    .await
                }
                BoardAction::Monitor => {
                    if let Some(proj_type) = project_type {
                        let registry = ProjectRegistry::new();
                        if let Some(handler) = registry.get_handler_by_type(&proj_type) {
                            Self::monitor_board_with_handler(
                                handler.as_ref(),
                                &board_name,
                                &project_dir,
                                &config_file,
                                &build_dir,
                                &log_file,
                                tx_clone.clone(),
                            )
                            .await
                        } else {
                            // Fallback to ESP-IDF monitor for unknown projects
                            Self::monitor_board(
                                &board_name,
                                &project_dir,
                                &build_dir,
                                &log_file,
                                tx_clone.clone(),
                            )
                            .await
                        }
                    } else {
                        // Fallback to ESP-IDF monitor for unknown projects
                        Self::monitor_board(
                            &board_name,
                            &project_dir,
                            &build_dir,
                            &log_file,
                            tx_clone.clone(),
                        )
                        .await
                    }
                }
                BoardAction::GenerateBinary => {
                    Self::generate_binary_action(
                        &board_name,
                        &project_dir,
                        &build_dir,
                        &log_file,
                        tx_clone.clone(),
                    )
                    .await
                }
                BoardAction::RemoteFlash => {
                    // This should never be reached as RemoteFlash is handled early
                    unreachable!("RemoteFlash should be handled before this match statement")
                }
                BoardAction::RemoteMonitor => {
                    // This should never be reached as RemoteMonitor is handled early
                    unreachable!("RemoteMonitor should be handled before this match statement")
                }
            };

            let _ = tx_clone.send(AppEvent::ActionFinished(
                board_name,
                action_name,
                result.is_ok(),
            ));
        });

        Ok(())
    }

    async fn execute_component_action_sync(&mut self, action: ComponentAction) -> Result<()> {
        if self.selected_component >= self.components.len() {
            return Err(anyhow::anyhow!("No component selected"));
        }

        let component = &self.components[self.selected_component].clone();

        match action {
            ComponentAction::MoveToComponents => {
                if !component.is_managed {
                    return Err(anyhow::anyhow!("Component is not managed"));
                }

                let target_dir = self.project_dir.join("components").join(&component.name);

                // Create components directory if it doesn't exist
                fs::create_dir_all(self.project_dir.join("components"))?;

                // Move the component
                Self::move_directory(&component.path, &target_dir)?;

                // Update the component in our list
                self.components[self.selected_component].path = target_dir;
                self.components[self.selected_component].is_managed = false;

                Ok(())
            }
            ComponentAction::CloneFromRepository => {
                // This is handled asynchronously in the main event loop
                Ok(())
            }
            ComponentAction::Remove => {
                // Remove the component directory
                if component.path.exists() {
                    fs::remove_dir_all(&component.path)?;
                }

                // Remove from our list
                self.components.remove(self.selected_component);

                // Adjust selected component if necessary
                if self.selected_component >= self.components.len() && !self.components.is_empty() {
                    self.selected_component = self.components.len() - 1;
                }

                // Update list state
                if self.components.is_empty() {
                    self.component_list_state.select(None);
                } else {
                    self.component_list_state
                        .select(Some(self.selected_component));
                }

                Ok(())
            }
            ComponentAction::OpenInEditor => {
                // Try to open in default editor (using 'open' on macOS, 'xdg-open' on Linux)
                #[cfg(target_os = "macos")]
                let cmd = "open";
                #[cfg(target_os = "linux")]
                let cmd = "xdg-open";
                #[cfg(target_os = "windows")]
                let cmd = "explorer";

                std::process::Command::new(cmd)
                    .arg(&component.path)
                    .spawn()?;

                Ok(())
            }
        }
    }

    fn move_directory(from: &Path, to: &Path) -> Result<()> {
        // If target exists, remove it first
        if to.exists() {
            fs::remove_dir_all(to)?;
        }

        // Create parent directory if it doesn't exist
        if let Some(parent) = to.parent() {
            fs::create_dir_all(parent)?;
        }

        // Try to rename first (fast if on same filesystem)
        if fs::rename(from, to).is_ok() {
            return Ok(());
        }

        // If rename fails, copy recursively and then remove source
        fn copy_recursive(from: &Path, to: &Path) -> Result<()> {
            if from.is_dir() {
                fs::create_dir_all(to)?;
                for entry in fs::read_dir(from)? {
                    let entry = entry?;
                    let from_path = entry.path();
                    let to_path = to.join(entry.file_name());
                    copy_recursive(&from_path, &to_path)?;
                }
            } else {
                fs::copy(from, to)?;
            }
            Ok(())
        }

        copy_recursive(from, to)?;
        fs::remove_dir_all(from)?;
        Ok(())
    }

    async fn execute_clone_component_async(
        component: ComponentConfig,
        project_dir: PathBuf,
        tx: mpsc::UnboundedSender<AppEvent>,
    ) -> Result<()> {
        if !component.is_managed {
            return Err(anyhow::anyhow!("Component is not managed"));
        }

        let manifest_path = component.path.join("idf_component.yml");
        let repo_url = parse_component_manifest(&manifest_path)?
            .ok_or_else(|| anyhow::anyhow!("No repository URL found in manifest"))?;

        // Check if this is a wrapper component
        if ComponentAction::is_wrapper_component(&component) {
            if let Some(subdirectory) = ComponentAction::find_wrapper_subdirectory(&component) {
                // Handle wrapper component cloning
                Self::clone_wrapper_component(
                    &repo_url,
                    &component,
                    &subdirectory,
                    &project_dir,
                    tx.clone(),
                )
                .await?;

                // Remove the managed component directory
                if component.path.exists() {
                    fs::remove_dir_all(&component.path)?;
                }

                Ok(())
            } else {
                return Err(anyhow::anyhow!(
                    "Wrapper component '{}' subdirectory mapping not found",
                    component.name
                ));
            }
        } else {
            // Handle regular component cloning with progress
            let _ = tx.send(AppEvent::ComponentActionProgress(
                component.name.clone(),
                format!("Cloning repository from {}...", repo_url),
            ));

            let target_dir = project_dir.join("components").join(&component.name);

            // Create components directory if it doesn't exist
            fs::create_dir_all(project_dir.join("components"))?;

            // Clone the repository using async command
            let mut cmd = TokioCommand::new("git");
            cmd.args(["clone", &repo_url, &target_dir.to_string_lossy()])
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());

            let output = cmd.output().await?;

            if !output.status.success() {
                let error = String::from_utf8_lossy(&output.stderr);
                return Err(anyhow::anyhow!("Git clone failed: {}", error));
            }

            let _ = tx.send(AppEvent::ComponentActionProgress(
                component.name.clone(),
                "Removing managed component...".to_string(),
            ));

            // Remove the managed component directory
            if component.path.exists() {
                fs::remove_dir_all(&component.path)?;
            }

            Ok(())
        }
    }

    async fn clone_wrapper_component(
        repo_url: &str,
        component: &ComponentConfig,
        subdirectory: &str,
        project_dir: &Path,
        tx: mpsc::UnboundedSender<AppEvent>,
    ) -> Result<()> {
        // Send progress update
        let _ = tx.send(AppEvent::ComponentActionProgress(
            component.name.clone(),
            "Preparing temporary directory...".to_string(),
        ));

        // Create a temporary directory for cloning the wrapper repository
        let temp_dir = project_dir.join(".tmp_clone");

        // Clean up any existing temp directory
        if temp_dir.exists() {
            fs::remove_dir_all(&temp_dir)?;
        }

        // Send progress update
        let _ = tx.send(AppEvent::ComponentActionProgress(
            component.name.clone(),
            format!("Cloning wrapper repository from {}...", repo_url),
        ));

        // Clone the wrapper repository with recursive submodules
        let mut cmd = TokioCommand::new("git");
        cmd.args([
            "clone",
            "--recursive",
            "--shallow-submodules",
            repo_url,
            &temp_dir.to_string_lossy(),
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

        let output = cmd.output().await?;

        if !output.status.success() {
            // Clean up on failure
            if temp_dir.exists() {
                let _ = fs::remove_dir_all(&temp_dir);
            }
            let error = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow::anyhow!(
                "Git clone with submodules failed: {}",
                error
            ));
        }

        // Send progress update
        let _ = tx.send(AppEvent::ComponentActionProgress(
            component.name.clone(),
            format!("Extracting {} subdirectory...", subdirectory),
        ));

        // Check if the subdirectory exists in the cloned repository
        let subdirectory_path = temp_dir.join(subdirectory);
        if !subdirectory_path.exists() {
            // Clean up on failure
            if temp_dir.exists() {
                let _ = fs::remove_dir_all(&temp_dir);
            }
            return Err(anyhow::anyhow!(
                "Subdirectory '{}' not found in wrapper component",
                subdirectory
            ));
        }

        // Send progress update
        let _ = tx.send(AppEvent::ComponentActionProgress(
            component.name.clone(),
            "Creating components directory...".to_string(),
        ));

        // Create components directory if it doesn't exist
        let components_dir = project_dir.join("components");
        fs::create_dir_all(&components_dir)?;

        // Send progress update
        let _ = tx.send(AppEvent::ComponentActionProgress(
            component.name.clone(),
            format!("Moving component to components/{}...", component.name),
        ));

        // Move the subdirectory to components with the component name
        let target_dir = components_dir.join(&component.name);

        // Remove target if it exists
        if target_dir.exists() {
            fs::remove_dir_all(&target_dir)?;
        }

        Self::move_directory(&subdirectory_path, &target_dir)?;

        // Send progress update
        let _ = tx.send(AppEvent::ComponentActionProgress(
            component.name.clone(),
            "Cleaning up temporary files...".to_string(),
        ));

        // Clean up the temporary directory
        if temp_dir.exists() {
            fs::remove_dir_all(&temp_dir)?;
        }

        Ok(())
    }

    async fn clean_board(
        board_name: &str,
        project_dir: &Path,
        build_dir: &Path,
        log_file: &Path,
        tx: mpsc::UnboundedSender<AppEvent>,
    ) -> Result<()> {
        let current_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let needs_cd = current_dir != *project_dir;
        let sdkconfig_path = build_dir.join("sdkconfig");

        let clean_cmd = if needs_cd {
            format!(
                "cd {} && idf.py -D SDKCONFIG='{}' -B '{}' clean",
                project_dir.display(),
                sdkconfig_path.display(),
                build_dir.display()
            )
        } else {
            format!(
                "idf.py -D SDKCONFIG='{}' -B '{}' clean",
                sdkconfig_path.display(),
                build_dir.display()
            )
        };

        let _ = tx.send(AppEvent::BuildOutput(
            board_name.to_string(),
            format!("🧡 Executing: {}", clean_cmd),
        ));

        let mut cmd = TokioCommand::new("idf.py");
        cmd.current_dir(project_dir)
            .env("PYTHONUNBUFFERED", "1") // Force unbuffered output
            .args([
                "-D",
                &format!("SDKCONFIG={}", sdkconfig_path.display()),
                "-B",
                &build_dir.to_string_lossy(),
                "clean",
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let output = cmd.output().await?;
        let stdout_str = String::from_utf8_lossy(&output.stdout);
        let stderr_str = String::from_utf8_lossy(&output.stderr);

        let log_content = format!(
            "🧡 CLEAN COMMAND: {}\n{}\n{}\n",
            clean_cmd, stdout_str, stderr_str
        );

        fs::write(log_file, &log_content)?;

        // Send output to TUI
        for line in stdout_str.lines().chain(stderr_str.lines()) {
            if !line.trim().is_empty() {
                let _ = tx.send(AppEvent::BuildOutput(
                    board_name.to_string(),
                    line.to_string(),
                ));
            }
        }

        if output.status.success() {
            let _ = tx.send(AppEvent::BuildOutput(
                board_name.to_string(),
                "✅ Clean completed successfully".to_string(),
            ));
            Ok(())
        } else {
            let _ = tx.send(AppEvent::BuildOutput(
                board_name.to_string(),
                format!(
                    "❌ Clean failed (exit code: {})",
                    output.status.code().unwrap_or(-1)
                ),
            ));
            Err(anyhow::anyhow!("Clean failed"))
        }
    }

    async fn purge_board(
        board_name: &str,
        build_dir: &Path,
        log_file: &Path,
        tx: mpsc::UnboundedSender<AppEvent>,
    ) -> Result<()> {
        let _ = tx.send(AppEvent::BuildOutput(
            board_name.to_string(),
            format!("🗑️ Purging build directory: {}", build_dir.display()),
        ));

        if build_dir.exists() {
            match fs::remove_dir_all(build_dir) {
                Ok(_) => {
                    let log_content =
                        format!("🗑️ PURGE: Successfully deleted {}\n", build_dir.display());
                    fs::write(log_file, &log_content)?;

                    let _ = tx.send(AppEvent::BuildOutput(
                        board_name.to_string(),
                        "✅ Build directory purged successfully".to_string(),
                    ));
                    Ok(())
                }
                Err(e) => {
                    let log_content = format!("🗑️ PURGE FAILED: {}\n", e);
                    fs::write(log_file, &log_content)?;

                    let _ = tx.send(AppEvent::BuildOutput(
                        board_name.to_string(),
                        format!("❌ Failed to purge build directory: {}", e),
                    ));
                    Err(anyhow::anyhow!("Purge failed: {}", e))
                }
            }
        } else {
            let log_content = "🗑️ PURGE: Build directory does not exist\n";
            fs::write(log_file, log_content)?;

            let _ = tx.send(AppEvent::BuildOutput(
                board_name.to_string(),
                "📁 Build directory does not exist".to_string(),
            ));
            Ok(())
        }
    }

    async fn flash_board_action(
        board_name: &str,
        project_dir: &Path,
        build_dir: &Path,
        log_file: &Path,
        tx: mpsc::UnboundedSender<AppEvent>,
    ) -> Result<()> {
        let current_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let needs_cd = current_dir != *project_dir;
        let sdkconfig_path = build_dir.join("sdkconfig");

        let flash_cmd = if needs_cd {
            format!(
                "cd {} && idf.py -D SDKCONFIG='{}' -B '{}' flash",
                project_dir.display(),
                sdkconfig_path.display(),
                build_dir.display()
            )
        } else {
            format!(
                "idf.py -D SDKCONFIG='{}' -B '{}' flash",
                sdkconfig_path.display(),
                build_dir.display()
            )
        };

        let _ = tx.send(AppEvent::BuildOutput(
            board_name.to_string(),
            format!("🔥 Executing: {}", flash_cmd),
        ));

        let mut cmd = TokioCommand::new("idf.py");
        cmd.current_dir(project_dir)
            .env("PYTHONUNBUFFERED", "1") // Force unbuffered output
            .args([
                "-D",
                &format!("SDKCONFIG={}", sdkconfig_path.display()),
                "-B",
                &build_dir.to_string_lossy(),
                "flash",
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = cmd.spawn()?;
        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();

        let tx_stdout = tx.clone();
        let tx_stderr = tx.clone();
        let board_name_stdout = board_name.to_string();
        let board_name_stderr = board_name.to_string();
        let log_file_clone = log_file.to_path_buf();

        // Handle stdout
        tokio::spawn(async move {
            let mut reader = BufReader::new(stdout);
            let mut log_content = format!("🔥 FLASH COMMAND: {}\n", flash_cmd);
            let mut buffer = String::new();

            while reader.read_line(&mut buffer).await.unwrap_or(0) > 0 {
                let line = buffer.trim().to_string();
                log_content.push_str(&format!("{}\n", line));
                let _ = fs::write(&log_file_clone, &log_content);
                let _ = tx_stdout.send(AppEvent::BuildOutput(board_name_stdout.clone(), line));
                buffer.clear();
            }
        });

        // Handle stderr
        tokio::spawn(async move {
            let mut reader = BufReader::new(stderr);
            let mut buffer = String::new();

            while reader.read_line(&mut buffer).await.unwrap_or(0) > 0 {
                let line = buffer.trim().to_string();
                let _ = tx_stderr.send(AppEvent::BuildOutput(board_name_stderr.clone(), line));
                buffer.clear();
            }
        });

        let status = child.wait().await?;
        if status.success() {
            Ok(())
        } else {
            Err(anyhow::anyhow!("Flash failed"))
        }
    }

    async fn flash_app_only_action(
        board_name: &str,
        project_dir: &Path,
        build_dir: &Path,
        log_file: &Path,
        tx: mpsc::UnboundedSender<AppEvent>,
    ) -> Result<()> {
        let current_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let needs_cd = current_dir != *project_dir;
        let sdkconfig_path = build_dir.join("sdkconfig");

        let flash_cmd = if needs_cd {
            format!(
                "cd {} && idf.py -D SDKCONFIG='{}' -B '{}' app-flash",
                project_dir.display(),
                sdkconfig_path.display(),
                build_dir.display()
            )
        } else {
            format!(
                "idf.py -D SDKCONFIG='{}' -B '{}' app-flash",
                sdkconfig_path.display(),
                build_dir.display()
            )
        };

        let _ = tx.send(AppEvent::BuildOutput(
            board_name.to_string(),
            format!("⚡ Executing: {}", flash_cmd),
        ));

        let mut cmd = TokioCommand::new("idf.py");
        cmd.current_dir(project_dir)
            .env("PYTHONUNBUFFERED", "1") // Force unbuffered output
            .args([
                "-D",
                &format!("SDKCONFIG={}", sdkconfig_path.display()),
                "-B",
                &build_dir.to_string_lossy(),
                "app-flash",
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = cmd.spawn()?;
        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();

        let tx_stdout = tx.clone();
        let tx_stderr = tx.clone();
        let board_name_stdout = board_name.to_string();
        let board_name_stderr = board_name.to_string();
        let log_file_clone = log_file.to_path_buf();

        // Handle stdout
        tokio::spawn(async move {
            let mut reader = BufReader::new(stdout);
            let mut log_content = format!("⚡ APP-FLASH COMMAND: {}\n", flash_cmd);
            let mut buffer = String::new();

            while reader.read_line(&mut buffer).await.unwrap_or(0) > 0 {
                let line = buffer.trim().to_string();
                log_content.push_str(&format!("{}\n", line));
                let _ = fs::write(&log_file_clone, &log_content);
                let _ = tx_stdout.send(AppEvent::BuildOutput(board_name_stdout.clone(), line));
                buffer.clear();
            }
        });

        // Handle stderr
        tokio::spawn(async move {
            let mut reader = BufReader::new(stderr);
            let mut buffer = String::new();

            while reader.read_line(&mut buffer).await.unwrap_or(0) > 0 {
                let line = buffer.trim().to_string();
                let _ = tx_stderr.send(AppEvent::BuildOutput(board_name_stderr.clone(), line));
                buffer.clear();
            }
        });

        let status = child.wait().await?;
        if status.success() {
            Ok(())
        } else {
            Err(anyhow::anyhow!("App flash failed"))
        }
    }

    async fn monitor_board(
        board_name: &str,
        project_dir: &Path,
        build_dir: &Path,
        log_file: &Path,
        tx: mpsc::UnboundedSender<AppEvent>,
    ) -> Result<()> {
        let current_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let needs_cd = current_dir != *project_dir;
        let sdkconfig_path = build_dir.join("sdkconfig");

        let monitor_cmd = if needs_cd {
            format!(
                "cd {} && idf.py -D SDKCONFIG='{}' -B '{}' flash monitor",
                project_dir.display(),
                sdkconfig_path.display(),
                build_dir.display()
            )
        } else {
            format!(
                "idf.py -D SDKCONFIG='{}' -B '{}' flash monitor",
                sdkconfig_path.display(),
                build_dir.display()
            )
        };

        let _ = tx.send(AppEvent::BuildOutput(
            board_name.to_string(),
            format!("📺 Executing: {}", monitor_cmd),
        ));

        let _ = tx.send(AppEvent::BuildOutput(
            board_name.to_string(),
            "Note: Monitor will run in background. Use Ctrl+] to exit when focus returns."
                .to_string(),
        ));

        let mut cmd = TokioCommand::new("idf.py");
        cmd.current_dir(project_dir)
            .env("PYTHONUNBUFFERED", "1") // Force unbuffered output
            .args([
                "-D",
                &format!("SDKCONFIG={}", sdkconfig_path.display()),
                "-B",
                &build_dir.to_string_lossy(),
                "flash",
                "monitor",
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = cmd.spawn()?;
        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();

        let tx_stdout = tx.clone();
        let tx_stderr = tx.clone();
        let board_name_stdout = board_name.to_string();
        let board_name_stderr = board_name.to_string();
        let log_file_clone = log_file.to_path_buf();

        // Handle stdout
        tokio::spawn(async move {
            let mut reader = BufReader::new(stdout);
            let mut log_content = format!("📺 MONITOR COMMAND: {}\n", monitor_cmd);
            let mut buffer = String::new();

            while reader.read_line(&mut buffer).await.unwrap_or(0) > 0 {
                let line = buffer.trim().to_string();
                log_content.push_str(&format!("{}\n", line));
                let _ = fs::write(&log_file_clone, &log_content);
                let _ = tx_stdout.send(AppEvent::BuildOutput(board_name_stdout.clone(), line));
                buffer.clear();
            }
        });

        // Handle stderr
        tokio::spawn(async move {
            let mut reader = BufReader::new(stderr);
            let mut buffer = String::new();

            while reader.read_line(&mut buffer).await.unwrap_or(0) > 0 {
                let line = buffer.trim().to_string();
                let _ = tx_stderr.send(AppEvent::BuildOutput(board_name_stderr.clone(), line));
                buffer.clear();
            }
        });

        let status = child.wait().await?;
        if status.success() {
            Ok(())
        } else {
            Err(anyhow::anyhow!("Monitor failed"))
        }
    }

    async fn generate_binary_action(
        board_name: &str,
        project_dir: &Path,
        build_dir: &Path,
        log_file: &Path,
        tx: mpsc::UnboundedSender<AppEvent>,
    ) -> Result<()> {
        let current_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let needs_cd = current_dir != *project_dir;

        // Check if build directory exists
        if !build_dir.exists() {
            let _ = tx.send(AppEvent::BuildOutput(
                board_name.to_string(),
                "❌ Build directory does not exist. Please build first.".to_string(),
            ));
            return Err(anyhow::anyhow!("Build directory does not exist"));
        }

        // Determine target chip from board name
        let target = if board_name.contains("esp32p4") || board_name.contains("m5stack_tab5") {
            "esp32p4"
        } else if board_name.contains("esp32c6") {
            "esp32c6"
        } else if board_name.contains("esp32c3") {
            "esp32c3"
        } else {
            "esp32s3" // default
        };

        let binary_name = format!("{}-{}.bin", board_name, target);
        let output_path = project_dir.join(&binary_name);

        let generate_cmd = if needs_cd {
            format!(
                "cd {} && esptool.py --chip {} merge_bin -o {} \"@{}/flash_args\"",
                project_dir.display(),
                target,
                binary_name,
                build_dir.display()
            )
        } else {
            format!(
                "esptool.py --chip {} merge_bin -o {} \"@{}/flash_args\"",
                target,
                binary_name,
                build_dir.display()
            )
        };

        let _ = tx.send(AppEvent::BuildOutput(
            board_name.to_string(),
            format!("📦 Executing: {}", generate_cmd),
        ));

        // Check if flash_args exists, if not, construct flash arguments manually
        let flash_args_path = build_dir.join("flash_args");
        let mut manual_args = Vec::new();

        if !flash_args_path.exists() {
            let _ = tx.send(AppEvent::BuildOutput(
                board_name.to_string(),
                "📋 Constructing flash arguments manually...".to_string(),
            ));

            // Manually construct typical ESP32 flash layout
            // These are the standard offsets for most ESP32 projects
            manual_args.extend_from_slice(&[
                "--chip".to_string(),
                target.to_string(),
                "merge_bin".to_string(),
                "-o".to_string(),
                output_path.to_string_lossy().to_string(),
            ]);

            // Add standard flash components with their typical offsets
            let bootloader_path = build_dir.join("bootloader").join("bootloader.bin");
            let partition_table_path = build_dir
                .join("partition_table")
                .join("partition-table.bin");

            // Find the main app binary (typically named after the project)
            let mut app_binary = None;
            if let Ok(entries) = fs::read_dir(build_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                        if name.ends_with(".bin")
                            && !name.starts_with("bootloader")
                            && !name.contains("partition")
                            && !name.contains("ota")
                        {
                            app_binary = Some(path);
                            break;
                        }
                    }
                }
            }

            // Bootloader at 0x0 (or 0x1000 for some chips)
            let bootloader_offset = if target == "esp32c3" || target == "esp32c6" {
                "0x0"
            } else {
                "0x1000"
            };
            if bootloader_path.exists() {
                manual_args.push(bootloader_offset.to_string());
                manual_args.push(bootloader_path.to_string_lossy().to_string());
            }

            // Partition table at 0x8000
            if partition_table_path.exists() {
                manual_args.push("0x8000".to_string());
                manual_args.push(partition_table_path.to_string_lossy().to_string());
            }

            // App binary at 0x10000
            if let Some(app_path) = app_binary {
                manual_args.push("0x10000".to_string());
                manual_args.push(app_path.to_string_lossy().to_string());
            } else {
                let _ = tx.send(AppEvent::BuildOutput(
                    board_name.to_string(),
                    "❌ Could not find main application binary".to_string(),
                ));
                return Err(anyhow::anyhow!("Main application binary not found"));
            }

            // Check for storage.bin (assets/data partition)
            let storage_path = build_dir.join("storage.bin");
            if storage_path.exists() {
                // Storage typically goes at a higher offset like 0x210000, but this varies
                // For now, we'll skip it unless we can determine the correct offset
                let _ = tx.send(AppEvent::BuildOutput(
                    board_name.to_string(),
                    "📝 Found storage.bin, but skipping (offset unknown)".to_string(),
                ));
            }

            let _ = tx.send(AppEvent::BuildOutput(
                board_name.to_string(),
                format!(
                    "📋 Using manual flash layout: bootloader@{}, partition@0x8000, app@0x10000",
                    bootloader_offset
                ),
            ));
        }

        // Now run the merge command
        let mut cmd = TokioCommand::new("esptool.py");
        cmd.current_dir(build_dir).env("PYTHONUNBUFFERED", "1"); // Force unbuffered output

        if flash_args_path.exists() {
            // Use the existing flash_args file
            cmd.args([
                "--chip",
                target,
                "merge_bin",
                "-o",
                &output_path.to_string_lossy(),
                "@flash_args",
            ]);
        } else {
            // Use our manually constructed arguments
            cmd.args(&manual_args);
        }

        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

        let mut child = cmd.spawn()?;
        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();

        let tx_stdout = tx.clone();
        let tx_stderr = tx.clone();
        let board_name_stdout = board_name.to_string();
        let board_name_stderr = board_name.to_string();
        let log_file_clone = log_file.to_path_buf();

        // Handle stdout
        tokio::spawn(async move {
            let mut reader = BufReader::new(stdout);
            let mut log_content = format!("📦 GENERATE BINARY COMMAND: {}\n", generate_cmd);
            let mut buffer = String::new();

            while reader.read_line(&mut buffer).await.unwrap_or(0) > 0 {
                let line = buffer.trim().to_string();
                log_content.push_str(&format!("{}\n", line));
                let _ = fs::write(&log_file_clone, &log_content);
                let _ = tx_stdout.send(AppEvent::BuildOutput(board_name_stdout.clone(), line));
                buffer.clear();
            }
        });

        // Handle stderr
        tokio::spawn(async move {
            let mut reader = BufReader::new(stderr);
            let mut buffer = String::new();

            while reader.read_line(&mut buffer).await.unwrap_or(0) > 0 {
                let line = buffer.trim().to_string();
                let _ = tx_stderr.send(AppEvent::BuildOutput(board_name_stderr.clone(), line));
                buffer.clear();
            }
        });

        let status = child.wait().await?;
        if status.success() {
            let _ = tx.send(AppEvent::BuildOutput(
                board_name.to_string(),
                format!(
                    "✅ Binary generated successfully: {}",
                    output_path.display()
                ),
            ));
            Ok(())
        } else {
            let _ = tx.send(AppEvent::BuildOutput(
                board_name.to_string(),
                "❌ Binary generation failed".to_string(),
            ));
            Err(anyhow::anyhow!("Binary generation failed"))
        }
    }

    // Monitoring modal methods
    async fn show_monitor_modal(&mut self, tx: mpsc::UnboundedSender<AppEvent>) -> Result<()> {
        if self.show_monitor_modal {
            return Ok(()); // Already showing
        }

        // Get selected board and server URL
        let (board_name, server_url) = {
            let selected_board = self.boards.get(self.selected_board);
            let server_url = self
                .server_url
                .as_deref()
                .unwrap_or("http://localhost:8080")
                .to_string();
            (selected_board.map(|b| b.name.clone()), server_url)
        };

        if let Some(board_name) = board_name {
            // Show the modal first
            self.show_monitor_modal = true;
            self.monitor_board_id = Some(format!("board__{}", board_name));
            self.monitor_logs.clear();
            self.monitor_connected = false;

            // Start monitoring session
            self.start_monitoring_session(&server_url, tx).await?;
        }

        Ok(())
    }

    fn hide_monitor_modal(&mut self) {
        // Stop monitoring session in background without blocking the UI
        self.stop_monitoring_session_async();

        // Clear modal state immediately
        self.show_monitor_modal = false;
        self.monitor_connected = false;
        self.monitor_session_id = None;
        self.monitor_board_id = None;
    }

    fn stop_monitoring_session_async(&mut self) {
        if let Some(session_id) = &self.monitor_session_id {
            let server_url = self
                .server_url
                .as_deref()
                .unwrap_or("http://localhost:8080")
                .to_string();
            let session_id = session_id.clone();

            // Add immediate feedback to logs
            self.monitor_logs
                .push("[SYSTEM] Stopping monitoring session...".to_string());

            // Spawn async task to stop the session with better error handling
            tokio::spawn(async move {
                let stop_request = StopMonitorRequest {
                    session_id: session_id.clone(),
                };

                let client = reqwest::Client::builder()
                    .timeout(std::time::Duration::from_secs(3))
                    .build();

                match client {
                    Ok(client) => {
                        let stop_url =
                            format!("{}/api/v1/monitor/stop", server_url.trim_end_matches('/'));
                        match client.post(&stop_url).json(&stop_request).send().await {
                            Ok(response) => {
                                if response.status().is_success() {
                                    // Session stopped successfully - no need to log in TUI mode
                                } else {
                                    // Session stop failed - could add to event system if needed
                                }
                            }
                            Err(_e) => {
                                // Failed to send stop request - could add to event system if needed
                            }
                        }
                    }
                    Err(_e) => {
                        // Failed to create HTTP client - silent fail in TUI mode
                    }
                }
            });
        } else {
            self.monitor_logs
                .push("[SYSTEM] No active session to stop".to_string());
        }
    }

    async fn hide_monitor_modal_async(&mut self) {
        // Stop monitoring session on server if we have a session ID
        if let Some(session_id) = &self.monitor_session_id {
            let server_url = self
                .server_url
                .as_deref()
                .unwrap_or("http://localhost:8080");

            let stop_request = StopMonitorRequest {
                session_id: session_id.clone(),
            };

            let client = reqwest::Client::new();
            let stop_url = format!("{}/api/v1/monitor/stop", server_url.trim_end_matches('/'));

            // Add timeout to prevent hanging
            let client_with_timeout = reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(5))
                .build()
                .unwrap_or(client);

            match client_with_timeout
                .post(&stop_url)
                .json(&stop_request)
                .send()
                .await
            {
                Ok(response) => {
                    if response.status().is_success() {
                        // Add to logs to show session was stopped
                        self.monitor_logs
                            .push("[SYSTEM] Monitoring session stopped on server".to_string());
                    } else {
                        self.monitor_logs.push(format!(
                            "[SYSTEM] Failed to stop session: HTTP {}",
                            response.status()
                        ));
                    }
                }
                Err(e) => {
                    // Add error to logs for debugging
                    self.monitor_logs
                        .push(format!("[SYSTEM] Session cleanup error: {}", e));
                }
            }
        } else {
            self.monitor_logs
                .push("[SYSTEM] No active session to stop".to_string());
        }

        // Clear modal state
        self.show_monitor_modal = false;
        self.monitor_connected = false;
        // Don't clear logs immediately so user can see the cleanup message
        // self.monitor_logs.clear();
        self.monitor_session_id = None;
        self.monitor_board_id = None;
    }

    async fn start_monitoring_session(
        &mut self,
        server_url: &str,
        tx: mpsc::UnboundedSender<AppEvent>,
    ) -> Result<()> {
        let board_id = self.monitor_board_id.clone().unwrap_or_default();

        // Create monitor request
        let request = MonitorRequest {
            board_id: board_id.clone(),
            baud_rate: Some(115200),
            filters: None,
        };

        // Send HTTP request to start monitoring
        let client = reqwest::Client::new();
        let url = format!("{}/api/v1/monitor/start", server_url.trim_end_matches('/'));

        let response = client
            .post(&url)
            .json(&request)
            .send()
            .await?
            .error_for_status()?;

        let monitor_response: MonitorResponse = response.json().await?;

        if monitor_response.success {
            if let (Some(session_id), Some(ws_url)) =
                (monitor_response.session_id, monitor_response.websocket_url)
            {
                self.monitor_session_id = Some(session_id.clone());

                // Start WebSocket connection
                let ws_url_full = format!(
                    "ws://{}{}",
                    server_url
                        .strip_prefix("http://")
                        .unwrap_or(server_url)
                        .trim_end_matches('/'),
                    ws_url
                );

                self.start_websocket_connection(ws_url_full, session_id, tx)
                    .await?;
            }
        } else {
            return Err(anyhow::anyhow!(
                "Failed to start monitoring: {}",
                monitor_response.message
            ));
        }

        Ok(())
    }

    async fn start_websocket_connection(
        &mut self,
        ws_url: String,
        session_id: String,
        tx: mpsc::UnboundedSender<AppEvent>,
    ) -> Result<()> {
        // Spawn WebSocket connection task
        tokio::spawn(async move {
            match connect_async(&ws_url).await {
                Ok((ws_stream, _)) => {
                    let _ = tx.send(AppEvent::MonitorLogReceived(format!(
                        "[SYSTEM] WebSocket connected to: {}",
                        ws_url
                    )));
                    let _ = tx.send(AppEvent::MonitorConnected(session_id.clone()));

                    let (mut _write, mut read) = ws_stream.split();
                    let _ = tx.send(AppEvent::MonitorLogReceived(
                        "[SYSTEM] WebSocket streaming started - waiting for logs...".to_string(),
                    ));

                    // Read messages from WebSocket
                    while let Some(msg) = read.next().await {
                        match msg {
                            Ok(Message::Text(text)) => {
                                // Parse WebSocket message
                                if let Ok(ws_msg) = serde_json::from_str::<WebSocketMessage>(&text)
                                {
                                    match ws_msg.message_type.as_str() {
                                        "log" => {
                                            if let Some(content) = ws_msg.content {
                                                let _ =
                                                    tx.send(AppEvent::MonitorLogReceived(content));
                                            }
                                        }
                                        "connection" => {
                                            // Connection established message
                                        }
                                        "error" => {
                                            if let Some(error) = ws_msg.error {
                                                let _ = tx.send(AppEvent::MonitorError(error));
                                            }
                                        }
                                        _ => {}
                                    }
                                }
                            }
                            Ok(Message::Close(_)) => {
                                let _ = tx.send(AppEvent::MonitorDisconnected);
                                break;
                            }
                            Err(e) => {
                                let error_msg = e.to_string();
                                if error_msg.contains("Connection reset")
                                    || error_msg.contains("protocol error")
                                {
                                    // Expected disconnection during reset - attempt to reconnect
                                    let _ = tx.send(AppEvent::MonitorLogReceived(
                                        "[SYSTEM] WebSocket disconnected during board reset (expected)".to_string()
                                    ));
                                    let _ = tx.send(AppEvent::MonitorLogReceived(
                                        "[SYSTEM] Attempting to reconnect to capture boot logs..."
                                            .to_string(),
                                    ));

                                    // Wait a moment for reset to complete
                                    tokio::time::sleep(tokio::time::Duration::from_millis(3000))
                                        .await;

                                    // Try to find new monitoring session for reconnection
                                    let server_url_for_reconnect = ws_url
                                        .replace("ws://", "http://")
                                        .split('/')
                                        .take(3)
                                        .collect::<Vec<_>>()
                                        .join("/");

                                    if let Ok(new_session) =
                                        Self::find_latest_monitoring_session_for_tui(
                                            &server_url_for_reconnect,
                                            &session_id,
                                        )
                                        .await
                                    {
                                        if let Some(new_ws_url) = new_session {
                                            let _ = tx.send(AppEvent::MonitorLogReceived(
                                                "[SYSTEM] Found new session - reconnecting..."
                                                    .to_string(),
                                            ));

                                            // Spawn new connection with the new session
                                            let tx_reconnect = tx.clone();
                                            tokio::spawn(async move {
                                                if let Err(reconnect_err) =
                                                    Self::reconnect_websocket(
                                                        new_ws_url,
                                                        tx_reconnect,
                                                    )
                                                    .await
                                                {
                                                    // If reconnection fails, just log it
                                                    let _ = tx.send(AppEvent::MonitorLogReceived(
                                                        format!(
                                                            "[SYSTEM] Reconnection failed: {}",
                                                            reconnect_err
                                                        ),
                                                    ));
                                                }
                                            });

                                            break; // Exit current connection loop
                                        }
                                    }

                                    let _ = tx.send(AppEvent::MonitorLogReceived(
                                        "[SYSTEM] Board reset completed - use web interface for persistent monitoring".to_string()
                                    ));
                                } else {
                                    let _ = tx.send(AppEvent::MonitorError(format!(
                                        "WebSocket error: {}",
                                        e
                                    )));
                                }
                                break;
                            }
                            _ => {}
                        }
                    }
                }
                Err(e) => {
                    let _ = tx.send(AppEvent::MonitorError(format!(
                        "Failed to connect to WebSocket: {}",
                        e
                    )));
                }
            }
        });

        Ok(())
    }

    // Helper function to find latest monitoring session for TUI reconnection
    async fn find_latest_monitoring_session_for_tui(
        server_url: &str,
        original_session_id: &str,
    ) -> Result<Option<String>> {
        let client = reqwest::Client::new();
        let url = format!(
            "{}/api/v1/monitor/sessions",
            server_url.trim_end_matches('/')
        );

        match client.get(&url).send().await {
            Ok(response) => {
                if let Ok(sessions_response) = response.json::<serde_json::Value>().await {
                    if let Some(sessions) =
                        sessions_response.get("sessions").and_then(|s| s.as_array())
                    {
                        // Find the most recent session that's not the original one
                        for session in sessions {
                            if let Some(session_id) =
                                session.get("session_id").and_then(|s| s.as_str())
                            {
                                if session_id != original_session_id {
                                    let ws_url = format!(
                                        "ws://{}/ws/monitor/{}",
                                        server_url
                                            .trim_start_matches("http://")
                                            .trim_start_matches("https://")
                                            .trim_end_matches('/'),
                                        session_id
                                    );
                                    return Ok(Some(ws_url));
                                }
                            }
                        }
                    }
                }
            }
            Err(_) => {}
        }

        Ok(None)
    }

    // Helper function to reconnect WebSocket for TUI
    async fn reconnect_websocket(
        ws_url: String,
        tx: mpsc::UnboundedSender<AppEvent>,
    ) -> Result<()> {
        use futures_util::StreamExt;
        use tokio_tungstenite::connect_async;

        match connect_async(&ws_url).await {
            Ok((ws_stream, _)) => {
                let _ = tx.send(AppEvent::MonitorLogReceived(
                    "[SYSTEM] Reconnected successfully - capturing boot logs...".to_string(),
                ));

                let (mut _write, mut read) = ws_stream.split();

                // Read messages from reconnected WebSocket
                while let Some(msg) = read.next().await {
                    match msg {
                        Ok(tokio_tungstenite::tungstenite::Message::Text(text)) => {
                            // Parse WebSocket message
                            if let Ok(ws_msg) = serde_json::from_str::<WebSocketMessage>(&text) {
                                match ws_msg.message_type.as_str() {
                                    "log" => {
                                        if let Some(content) = ws_msg.content {
                                            let _ = tx.send(AppEvent::MonitorLogReceived(content));
                                        }
                                    }
                                    "connection" => {
                                        // Connection established message
                                    }
                                    "error" => {
                                        if let Some(error) = ws_msg.error {
                                            let _ = tx.send(AppEvent::MonitorError(error));
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        }
                        Ok(tokio_tungstenite::tungstenite::Message::Close(_)) => {
                            let _ = tx.send(AppEvent::MonitorLogReceived(
                                "[SYSTEM] Reconnected session closed".to_string(),
                            ));
                            break;
                        }
                        Err(e) => {
                            let _ = tx.send(AppEvent::MonitorLogReceived(format!(
                                "[SYSTEM] Reconnection error: {}",
                                e
                            )));
                            break;
                        }
                        _ => {}
                    }
                }
                Ok(())
            }
            Err(e) => Err(anyhow::anyhow!("Failed to reconnect: {}", e)),
        }
    }

    fn scroll_monitor_up(&mut self) {
        if self.monitor_logs.is_empty() {
            return;
        }

        self.monitor_auto_scroll = false;
        if self.monitor_scroll_offset > 0 {
            self.monitor_scroll_offset -= 1;
        }
    }

    fn scroll_monitor_down(&mut self) {
        if self.monitor_logs.is_empty() {
            return;
        }

        self.monitor_auto_scroll = false;
        let max_scroll = self.monitor_logs.len().saturating_sub(1);
        if self.monitor_scroll_offset < max_scroll {
            self.monitor_scroll_offset += 1;
        }
    }

    fn scroll_monitor_page_up(&mut self) {
        if self.monitor_logs.is_empty() {
            return;
        }

        self.monitor_auto_scroll = false;
        self.monitor_scroll_offset = self.monitor_scroll_offset.saturating_sub(10);
    }

    fn scroll_monitor_page_down(&mut self) {
        if self.monitor_logs.is_empty() {
            return;
        }

        self.monitor_auto_scroll = false;
        let max_scroll = self.monitor_logs.len().saturating_sub(10);
        self.monitor_scroll_offset = (self.monitor_scroll_offset + 10).min(max_scroll);
    }

    fn toggle_monitor_auto_scroll(&mut self) {
        self.monitor_auto_scroll = !self.monitor_auto_scroll;

        // If enabling auto-scroll, jump to bottom
        if self.monitor_auto_scroll && !self.monitor_logs.is_empty() {
            self.monitor_scroll_offset = self.monitor_logs.len().saturating_sub(1);
        }
    }

    fn clear_monitor_logs(&mut self) {
        self.monitor_logs.clear();
        self.monitor_scroll_offset = 0;
    }

    async fn execute_monitor_reset(&mut self, tx: mpsc::UnboundedSender<AppEvent>) -> Result<()> {
        if let Some(board_id) = &self.monitor_board_id {
            let server_url = self.get_server_url();

            // Create reset request
            #[derive(Serialize)]
            struct ResetRequest {
                board_id: String,
            }

            let request = ResetRequest {
                board_id: board_id.clone(),
            };

            // Send HTTP request to reset board
            let client = reqwest::Client::new();
            let url = format!("{}/api/v1/reset", server_url.trim_end_matches('/'));

            match client.post(&url).json(&request).send().await {
                Ok(response) => {
                    if response.status().is_success() {
                        // Add reset notification to logs
                        self.monitor_logs
                            .push("[SYSTEM] Board reset initiated...".to_string());
                        let _ = tx.send(AppEvent::MonitorLogReceived(
                            "[SYSTEM] Board reset initiated...".to_string(),
                        ));
                    } else {
                        let _ = tx.send(AppEvent::MonitorError("Reset request failed".to_string()));
                    }
                }
                Err(e) => {
                    let _ = tx.send(AppEvent::MonitorError(format!(
                        "Reset request failed: {}",
                        e
                    )));
                }
            }
        }

        Ok(())
    }

    // Start remote monitoring session and open monitor modal from remote board selection
    async fn start_remote_monitor_modal(
        &mut self,
        tx: mpsc::UnboundedSender<AppEvent>,
    ) -> Result<()> {
        if self.selected_remote_board >= self.remote_boards.len() {
            return Err(anyhow::anyhow!("No remote board selected"));
        }

        let selected_board = &self.remote_boards[self.selected_remote_board];
        let server_url = self.get_server_url();

        // Show the monitor modal first
        self.show_monitor_modal = true;
        self.monitor_board_id = Some(selected_board.id.clone());
        self.monitor_logs.clear();
        self.monitor_connected = false;

        // Add initial log messages for debugging
        self.monitor_logs.push(format!(
            "📺 Starting remote monitoring for: {}",
            selected_board
                .logical_name
                .as_ref()
                .unwrap_or(&selected_board.id)
        ));
        self.monitor_logs
            .push(format!("🔗 Board ID: {}", selected_board.id));
        self.monitor_logs
            .push(format!("🌍 Server URL: {}", server_url));
        self.monitor_logs
            .push("🔄 Connecting to WebSocket...".to_string());

        // Test that monitor modal can receive and display messages
        let _ = tx.send(AppEvent::MonitorLogReceived(
            "[TEST] Monitor modal is receiving events".to_string(),
        ));

        // Start monitoring session
        self.start_monitoring_session(&server_url, tx).await?;

        Ok(())
    }

    // Start automatic server discovery via mDNS
    fn start_server_discovery(&mut self, tx: mpsc::UnboundedSender<AppEvent>) {
        if self.server_discovery_in_progress {
            return; // Already discovering
        }

        self.server_discovery_in_progress = true;
        self.server_discovery_status = "mDNS: scanning...".to_string();
        self.discovered_servers.clear();

        let _ = tx.send(AppEvent::ServerDiscoveryStarted);

        // Spawn mDNS discovery task (use silent version for TUI)
        tokio::spawn(async move {
            match discover_espbrew_servers_silent(5).await {
                Ok(servers) => {
                    let _ = tx.send(AppEvent::ServerDiscoveryCompleted(servers));
                }
                Err(e) => {
                    let _ = tx.send(AppEvent::ServerDiscoveryFailed(e.to_string()));
                }
            }
        });
    }

    // Get the preferred server URL (discovered server takes priority)
    fn get_server_url(&self) -> String {
        // If we have discovered servers, use the first one
        if let Some(server) = self.discovered_servers.first() {
            format!("http://{}:{}", server.ip, server.port)
        } else {
            // Fallback to configured server_url or localhost
            self.server_url
                .as_deref()
                .unwrap_or("http://localhost:8080")
                .to_string()
        }
    }

    // Handle server discovery events
    fn handle_server_discovery_event(&mut self, event: AppEvent) {
        match event {
            AppEvent::ServerDiscoveryStarted => {
                self.server_discovery_in_progress = true;
                self.server_discovery_status = "mDNS: scanning...".to_string();
            }
            AppEvent::ServerDiscovered(server) => {
                self.discovered_servers.push(server);
                self.server_discovery_status =
                    format!("mDNS: found {} server(s)", self.discovered_servers.len());
            }
            AppEvent::ServerDiscoveryCompleted(servers) => {
                self.server_discovery_in_progress = false;
                self.discovered_servers = servers;
                if self.discovered_servers.is_empty() {
                    self.server_discovery_status = "mDNS: no servers".to_string();
                } else {
                    self.server_discovery_status =
                        format!("mDNS: {} server(s) found", self.discovered_servers.len());
                }
            }
            AppEvent::ServerDiscoveryFailed(error) => {
                self.server_discovery_in_progress = false;
                self.server_discovery_status = format!("mDNS: error - {}", error);
            }
            _ => {}
        }
    }

    // Handle monitor events
    fn handle_monitor_event(&mut self, event: AppEvent) {
        match event {
            AppEvent::MonitorLogReceived(log_line) => {
                self.monitor_logs.push(log_line);

                // Limit log buffer size to prevent memory issues
                if self.monitor_logs.len() > 1000 {
                    self.monitor_logs.drain(0..100); // Remove first 100 lines
                    self.monitor_scroll_offset = self.monitor_scroll_offset.saturating_sub(100);
                }

                // Auto-scroll if enabled
                if self.monitor_auto_scroll {
                    self.monitor_scroll_offset = self.monitor_logs.len().saturating_sub(1);
                }
            }
            AppEvent::MonitorConnected(session_id) => {
                self.monitor_connected = true;
                self.monitor_session_id = Some(session_id);
            }
            AppEvent::MonitorDisconnected => {
                self.monitor_connected = false;
                self.monitor_logs
                    .push("[SYSTEM] Connection lost".to_string());
            }
            AppEvent::MonitorError(error) => {
                self.monitor_logs.push(format!("[ERROR] {}", error));
            }
            _ => {}
        }
    }
}

fn ui(f: &mut Frame, app: &App) {
    // Main layout with help bar at bottom
    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(3)])
        .split(f.area());

    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(32), Constraint::Percentage(68)])
        .split(main_chunks[0]);

    // Split left panel into boards (top) and components (bottom)
    let left_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
        .split(chunks[0]);

    // Board list (top of left panel)
    let board_items: Vec<ListItem> = app
        .boards
        .iter()
        .map(|board| {
            let status_symbol = board.status.symbol();
            let time_info = if let Some(duration) = board.build_time {
                format!(" ({}s)", duration.as_secs())
            } else {
                String::new()
            };

            ListItem::new(Line::from(vec![
                Span::styled(status_symbol, Style::default().fg(board.status.color())),
                Span::raw(" "),
                Span::raw(&board.name),
                Span::styled(time_info, Style::default().fg(Color::Gray)),
            ]))
        })
        .collect();

    let project_type_display = if let Some(project_type) = &app.project_type {
        format!(" ({})", project_type.name())
    } else {
        String::new()
    };

    let board_list_title = if app.focused_pane == FocusedPane::BoardList {
        format!("🍺 Boards{} [FOCUSED]", project_type_display)
    } else {
        format!("🍺 Boards{}", project_type_display)
    };

    let board_list_block = if app.focused_pane == FocusedPane::BoardList {
        Block::default()
            .title(board_list_title.clone())
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan))
    } else {
        Block::default()
            .title(board_list_title.clone())
            .borders(Borders::ALL)
    };

    let board_list = List::new(board_items)
        .block(board_list_block)
        .highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        );

    f.render_stateful_widget(board_list, left_chunks[0], &mut app.list_state.clone());

    // Component list (bottom of left panel)
    let component_items: Vec<ListItem> = app
        .components
        .iter()
        .map(|component| {
            let type_indicator = if component.is_managed {
                "📦" // Package icon for managed components
            } else {
                "🔧" // Tool icon for regular components
            };

            let mut spans = vec![
                Span::styled(type_indicator, Style::default().fg(Color::White)),
                Span::raw(" "),
                Span::raw(&component.name),
            ];

            // Add action status if present
            if let Some(action_status) = &component.action_status {
                spans.push(Span::styled(
                    format!(" [{}]", action_status),
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::ITALIC),
                ));
            } else {
                // Add component type status
                if component.is_managed {
                    spans.push(Span::styled(
                        " (managed)",
                        Style::default().fg(Color::Yellow),
                    ));
                } else {
                    spans.push(Span::styled(" (local)", Style::default().fg(Color::Green)));
                }
            }

            ListItem::new(Line::from(spans))
        })
        .collect();

    let component_list_title = if app.focused_pane == FocusedPane::ComponentList {
        "🧩 Components [FOCUSED]"
    } else {
        "🧩 Components"
    };

    let component_list_block = if app.focused_pane == FocusedPane::ComponentList {
        Block::default()
            .title(component_list_title)
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan))
    } else {
        Block::default()
            .title(component_list_title)
            .borders(Borders::ALL)
    };

    let component_list = List::new(component_items)
        .block(component_list_block)
        .highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        );

    f.render_stateful_widget(
        component_list,
        left_chunks[1],
        &mut app.component_list_state.clone(),
    );

    // Right panel - Details
    let right_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(8), Constraint::Min(0)])
        .split(chunks[1]);

    // Board details
    if let Some(selected_board) = app.boards.get(app.selected_board) {
        let details = vec![
            Line::from(vec![
                Span::styled("Board: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(&selected_board.name),
            ]),
            Line::from(vec![
                Span::styled("Status: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::styled(
                    format!(
                        "{} {:?}",
                        selected_board.status.symbol(),
                        selected_board.status
                    ),
                    Style::default().fg(selected_board.status.color()),
                ),
            ]),
            Line::from(vec![
                Span::styled("Config: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(selected_board.config_file.display().to_string()),
            ]),
            Line::from(vec![
                Span::styled("Build Dir: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(selected_board.build_dir.display().to_string()),
            ]),
            Line::from(vec![
                Span::styled("Updated: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(selected_board.last_updated.format("%H:%M:%S").to_string()),
            ]),
        ];

        let details_paragraph = Paragraph::new(details)
            .block(
                Block::default()
                    .title("Board Details")
                    .borders(Borders::ALL),
            )
            .wrap(Wrap { trim: true });

        f.render_widget(details_paragraph, right_chunks[0]);

        // Build log with scrolling support
        let total_lines = selected_board.log_lines.len();
        let available_height = right_chunks[1].height.saturating_sub(2) as usize; // Account for borders

        // Auto-adjust scroll for real-time streaming (show latest content)
        let adjusted_scroll_offset = if total_lines > available_height {
            // For live streaming, prioritize showing the latest content
            let max_scroll = total_lines.saturating_sub(available_height);
            // If we're near the bottom or auto-scrolling, show latest content
            if app.log_scroll_offset >= max_scroll.saturating_sub(3) {
                max_scroll // Stay at bottom for live updates
            } else {
                app.log_scroll_offset // Preserve user's manual scroll position
            }
        } else {
            0
        };

        let log_lines: Vec<Line> = if total_lines > 0 {
            let start_index = adjusted_scroll_offset;
            let end_index = (start_index + available_height).min(total_lines);

            selected_board
                .log_lines
                .get(start_index..end_index)
                .unwrap_or_default()
                .iter()
                .map(|line| App::colorize_log_line(line))
                .collect()
        } else {
            vec![Line::from("No logs available")]
        };

        let log_title = if app.focused_pane == FocusedPane::LogPane {
            if total_lines > 0 {
                format!(
                    "Build Log [FOCUSED] ({}/{} lines, scroll: {}) - Live Updates",
                    (adjusted_scroll_offset + log_lines.len()).min(total_lines),
                    total_lines,
                    adjusted_scroll_offset
                )
            } else {
                "Build Log [FOCUSED] (No logs)".to_string()
            }
        } else if total_lines > 0 {
            format!("Build Log ({} lines) - Live Updates", total_lines)
        } else {
            "Build Log".to_string()
        };

        let log_block = if app.focused_pane == FocusedPane::LogPane {
            Block::default()
                .title(log_title)
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan))
        } else {
            Block::default().title(log_title).borders(Borders::ALL)
        };

        let log_paragraph = Paragraph::new(log_lines)
            .block(log_block)
            .wrap(Wrap { trim: true });

        f.render_widget(log_paragraph, right_chunks[1]);
    }

    // Tool warning modal (project-specific)
    if app.show_tool_warning && !app.tool_warning_acknowledged {
        let area = centered_rect(70, 20, f.area());
        f.render_widget(Clear, area);

        let warning_lines: Vec<Line> = app
            .tool_warning_message
            .split('\n')
            .map(|line| Line::from(line))
            .collect();

        let warning_paragraph = Paragraph::new(warning_lines)
            .block(
                Block::default()
                    .title("Development Tools Warning")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Red)),
            )
            .style(Style::default().bg(Color::Black))
            .wrap(Wrap { trim: true });

        f.render_widget(warning_paragraph, area);
    }
    // Help popup
    else if app.show_help {
        let area = centered_rect(60, 20, f.area());
        f.render_widget(Clear, area);

        let help_text = vec![
            Line::from("🍺 ESPBrew Help"),
            Line::from(""),
            Line::from("Navigation:"),
            Line::from("↑/↓ or j/k    Navigate boards (Board List) / Scroll logs (Log Pane)"),
            Line::from("Tab           Switch between Board List and Log Pane"),
            Line::from("PgUp/PgDn     Scroll logs by page (Log Pane only)"),
            Line::from("Home/End      Jump to top/bottom of logs (Log Pane only)"),
            Line::from(""),
            Line::from("Building:"),
            Line::from("Space or b    Build selected board only"),
            Line::from("x             Build all boards (rebuild all)"),
            Line::from(""),
            Line::from("Other Actions:"),
            Line::from("Enter         Show action menu (Build/Flash/Monitor/Clean/Purge)"),
            Line::from("r             Refresh board list"),
            Line::from("h or ?        Toggle this help"),
            Line::from("q/Ctrl+C/ESC Quit"),
            Line::from(""),
            Line::from("Note: Focused pane is highlighted with cyan border"),
            Line::from("Logs are saved in ./logs/ | Scripts in ./support/"),
            Line::from("Text selection: Mouse support enabled for copy/paste"),
        ];

        let help_paragraph = Paragraph::new(help_text)
            .block(Block::default().title("Help").borders(Borders::ALL))
            .style(Style::default().bg(Color::Black));

        f.render_widget(help_paragraph, area);
    }

    // Help bar at bottom
    let mut help_text = if app.focused_pane == FocusedPane::LogPane {
        vec![
            Span::styled("[↑↓]Scroll ", Style::default().fg(Color::Cyan)),
            Span::styled("[PgUp/PgDn]Page ", Style::default().fg(Color::Cyan)),
            Span::styled("[Home/End]Top/Bottom ", Style::default().fg(Color::Cyan)),
            Span::styled("[Tab]Switch Pane ", Style::default().fg(Color::White)),
            Span::styled("[Enter]Actions ", Style::default().fg(Color::Green)),
        ]
    } else {
        vec![
            Span::styled("[↑↓]Navigate ", Style::default().fg(Color::Cyan)),
            Span::styled("[Tab]Switch Pane ", Style::default().fg(Color::White)),
            Span::styled("[Enter]Actions ", Style::default().fg(Color::Green)),
        ]
    };

    // Add build status and controls
    if app.build_in_progress {
        help_text.extend(vec![Span::styled(
            "🔨 Building... ",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )]);
    } else {
        help_text.extend(vec![
            Span::styled(
                "[Space/B]Build Selected ",
                Style::default().fg(Color::LightYellow),
            ),
            Span::styled("[X]Build All ", Style::default().fg(Color::Yellow)),
        ]);
    }

    // Add remaining controls
    if !app.build_in_progress {
        help_text.push(Span::styled(
            "[R]Refresh ",
            Style::default().fg(Color::Magenta),
        ));
    }
    help_text.extend(vec![
        Span::styled("[H/?]Help ", Style::default().fg(Color::Blue)),
        Span::styled("[Q/Ctrl+C/ESC]Quit ", Style::default().fg(Color::Red)),
    ]);

    // Add server discovery status
    let server_status_color = if app.server_discovery_in_progress {
        Color::Yellow
    } else if app.discovered_servers.is_empty() {
        Color::Gray
    } else {
        Color::Green
    };

    // Show server status and first discovered server info if available
    if !app.discovered_servers.is_empty() {
        let server = &app.discovered_servers[0];
        help_text.push(Span::styled(
            format!(
                "| {} ({}:{}) ",
                app.server_discovery_status, server.ip, server.port
            ),
            Style::default().fg(server_status_color),
        ));
    } else {
        help_text.push(Span::styled(
            format!("| {} ", app.server_discovery_status),
            Style::default().fg(server_status_color),
        ));
    }

    let help_bar = Paragraph::new(Line::from(help_text))
        .block(Block::default().borders(Borders::ALL))
        .style(Style::default().bg(Color::DarkGray));

    f.render_widget(help_bar, main_chunks[1]);

    // Action menu modal
    if app.show_action_menu {
        let area = centered_rect(50, 40, f.area());
        f.render_widget(Clear, area);

        let selected_board_name = if let Some(board) = app.boards.get(app.selected_board) {
            &board.name
        } else {
            "Unknown"
        };

        let action_items: Vec<ListItem> = app
            .available_actions
            .iter()
            .map(|action| {
                ListItem::new(Line::from(vec![
                    Span::raw(action.name()),
                    Span::styled(
                        format!(" - {}", action.description()),
                        Style::default().fg(Color::Gray),
                    ),
                ]))
            })
            .collect();

        let mut action_list_state = ListState::default();
        action_list_state.select(Some(app.action_menu_selected));

        let action_list = List::new(action_items)
            .block(
                Block::default()
                    .title(format!("Actions for: {}", selected_board_name))
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Green)),
            )
            .highlight_style(
                Style::default()
                    .bg(Color::Green)
                    .fg(Color::Black)
                    .add_modifier(Modifier::BOLD),
            );

        f.render_stateful_widget(action_list, area, &mut action_list_state);

        // Instructions at the bottom of the modal
        let instruction_area = Rect {
            x: area.x + 1,
            y: area.y + area.height - 3,
            width: area.width - 2,
            height: 1,
        };

        let instructions = Paragraph::new(Line::from(vec![
            Span::styled("[↑↓]", Style::default().fg(Color::Cyan)),
            Span::raw(" Navigate "),
            Span::styled("[Enter]", Style::default().fg(Color::Green)),
            Span::raw(" Execute "),
            Span::styled("[ESC]", Style::default().fg(Color::Red)),
            Span::raw(" Cancel"),
        ]));

        f.render_widget(instructions, instruction_area);
    }

    // Component action menu modal
    if app.show_component_action_menu {
        let area = centered_rect(50, 40, f.area());
        f.render_widget(Clear, area);

        let selected_component_name =
            if let Some(component) = app.components.get(app.selected_component) {
                &component.name
            } else {
                "Unknown"
            };

        let selected_component = app.components.get(app.selected_component);
        let available_actions: Vec<&ComponentAction> = app
            .available_component_actions
            .iter()
            .filter(|action| {
                if let Some(comp) = selected_component {
                    action.is_available_for(comp)
                } else {
                    false
                }
            })
            .collect();

        let action_items: Vec<ListItem> = available_actions
            .iter()
            .map(|action| {
                ListItem::new(Line::from(vec![
                    Span::raw(action.name()),
                    Span::styled(
                        format!(" - {}", action.description()),
                        Style::default().fg(Color::Gray),
                    ),
                ]))
            })
            .collect();

        let mut component_action_list_state = ListState::default();
        // Ensure the selected index is within bounds of available actions
        let adjusted_selected = app
            .component_action_menu_selected
            .min(available_actions.len().saturating_sub(1));
        if !available_actions.is_empty() {
            component_action_list_state.select(Some(adjusted_selected));
        }

        let component_action_list = List::new(action_items)
            .block(
                Block::default()
                    .title(format!(
                        "Component Actions for: {}",
                        selected_component_name
                    ))
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Magenta)),
            )
            .highlight_style(
                Style::default()
                    .bg(Color::Magenta)
                    .fg(Color::Black)
                    .add_modifier(Modifier::BOLD),
            );

        f.render_stateful_widget(
            component_action_list,
            area,
            &mut component_action_list_state,
        );

        // Instructions at the bottom of the modal
        let instruction_area = Rect {
            x: area.x + 1,
            y: area.y + area.height - 3,
            width: area.width - 2,
            height: 1,
        };

        let instructions = Paragraph::new(Line::from(vec![
            Span::styled("[↑↓]", Style::default().fg(Color::Cyan)),
            Span::raw(" Navigate "),
            Span::styled("[Enter]", Style::default().fg(Color::Green)),
            Span::raw(" Execute "),
            Span::styled("[ESC]", Style::default().fg(Color::Red)),
            Span::raw(" Cancel"),
        ]));

        f.render_widget(instructions, instruction_area);
    }

    // Remote board selection dialog
    if app.show_remote_board_dialog {
        let area = centered_rect(70, 50, f.area());
        f.render_widget(Clear, area);

        let title = "🌐 Remote Board Selection";
        let server_url = &app.get_server_url();
        let server_info = format!(" - Connected to {}", server_url);

        let board_items: Vec<ListItem> = if app.remote_boards_loading {
            vec![ListItem::new(Line::from(vec![
                Span::styled("🔄", Style::default().fg(Color::Yellow)),
                Span::raw(" Loading remote boards..."),
            ]))]
        } else if let Some(ref error) = app.remote_boards_fetch_error {
            vec![ListItem::new(Line::from(vec![
                Span::styled("❌", Style::default().fg(Color::Red)),
                Span::raw(" Connection failed: "),
                Span::styled(error, Style::default().fg(Color::Red)),
            ]))]
        } else {
            app.remote_boards
                .iter()
                .map(|board| {
                    let chip_type_upper = board.chip_type.to_uppercase();
                    ListItem::new(Line::from(vec![
                        Span::styled("📱", Style::default().fg(Color::Cyan)),
                        Span::raw(" "),
                        Span::styled(
                            board.logical_name.as_ref().unwrap_or(&board.id),
                            Style::default().add_modifier(Modifier::BOLD),
                        ),
                        Span::raw(" ("),
                        Span::styled(chip_type_upper, Style::default().fg(Color::Yellow)),
                        Span::raw(") - "),
                        Span::styled(&board.port, Style::default().fg(Color::Gray)),
                        Span::raw(" - "),
                        Span::styled(
                            &board.status,
                            match board.status.as_str() {
                                "Available" => Style::default().fg(Color::Green),
                                "Busy" => Style::default().fg(Color::Red),
                                _ => Style::default().fg(Color::Yellow),
                            },
                        ),
                    ]))
                })
                .collect()
        };

        let mut remote_board_list_state = ListState::default();
        if !app.remote_boards.is_empty() {
            remote_board_list_state.select(Some(app.selected_remote_board));
        }

        let remote_board_list = List::new(board_items)
            .block(
                Block::default()
                    .title(format!("{}{}", title, server_info))
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Cyan)),
            )
            .highlight_style(
                Style::default()
                    .bg(Color::Cyan)
                    .fg(Color::Black)
                    .add_modifier(Modifier::BOLD),
            );

        f.render_stateful_widget(remote_board_list, area, &mut remote_board_list_state);

        // Show remote flash status if available
        if let Some(ref status) = app.remote_flash_status {
            let status_area = Rect {
                x: area.x + 1,
                y: area.y + area.height - 5,
                width: area.width - 2,
                height: 2,
            };

            let status_color = if status.contains("Failed") || status.contains("Error") {
                Color::Red
            } else if status.contains("completed successfully") {
                Color::Green
            } else {
                Color::Yellow
            };

            let status_paragraph = Paragraph::new(status.clone())
                .style(Style::default().fg(status_color))
                .wrap(Wrap { trim: true });

            f.render_widget(status_paragraph, status_area);
        }

        // Show remote monitor status if available
        if let Some(ref status) = app.remote_monitor_status {
            let status_area = Rect {
                x: area.x + 1,
                y: area.y + area.height - 5,
                width: area.width - 2,
                height: 2,
            };

            let status_color = if status.contains("Failed") || status.contains("Error") {
                Color::Red
            } else if status.contains("WebSocket connected") || status.contains("streaming") {
                Color::Green
            } else {
                Color::Yellow
            };

            let status_paragraph = Paragraph::new(status.clone())
                .style(Style::default().fg(status_color))
                .wrap(Wrap { trim: true });

            f.render_widget(status_paragraph, status_area);
        }

        // Instructions at the bottom of the modal
        let instruction_area = Rect {
            x: area.x + 1,
            y: area.y + area.height - 3,
            width: area.width - 2,
            height: 1,
        };

        let instructions = if app.remote_flash_in_progress {
            Paragraph::new(Line::from(vec![
                Span::styled(
                    "🔄 Remote flash in progress...",
                    Style::default().fg(Color::Yellow),
                ),
                Span::styled(" [ESC]", Style::default().fg(Color::Red)),
                Span::raw(" Cancel"),
            ]))
        } else if app.remote_boards_loading {
            Paragraph::new(Line::from(vec![
                Span::styled(
                    "🔄 Connecting to server...",
                    Style::default().fg(Color::Yellow),
                ),
                Span::styled(" [ESC]", Style::default().fg(Color::Red)),
                Span::raw(" Cancel"),
            ]))
        } else if app.remote_boards_fetch_error.is_some() {
            Paragraph::new(Line::from(vec![
                Span::styled("❌ Connection failed", Style::default().fg(Color::Red)),
                Span::styled(" [R]", Style::default().fg(Color::Yellow)),
                Span::raw(" Retry "),
                Span::styled(" [ESC]", Style::default().fg(Color::Red)),
                Span::raw(" Cancel"),
            ]))
        } else if app.remote_boards.is_empty() {
            Paragraph::new(Line::from(vec![
                Span::styled(
                    "⚠️ No remote boards available",
                    Style::default().fg(Color::Red),
                ),
                Span::styled(" [ESC]", Style::default().fg(Color::Red)),
                Span::raw(" Close"),
            ]))
        } else {
            let action_text = match app.remote_action_type {
                RemoteActionType::Flash => " Flash ",
                RemoteActionType::Monitor => " Monitor ",
            };
            Paragraph::new(Line::from(vec![
                Span::styled("[↑↓]", Style::default().fg(Color::Cyan)),
                Span::raw(" Navigate "),
                Span::styled("[Enter]", Style::default().fg(Color::Green)),
                Span::raw(action_text),
                Span::styled("[R]", Style::default().fg(Color::Yellow)),
                Span::raw(" Reset Board "),
                Span::styled("[ESC]", Style::default().fg(Color::Red)),
                Span::raw(" Cancel"),
            ]))
        };

        f.render_widget(instructions, instruction_area);
    }

    // Monitoring modal
    if app.show_monitor_modal {
        let area = centered_rect(80, 70, f.area());
        f.render_widget(Clear, area);

        // Split the modal into header, log area, and footer
        let modal_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Header
                Constraint::Min(10),   // Log area
                Constraint::Length(3), // Footer
            ])
            .split(area);

        // Header with connection status
        let connection_status = if app.monitor_connected {
            ("🟢 Connected", Color::Green)
        } else {
            ("🔴 Disconnected", Color::Red)
        };

        let board_name = app.monitor_board_id.as_deref().unwrap_or("Unknown");
        let header_text = vec![Line::from(vec![
            Span::styled("📺 Serial Monitor - ", Style::default().fg(Color::Cyan)),
            Span::styled(board_name, Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" "),
            Span::styled(
                connection_status.0,
                Style::default().fg(connection_status.1),
            ),
        ])];

        let header = Paragraph::new(header_text)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Cyan)),
            )
            .style(Style::default().bg(Color::Black));

        f.render_widget(header, modal_chunks[0]);

        // Log display area with scrolling
        let total_lines = app.monitor_logs.len();
        let visible_lines = (modal_chunks[1].height.saturating_sub(2)) as usize; // Account for borders

        let (start_line, end_line) = if app.monitor_auto_scroll {
            // Auto-scroll: show the most recent lines
            if total_lines <= visible_lines {
                (0, total_lines)
            } else {
                (total_lines - visible_lines, total_lines)
            }
        } else {
            // Manual scroll: use scroll_offset
            let start = app
                .monitor_scroll_offset
                .min(total_lines.saturating_sub(visible_lines));
            let end = (start + visible_lines).min(total_lines);
            (start, end)
        };

        let log_lines: Vec<Line> = if total_lines > 0 {
            app.monitor_logs[start_line..end_line]
                .iter()
                .map(|line| {
                    // Simple syntax highlighting for common log patterns
                    let line_style = if line.contains("ERROR") || line.contains("FAIL") {
                        Style::default().fg(Color::Red)
                    } else if line.contains("WARN") || line.contains("WARNING") {
                        Style::default().fg(Color::Yellow)
                    } else if line.contains("INFO") {
                        Style::default().fg(Color::Cyan)
                    } else if line.contains("DEBUG") {
                        Style::default().fg(Color::Gray)
                    } else {
                        Style::default().fg(Color::White)
                    };
                    Line::from(Span::styled(line, line_style))
                })
                .collect()
        } else {
            vec![Line::from(Span::styled(
                "Waiting for log data...",
                Style::default().fg(Color::Gray),
            ))]
        };

        let scroll_indicator = if total_lines > visible_lines {
            if app.monitor_auto_scroll {
                " [Auto-scroll: ON] "
            } else {
                &format!(" [{}/{}] ", start_line + 1, total_lines)
            }
        } else {
            " "
        };

        let log_title = format!("Log ({} lines){}", total_lines, scroll_indicator);

        let log_paragraph = Paragraph::new(log_lines)
            .block(
                Block::default()
                    .title(log_title)
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::White)),
            )
            .wrap(Wrap { trim: false })
            .style(Style::default().bg(Color::Black));

        f.render_widget(log_paragraph, modal_chunks[1]);

        // Footer with controls
        let controls = vec![Line::from(vec![
            Span::styled("[↑↓]", Style::default().fg(Color::Cyan)),
            Span::raw(" Scroll "),
            Span::styled("[PgUp/PgDn]", Style::default().fg(Color::Cyan)),
            Span::raw(" Page "),
            Span::styled("[A]", Style::default().fg(Color::Green)),
            Span::raw(" Toggle Auto-scroll "),
            Span::styled("[Ctrl+R]", Style::default().fg(Color::Yellow)),
            Span::raw(" Reset Board "),
            Span::styled("[Ctrl+C]", Style::default().fg(Color::Magenta)),
            Span::raw(" Clear Logs "),
            Span::styled("[ESC]", Style::default().fg(Color::Red)),
            Span::raw(" Close"),
        ])];

        let footer = Paragraph::new(controls)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::White)),
            )
            .style(Style::default().bg(Color::Black));

        f.render_widget(footer, modal_chunks[2]);
    }
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

async fn run_cli_only(mut app: App, command: Option<Commands>) -> Result<()> {
    match command.unwrap_or(Commands::List) {
        Commands::List => {
            println!("🍺 ESPBrew CLI Mode - Project Information");
            println!("Found {} boards:", app.boards.len());

            for board in &app.boards {
                println!("  - {} ({})", board.name, board.config_file.display());
            }

            println!("\nFound {} components:", app.components.len());
            for component in &app.components {
                let component_type = if component.is_managed {
                    "managed"
                } else {
                    "local"
                };
                println!(
                    "  - {} ({}) [{}]",
                    component.name,
                    component.path.display(),
                    component_type
                );
            }

            println!("\nUse 'espbrew --cli-only build' to start building all boards.");
            println!(
                "Use 'espbrew' (without --cli-only) to launch the TUI for component management."
            );
            println!(
                "📦 Pro tip: Use ./support/build-all-idf-build-apps.sh for professional parallel builds"
            );
            return Ok(());
        }
        Commands::Build => {
            println!("🍺 ESPBrew CLI Mode - Building all boards...");
            println!("Found {} boards:", app.boards.len());

            for board in &app.boards {
                println!("  - {} ({})", board.name, board.config_file.display());
            }

            println!("\nFound {} components:", app.components.len());
            for component in &app.components {
                let component_type = if component.is_managed {
                    "managed"
                } else {
                    "local"
                };
                println!(
                    "  - {} ({}) [{}]",
                    component.name,
                    component.path.display(),
                    component_type
                );
            }

            // Build logic for build command
            println!();
            println!("🔄 Starting builds for all boards...");
            println!();

            // Create event channel for CLI mode
            let (tx, mut rx) = mpsc::unbounded_channel();

            // Start building all boards immediately in CLI mode
            app.build_all_boards(tx.clone()).await?;

            let total_boards = app.boards.len();
            let mut completed = 0;
            let mut succeeded = 0;
            let mut failed = 0;

            // Wait for all builds to complete
            while completed < total_boards {
                if let Some(event) = rx.recv().await {
                    match event {
                        AppEvent::BuildOutput(board_name, line) => {
                            println!("🔨 [{}] {}", board_name, line);
                        }
                        AppEvent::BuildFinished(board_name, success) => {
                            completed += 1;
                            if success {
                                succeeded += 1;
                                println!(
                                    "✅ [{}] Build completed successfully! ({}/{} done)",
                                    board_name, completed, total_boards
                                );
                            } else {
                                failed += 1;
                                println!(
                                    "❌ [{}] Build failed! ({}/{} done)",
                                    board_name, completed, total_boards
                                );
                            }
                        }
                        AppEvent::ActionFinished(_board_name, _action_name, _success) => {
                            // Actions are not used in CLI mode, only direct builds
                        }
                        AppEvent::ComponentActionStarted(_component_name, _action_name) => {
                            // Component actions are not used in CLI mode
                        }
                        AppEvent::ComponentActionProgress(_component_name, _message) => {
                            // Component actions are not used in CLI mode
                        }
                        AppEvent::ComponentActionFinished(
                            _component_name,
                            _action_name,
                            _success,
                        ) => {
                            // Component actions are not used in CLI mode
                        }
                        AppEvent::BuildCompleted => {
                            // Build completion event - no specific action needed in CLI mode
                        }
                        // Monitoring events are not used in CLI mode
                        AppEvent::MonitorLogReceived(_)
                        | AppEvent::MonitorConnected(_)
                        | AppEvent::MonitorDisconnected
                        | AppEvent::MonitorError(_) => {}
                        // Remote board fetching events are not used in CLI mode
                        AppEvent::RemoteBoardsFetched(_) | AppEvent::RemoteBoardsFetchFailed(_) => {
                        }
                        AppEvent::ServerDiscoveryStarted
                        | AppEvent::ServerDiscovered(_)
                        | AppEvent::ServerDiscoveryCompleted(_)
                        | AppEvent::ServerDiscoveryFailed(_) => {
                            // These are handled in the main loop
                        }
                        AppEvent::Tick => {}
                    }
                }
            }

            println!();
            println!("🍺 ESPBrew CLI Build Summary:");
            println!("  Total boards: {}", total_boards);
            println!("  ✅ Succeeded: {}", succeeded);
            println!("  ❌ Failed: {}", failed);
            println!();
            println!("Build logs saved in ./logs/");
            println!("Flash scripts available in ./support/");
            println!(
                "📦 Pro tip: Use ./support/build-all-idf-build-apps.sh for conflict-free parallel builds"
            );

            if failed > 0 {
                println!("⚠️  Some builds failed. Check the logs for details.");
                std::process::exit(1);
            } else {
                println!("🎆 All builds completed successfully!");
            }
        }
        Commands::Flash {
            binary,
            config: _,
            port,
        } => {
            println!("🍺 ESPBrew Flash Mode - Local Flashing");

            // Find binary to flash
            let binary_path = match binary {
                Some(path) => path,
                None => match find_binary_file(&app.project_dir, None) {
                    Ok(path) => path,
                    Err(e) => {
                        println!("❌ Failed to find binary file: {}", e);
                        return Err(e);
                    }
                },
            };

            println!("📦 Flashing binary: {}", binary_path.display());

            // If port is specified, use esptool directly
            if let Some(port_name) = port {
                println!(
                    "🔥 Using esptool to flash {} on port {}",
                    binary_path.display(),
                    port_name
                );

                match run_local_flash_esptool(&binary_path, &port_name).await {
                    Ok(()) => {
                        println!("✅ Local flash completed successfully!");
                    }
                    Err(e) => {
                        println!("❌ Local flash failed: {}", e);
                        return Err(e);
                    }
                }
            } else {
                // Use the appropriate flash method based on project type and available flash utilities
                println!("🔥 Auto-detecting flash method...");

                // Try using our improved espflash utilities for ELF files
                match crate::espflash_utils::flash_binary_to_esp(&binary_path, None).await {
                    Ok(()) => {
                        println!("✅ Local flash completed successfully!");
                    }
                    Err(e) => {
                        println!("⚠️ espflash failed: {}", e);
                        println!("🔄 Trying idf.py flash as fallback...");

                        match run_local_flash_idf(&app.project_dir).await {
                            Ok(()) => {
                                println!("✅ idf.py flash completed successfully!");
                            }
                            Err(e) => {
                                println!("❌ Local flash failed: {}", e);
                                return Err(e);
                            }
                        }
                    }
                }
            }
        }
        Commands::RemoteFlash {
            binary,
            config: _,
            mac,
            name,
            server,
        } => {
            println!("🍺 ESPBrew Remote Flash Mode - API Flashing");

            // Use provided server URL or default
            let server_url = server
                .as_deref()
                .or(app.server_url.as_deref())
                .unwrap_or("http://localhost:8080");
            println!("🔍 Connecting to ESPBrew server: {}", server_url);

            // Fetch available boards from server
            match fetch_remote_boards(server_url).await {
                Ok(remote_boards) => {
                    if remote_boards.is_empty() {
                        println!("⚠️  No boards found on the remote server");
                        return Ok(());
                    }

                    println!("📊 Found {} board(s) on server", remote_boards.len());

                    // If no MAC or name specified, list available boards and exit
                    if mac.is_none() && name.is_none() {
                        println!("📋 Available boards:");
                        for (i, board) in remote_boards.iter().enumerate() {
                            let display_name = board.logical_name.as_ref().unwrap_or(&board.id);
                            println!(
                                "  {}. {} - {} ({})",
                                i + 1,
                                display_name,
                                board.device_description,
                                board.mac_address
                            );
                            println!("     MAC: {}", board.mac_address);
                            println!("     Port: {}", board.port);
                            println!("     Status: {}", board.status);
                            println!();
                        }
                        println!("💡 To flash a specific board, use:");
                        println!("  espbrew --cli-only remote-flash --mac <MAC_ADDRESS>");
                        println!("  espbrew --cli-only remote-flash --name <BOARD_NAME>");
                        return Ok(());
                    }

                    // Select target board by MAC or name
                    let selected_board = if let Some(target_mac) = &mac {
                        println!("🎯 Targeting board with MAC: {}", target_mac);
                        remote_boards.iter().find(|board| {
                            board.mac_address.to_lowercase() == target_mac.to_lowercase()
                                || board
                                    .unique_id
                                    .to_lowercase()
                                    .contains(&target_mac.to_lowercase())
                        })
                    } else if let Some(target_name) = &name {
                        println!("🎯 Targeting board with name: {}", target_name);
                        remote_boards.iter().find(|board| {
                            board.logical_name.as_ref().map_or(false, |n| {
                                n.to_lowercase().contains(&target_name.to_lowercase())
                            }) || board
                                .id
                                .to_lowercase()
                                .contains(&target_name.to_lowercase())
                        })
                    } else {
                        None
                    };

                    let selected_board = match selected_board {
                        Some(board) => board,
                        None => {
                            let target = mac.as_ref().or(name.as_ref()).unwrap();
                            println!("❌ No board found matching: {}", target);
                            println!("📋 Available boards:");
                            for board in &remote_boards {
                                let display_name = board.logical_name.as_ref().unwrap_or(&board.id);
                                println!("  - {} (MAC: {})", display_name, board.mac_address);
                            }
                            return Err(anyhow::anyhow!("Board not found: {}", target));
                        }
                    };

                    let display_name = selected_board
                        .logical_name
                        .as_ref()
                        .unwrap_or(&selected_board.id);
                    println!(
                        "✅ Selected board: {} ({})",
                        display_name, selected_board.mac_address
                    );

                    // Check project type and use appropriate flash method
                    if let Some(project_type) = &app.project_type {
                        match project_type {
                            ProjectType::RustNoStd => {
                                println!(
                                    "🦀 Detected Rust no_std project, using Rust-specific remote flash"
                                );

                                // Use Rust-specific remote flash with event channel that prints messages
                                let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

                                // Spawn a task to print messages from the upload function
                                let print_handle = tokio::spawn(async move {
                                    while let Some(event) = rx.recv().await {
                                        match event {
                                            AppEvent::BuildOutput(source, message) => {
                                                println!("[{}] {}", source, message);
                                            }
                                            _ => {} // Ignore other event types
                                        }
                                    }
                                });

                                let result = upload_and_flash_rust_binary(
                                    server_url,
                                    selected_board,
                                    &app.project_dir,
                                    tx,
                                )
                                .await;

                                // Give the print task a moment to process remaining messages
                                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                                print_handle.abort();

                                match result {
                                    Ok(()) => {
                                        println!(
                                            "✅ Rust no_std remote flash completed successfully!"
                                        );
                                    }
                                    Err(e) => {
                                        println!("❌ Rust no_std remote flash failed: {}", e);
                                        return Err(e);
                                    }
                                }
                                return Ok(());
                            }
                            ProjectType::Arduino => {
                                println!(
                                    "🔧 Detected Arduino project, using Arduino-specific remote flash"
                                );

                                // Use Arduino-specific remote flash with event channel that prints messages
                                let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

                                // Spawn a task to print messages from the upload function
                                let print_handle = tokio::spawn(async move {
                                    while let Some(event) = rx.recv().await {
                                        match event {
                                            AppEvent::BuildOutput(source, message) => {
                                                println!("[{}] {}", source, message);
                                            }
                                            _ => {} // Ignore other event types
                                        }
                                    }
                                });

                                let result = upload_and_flash_arduino_remote(
                                    server_url,
                                    selected_board,
                                    &app.project_dir,
                                    tx,
                                )
                                .await;

                                // Give the print task a moment to process remaining messages
                                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                                print_handle.abort();

                                match result {
                                    Ok(()) => {
                                        println!("✅ Arduino remote flash completed successfully!");
                                    }
                                    Err(e) => {
                                        println!("❌ Arduino remote flash failed: {}", e);
                                        return Err(e);
                                    }
                                }
                                return Ok(());
                            }
                            _ => {
                                println!(
                                    "🏗️ Detected {} project, using ESP-IDF remote flash",
                                    project_type.name()
                                );
                            }
                        }
                    }

                    // Try to find ESP-IDF build artifacts for proper multi-binary flashing
                    let board_name = selected_board
                        .board_type_id
                        .as_ref()
                        .or(selected_board.logical_name.as_ref())
                        .map(|s| s.as_str());

                    match find_esp_build_artifacts(&app.project_dir, board_name) {
                        Ok((flash_config, binaries)) => {
                            println!(
                                "📦 Found ESP-IDF build artifacts: {} binaries",
                                binaries.len()
                            );
                            for binary in &binaries {
                                println!(
                                    "  - {} at 0x{:x}: {}",
                                    binary.name,
                                    binary.offset,
                                    binary.file_path.display()
                                );
                            }

                            // Upload and flash with proper multi-binary support
                            match upload_and_flash_esp_build(
                                server_url,
                                selected_board,
                                &flash_config,
                                &binaries,
                            )
                            .await
                            {
                                Ok(()) => {
                                    println!(
                                        "✅ ESP-IDF multi-binary remote flash completed successfully!"
                                    );
                                }
                                Err(e) => {
                                    println!("❌ ESP-IDF multi-binary remote flash failed: {}", e);
                                    return Err(e);
                                }
                            }
                        }
                        Err(_) => {
                            // Fall back to single binary flash
                            match find_binary_file(&app.project_dir, binary.as_deref()) {
                                Ok(binary_path) => {
                                    println!(
                                        "⚠️ Using legacy single binary flash: {}",
                                        binary_path.display()
                                    );

                                    // Upload and flash single binary (legacy)
                                    match upload_and_flash_remote_legacy(
                                        server_url,
                                        selected_board,
                                        &binary_path,
                                    )
                                    .await
                                    {
                                        Ok(()) => {
                                            println!(
                                                "✅ Legacy remote flash completed successfully!"
                                            );
                                        }
                                        Err(e) => {
                                            println!("❌ Legacy remote flash failed: {}", e);
                                            return Err(e);
                                        }
                                    }
                                }
                                Err(e) => {
                                    println!("❌ Failed to find binary file: {}", e);
                                    return Err(e);
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    println!("❌ Failed to connect to remote server: {}", e);
                    return Err(e);
                }
            }
        }
        Commands::Discover { timeout } => {
            println!("📻 ESPBrew Server Discovery Mode - mDNS Discovery");
            println!("Scanning for ESPBrew servers on the local network...");

            match discover_espbrew_servers(timeout).await {
                Ok(servers) => {
                    if servers.is_empty() {
                        println!("⚠️  No ESPBrew servers found on the local network");
                        println!(
                            "💡 Make sure at least one ESPBrew server is running with mDNS enabled"
                        );
                    } else {
                        println!("🎆 Found {} ESPBrew server(s):", servers.len());
                        println!();

                        for (i, server) in servers.iter().enumerate() {
                            println!("{}. Server: {}", i + 1, server.name);
                            println!("   URL: http://{}:{}", server.ip, server.port);
                            println!("   Hostname: {}", server.hostname);
                            println!("   Version: {}", server.version);
                            println!("   Description: {}", server.description);
                            println!("   Boards: {}", server.board_count);
                            if !server.boards_list.is_empty() {
                                println!("   Available boards: {}", server.boards_list);
                            }
                            println!();
                        }

                        println!("💡 To use a server, set the --server-url option:");
                        for server in &servers {
                            println!(
                                "  espbrew --server-url http://{}:{} <command>",
                                server.ip, server.port
                            );
                        }
                    }
                }
                Err(e) => {
                    println!("❌ Failed to discover servers: {}", e);
                    return Err(e);
                }
            }
        }
        Commands::RemoteMonitor {
            mac,
            name,
            server,
            baud_rate,
            reset,
        } => {
            println!("📺 ESPBrew Remote Monitor Mode - API Monitoring");

            // Use provided server URL or default
            let server_url = server
                .as_deref()
                .or(app.server_url.as_deref())
                .unwrap_or("http://localhost:8080");
            println!("🔍 Connecting to ESPBrew server: {}", server_url);

            // Fetch available boards from server
            match fetch_remote_boards(server_url).await {
                Ok(remote_boards) => {
                    if remote_boards.is_empty() {
                        println!("⚠️  No boards found on the remote server");
                        return Ok(());
                    }

                    println!("📊 Found {} board(s) on server", remote_boards.len());

                    // If no MAC or name specified, list available boards and exit
                    if mac.is_none() && name.is_none() {
                        println!("📋 Available boards:");
                        for (i, board) in remote_boards.iter().enumerate() {
                            let display_name = board.logical_name.as_ref().unwrap_or(&board.id);
                            println!(
                                "  {}. {} - {} ({})",
                                i + 1,
                                display_name,
                                board.device_description,
                                board.mac_address
                            );
                            println!("     MAC: {}", board.mac_address);
                            println!("     Port: {}", board.port);
                            println!("     Status: {}", board.status);
                            println!();
                        }
                        println!("💡 To monitor a specific board, use:");
                        println!("  espbrew --cli-only remote-monitor --mac <MAC_ADDRESS>");
                        println!("  espbrew --cli-only remote-monitor --name <BOARD_NAME>");
                        return Ok(());
                    }

                    // Select target board by MAC or name
                    let selected_board = if let Some(target_mac) = &mac {
                        println!("🎯 Targeting board with MAC: {}", target_mac);
                        remote_boards.iter().find(|board| {
                            board.mac_address.to_lowercase() == target_mac.to_lowercase()
                                || board
                                    .unique_id
                                    .to_lowercase()
                                    .contains(&target_mac.to_lowercase())
                        })
                    } else if let Some(target_name) = &name {
                        println!("🎯 Targeting board with name: {}", target_name);
                        remote_boards.iter().find(|board| {
                            board.logical_name.as_ref().map_or(false, |n| {
                                n.to_lowercase().contains(&target_name.to_lowercase())
                            }) || board
                                .id
                                .to_lowercase()
                                .contains(&target_name.to_lowercase())
                        })
                    } else {
                        None
                    };

                    let selected_board = match selected_board {
                        Some(board) => board,
                        None => {
                            let target = mac.as_ref().or(name.as_ref()).unwrap();
                            println!("❌ No board found matching: {}", target);
                            println!("📋 Available boards:");
                            for board in &remote_boards {
                                let display_name = board.logical_name.as_ref().unwrap_or(&board.id);
                                println!("  - {} (MAC: {})", display_name, board.mac_address);
                            }
                            return Err(anyhow::anyhow!("Board not found: {}", target));
                        }
                    };

                    let display_name = selected_board
                        .logical_name
                        .as_ref()
                        .unwrap_or(&selected_board.id);
                    println!(
                        "✅ Selected board: {} ({})",
                        display_name, selected_board.mac_address
                    );

                    // Start remote monitoring
                    let monitor_request = MonitorRequest {
                        board_id: selected_board.id.clone(),
                        baud_rate: Some(baud_rate),
                        filters: None,
                    };

                    let client = reqwest::Client::new();
                    let url = format!("{}/api/v1/monitor/start", server_url.trim_end_matches('/'));

                    println!("📺 Starting remote monitoring session...");
                    match client.post(&url).json(&monitor_request).send().await {
                        Ok(response) => {
                            match response.json::<MonitorResponse>().await {
                                Ok(monitor_response) => {
                                    if monitor_response.success {
                                        println!(
                                            "✅ Remote monitoring started: {}",
                                            monitor_response.message
                                        );

                                        if let Some(session_id) = monitor_response.session_id {
                                            println!("🔗 Session ID: {}", session_id);

                                            // Build WebSocket URL
                                            let ws_url = format!(
                                                "ws://{}/ws/monitor/{}",
                                                server_url
                                                    .trim_start_matches("http://")
                                                    .trim_start_matches("https://")
                                                    .trim_end_matches('/'),
                                                session_id
                                            );

                                            println!("🔔 Connecting to WebSocket: {}", ws_url);

                                            // Start CLI WebSocket streaming with keep-alive and reconnection support
                                            match start_cli_websocket_streaming_with_reconnect(
                                                ws_url,
                                                session_id.clone(),
                                                server_url.to_string(),
                                                display_name.clone(),
                                                selected_board.id.clone(),
                                                reset,
                                            )
                                            .await
                                            {
                                                Ok(_) => {
                                                    println!("🎉 Remote monitoring session ended.");
                                                }
                                                Err(e) => {
                                                    println!(
                                                        "❌ WebSocket streaming failed: {}",
                                                        e
                                                    );
                                                    println!(
                                                        "💡 You can still use the web interface: {}",
                                                        server_url
                                                    );
                                                    return Err(e);
                                                }
                                            }
                                        }
                                    } else {
                                        println!("❌ Server error: {}", monitor_response.message);
                                        return Err(anyhow::anyhow!(
                                            "Server error: {}",
                                            monitor_response.message
                                        ));
                                    }
                                }
                                Err(e) => {
                                    println!("❌ Failed to parse response: {}", e);
                                    return Err(anyhow::anyhow!("Failed to parse response: {}", e));
                                }
                            }
                        }
                        Err(e) => {
                            println!("❌ Failed to start monitoring: {}", e);
                            return Err(anyhow::anyhow!("Failed to start monitoring: {}", e));
                        }
                    }
                }
                Err(e) => {
                    println!("❌ Failed to connect to remote server: {}", e);
                    return Err(e);
                }
            }
        }
    }

    Ok(())
}

/// Discover ESPBrew servers on the local network using mDNS (silent version for TUI)
async fn discover_espbrew_servers_silent(timeout_secs: u64) -> Result<Vec<DiscoveredServer>> {
    let mdns =
        ServiceDaemon::new().map_err(|e| anyhow::anyhow!("Failed to create mDNS daemon: {}", e))?;

    // Browse for ESPBrew services
    let service_type = "_espbrew._tcp.local.";
    let receiver = mdns
        .browse(service_type)
        .map_err(|e| anyhow::anyhow!("Failed to start mDNS browse: {}", e))?;

    // Silent version - no println!

    let mut servers = Vec::new();
    let timeout = tokio::time::Duration::from_secs(timeout_secs);
    let start_time = tokio::time::Instant::now();

    // Listen for mDNS events with timeout
    let mut receiver = receiver;
    while start_time.elapsed() < timeout {
        let remaining_time = timeout - start_time.elapsed();

        match tokio::time::timeout(remaining_time, receiver.recv_async()).await {
            Ok(Ok(event)) => {
                match event {
                    ServiceEvent::ServiceResolved(info) => {
                        // Silent service resolution

                        // Parse TXT records
                        let mut version = "unknown".to_string();
                        let mut hostname = "unknown".to_string();
                        let mut description = "ESPBrew Server".to_string();
                        let mut board_count = 0u32;
                        let mut boards_list = String::new();
                        // Parse TXT record properties
                        let properties = info.get_properties();
                        for property in properties.iter() {
                            let property_string = format!("{}", property);
                            if let Some((key, value)) = property_string.split_once('=') {
                                match key {
                                    "version" => version = value.to_string(),
                                    "hostname" => hostname = value.to_string(),
                                    "description" => description = value.to_string(),
                                    "board_count" => {
                                        board_count = value.parse().unwrap_or(0);
                                    }
                                    "boards" => boards_list = value.to_string(),
                                    _ => {}
                                }
                            }
                        }

                        let server =
                            DiscoveredServer {
                                name: info.get_hostname().to_string(),
                                ip: *info.get_addresses().iter().next().unwrap_or(
                                    &std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
                                ),
                                port: info.get_port(),
                                hostname,
                                version,
                                description,
                                board_count,
                                boards_list,
                            };

                        // Silent - no println!
                        servers.push(server);
                    }
                    ServiceEvent::SearchStarted(_) => {
                        // Silent - no println!
                    }
                    ServiceEvent::SearchStopped(_) => {
                        // Silent - no println!
                        break;
                    }
                    _ => {}
                }
            }
            Ok(Err(_e)) => {
                // Silent error handling - no eprintln!
                break;
            }
            Err(_) => {
                // Timeout reached - silent
                break;
            }
        }
    }

    // Stop the browse operation
    let _ = mdns.stop_browse(service_type);

    Ok(servers)
}

/// Discover ESPBrew servers on the local network using mDNS
async fn discover_espbrew_servers(timeout_secs: u64) -> Result<Vec<DiscoveredServer>> {
    let mdns =
        ServiceDaemon::new().map_err(|e| anyhow::anyhow!("Failed to create mDNS daemon: {}", e))?;

    // Browse for ESPBrew services
    let service_type = "_espbrew._tcp.local.";
    let receiver = mdns
        .browse(service_type)
        .map_err(|e| anyhow::anyhow!("Failed to start mDNS browse: {}", e))?;

    println!("🔍 Browsing for {} services...", service_type);

    let mut servers = Vec::new();
    let timeout = tokio::time::Duration::from_secs(timeout_secs);
    let start_time = tokio::time::Instant::now();

    // Listen for mDNS events with timeout
    let mut receiver = receiver;
    while start_time.elapsed() < timeout {
        let remaining_time = timeout - start_time.elapsed();

        match tokio::time::timeout(remaining_time, receiver.recv_async()).await {
            Ok(Ok(event)) => {
                match event {
                    ServiceEvent::ServiceResolved(info) => {
                        println!("🔍 Found service: {}", info.get_fullname());

                        // Parse TXT records
                        let mut version = "unknown".to_string();
                        let mut hostname = "unknown".to_string();
                        let mut description = "ESPBrew Server".to_string();
                        let mut board_count = 0u32;
                        let mut boards_list = String::new();
                        // Parse TXT record properties
                        let properties = info.get_properties();
                        for property in properties.iter() {
                            let property_string = format!("{}", property);
                            if let Some((key, value)) = property_string.split_once('=') {
                                match key {
                                    "version" => version = value.to_string(),
                                    "hostname" => hostname = value.to_string(),
                                    "description" => description = value.to_string(),
                                    "board_count" => {
                                        board_count = value.parse().unwrap_or(0);
                                    }
                                    "boards" => boards_list = value.to_string(),
                                    _ => {}
                                }
                            }
                        }

                        let server =
                            DiscoveredServer {
                                name: info.get_hostname().to_string(),
                                ip: *info.get_addresses().iter().next().unwrap_or(
                                    &std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
                                ),
                                port: info.get_port(),
                                hostname,
                                version,
                                description,
                                board_count,
                                boards_list,
                            };

                        println!(
                            "✅ Discovered: {} at {}:{}",
                            server.name, server.ip, server.port
                        );
                        servers.push(server);
                    }
                    ServiceEvent::SearchStarted(_) => {
                        println!("🔍 Search started for ESPBrew services...");
                    }
                    ServiceEvent::SearchStopped(_) => {
                        println!("🔍 Search stopped.");
                        break;
                    }
                    _ => {}
                }
            }
            Ok(Err(e)) => {
                eprintln!("⚠️ mDNS receiver error: {}", e);
                break;
            }
            Err(_) => {
                // Timeout reached
                println!("🕐 Discovery timeout reached ({} seconds)", timeout_secs);
                break;
            }
        }
    }

    // Stop the browse operation
    let _ = mdns.stop_browse(service_type);

    Ok(servers)
}

/// Start CLI WebSocket streaming with keep-alive and reconnection support for reset scenarios
async fn start_cli_websocket_streaming_with_reconnect(
    ws_url: String,
    session_id: String,
    server_url: String,
    board_name: String,
    board_id: String,
    reset_board: bool,
) -> Result<()> {
    let mut attempt = 1;
    let max_attempts = if reset_board { 3 } else { 1 };
    let mut current_ws_url = ws_url;
    let mut current_session_id = session_id;

    loop {
        if attempt > 1 {
            println!("🔄 Reconnection attempt {}/{}", attempt, max_attempts);
        }

        match start_cli_websocket_streaming(
            current_ws_url.clone(),
            current_session_id.clone(),
            server_url.clone(),
            board_name.clone(),
            board_id.clone(),
            reset_board && attempt == 1, // Only reset on first attempt
        )
        .await
        {
            Ok(_) => {
                if reset_board && attempt == 1 {
                    // WebSocket disconnected during reset - try to reconnect to new session
                    println!("🔄 Attempting to reconnect after board reset...");

                    // Wait for board to complete reset and server to restart monitoring
                    tokio::time::sleep(tokio::time::Duration::from_millis(3000)).await;

                    // Try to find new monitoring session for this board
                    if let Ok(new_session) =
                        find_latest_monitoring_session(&server_url, &board_id).await
                    {
                        if let (Some(new_session_id), Some(new_ws_url)) = new_session {
                            println!("🔗 Found new session after reset: {}", new_session_id);
                            current_session_id = new_session_id;
                            current_ws_url = new_ws_url;
                            attempt += 1;
                            if attempt <= max_attempts {
                                continue; // Try connecting to the new session
                            }
                        }
                    }

                    println!("📝 Unable to reconnect to new session - monitoring ended");
                }
                return Ok(());
            }
            Err(e) => {
                if reset_board
                    && attempt == 1
                    && (e.to_string().contains("Connection reset")
                        || e.to_string().contains("protocol error"))
                {
                    // Expected disconnection during reset
                    println!("🔄 WebSocket disconnected during board reset (expected)");
                    println!("🔄 Attempting to reconnect to capture boot logs...");

                    // Wait for reset to complete
                    tokio::time::sleep(tokio::time::Duration::from_millis(3000)).await;

                    // Try to find new monitoring session
                    if let Ok(new_session) =
                        find_latest_monitoring_session(&server_url, &board_id).await
                    {
                        if let (Some(new_session_id), Some(new_ws_url)) = new_session {
                            println!("🔗 Found new session after reset: {}", new_session_id);
                            current_session_id = new_session_id;
                            current_ws_url = new_ws_url;
                            attempt += 1;
                            if attempt <= max_attempts {
                                continue;
                            }
                        }
                    }

                    println!("📝 Board reset completed - monitoring continues on server");
                    println!(
                        "🌍 Use web interface for persistent sessions: {}",
                        server_url
                    );
                    return Ok(());
                } else if attempt < max_attempts {
                    println!(
                        "⚠️ WebSocket error (attempt {}): {} - retrying...",
                        attempt, e
                    );
                    attempt += 1;
                    tokio::time::sleep(tokio::time::Duration::from_millis(2000)).await;
                    continue;
                }
                return Err(e);
            }
        }
    }
}

/// Find the latest monitoring session for a board
async fn find_latest_monitoring_session(
    server_url: &str,
    board_id: &str,
) -> Result<(Option<String>, Option<String>)> {
    let client = reqwest::Client::new();
    let url = format!(
        "{}/api/v1/monitor/sessions",
        server_url.trim_end_matches('/')
    );

    match client.get(&url).send().await {
        Ok(response) => {
            if let Ok(sessions_response) = response.json::<serde_json::Value>().await {
                if let Some(sessions) = sessions_response.get("sessions").and_then(|s| s.as_array())
                {
                    // Find the most recent session for this board
                    let mut latest_session: Option<(String, String)> = None;

                    for session in sessions {
                        if let (Some(session_board_id), Some(session_id)) = (
                            session.get("board_id").and_then(|b| b.as_str()),
                            session.get("session_id").and_then(|s| s.as_str()),
                        ) {
                            if session_board_id == board_id {
                                let ws_url = format!(
                                    "ws://{}/ws/monitor/{}",
                                    server_url
                                        .trim_start_matches("http://")
                                        .trim_start_matches("https://")
                                        .trim_end_matches('/'),
                                    session_id
                                );
                                latest_session = Some((session_id.to_string(), ws_url));
                            }
                        }
                    }

                    if let Some((session_id, ws_url)) = latest_session {
                        return Ok((Some(session_id), Some(ws_url)));
                    }
                }
            }
        }
        Err(e) => {
            println!("⚠️ Failed to query sessions: {}", e);
        }
    }

    Ok((None, None))
}

/// Start CLI WebSocket streaming with keep-alive for remote monitoring
async fn start_cli_websocket_streaming(
    ws_url: String,
    session_id: String,
    server_url: String,
    board_name: String,
    board_id: String,
    reset_board: bool,
) -> Result<()> {
    use futures_util::StreamExt;
    use tokio::signal;
    use tokio_tungstenite::{connect_async, tungstenite::Message};

    println!("🔌 Connecting to WebSocket...");

    // Connect to WebSocket
    let (ws_stream, _) = connect_async(&ws_url)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to connect to WebSocket: {}", e))?;

    let (_ws_sender, mut ws_receiver) = ws_stream.split();

    println!(
        "✅ WebSocket connected! Streaming logs from {}...",
        board_name
    );

    // Reset the board if requested to capture boot sequence
    if reset_board {
        println!("🔄 Resetting board to capture complete boot sequence...");
        let reset_request = serde_json::json!({
            "board_id": board_id
        });

        let client = reqwest::Client::new();
        let reset_url = format!("{}/api/v1/reset", server_url.trim_end_matches('/'));

        match client.post(&reset_url).json(&reset_request).send().await {
            Ok(response) => {
                if response.status().is_success() {
                    match response.json::<serde_json::Value>().await {
                        Ok(response_data) => {
                            if let Some(message) =
                                response_data.get("message").and_then(|m| m.as_str())
                            {
                                println!("✅ {}", message);
                                if message.contains("monitoring restarted") {
                                    println!(
                                        "⏳ Server automatically restarted monitoring - WebSocket may reconnect..."
                                    );
                                    println!(
                                        "💡 If connection drops, the server continues monitoring. Use web interface for persistent sessions."
                                    );
                                }
                            }
                        }
                        Err(_) => {
                            println!("✅ Board reset successfully - capturing boot logs...");
                        }
                    }
                    tokio::time::sleep(tokio::time::Duration::from_millis(1500)).await;
                } else {
                    println!("⚠️ Reset request failed, continuing with current logs...");
                }
            }
            Err(e) => {
                println!(
                    "⚠️ Failed to send reset command: {} - continuing with current logs...",
                    e
                );
            }
        }
    }

    println!("💡 Press Ctrl+C to stop monitoring");
    println!("{}", "─".repeat(60));

    // Clone variables for keep-alive task
    let session_id_keepalive = session_id.clone();
    let server_url_keepalive = server_url.clone();

    // Spawn keep-alive task
    let keepalive_handle = tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(60));
        let client = reqwest::Client::new();

        loop {
            interval.tick().await;

            let keepalive_request = KeepAliveRequest {
                session_id: session_id_keepalive.clone(),
            };

            let keepalive_url = format!(
                "{}/api/v1/monitor/keepalive",
                server_url_keepalive.trim_end_matches('/')
            );

            if let Err(e) = client
                .post(&keepalive_url)
                .json(&keepalive_request)
                .send()
                .await
            {
                // Silently log keep-alive failures - don't spam the console
                eprintln!("\r⚠️ Keep-alive failed: {}", e);
            }
        }
    });

    // Setup Ctrl+C handling
    let ctrl_c = signal::ctrl_c();

    // Main streaming loop
    let streaming_result = tokio::select! {
        // Handle Ctrl+C
        _ = ctrl_c => {
            println!("\n\n🛑 Monitoring stopped by user (Ctrl+C)");
            Ok(())
        }

        // Handle WebSocket messages
        result = async {
            while let Some(msg) = ws_receiver.next().await {
                match msg {
                    Ok(Message::Text(text)) => {
                        // Parse the WebSocket message
                        match serde_json::from_str::<WebSocketMessage>(&text) {
                            Ok(ws_msg) => {
                                match ws_msg.message_type.as_str() {
                                    "log" => {
                                        if let Some(content) = ws_msg.content {
                                            // Print log content directly to console
                                            println!("{}", content.trim_end());
                                        }
                                    }
                                    "connection" => {
                                        if let Some(message) = ws_msg.message {
                                            println!("🔗 {}", message);
                                        }
                                    }
                                    "error" => {
                                        if let Some(error) = ws_msg.error {
                                            println!("❌ WebSocket error: {}", error);
                                        }
                                    }
                                    _ => {
                                        // Unknown message type, log as-is
                                        println!("📨 {}", text);
                                    }
                                }
                            }
                            Err(_) => {
                                // If we can't parse as JSON, treat as raw log line
                                println!("{}", text.trim_end());
                            }
                        }
                    }
                    Ok(Message::Close(_)) => {
                        println!("\n🔌 WebSocket connection closed by server");
                        break;
                    }
                    Err(e) => {
                        let error_msg = e.to_string();
                        if error_msg.contains("Connection reset") || error_msg.contains("protocol error") {
                            println!("\n🔄 WebSocket disconnected during board reset (expected behavior)");
                            println!("🔄 Attempting to reconnect to continue monitoring boot sequence...");

                            // Try to reconnect to capture boot logs after reset
                            return Ok(()); // Return to trigger reconnection logic
                        } else {
                            println!("\n❌ WebSocket error: {}", e);
                            return Err(anyhow::anyhow!("WebSocket error: {}", e));
                        }
                    }
                    _ => {
                        // Ignore other message types (Binary, Ping, Pong)
                    }
                }
            }
            Ok(())
        } => result
    };

    // Cancel keep-alive task
    keepalive_handle.abort();

    // Stop the monitoring session on the server
    let stop_request = StopMonitorRequest {
        session_id: session_id.clone(),
    };

    let client = reqwest::Client::new();
    let stop_url = format!("{}/api/v1/monitor/stop", server_url.trim_end_matches('/'));

    if let Err(e) = client.post(&stop_url).json(&stop_request).send().await {
        eprintln!("⚠️ Failed to stop monitoring session: {}", e);
    } else {
        println!("✅ Monitoring session stopped on server");
    }

    streaming_result
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    let project_dir = cli
        .project_dir
        .unwrap_or_else(|| std::env::current_dir().expect("Failed to get current directory"));

    if !project_dir.exists() {
        return Err(anyhow::anyhow!(
            "Project directory does not exist: {:?}",
            project_dir
        ));
    }

    // Detect project type
    let project_registry = ProjectRegistry::new();
    let project_handler = project_registry.detect_project(&project_dir);

    if let Some(ref handler) = project_handler {
        println!(
            "🔍 Detected {} project in {}",
            handler.project_type().name(),
            project_dir.display()
        );

        // Show project description
        println!("📖 {}", handler.project_type().description());

        // Discover boards/targets
        match handler.discover_boards(&project_dir) {
            Ok(boards) => {
                if boards.is_empty() {
                    println!("⚠️  No boards/targets found in this project.");
                } else {
                    println!("🎯 Found {} board(s)/target(s):", boards.len());
                    for board in &boards {
                        println!(
                            "  - {} ({})",
                            board.name,
                            board.target.as_deref().unwrap_or("auto-detect")
                        );
                    }
                }
            }
            Err(e) => {
                eprintln!("❌ Error discovering boards: {}", e);
            }
        }
        println!();
    } else {
        println!(
            "⚠️  Unknown project type in {}. Falling back to ESP-IDF mode.",
            project_dir.display()
        );
        println!("   Supported project types: ESP-IDF, Rust no_std, Arduino");
        println!();
    }

    let mut app = App::new(
        project_dir,
        cli.build_strategy.clone(),
        cli.server_url.clone(),
        cli.board_mac.clone(),
        project_handler,
    )?;

    // Generate support scripts
    println!("🍺 Generating build and flash scripts...");
    app.generate_support_scripts()?;
    println!("✅ Scripts generated in ./support/");
    println!("📦 Professional multi-board build: ./support/build-all-idf-build-apps.sh");

    if cli.cli_only || cli.command.is_some() {
        return run_cli_only(app, cli.command).await;
    }

    println!();
    println!("🍺 Starting ESPBrew TUI...");
    println!(
        "Found {} boards and {} components.",
        app.boards.len(),
        app.components.len()
    );
    println!("Press 'b' to build all boards, Tab to switch between panes.");
    println!("Press 'h' for help, 'q' to quit.");
    println!();

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    // Note: We don't enable mouse capture to allow terminal text selection
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create event channel
    let (tx, mut rx) = mpsc::unbounded_channel();

    // Start automatic server discovery
    app.start_server_discovery(tx.clone());

    // Spawn tick generator
    let tx_tick = tx.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_millis(250));
        loop {
            interval.tick().await;
            let _ = tx_tick.send(AppEvent::Tick);
        }
    });

    // Main loop
    let result = loop {
        terminal.draw(|f| ui(f, &app))?;

        // Handle events
        tokio::select! {
            // Handle crossterm events
            _ = tokio::task::spawn_blocking(|| event::poll(Duration::from_millis(50))) => {
                if event::poll(Duration::from_millis(0))? {
                    match event::read()? {
                        Event::Key(key) => {
                            if key.kind == KeyEventKind::Press {
                                // Handle tool warning modal first
                                if app.show_tool_warning && !app.tool_warning_acknowledged {
                                    match key.code {
                                        KeyCode::Enter => {
                                            app.acknowledge_tool_warning();
                                        }
                                        KeyCode::Char('q') | KeyCode::Esc => break Ok(()),
                                        _ => {}
                                    }
                                    continue;
                                }

                                match key.code {
                                    KeyCode::Char('q') => break Ok(()),
                                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                                    if app.show_monitor_modal {
                                        // Clear monitor logs
                                        app.clear_monitor_logs();
                                    } else {
                                        break Ok(());
                                    }
                                }
                                KeyCode::Esc => {
                                    if app.show_action_menu {
                                        app.hide_action_menu();
                                    } else if app.show_component_action_menu {
                                        app.hide_component_action_menu();
                                    } else if app.show_remote_board_dialog {
                                        app.hide_remote_board_dialog();
                                    } else if app.show_monitor_modal {
                                        app.hide_monitor_modal();
                                    } else {
                                        break Ok(());
                                    }
                                }
                                KeyCode::Tab => {
                                    app.toggle_focused_pane();
                                }
                                KeyCode::Char('h') | KeyCode::Char('?') => {
                                    app.show_help = !app.show_help;
                                }
                                KeyCode::Char('b') => {
                                    if !app.build_in_progress {
                                        app.start_single_board_build(tx.clone());
                                    }
                                }
                                KeyCode::Char('x') => {
                                    if !app.build_in_progress {
                                        app.start_all_boards_build(tx.clone());
                                    }
                                }
                                KeyCode::Char(' ') => {
                                    if !app.build_in_progress {
                                        app.start_single_board_build(tx.clone());
                                    }
                                }
                                KeyCode::Char('m') | KeyCode::Char('M') => {
                                    if !app.show_monitor_modal && !app.show_action_menu && !app.show_component_action_menu && !app.show_remote_board_dialog {
                                        app.show_monitor_modal(tx.clone()).await?;
                                    }
                                }
                                KeyCode::Char('r') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                                    if app.show_monitor_modal {
                                        // Reset board in monitoring modal
                                        app.execute_monitor_reset(tx.clone()).await?;
                                    }
                                }
                                KeyCode::Char('a') | KeyCode::Char('A') => {
                                    if app.show_monitor_modal {
                                        // Toggle auto-scroll
                                        app.toggle_monitor_auto_scroll();
                                    }
                                }
                                KeyCode::Char('r') | KeyCode::Char('R') => {
                                    if app.show_remote_board_dialog {
                                        // Check if we should retry or reset board
                                        if app.remote_boards_fetch_error.is_some() {
                                            // Retry fetching remote boards
                                            app.start_fetching_remote_boards(tx.clone());
                                        } else if !app.remote_boards.is_empty() {
                                            // Reset selected remote board
                                            app.execute_remote_reset(tx.clone()).await?;
                                        }
                                    } else if !app.show_monitor_modal {
                                        // Refresh board list (original 'r' functionality)
                                        app.boards = App::discover_boards(&app.project_dir)?;
                                        app.components = App::discover_components(&app.project_dir)?;
                                        app.selected_board = 0;
                                        app.selected_component = 0;
                                        if !app.boards.is_empty() {
                                            app.list_state.select(Some(0));
                                        } else {
                                            app.list_state.select(None);
                                        }
                                        if !app.components.is_empty() {
                                            app.component_list_state.select(Some(0));
                                        } else {
                                            app.component_list_state.select(None);
                                        }
                                        app.reset_log_scroll();
                                    }
                                }
                                KeyCode::Up | KeyCode::Char('k') => {
                                    if app.show_action_menu {
                                        app.previous_action();
                                    } else if app.show_component_action_menu {
                                        app.previous_component_action();
                                    } else if app.show_remote_board_dialog {
                                        app.previous_remote_board();
                                    } else if app.show_monitor_modal {
                                        app.scroll_monitor_up();
                                    } else {
                                        match app.focused_pane {
                                            FocusedPane::BoardList => {
                                                let old_board = app.selected_board;
                                                app.previous_board();
                                                if old_board != app.selected_board {
                                                    app.reset_log_scroll();
                                                }
                                            }
                                            FocusedPane::ComponentList => {
                                                app.previous_component();
                                            }
                                            FocusedPane::LogPane => {
                                                app.scroll_log_up();
                                            }
                                        }
                                    }
                                }
                                KeyCode::Down | KeyCode::Char('j') => {
                                    if app.show_action_menu {
                                        app.next_action();
                                    } else if app.show_component_action_menu {
                                        app.next_component_action();
                                    } else if app.show_remote_board_dialog {
                                        app.next_remote_board();
                                    } else if app.show_monitor_modal {
                                        app.scroll_monitor_down();
                                    } else {
                                        match app.focused_pane {
                                            FocusedPane::BoardList => {
                                                let old_board = app.selected_board;
                                                app.next_board();
                                                if old_board != app.selected_board {
                                                    app.reset_log_scroll();
                                                }
                                            }
                                            FocusedPane::ComponentList => {
                                                app.next_component();
                                            }
                                            FocusedPane::LogPane => {
                                                app.scroll_log_down();
                                            }
                                        }
                                    }
                                }
                                KeyCode::PageUp => {
                                    if app.show_monitor_modal {
                                        app.scroll_monitor_page_up();
                                    } else if app.focused_pane == FocusedPane::LogPane {
                                        app.scroll_log_page_up();
                                    }
                                }
                                KeyCode::PageDown => {
                                    if app.show_monitor_modal {
                                        app.scroll_monitor_page_down();
                                    } else if app.focused_pane == FocusedPane::LogPane {
                                        app.scroll_log_page_down();
                                    }
                                }
                                KeyCode::Home => {
                                    if app.focused_pane == FocusedPane::LogPane {
                                        app.scroll_to_top();
                                    }
                                }
                                KeyCode::End => {
                                    if app.focused_pane == FocusedPane::LogPane {
                                        app.scroll_to_bottom();
                                    }
                                }
                                KeyCode::Enter => {
                                    if app.show_action_menu {
                                        // Execute selected board action
                                        if let Some(action) = app.available_actions.get(app.action_menu_selected) {
                                            let action = action.clone();
                                            app.hide_action_menu();
                                            app.execute_action(action, tx.clone()).await?;
                                        }
                                    } else if app.show_remote_board_dialog {
                                        // Execute remote action based on action type
                                        if !app.remote_flash_in_progress && !app.remote_monitor_in_progress &&
                                           !app.remote_boards_loading && app.remote_boards_fetch_error.is_none() &&
                                           !app.remote_boards.is_empty() {
                                            match app.remote_action_type {
                                                RemoteActionType::Flash => {
                                                    app.execute_remote_flash(tx.clone()).await?;
                                                }
                                                RemoteActionType::Monitor => {
                                                    // Start monitoring session and open monitor modal
                                                    if let Err(e) = app.start_remote_monitor_modal(tx.clone()).await {
                                                        // Show error in logs if monitor modal fails to start
                                                        if app.selected_board < app.boards.len() {
                                                            app.boards[app.selected_board].log_lines.push(
                                                                format!("❌ Remote Monitor Modal Error: {}", e)
                                                            );
                                                            app.boards[app.selected_board].status = BuildStatus::Failed;
                                                        }
                                                    }
                                                }
                                            }
                                            app.hide_remote_board_dialog();
                                        }
                                    } else if app.show_component_action_menu {
                                        // Execute selected component action
                                        let selected_component = app.components.get(app.selected_component);
                                        let available_actions: Vec<&ComponentAction> = app
                                            .available_component_actions
                                            .iter()
                                            .filter(|action| {
                                                if let Some(comp) = selected_component {
                                                    action.is_available_for(comp)
                                                } else {
                                                    false
                                                }
                                            })
                                            .collect();

                                        let adjusted_selected = app.component_action_menu_selected.min(available_actions.len().saturating_sub(1));
                                        if let Some(action) = available_actions.get(adjusted_selected) {
                                            let action = (*action).clone();
                                            app.hide_component_action_menu();

                                            // For cloning actions, run async. For others, run sync.
                                            match action {
                                                ComponentAction::CloneFromRepository => {
                                                    // Handle async cloning
                                                    if let Some(component) = app.components.get(app.selected_component) {
                                                        let component = component.clone();
                                                        let action_name = action.name().to_string();
                                                        let component_name = component.name.clone();
                                                        let selected_index = app.selected_component;
                                                        let project_dir = app.project_dir.clone();
                                                        let tx_clone = tx.clone();

                                                        // Send started event
                                                        let _ = tx.send(AppEvent::ComponentActionStarted(
                                                            component_name.clone(),
                                                            action_name.clone(),
                                                        ));

                                                        // Set component action status
                                                        app.components[selected_index].action_status = Some(format!("{}...", action_name));

                                                        // Spawn async task for cloning
                                                        tokio::spawn(async move {
                                                            let result = App::execute_clone_component_async(
                                                                component,
                                                                project_dir,
                                                                tx_clone.clone(),
                                                            ).await;

                                                            let _ = tx_clone.send(AppEvent::ComponentActionFinished(
                                                                component_name,
                                                                action_name,
                                                                result.is_ok(),
                                                            ));
                                                        });
                                                    }
                                                }
                                                _ => {
                                                    // Handle sync actions immediately
                                                    if let Err(_e) = app.execute_component_action_sync(action).await {
                                                        // Don't print to console when in TUI mode - this breaks the interface
                                                        // eprintln!("Component action failed: {}", e);
                                                    }
                                                }
                                            }
                                        }
                                    } else {
                                        // Show appropriate action menu based on focused pane
                                        match app.focused_pane {
                                            FocusedPane::BoardList => {
                                                app.show_action_menu();
                                            }
                                            FocusedPane::ComponentList => {
                                                app.show_component_action_menu();
                                            }
                                            FocusedPane::LogPane => {
                                                // For log pane, default to board action menu
                                                app.show_action_menu();
                                            }
                                        }
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                        Event::Mouse(_mouse) => {
                            // Mouse events are not captured, allowing terminal text selection
                            // This branch should rarely be hit since we don't enable mouse capture
                        }
                        _ => {}
                    }
                }
            }

            // Handle app events
            Some(event) = rx.recv() => {
                match event {
                    AppEvent::BuildOutput(board_name, line) => {
                        app.add_log_line(&board_name, line);
                    }
                    AppEvent::BuildFinished(board_name, success) => {
                        let status = if success {
                            BuildStatus::Success
                        } else {
                            BuildStatus::Failed
                        };
                        app.update_board_status(&board_name, status);
                    }
                    AppEvent::ActionFinished(board_name, action_name, success) => {
                        if board_name == "remote" && action_name == "Remote Flash" {
                            // Handle remote flash completion
                            app.remote_flash_in_progress = false;
                            if success {
                                app.remote_flash_status = Some("Remote flash completed successfully!".to_string());
                                if app.selected_board < app.boards.len() {
                                    app.boards[app.selected_board].status = BuildStatus::Flashed;
                                }
                            } else {
                                app.remote_flash_status = Some("Remote flash failed. Check server logs.".to_string());
                                if app.selected_board < app.boards.len() {
                                    app.boards[app.selected_board].status = BuildStatus::Failed;
                                }
                            }
                        } else if board_name == "remote" && action_name == "Remote Monitor" {
                            // Handle remote monitor completion
                            app.remote_monitor_in_progress = false;
                            if success {
                                app.remote_monitor_status = Some("Remote monitoring started successfully!".to_string());
                                if app.selected_board < app.boards.len() {
                                    app.boards[app.selected_board].status = BuildStatus::Success;
                                }
                            } else {
                                app.remote_monitor_status = Some("Remote monitoring failed. Check server logs.".to_string());
                                if app.selected_board < app.boards.len() {
                                    app.boards[app.selected_board].status = BuildStatus::Failed;
                                }
                            }
                        } else {
                            let status = if success {
                                match action_name.as_str() {
                                    "Flash" => BuildStatus::Flashed,
                                    _ => BuildStatus::Success,
                                }
                            } else {
                                BuildStatus::Failed
                            };
                            app.update_board_status(&board_name, status);
                        }
                    }
                    AppEvent::ComponentActionStarted(component_name, action_name) => {
                        // Component action started - status is already set in the UI thread
                        // Don't print to console when in TUI mode - this breaks the interface
                        // eprintln!("🧩 [{}] Started: {}", component_name, action_name);
                    }
                    AppEvent::ComponentActionProgress(component_name, message) => {
                        // Don't print to console when in TUI mode - this breaks the interface
                        // eprintln!("🧩 [{}] {}", component_name, message);
                    }
                    AppEvent::ComponentActionFinished(component_name, action_name, success) => {
                        // Clear component action status and refresh component list
                        if let Some(component) = app.components.iter_mut().find(|c| c.name == component_name) {
                            component.action_status = None;
                        }

                        if success {
                            // Don't print to console when in TUI mode - this breaks the interface
                            // eprintln!("✅ [{}] {} completed successfully", component_name, action_name);
                            // Refresh component list to show the updated state
                            if let Ok(new_components) = App::discover_components(&app.project_dir) {
                                app.components = new_components;
                                // Adjust selection if needed
                                if app.selected_component >= app.components.len() && !app.components.is_empty() {
                                    app.selected_component = app.components.len() - 1;
                                }
                                if app.components.is_empty() {
                                    app.component_list_state.select(None);
                                } else {
                                    app.component_list_state.select(Some(app.selected_component));
                                }
                            }
                        } else {
                            // Don't print to console when in TUI mode - this breaks the interface
                            // eprintln!("❌ [{}] {} failed", component_name, action_name);
                        }
                    }
                    AppEvent::BuildCompleted => {
                        // Reset build in progress flag
                        app.build_in_progress = false;

                        // Update board statuses from build logs for idf-build-apps builds
                        if let Err(e) = app.update_board_statuses_from_build_logs().await {
                            // Don't print to console when in TUI mode - this breaks the interface
                            // eprintln!("Failed to update board statuses from logs: {}", e);
                        }
                    }
                    AppEvent::MonitorLogReceived(_) | AppEvent::MonitorConnected(_) |
                    AppEvent::MonitorDisconnected | AppEvent::MonitorError(_) => {
                        // Handle monitoring events
                        app.handle_monitor_event(event);
                    }
                    AppEvent::RemoteBoardsFetched(remote_boards) => {
                        app.handle_remote_boards_fetched(remote_boards);
                        // Update status for successful connection
                        if app.selected_board < app.boards.len() {
                            app.boards[app.selected_board].log_lines.push(format!(
                                "✅ Connected to server! Found {} remote board(s)",
                                app.remote_boards.len()
                            ));
                            app.boards[app.selected_board].status = BuildStatus::Success;
                        }
                    }
                    AppEvent::RemoteBoardsFetchFailed(error_msg) => {
                        app.handle_remote_boards_fetch_failed(error_msg.clone());
                        // Update status for failed connection
                        if app.selected_board < app.boards.len() {
                            app.boards[app.selected_board].log_lines.push("💡 Please ensure:".to_string());
                            app.boards[app.selected_board].log_lines.push(
                                "   1. ESPBrew server is running: cargo run --bin espbrew-server --release".to_string(),
                            );
                            app.boards[app.selected_board].log_lines.push(
                                "   2. Server is accessible at the configured URL".to_string(),
                            );
                            app.boards[app.selected_board].log_lines.push(
                                "   3. Firewall allows connections to the server port".to_string(),
                            );
                            app.boards[app.selected_board].status = BuildStatus::Failed;
                        }
                    }
                    AppEvent::ServerDiscoveryStarted |
                    AppEvent::ServerDiscovered(_) |
                    AppEvent::ServerDiscoveryCompleted(_) |
                    AppEvent::ServerDiscoveryFailed(_) => {
                        app.handle_server_discovery_event(event);
                    }
                    AppEvent::Tick => {
                        // Regular tick for UI updates
                    }
                }
            }
        }
    };

    // Restore terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}
