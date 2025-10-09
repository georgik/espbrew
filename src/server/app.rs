//! Server application implementation

use anyhow::Result;
use chrono::{DateTime, Local};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{RwLock, broadcast};
use warp::Filter;

use super::ServerConfig;
use crate::models::board::{BoardAssignment, BoardType, ConnectedBoard, EnhancedBoardInfo};

/// Server application main struct
pub struct ServerApp {
    config: ServerConfig,
    state: Arc<RwLock<ServerState>>,
    /// Handle for the background board scanner task
    scanner_task: Option<tokio::task::JoinHandle<()>>,
    /// Cancellation signal for background tasks
    cancel_signal: Arc<std::sync::atomic::AtomicBool>,
    /// mDNS service for server discovery
    mdns_service: Option<crate::server::services::MdnsService>,
}

/// Comprehensive server state management
#[derive(Debug)]
pub struct ServerState {
    pub boards: HashMap<String, ConnectedBoard>,
    pub config: ServerConfig,
    pub last_scan: DateTime<Local>,
    /// Cache of enhanced board information by device path
    pub enhanced_info_cache: Arc<RwLock<HashMap<String, EnhancedBoardInfo>>>,
    /// Currently running background enhancement tasks by device path
    pub enhancement_tasks: Arc<RwLock<HashMap<String, tokio::task::JoinHandle<()>>>>,
    /// Persistent configuration (board types, assignments, etc.)
    pub persistent_config: PersistentConfig,
    /// Path to persistent configuration file
    pub config_path: PathBuf,
    /// Active monitoring sessions by session ID
    pub monitoring_sessions: Arc<RwLock<HashMap<String, MonitoringSession>>>,
}

/// Persistent configuration stored in RON format
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
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
        if let Err(e) = std::fs::create_dir_all(&config_dir) {
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
        let content = std::fs::read_to_string(config_path)?;
        let config: PersistentConfig = ron::from_str(&content)?;
        Ok(config)
    }

    /// Save persistent configuration to RON file
    pub fn save_persistent_config(&self) -> Result<()> {
        let mut config = self.persistent_config.clone();
        config.last_updated = Local::now();

        let ron_string = ron::ser::to_string_pretty(&config, ron::ser::PrettyConfig::default())?;
        std::fs::write(&self.config_path, ron_string)?;

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
        if let Ok(entries) = std::fs::read_dir(&snow_path) {
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
}

impl ServerApp {
    pub async fn new(config: ServerConfig) -> Result<Self> {
        let state = Arc::new(RwLock::new(ServerState::new(config.clone())));
        let cancel_signal = Arc::new(std::sync::atomic::AtomicBool::new(false));

        // Initialize mDNS service if enabled
        let mdns_service = if config.enable_mdns {
            match crate::server::services::MdnsService::new(&config) {
                Ok(service) => Some(service),
                Err(e) => {
                    println!("⚠️ Failed to initialize mDNS service: {}", e);
                    None
                }
            }
        } else {
            None
        };

        Ok(Self {
            config,
            state,
            scanner_task: None,
            cancel_signal,
            mdns_service,
        })
    }

    pub fn get_state(&self) -> Arc<RwLock<ServerState>> {
        Arc::clone(&self.state)
    }

    pub async fn run(mut self) -> Result<()> {
        println!(
            "🚀 Server starting on {}:{}",
            self.config.bind_address, self.config.port
        );

        // Get server state reference
        let state = self.get_state();

        // Perform initial board scan
        println!("🔍 Performing initial board scan...");
        let scanner = crate::server::services::board_scanner::BoardScanner::new(state.clone());
        match scanner.scan_boards().await {
            Ok(count) => println!("✅ Initial scan found {} boards", count),
            Err(e) => println!("⚠️ Initial scan failed: {}", e),
        }

        // Start background board scanner task
        let scanner_state = state.clone();
        let scanner_cancel = self.cancel_signal.clone();
        let scan_interval = self.config.scan_interval;
        self.scanner_task = Some(tokio::spawn(async move {
            let scanner = crate::server::services::board_scanner::BoardScanner::new(scanner_state);
            let mut interval =
                tokio::time::interval(tokio::time::Duration::from_secs(scan_interval));

            loop {
                interval.tick().await;

                // Check for cancellation
                if scanner_cancel.load(std::sync::atomic::Ordering::Relaxed) {
                    println!("🛑 Background board scanner shutting down...");
                    break;
                }

                // Perform background board scan
                match scanner
                    .scan_boards_with_cancellation(Some(scanner_cancel.clone()))
                    .await
                {
                    Ok(count) => {
                        if count > 0 {
                            println!("🔄 Background scan complete: {} boards found", count);
                        }
                    }
                    Err(e) => println!("⚠️ Background board scan failed: {}", e),
                }
            }
        }));

        // Register mDNS service for discovery
        if let Some(ref mdns_service) = self.mdns_service {
            if let Err(e) = mdns_service.register(&self.config, state.clone()).await {
                println!("⚠️ Failed to register mDNS service: {}", e);
            }
        }

        // Set up HTTP routes
        let board_routes = crate::server::routes::boards::create_board_routes(state.clone());
        let reset_route = crate::server::routes::boards::create_reset_route(state.clone());
        let board_types_routes =
            crate::server::routes::board_types::create_board_types_routes(state.clone());
        let flash_routes = crate::server::routes::flash::create_flash_routes(state.clone());
        let monitor_routes = crate::server::routes::monitor::create_monitor_routes(state.clone());
        let health_route = crate::server::routes::health::create_health_route();

        // Static file serving for web dashboard
        let static_files = warp::path("static").and(warp::fs::dir("./static"));

        // Serve index.html at root
        let index_route = warp::path::end().and(warp::fs::file("./static/index.html"));

        // Combine all API routes
        let api_routes = board_routes
            .or(reset_route)
            .or(board_types_routes)
            .or(flash_routes)
            .or(monitor_routes);

        let all_routes = api_routes.or(health_route).or(static_files).or(index_route);

        // Add CORS middleware
        let cors = warp::cors()
            .allow_any_origin()
            .allow_headers(vec!["content-type", "authorization"])
            .allow_methods(vec!["GET", "POST", "PUT", "DELETE", "OPTIONS"]);

        // Add enhanced request logging middleware with status codes and timing
        let logging = crate::server::middleware::logging::with_request_logging();

        let routes = all_routes.with(logging).with(cors);

        // Parse bind address
        let bind_addr: std::net::SocketAddr =
            format!("{}:{}", self.config.bind_address, self.config.port)
                .parse()
                .map_err(|e| anyhow::anyhow!("Invalid bind address: {}", e))?;

        println!("🌍 Server listening on http://{}", bind_addr);
        println!("📄 API endpoints:");
        println!("   GET    /api/v1/boards              - List all connected boards");
        println!("   GET    /api/v1/boards/{{id}}        - Get board information");
        println!("   GET    /api/v1/board-types         - Get available board types");
        println!("   POST   /api/v1/assign-board        - Assign a board to a board type");
        println!("   DELETE /api/v1/assign-board/{{id}}  - Unassign a board");
        println!("   POST   /api/v1/boards/scan         - Trigger board scan");
        println!("   POST   /api/v1/flash               - Flash a board");
        println!("   POST   /api/v1/reset               - Reset a board");
        println!("   POST   /api/v1/monitor/start       - Start monitoring a board");
        println!("   POST   /api/v1/monitor/stop        - Stop monitoring session");
        println!("   POST   /api/v1/monitor/keepalive   - Keep monitoring session alive");
        println!("   GET    /api/v1/monitor/sessions    - List active monitoring sessions");
        println!("   WS     /ws/monitor/{{session_id}}   - WebSocket for receiving logs");
        println!("   GET    /health                     - Health check");
        println!("   GET    /                           - Web dashboard (index.html)");
        println!("   GET    /static/*                   - Static files");
        println!("🚀 ESPBrew Server ready!");

        // Create a shutdown signal that we can trigger
        let (_shutdown_tx, mut shutdown_rx) = tokio::sync::oneshot::channel::<()>();
        let cancel_signal = self.cancel_signal.clone();

        // Start the HTTP server
        let (_addr, server) =
            warp::serve(routes).bind_with_graceful_shutdown(bind_addr, async move {
                tokio::select! {
                    _ = tokio::signal::ctrl_c() => {
                        println!("ℹ️ Received shutdown signal (Ctrl+C)...");
                    }
                    _ = &mut shutdown_rx => {
                        println!("ℹ️ Received shutdown signal...");
                    }
                }

                // Signal all background tasks to cancel
                cancel_signal.store(true, std::sync::atomic::Ordering::Relaxed);
            });

        // Wait for server to complete
        server.await;

        // Clean up background tasks
        println!("🧹 Cleaning up background tasks...");

        // Wait for scanner task to finish
        if let Some(scanner_task) = self.scanner_task {
            match tokio::time::timeout(
                tokio::time::Duration::from_secs(1), // Reduced from 5 seconds for faster shutdown
                scanner_task,
            )
            .await
            {
                Ok(Ok(())) => println!("✅ Background scanner shut down cleanly"),
                Ok(Err(e)) => println!("⚠️ Background scanner task error: {}", e),
                Err(_) => {
                    println!("🔄 Background scanner shutdown timeout (forced)");
                    // Task is automatically dropped and cancelled when timeout expires
                }
            }
        }

        // Clean up any active monitoring sessions
        {
            let state_lock = self.state.read().await;
            let sessions_lock = state_lock.monitoring_sessions.write().await;
            let session_count = sessions_lock.len();
            if session_count > 0 {
                println!("🧹 Cleaning up {} monitoring sessions...", session_count);
                // Note: Sessions will be cleaned up when the sessions_lock is dropped
            }
        }

        // Unregister and shutdown mDNS service
        if let Some(mdns_service) = self.mdns_service {
            if let Err(e) = mdns_service.unregister() {
                println!("⚠️ Failed to unregister mDNS service: {}", e);
            }
            if let Err(e) = mdns_service.shutdown() {
                println!("⚠️ Failed to shutdown mDNS daemon: {}", e);
            }
        }

        println!("🛑 Server shut down gracefully");
        Ok(())
    }
}
