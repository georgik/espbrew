#!/usr/bin/env rust
//! ESPBrew Server - Remote ESP32 Flashing Server
//!
//! A network-based server that manages connected ESP32 boards and provides
//! remote flashing capabilities. This enables ESPBrew to work with remote
//! test farms, CI/CD environments, and distributed development setups.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use chrono::{DateTime, Local};
use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use warp::Filter;

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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BoardStatus {
    Available,
    Flashing,
    Monitoring,
    Error(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlashRequest {
    /// Board ID to flash
    pub board_id: String,
    /// Binary data to flash (base64 encoded)
    pub binary_data: Vec<u8>,
    /// Flash offset (usually 0x0 for merged binaries)
    pub offset: u32,
    /// Optional chip type override
    pub chip_type: Option<String>,
    /// Flash after completion
    pub verify: bool,
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

/// Server state management
#[derive(Debug)]
pub struct ServerState {
    boards: HashMap<String, ConnectedBoard>,
    config: ServerConfig,
    last_scan: DateTime<Local>,
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
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            bind_address: "0.0.0.0".to_string(),
            port: 8080,
            scan_interval: 30,
            board_mappings: HashMap::new(),
            max_binary_size_mb: 50,
        }
    }
}

impl ServerState {
    pub fn new(config: ServerConfig) -> Self {
        Self {
            boards: HashMap::new(),
            config,
            last_scan: Local::now(),
        }
    }

    /// Discover and update connected boards
    pub async fn scan_boards(&mut self) -> Result<()> {
        println!("🔍 Scanning for connected ESP32 boards...");

        // Use espflash to discover serial ports
        use serialport::SerialPortType;
        let ports = serialport::available_ports()?;
        let mut discovered_boards = HashMap::new();

        // Filter for relevant USB ports on macOS
        let relevant_ports: Vec<_> = ports
            .into_iter()
            .filter(|port_info| {
                let port_name = &port_info.port_name;
                // On macOS, focus on USB modem and USB serial ports
                port_name.contains("/dev/cu.usbmodem") || port_name.contains("/dev/cu.usbserial")
            })
            .collect();

        println!(
            "📡 Found {} potential ESP32 ports to check",
            relevant_ports.len()
        );
        for port_info in &relevant_ports {
            println!("  🔌 {}", port_info.port_name);
        }

        if relevant_ports.is_empty() {
            println!(
                "⚠️  No USB serial/modem ports found. Make sure your ESP32 board is connected."
            );
        }

        for (index, port_info) in relevant_ports.iter().enumerate() {
            println!(
                "🔍 [{}/{}] Checking port: {}",
                index + 1,
                relevant_ports.len(),
                port_info.port_name
            );

            // Try to connect and identify the board
            match self.identify_board(&port_info.port_name).await {
                Ok(Some(board)) => {
                    println!(
                        "✅ Found ESP32 board on {}: {} ({})",
                        port_info.port_name, board.chip_type, board.features
                    );
                    let board_id =
                        format!("board_{}", board.port.replace("/", "_").replace(".", "_"));

                    // Apply logical name mapping if configured
                    let logical_name = self.config.board_mappings.get(&board.port).cloned();

                    let connected_board = ConnectedBoard {
                        id: board_id.clone(),
                        port: board.port,
                        chip_type: board.chip_type,
                        crystal_frequency: board.crystal_frequency,
                        flash_size: board.flash_size,
                        features: board.features,
                        mac_address: board.mac_address,
                        device_description: match &port_info.port_type {
                            SerialPortType::UsbPort(usb) => {
                                format!(
                                    "{} - {}",
                                    usb.manufacturer.as_deref().unwrap_or("Unknown"),
                                    usb.product.as_deref().unwrap_or("USB Device")
                                )
                            }
                            SerialPortType::PciPort => "PCI serial port".to_string(),
                            SerialPortType::BluetoothPort => "Bluetooth serial port".to_string(),
                            SerialPortType::Unknown => "Unknown serial port".to_string(),
                        },
                        status: BoardStatus::Available,
                        last_updated: Local::now(),
                        logical_name,
                    };

                    discovered_boards.insert(board_id, connected_board);
                }
                Ok(None) => {
                    println!("❌ No ESP32 detected on {}", port_info.port_name);
                    continue;
                }
                Err(e) => {
                    println!(
                        "⚠️  Failed to identify board on {}: {}",
                        port_info.port_name, e
                    );
                    continue;
                }
            }
        }

        self.boards = discovered_boards;
        self.last_scan = Local::now();

        println!("✅ Scan complete. Found {} ESP32 boards", self.boards.len());

        for board in self.boards.values() {
            let logical_name = board.logical_name.as_deref().unwrap_or("(unmapped)");
            println!(
                "  📱 {} [{}] - {} @ {} ({})",
                board.id, logical_name, board.chip_type, board.port, board.device_description
            );
        }

        Ok(())
    }

    /// Identify a board on the given port (safer approach without probe-rs)
    async fn identify_board(&self, port: &str) -> Result<Option<BoardInfo>> {
        use std::time::Duration;

        // Use a shorter timeout to prevent hanging
        let timeout_dur = Duration::from_millis(1200); // Increased timeout for ESP32-P4
        let port_str = port.to_string();

        // First, do a quick serial port accessibility check
        if !Self::is_port_accessible(&port_str).await {
            println!("⚠️ Port {} not accessible, skipping", port_str);
            return Ok(None);
        }

        // Check if the port name contains indicators of board type
        let possible_board_type = if port_str.contains("usbmodem") {
            // Modern ESP32 dev kits often use usbmodem interface
            // This includes ESP32-S3, ESP32-C3, ESP32-C6, ESP32-P4
            Some("esp32-usb")
        } else if port_str.contains("usbserial") {
            // Traditional ESP32 boards with CP210x/FTDI often use usbserial
            Some("esp32-serial")
        } else {
            None
        };

        if let Some(board_type) = possible_board_type {
            println!("🔍 Detected possible {} board on {}", board_type, port_str);
        }

        // Skip the probe-rs step entirely - it doesn't detect serial ports correctly
        // Instead, go directly to the espflash method which is more reliable for serial port detection
        println!(
            "🔍 Attempting to identify board on {} with espflash",
            port_str
        );
        let result = tokio::time::timeout(
            timeout_dur,
            Self::identify_with_espflash_subprocess(&port_str),
        )
        .await;

        match result {
            Ok(inner_result) => inner_result,
            Err(_) => {
                println!(
                    "⏰ Timeout identifying board on {} (espflash took too long)",
                    port
                );
                Ok(None)
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

    /// Identification using espflash as subprocess with proper timeout and cancellation
    async fn identify_with_espflash_subprocess(port: &str) -> Result<Option<BoardInfo>> {
        use std::process::Stdio;
        use std::time::Duration;
        use tokio::process::Command;

        // Run espflash as separate subprocess to avoid terminal blocking
        // Increased timeout specifically for ESP32-P4 which can take longer to identify
        let timeout_dur = Duration::from_millis(1000);

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

                // Parse espflash output
                let mut chip_type = "esp32".to_string();
                let mut flash_size = "Unknown".to_string();
                let mut features = "WiFi".to_string();

                for line in stdout.lines() {
                    let line = line.trim();
                    if line.contains("Chip type:") {
                        if let Some(chip) = line.split(':').nth(1) {
                            chip_type = chip.trim().to_lowercase().replace("-", "");
                        }
                    } else if line.contains("Flash size:") {
                        if let Some(size) = line.split(':').nth(1) {
                            flash_size = size.trim().to_string();
                        }
                    } else if line.contains("Features:") {
                        if let Some(feat) = line.split(':').nth(1) {
                            features = feat.trim().to_string();
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

                println!(
                    "✅ Successfully identified {} board: {}",
                    normalized_chip_type, port
                );

                Ok(Some(BoardInfo {
                    port: port.to_string(),
                    chip_type: normalized_chip_type.to_string(),
                    crystal_frequency: "40 MHz".to_string(),
                    flash_size,
                    features,
                    mac_address: "**:**:**:**:**:**".to_string(),
                }))
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
        let result = Self::perform_flash(&board_port, &request.binary_data, request.offset).await;

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

    /// Perform the actual flashing operation using subprocess (safer)
    async fn perform_flash(port: &str, binary_data: &[u8], offset: u32) -> Result<()> {
        use std::process::Stdio;
        use tokio::fs;
        use tokio::process::Command;

        // Create temporary file for binary data
        let temp_dir = std::env::temp_dir();
        let temp_file = temp_dir.join(format!(
            "espbrew_flash_{}.bin",
            uuid::Uuid::new_v4().simple()
        ));

        // Write binary data to temp file
        fs::write(&temp_file, binary_data).await?;

        // Use espflash as subprocess for safer operation
        let mut cmd = Command::new("espflash")
            .args([
                "write-bin",
                "--port",
                port,
                "--flash-addr",
                &format!("0x{:x}", offset),
                temp_file.to_str().unwrap(),
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        // Wait for completion with timeout
        let timeout_dur = std::time::Duration::from_secs(60); // 1 minute timeout for flashing
        let result = tokio::time::timeout(timeout_dur, cmd.wait()).await;

        // Clean up temp file
        let _ = fs::remove_file(&temp_file).await;

        match result {
            Ok(Ok(status)) if status.success() => Ok(()),
            Ok(Ok(_)) => Err(anyhow::anyhow!("Flash command failed")),
            Ok(Err(e)) => Err(anyhow::anyhow!("Failed to run flash command: {}", e)),
            Err(_) => Err(anyhow::anyhow!("Flash operation timed out")),
        }
    }
}

#[derive(Debug)]
struct BoardInfo {
    port: String,
    chip_type: String,
    crystal_frequency: String,
    flash_size: String,
    features: String,
    mac_address: String,
}

// API Handlers
pub async fn list_boards(
    state: Arc<RwLock<ServerState>>,
) -> Result<impl warp::Reply, warp::Rejection> {
    let state = state.read().await;
    let response = BoardListResponse {
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

pub async fn get_board_info(
    board_id: String,
    state: Arc<RwLock<ServerState>>,
) -> Result<impl warp::Reply, warp::Rejection> {
    let state = state.read().await;

    match state.boards.get(&board_id) {
        Some(board) => Ok(warp::reply::json(board)),
        None => {
            let error = serde_json::json!({
                "error": format!("Board not found: {}", board_id)
            });
            Ok(warp::reply::json(&error))
        }
    }
}

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
    println!(
        "🌐 Starting server on {}:{}",
        config.bind_address, config.port
    );

    // Initialize server state
    let state = Arc::new(RwLock::new(ServerState::new(config.clone())));

    // Perform initial board scan
    {
        let mut state_lock = state.write().await;
        state_lock.scan_boards().await?;
    }

    // Setup shutdown notify for background tasks
    let shutdown_notify = Arc::new(tokio::sync::Notify::new());

    // Start periodic board scanning
    let scan_state = state.clone();
    let scan_interval = config.scan_interval;
    let scan_shutdown = shutdown_notify.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(scan_interval));
        loop {
            tokio::select! {
                _ = interval.tick() => {
                    if let Ok(mut state) = scan_state.try_write() {
                        if let Err(e) = state.scan_boards().await {
                            eprintln!("❌ Board scan failed: {}", e);
                        }
                    }
                }
                _ = scan_shutdown.notified() => {
                    println!("🛑 Stopping scanner task...");
                    break;
                }
            }
        }
    });

    // Define API routes
    let state_filter = warp::any().map(move || state.clone());

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

    // POST /api/v1/flash - Flash a board
    let flash = api
        .and(warp::path("flash"))
        .and(warp::path::end())
        .and(warp::post())
        .and(warp::body::json())
        .and(state_filter.clone())
        .and_then(flash_board);

    // Health check endpoint
    let health = warp::path("health").and(warp::get()).map(|| {
        warp::reply::json(&serde_json::json!({
            "status": "healthy",
            "version": env!("CARGO_PKG_VERSION")
        }))
    });

    let routes = boards
        .or(board_info)
        .or(flash)
        .or(health)
        .with(warp::cors().allow_any_origin())
        .with(warp::log("espbrew-server"));

    // Start the server
    println!(
        "🚀 Server running at http://{}:{}",
        config.bind_address, config.port
    );
    println!("📡 API endpoints:");
    println!("   GET  /api/v1/boards       - List all connected boards");
    println!("   GET  /api/v1/boards/{{id}} - Get board information");
    println!("   POST /api/v1/flash        - Flash a board");
    println!("   GET  /health              - Health check");
    println!();
    println!("Press Ctrl+C to stop the server");

    // Build graceful shutdown future (Ctrl+C and SIGTERM)
    #[cfg(unix)]
    async fn shutdown_signal() {
        use tokio::signal::unix::{SignalKind, signal};
        let mut sigterm = signal(SignalKind::terminate()).expect("create SIGTERM handler");
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {},
            _ = sigterm.recv() => {},
        }
    }

    #[cfg(not(unix))]
    async fn shutdown_signal() {
        let _ = tokio::signal::ctrl_c().await;
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
                println!("\n🛑 Shutdown signal received. Stopping HTTP server...");
                shutdown_notify.notify_waiters();
            });
        (addr, server)
    };

    server_fut.await;
    println!("✅ Server stopped cleanly");

    Ok(())
}
