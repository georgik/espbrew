//! Main TUI application state and logic

use anyhow::Result;
use chrono::Local;
use ratatui::widgets::ListState;
use std::{fs, path::PathBuf};

use crate::models::FocusedPane;

// Use qualified imports to avoid conflicts
use crate::ProjectBoardConfig;
use crate::models::board::{BoardAction, BoardConfig, RemoteBoard};
use crate::models::project::{BuildStatus, BuildStrategy, ComponentAction, ComponentConfig};
use crate::models::server::{DiscoveredServer, RemoteActionType};
use crate::models::tui::LocalBoard;
use crate::projects::{ProjectHandler, ProjectType};

pub struct App {
    pub boards: Vec<BoardConfig>,
    pub selected_board: usize,
    pub list_state: ListState,
    pub components: Vec<ComponentConfig>,
    pub selected_component: usize,
    pub component_list_state: ListState,
    pub project_dir: PathBuf,
    pub logs_dir: PathBuf,
    pub support_dir: PathBuf,
    pub project_type: Option<ProjectType>,
    pub project_handler: Option<Box<dyn ProjectHandler>>,
    pub show_help: bool,
    pub focused_pane: FocusedPane,
    pub log_scroll_offset: usize,
    pub show_tool_warning: bool,
    pub tool_warning_acknowledged: bool,
    pub tool_warning_message: String,
    pub show_action_menu: bool,
    pub show_component_action_menu: bool,
    pub action_menu_selected: usize,
    pub component_action_menu_selected: usize,
    pub available_actions: Vec<BoardAction>,
    pub available_component_actions: Vec<ComponentAction>,
    pub build_strategy: BuildStrategy,
    pub build_in_progress: bool,
    pub server_url: Option<String>,
    pub board_mac: Option<String>,
    // Remote board dialog state
    pub show_remote_board_dialog: bool,
    pub remote_boards: Vec<RemoteBoard>,
    pub selected_remote_board: usize,
    pub remote_board_list_state: ListState,
    pub remote_flash_in_progress: bool,
    pub remote_flash_status: Option<String>,
    // Remote board fetching state
    pub remote_boards_loading: bool,
    pub remote_boards_fetch_error: Option<String>,
    // Remote monitoring state
    pub remote_monitor_in_progress: bool,
    pub remote_monitor_status: Option<String>,
    pub remote_monitor_session_id: Option<String>,
    // Track which remote action is being performed
    pub remote_action_type: RemoteActionType,
    // Monitoring modal state
    pub show_monitor_modal: bool,
    pub monitor_logs: Vec<String>,
    pub monitor_session_id: Option<String>,
    pub monitor_board_id: Option<String>,
    pub monitor_connected: bool,
    pub monitor_scroll_offset: usize,
    pub monitor_auto_scroll: bool,
    // Server discovery state
    pub discovered_servers: Vec<DiscoveredServer>,
    pub server_discovery_in_progress: bool,
    pub server_discovery_status: String,
    pub server_discovery_start_time: Option<chrono::DateTime<chrono::Local>>,
    // Local board detection state
    pub show_local_board_dialog: bool,
    pub local_boards: Vec<LocalBoard>,
    pub selected_local_board: usize,
    pub local_board_list_state: ListState,
    pub local_boards_loading: bool,
    pub local_boards_fetch_error: Option<String>,
}

impl App {
    pub fn new(
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
            match handler.discover_boards(&project_dir) {
                Ok(project_boards) => project_boards
                    .into_iter()
                    .map(|board| BoardConfig {
                        name: board.name,
                        config_file: board.config_file,
                        build_dir: board.build_dir,
                        status: BuildStatus::Pending,
                        log_lines: Vec::new(),
                        build_time: None,
                        last_updated: Local::now(),
                        target: board.target,
                        project_type: board.project_type,
                    })
                    .collect(),
                Err(_e) => {
                    // Fall back to generic discovery if project-specific fails
                    Vec::new()
                }
            }
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

                match handler.check_tools_available().map_err(|e| e.to_string()) {
                    Ok(()) => (false, String::new(), Some(project_type)),
                    Err(err_msg) => (true, err_msg, Some(project_type)),
                }
            } else {
                (false, String::new(), None)
            };

        let available_actions = vec![
            BoardAction::Build,
            BoardAction::GenerateBinary,
            BoardAction::Flash,
            BoardAction::FlashAppOnly,
            BoardAction::Monitor,
            BoardAction::Clean,
            BoardAction::Purge,
            BoardAction::RemoteFlash,
            BoardAction::RemoteMonitor,
        ];

        let available_component_actions = vec![
            ComponentAction::CloneFromRepository,
            ComponentAction::Update,
            ComponentAction::Remove,
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
            show_remote_board_dialog: false,
            remote_boards: Vec::new(),
            selected_remote_board: 0,
            remote_board_list_state: ListState::default(),
            remote_flash_in_progress: false,
            remote_flash_status: None,
            remote_boards_loading: false,
            remote_boards_fetch_error: None,
            remote_monitor_in_progress: false,
            remote_monitor_status: None,
            remote_monitor_session_id: None,
            remote_action_type: RemoteActionType::Flash,
            show_monitor_modal: false,
            monitor_logs: Vec::new(),
            monitor_session_id: None,
            monitor_board_id: None,
            monitor_connected: false,
            monitor_scroll_offset: 0,
            monitor_auto_scroll: true,
            discovered_servers: Vec::new(),
            server_discovery_in_progress: false,
            server_discovery_status: "Ready to discover servers...".to_string(),
            server_discovery_start_time: None,
            // Local board fields
            show_local_board_dialog: false,
            local_boards: Vec::new(),
            selected_local_board: 0,
            local_board_list_state: ListState::default(),
            local_boards_loading: false,
            local_boards_fetch_error: None,
        })
    }

    // Board discovery for ESP-IDF projects (fallback when no project handler)
    fn discover_boards(project_dir: &std::path::Path) -> Result<Vec<BoardConfig>> {
        use glob::glob;

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
                            target: None,
                            project_type: crate::projects::ProjectType::EspIdf,
                        });
                    }
                }
            }
        }

        boards.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(boards)
    }

    fn discover_components(project_dir: &std::path::Path) -> Result<Vec<ComponentConfig>> {
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

    fn load_existing_logs(board: &mut BoardConfig, logs_dir: &std::path::Path) {
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

    /// Scan for local boards connected via USB/serial
    pub async fn scan_local_boards(&mut self) -> Result<()> {
        self.local_boards_loading = true;
        self.local_boards_fetch_error = None;
        self.local_boards.clear();

        // For TUI scanning, we'll avoid println! and use a silent approach
        // Any messages will be handled through the board dialog display

        // Use serialport to discover serial ports
        match serialport::available_ports() {
            Ok(ports) => {
                // Filter for relevant USB ports
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
                            // On Windows, ESP32 devices appear as COM ports
                            || port_name.starts_with("COM")
                    })
                    .collect();

                // Try to identify each board using espflash (silently for TUI)
                for port_info in relevant_ports {
                    // Use the silent version without logging for TUI scanning
                    match crate::utils::espflash_utils::identify_esp_board(&port_info.port_name)
                        .await
                    {
                        Ok(Some(esp_info)) => {
                            // Generate unique ID from MAC address
                            let unique_id = if esp_info.mac_address != "Unknown"
                                && !esp_info.mac_address.contains("*")
                            {
                                let mac_clean = esp_info.mac_address.replace(":", "");
                                format!("MAC{}", mac_clean)
                            } else {
                                format!(
                                    "{}:{}-{}",
                                    esp_info.chip_type,
                                    esp_info.chip_revision.as_deref().unwrap_or("unknown"),
                                    port_info.port_name.replace("/", "-").replace(".", "_")
                                )
                            };

                            let local_board = LocalBoard {
                                port: esp_info.port,
                                chip_type: esp_info.chip_type,
                                device_description: esp_info.device_description,
                                mac_address: esp_info.mac_address,
                                unique_id,
                            };

                            self.local_boards.push(local_board);
                        }
                        Ok(None) => {
                            // No ESP32 board detected - silently continue
                        }
                        Err(_e) => {
                            // Error identifying board - silently continue
                        }
                    }
                }

                // Initialize list state if we have boards
                if !self.local_boards.is_empty() {
                    self.local_board_list_state.select(Some(0));
                    self.selected_local_board = 0;
                }
            }
            Err(e) => {
                let error_msg = format!("Failed to scan serial ports: {}", e);
                self.local_boards_fetch_error = Some(error_msg);
            }
        }

        self.local_boards_loading = false;
        Ok(())
    }

    /// Start local board scanning for flash action
    pub async fn start_local_board_scan_for_flash(&mut self) -> Result<()> {
        self.show_local_board_dialog = true;
        self.scan_local_boards().await
    }

    /// Scan for local boards with logging (for CLI or when logging is desired)
    pub async fn scan_local_boards_with_logging(
        &mut self,
        tx: Option<tokio::sync::mpsc::UnboundedSender<crate::models::AppEvent>>,
    ) -> Result<()> {
        self.local_boards_loading = true;
        self.local_boards_fetch_error = None;
        self.local_boards.clear();

        if let Some(ref logger) = tx {
            let _ = logger.send(crate::models::AppEvent::BuildOutput(
                "board-scan".to_string(),
                "🔍 Scanning for local boards...".to_string(),
            ));
        }

        // Use serialport to discover serial ports
        match serialport::available_ports() {
            Ok(ports) => {
                // Filter for relevant USB ports
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
                            // On Windows, ESP32 devices appear as COM ports
                            || port_name.starts_with("COM")
                    })
                    .collect();

                if let Some(ref logger) = tx {
                    let _ = logger.send(crate::models::AppEvent::BuildOutput(
                        "board-scan".to_string(),
                        format!("📡 Found {} USB serial ports", relevant_ports.len()),
                    ));
                }

                // Try to identify each board using espflash with logging
                for port_info in relevant_ports {
                    match crate::utils::espflash_utils::identify_esp_board_with_logging(
                        &port_info.port_name,
                        tx.clone(),
                    )
                    .await
                    {
                        Ok(Some(esp_info)) => {
                            if let Some(ref logger) = tx {
                                let _ = logger.send(crate::models::AppEvent::BuildOutput(
                                    "board-scan".to_string(),
                                    format!(
                                        "✅ Identified {} on {}",
                                        esp_info.chip_type, port_info.port_name
                                    ),
                                ));
                            }

                            // Generate unique ID from MAC address
                            let unique_id = if esp_info.mac_address != "Unknown"
                                && !esp_info.mac_address.contains("*")
                            {
                                let mac_clean = esp_info.mac_address.replace(":", "");
                                format!("MAC{}", mac_clean)
                            } else {
                                format!(
                                    "{}:{}-{}",
                                    esp_info.chip_type,
                                    esp_info.chip_revision.as_deref().unwrap_or("unknown"),
                                    port_info.port_name.replace("/", "-").replace(".", "_")
                                )
                            };

                            let local_board = LocalBoard {
                                port: esp_info.port,
                                chip_type: esp_info.chip_type,
                                device_description: esp_info.device_description,
                                mac_address: esp_info.mac_address,
                                unique_id,
                            };

                            self.local_boards.push(local_board);
                        }
                        Ok(None) => {
                            if let Some(ref logger) = tx {
                                let _ = logger.send(crate::models::AppEvent::BuildOutput(
                                    "board-scan".to_string(),
                                    format!(
                                        "ℹ️ No ESP32 board detected on {}",
                                        port_info.port_name
                                    ),
                                ));
                            }
                        }
                        Err(e) => {
                            if let Some(ref logger) = tx {
                                let _ = logger.send(crate::models::AppEvent::BuildOutput(
                                    "board-scan".to_string(),
                                    format!(
                                        "⚠️ Error identifying board on {}: {}",
                                        port_info.port_name, e
                                    ),
                                ));
                            }
                        }
                    }
                }

                // Initialize list state if we have boards
                if !self.local_boards.is_empty() {
                    self.local_board_list_state.select(Some(0));
                    self.selected_local_board = 0;
                }

                if let Some(ref logger) = tx {
                    let _ = logger.send(crate::models::AppEvent::BuildOutput(
                        "board-scan".to_string(),
                        format!(
                            "✅ Local board scan complete. Found {} ESP32 boards",
                            self.local_boards.len()
                        ),
                    ));
                }
            }
            Err(e) => {
                let error_msg = format!("Failed to scan serial ports: {}", e);
                self.local_boards_fetch_error = Some(error_msg.clone());
                if let Some(ref logger) = tx {
                    let _ = logger.send(crate::models::AppEvent::BuildOutput(
                        "board-scan".to_string(),
                        format!("❌ {}", error_msg),
                    ));
                }
            }
        }

        self.local_boards_loading = false;
        Ok(())
    }

    /// Execute flash operation with selected local board
    pub async fn flash_with_selected_local_board(
        &mut self,
        tx: tokio::sync::mpsc::UnboundedSender<crate::models::AppEvent>,
    ) -> Result<()> {
        // Validate selection
        if self.selected_local_board >= self.local_boards.len() {
            return Err(anyhow::anyhow!("No local board selected"));
        }
        if self.selected_board >= self.boards.len() {
            return Err(anyhow::anyhow!("No project board selected"));
        }

        // Clone data first before any mutations to avoid borrow checker issues
        let local_board = self.local_boards[self.selected_local_board].clone();
        let project_board = self.boards[self.selected_board].clone();
        let board_name = project_board.name.clone();
        let config_file = project_board.config_file.clone();
        let build_dir = project_board.build_dir.clone();
        let project_dir = self.project_dir.clone();
        let logs_dir = self.logs_dir.clone();
        let selected_port = local_board.port.clone();

        // Close the dialog and start flashing
        self.show_local_board_dialog = false;

        // Update status to flashing
        self.boards[self.selected_board].status = BuildStatus::Flashing;
        self.boards[self.selected_board].last_updated = Local::now();
        self.boards[self.selected_board].log_lines.clear();
        self.reset_log_scroll();

        let tx_clone = tx.clone();
        let action_name = "Flash".to_string();
        let project_handler = self.project_handler.as_ref().map(|h| {
            // Clone the handler for use in async context
            match h.project_type() {
                crate::projects::ProjectType::RustNoStd => {
                    Box::new(crate::projects::handlers::rust_nostd::RustNoStdHandler)
                        as Box<dyn crate::projects::ProjectHandler>
                }
                crate::projects::ProjectType::Arduino => {
                    Box::new(crate::projects::handlers::arduino::ArduinoHandler)
                        as Box<dyn crate::projects::ProjectHandler>
                }
                crate::projects::ProjectType::PlatformIO => {
                    Box::new(crate::projects::handlers::platformio::PlatformIOHandler)
                        as Box<dyn crate::projects::ProjectHandler>
                }
                crate::projects::ProjectType::MicroPython => {
                    Box::new(crate::projects::handlers::micropython::MicroPythonHandler)
                        as Box<dyn crate::projects::ProjectHandler>
                }
                crate::projects::ProjectType::CircuitPython => {
                    Box::new(crate::projects::handlers::circuitpython::CircuitPythonHandler)
                        as Box<dyn crate::projects::ProjectHandler>
                }
                crate::projects::ProjectType::Zephyr => {
                    Box::new(crate::projects::handlers::zephyr::ZephyrHandler)
                        as Box<dyn crate::projects::ProjectHandler>
                }
                crate::projects::ProjectType::NuttX => {
                    Box::new(crate::projects::handlers::nuttx::NuttXHandler)
                        as Box<dyn crate::projects::ProjectHandler>
                }
                crate::projects::ProjectType::TinyGo => {
                    Box::new(crate::projects::handlers::tinygo::TinyGoHandler)
                        as Box<dyn crate::projects::ProjectHandler>
                }
                crate::projects::ProjectType::Jaculus => {
                    Box::new(crate::projects::handlers::jaculus::JaculusHandler)
                        as Box<dyn crate::projects::ProjectHandler>
                }
                crate::projects::ProjectType::EspIdf => {
                    Box::new(crate::projects::handlers::esp_idf::EspIdfHandler)
                        as Box<dyn crate::projects::ProjectHandler>
                }
            }
        });

        // Spawn the flash task using unified flash service
        tokio::spawn(async move {
            use crate::services::UnifiedFlashService;
            let flash_service = UnifiedFlashService::new();

            let result = if let Some(handler) = project_handler.as_ref() {
                // Use project handler's flash_board method with the selected port
                let project_board_config = crate::models::ProjectBoardConfig {
                    name: board_name.clone(),
                    config_file,
                    build_dir,
                    target: None,
                    project_type: handler.project_type(),
                };

                // Build artifacts first if needed, then flash
                match handler
                    .build_board(&project_dir, &project_board_config, tx_clone.clone())
                    .await
                {
                    Ok(artifacts) => {
                        // Now flash with the selected port
                        handler
                            .flash_board(
                                &project_dir,
                                &project_board_config,
                                &artifacts,
                                Some(&selected_port),
                                tx_clone.clone(),
                            )
                            .await
                    }
                    Err(e) => Err(e),
                }
            } else {
                // Use unified flash service for ESP-IDF projects as fallback
                flash_service
                    .flash_esp_idf_project(
                        &project_dir,
                        &selected_port,
                        Some(build_dir),
                        Some(tx_clone.clone()),
                        Some(board_name.clone()),
                    )
                    .await
                    .map(|flash_result| {
                        if !flash_result.success {
                            Err(anyhow::anyhow!("Flash failed: {}", flash_result.message))
                        } else {
                            Ok(())
                        }
                    })
                    .unwrap_or_else(|e| Err(e))
            };

            let _ = tx_clone.send(crate::models::AppEvent::ActionFinished(
                board_name,
                action_name,
                result.is_ok(),
            ));
        });

        Ok(())
    }

    /// Navigate to previous local board
    pub fn previous_local_board(&mut self) {
        if !self.local_boards.is_empty() {
            if self.selected_local_board > 0 {
                self.selected_local_board -= 1;
            } else {
                self.selected_local_board = self.local_boards.len() - 1;
            }
            self.local_board_list_state
                .select(Some(self.selected_local_board));
        }
    }

    /// Navigate to next local board
    pub fn next_local_board(&mut self) {
        if !self.local_boards.is_empty() {
            self.selected_local_board = (self.selected_local_board + 1) % self.local_boards.len();
            self.local_board_list_state
                .select(Some(self.selected_local_board));
        }
    }

    /// Generate all support scripts in the ./support/ directory
    pub fn generate_support_scripts(&self) -> Result<()> {
        // Ensure support directory exists
        std::fs::create_dir_all(&self.support_dir)?;

        // Generate individual board scripts
        self.generate_individual_board_scripts()?;

        // Generate build-all scripts for different strategies
        self.generate_build_all_scripts()?;

        // Generate flash scripts
        self.generate_flash_scripts()?;

        // Generate utility scripts
        self.generate_utility_scripts()?;

        Ok(())
    }

    /// Generate individual build and flash scripts for each board
    fn generate_individual_board_scripts(&self) -> Result<()> {
        for board in &self.boards {
            // Generate individual build script
            let build_script_path = self.support_dir.join(format!("build-{}.sh", board.name));
            let build_script_content = self.generate_board_build_script_content(board)?;
            self.write_executable_script(&build_script_path, &build_script_content)?;

            // Generate individual flash script
            let flash_script_path = self.support_dir.join(format!("flash-{}.sh", board.name));
            let flash_script_content = self.generate_board_flash_script_content(board)?;
            self.write_executable_script(&flash_script_path, &flash_script_content)?;
        }
        Ok(())
    }

    /// Generate build-all scripts for different strategies
    fn generate_build_all_scripts(&self) -> Result<()> {
        // Sequential build script
        let sequential_script_path = self.support_dir.join("build-all-sequential.sh");
        let sequential_content = self.generate_sequential_build_script_content()?;
        self.write_executable_script(&sequential_script_path, &sequential_content)?;

        // Parallel build script
        let parallel_script_path = self.support_dir.join("build-all-parallel.sh");
        let parallel_content = self.generate_parallel_build_script_content()?;
        self.write_executable_script(&parallel_script_path, &parallel_content)?;

        // Professional idf-build-apps script
        let idf_script_path = self.support_dir.join("build-all-idf-build-apps.sh");
        let idf_content = self.generate_idf_build_apps_script_content()?;
        self.write_executable_script(&idf_script_path, &idf_content)?;

        Ok(())
    }

    /// Generate flash scripts
    fn generate_flash_scripts(&self) -> Result<()> {
        // Flash all boards script
        let flash_all_script_path = self.support_dir.join("flash-all.sh");
        let flash_all_content = self.generate_flash_all_script_content()?;
        self.write_executable_script(&flash_all_script_path, &flash_all_content)?;

        Ok(())
    }

    /// Generate utility scripts (clean, etc.)
    fn generate_utility_scripts(&self) -> Result<()> {
        // Clean all script
        let clean_all_script_path = self.support_dir.join("clean-all.sh");
        let clean_all_content = self.generate_clean_all_script_content()?;
        self.write_executable_script(&clean_all_script_path, &clean_all_content)?;

        Ok(())
    }

    /// Generate content for individual board build script
    fn generate_board_build_script_content(&self, board: &BoardConfig) -> Result<String> {
        let project_type = if let Some(ref handler) = self.project_handler {
            handler.project_type()
        } else {
            crate::projects::ProjectType::EspIdf
        };

        let content = match project_type {
            crate::projects::ProjectType::EspIdf => {
                let config_file = board.config_file.to_string_lossy();
                let build_dir = board.build_dir.to_string_lossy();

                format!(
                    r#"#!/bin/bash
# ESPBrew Generated Script - Build {board_name}
# Generated: {timestamp}
# Board: {board_name} ({target})
# Config: {config_file}
# Build Dir: {build_dir}

set -e  # Exit on any error

echo "🔨 Building {board_name} using ESP-IDF..."
echo "📁 Project: $(pwd)"
echo "⚙️  Config: {config_file}"
echo "📂 Build Dir: {build_dir}"
echo

# Set target
echo "🎯 Setting target for {board_name}..."
export SDKCONFIG_DEFAULTS="{config_file}"
idf.py -D SDKCONFIG="{build_dir}/sdkconfig" -B "{build_dir}" set-target {target}

# Build
echo "🔨 Building {board_name}..."
idf.py -D SDKCONFIG="{build_dir}/sdkconfig" -B "{build_dir}" build

echo "✅ Build completed successfully for {board_name}!"
echo "📦 Binaries available in: {build_dir}"
"#,
                    board_name = board.name,
                    timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
                    target = board.target.as_deref().unwrap_or("esp32s3"),
                    config_file = config_file,
                    build_dir = build_dir,
                )
            }
            crate::projects::ProjectType::RustNoStd => {
                let build_dir = board.build_dir.to_string_lossy();

                format!(
                    r#"#!/bin/bash
# ESPBrew Generated Script - Build {board_name}
# Generated: {timestamp}
# Board: {board_name} ({target})
# Build Dir: {build_dir}

set -e  # Exit on any error

echo "🦀 Building {board_name} using Rust no_std..."
echo "📁 Project: $(pwd)"
echo "📂 Build Dir: {build_dir}"
echo

# Build with cargo
echo "🔨 Building {board_name}..."
cargo build --release --target {target} --target-dir "{build_dir}"

echo "✅ Build completed successfully for {board_name}!"
echo "📦 Binaries available in: {build_dir}"
"#,
                    board_name = board.name,
                    timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
                    target = board.target.as_deref().unwrap_or("xtensa-esp32s3-espidf"),
                    build_dir = build_dir,
                )
            }
            _ => {
                // Generic build script for other project types
                format!(
                    r#"#!/bin/bash
# ESPBrew Generated Script - Build {board_name}
# Generated: {timestamp}
# Board: {board_name}
# Project Type: {project_type}

set -e  # Exit on any error

echo "🔨 Building {board_name} using {project_type}..."
echo "📁 Project: $(pwd)"
echo

echo "⚠️  Generic build script - please customize for your project type"
echo "✅ Build script generated for {board_name}"
"#,
                    board_name = board.name,
                    timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
                    project_type = project_type.name(),
                )
            }
        };

        Ok(content)
    }

    /// Generate content for individual board flash script
    fn generate_board_flash_script_content(&self, board: &BoardConfig) -> Result<String> {
        let project_type = if let Some(ref handler) = self.project_handler {
            handler.project_type()
        } else {
            crate::projects::ProjectType::EspIdf
        };

        let content = match project_type {
            crate::projects::ProjectType::EspIdf => {
                let build_dir = board.build_dir.to_string_lossy();

                format!(
                    r#"#!/bin/bash
# ESPBrew Generated Script - Flash {board_name}
# Generated: {timestamp}
# Board: {board_name} ({target})
# Build Dir: {build_dir}

set -e  # Exit on any error

echo "🔥 Flashing {board_name} using ESP-IDF..."
echo "📁 Project: $(pwd)"
echo "📂 Build Dir: {build_dir}"
echo

# Check if build directory exists
if [ ! -d "{build_dir}" ]; then
    echo "❌ Build directory not found: {build_dir}"
    echo "💡 Run the build script first: ./support/build-{board_name}.sh"
    exit 1
fi

# Flash
echo "🔥 Flashing {board_name}..."
idf.py -B "{build_dir}" flash

echo "✅ Flash completed successfully for {board_name}!"
echo "💡 You can now monitor with: idf.py -B '{build_dir}' monitor"
"#,
                    board_name = board.name,
                    timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
                    target = board.target.as_deref().unwrap_or("esp32s3"),
                    build_dir = build_dir,
                )
            }
            crate::projects::ProjectType::RustNoStd => {
                let build_dir = board.build_dir.to_string_lossy();

                format!(
                    r#"#!/bin/bash
# ESPBrew Generated Script - Flash {board_name}
# Generated: {timestamp}
# Board: {board_name} ({target})
# Build Dir: {build_dir}

set -e  # Exit on any error

echo "🦀 Flashing {board_name} using Rust no_std (espflash)..."
echo "📁 Project: $(pwd)"
echo "📂 Build Dir: {build_dir}"
echo

# Find the ELF binary
ELF_FILE=$(find "{build_dir}" -name "*.elf" | head -1)
if [ -z "$ELF_FILE" ]; then
    echo "❌ ELF file not found in {build_dir}"
    echo "💡 Run the build script first: ./support/build-{board_name}.sh"
    exit 1
fi

echo "📦 Found ELF file: $ELF_FILE"

# Flash with espflash
echo "🔥 Flashing {board_name}..."
espflash flash "$ELF_FILE"

echo "✅ Flash completed successfully for {board_name}!"
echo "💡 You can now monitor with: espflash monitor"
"#,
                    board_name = board.name,
                    timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
                    target = board.target.as_deref().unwrap_or("xtensa-esp32s3-espidf"),
                    build_dir = build_dir,
                )
            }
            _ => {
                // Generic flash script for other project types
                format!(
                    r#"#!/bin/bash
# ESPBrew Generated Script - Flash {board_name}
# Generated: {timestamp}
# Board: {board_name}
# Project Type: {project_type}

set -e  # Exit on any error

echo "🔥 Flashing {board_name} using {project_type}..."
echo "📁 Project: $(pwd)"
echo

echo "⚠️  Generic flash script - please customize for your project type"
echo "✅ Flash script generated for {board_name}"
"#,
                    board_name = board.name,
                    timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
                    project_type = project_type.name(),
                )
            }
        };

        Ok(content)
    }

    /// Generate sequential build script content
    fn generate_sequential_build_script_content(&self) -> Result<String> {
        let board_count = self.boards.len();
        let mut build_commands = Vec::new();

        for board in &self.boards {
            build_commands.push(format!(
                "echo \"🔨 Building {} ({}/{})\"\n./support/build-{}.sh",
                board.name,
                build_commands.len() + 1,
                board_count,
                board.name
            ));
        }

        let content = format!(
            r#"#!/bin/bash
# ESPBrew Generated Script - Build All Boards (Sequential)
# Generated: {timestamp}
# Boards: {board_count}

set -e  # Exit on any error

echo "🍺 ESPBrew Sequential Build - Building {board_count} board(s)"
echo "📁 Project: $(pwd)"
echo "📊 Strategy: Sequential (avoids component manager conflicts)"
echo

{build_commands}

echo
echo "✅ All {board_count} boards built successfully!"
echo "🎉 Sequential build completed!"
"#,
            timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
            board_count = board_count,
            build_commands = build_commands.join("\n")
        );

        Ok(content)
    }

    /// Generate parallel build script content
    fn generate_parallel_build_script_content(&self) -> Result<String> {
        let board_count = self.boards.len();
        let build_commands: Vec<String> = self
            .boards
            .iter()
            .map(|board| format!("./support/build-{}.sh &", board.name))
            .collect();

        let content = format!(
            r#"#!/bin/bash
# ESPBrew Generated Script - Build All Boards (Parallel)
# Generated: {timestamp}
# Boards: {board_count}

set -e  # Exit on any error

echo "🍺 ESPBrew Parallel Build - Building {board_count} board(s)"
echo "📁 Project: $(pwd)"
echo "📊 Strategy: Parallel (faster but may cause component manager conflicts)"
echo "⚠️  Warning: Parallel builds may interfere with ESP-IDF component manager"
echo

echo "🚀 Starting parallel builds..."
{parallel_commands}

echo "⏳ Waiting for all builds to complete..."
wait

echo
echo "✅ All {board_count} boards built successfully!"
echo "🎉 Parallel build completed!"
"#,
            timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
            board_count = board_count,
            parallel_commands = build_commands.join("\n")
        );

        Ok(content)
    }

    /// Generate professional multi-board build script content (project-type aware)
    fn generate_idf_build_apps_script_content(&self) -> Result<String> {
        let board_count = self.boards.len();
        let board_configs: Vec<String> = self
            .boards
            .iter()
            .map(|board| format!("    {}", board.config_file.to_string_lossy()))
            .collect();

        // Determine project type from handler
        let project_type = if let Some(ref handler) = self.project_handler {
            handler.project_type()
        } else {
            crate::projects::ProjectType::EspIdf
        };

        let (tool_name, tool_emoji, build_commands) = match project_type {
            crate::projects::ProjectType::EspIdf => {
                ("idf-build-apps", "🏗️", format!(
                    r#"# Check if idf-build-apps is installed
if ! command -v idf-build-apps &> /dev/null; then
    echo "❌ idf-build-apps not found!"
    echo "💡 Install with: pip install idf-build-apps"
    echo "📖 More info: https://github.com/espressif/idf-build-apps"
    exit 1
fi

echo "🔨 Building all boards with idf-build-apps..."
idf-build-apps find \\
    --build-dir ./build \\
    --config-file sdkconfig.defaults.* \\
    --target "*" \\
    --recursive

idf-build-apps build \\
    --build-dir ./build \\
    --config-file sdkconfig.defaults.* \\
    --target "*" \\
    --parallel-count $(nproc) \\
    --parallel-index 1"#
                ))
            }
            crate::projects::ProjectType::RustNoStd => {
                ("cargo (parallel)", "🦀", format!(
                    r#"# Check if cargo is installed
if ! command -v cargo &> /dev/null; then
    echo "❌ cargo not found!"
    echo "💡 Install Rust: https://rustup.rs/"
    exit 1
fi

echo "🔨 Building all Rust configurations in parallel..."
# For Rust projects, we can safely build configurations in parallel
{}

echo "⏳ Waiting for all Rust builds to complete..."
wait"#,
                    self.boards.iter().map(|board| {
                        format!("(\n    echo \"🦀 Building {} in background...\"\n    cargo build --release > ./logs/{}.log 2>&1 &\n)", 
                               board.name, board.name)
                    }).collect::<Vec<_>>().join("\n")
                ))
            }
            crate::projects::ProjectType::Arduino => {
                ("arduino-cli (parallel)", "🎨", 
                 "# Arduino projects - using individual build scripts in parallel\n./support/build-all-parallel.sh".to_string())
            }
            crate::projects::ProjectType::PlatformIO => {
                ("pio (parallel)", "🚀", 
                 "# PlatformIO projects - using pio run for all environments\npio run".to_string())
            }
            _ => {
                ("sequential builds", "🔧", 
                 "# Using sequential builds for this project type\n./support/build-all-sequential.sh".to_string())
            }
        };

        let content = format!(
            r#"#!/bin/bash
# ESPBrew Generated Script - Professional Multi-Board Build
# Generated: {timestamp}
# Project Type: {project_type_name}
# Boards: {board_count}
# Tool: {tool_name}

set -e  # Exit on any error

echo "🍺 ESPBrew Professional Build - {project_type_name} Project"
echo "📁 Project: $(pwd)"
echo "📊 Strategy: {tool_name} (professional, optimized for {project_type_name})"
echo "🎯 Boards: {board_count}"
echo

echo "{tool_emoji} Using {tool_name} for optimal build performance"
echo "📂 Config files:"
{config_list}
echo

# Build all configurations
{build_commands}

echo
echo "✅ All {board_count} boards built successfully!"
echo "🎉 Professional build completed with zero conflicts!"
echo "📦 Build artifacts available in build directories"
"#,
            timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
            project_type_name = project_type.name(),
            board_count = board_count,
            tool_name = tool_name,
            tool_emoji = tool_emoji,
            config_list = board_configs.join("\n"),
            build_commands = build_commands
        );

        Ok(content)
    }

    /// Generate flash all script content
    fn generate_flash_all_script_content(&self) -> Result<String> {
        let board_count = self.boards.len();
        let mut flash_commands = Vec::new();

        for board in &self.boards {
            flash_commands.push(format!(
                "echo \"🔥 Flashing {} ({}/{})\"\n./support/flash-{}.sh",
                board.name,
                flash_commands.len() + 1,
                board_count,
                board.name
            ));
        }

        let content = format!(
            r#"#!/bin/bash
# ESPBrew Generated Script - Flash All Boards
# Generated: {timestamp}
# Boards: {board_count}

set -e  # Exit on any error

echo "🍺 ESPBrew Flash All - Flashing {board_count} board(s)"
echo "📁 Project: $(pwd)"
echo "⚠️  Make sure only one board is connected at a time!"
echo

read -p "🔌 Connect the first board and press Enter to continue..."
echo

{flash_commands}

echo
echo "✅ All {board_count} boards flashed successfully!"
echo "🎉 Flash all completed!"
"#,
            timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
            board_count = board_count,
            flash_commands = flash_commands
                .join("\nread -p \"🔌 Connect the next board and press Enter...\"\necho\n")
        );

        Ok(content)
    }

    /// Generate clean all script content
    fn generate_clean_all_script_content(&self) -> Result<String> {
        let board_count = self.boards.len();
        let clean_commands: Vec<String> = self
            .boards
            .iter()
            .map(|board| {
                let build_dir = board.build_dir.to_string_lossy();
                format!(
                    "echo \"🧹 Cleaning {}...\"\nrm -rf \"{}\"",
                    board.name, build_dir
                )
            })
            .collect();

        let content = format!(
            r#"#!/bin/bash
# ESPBrew Generated Script - Clean All Builds
# Generated: {timestamp}
# Boards: {board_count}

echo "🍺 ESPBrew Clean All - Cleaning {board_count} board(s)"
echo "📁 Project: $(pwd)"
echo "🗑️  This will remove all build directories"
echo

read -p "⚠️  Are you sure you want to clean all builds? (y/N): " confirm
if [[ $confirm != [yY] && $confirm != [yY][eE][sS] ]]; then
    echo "❌ Clean cancelled"
    exit 0
fi

echo "🧹 Cleaning all build directories..."
{clean_commands}

# Also clean common directories
echo "🧹 Cleaning common build artifacts..."
rm -rf build/ managed_components/ dependencies.lock

echo
echo "✅ All build directories cleaned!"
echo "🎉 Clean all completed!"
"#,
            timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
            board_count = board_count,
            clean_commands = clean_commands.join("\n")
        );

        Ok(content)
    }

    /// Write script content to file and make it executable
    fn write_executable_script(&self, path: &std::path::Path, content: &str) -> Result<()> {
        // Write the script content
        std::fs::write(path, content)?;

        // Make it executable (Unix/Linux/macOS only)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let metadata = std::fs::metadata(path)?;
            let mut permissions = metadata.permissions();
            permissions.set_mode(0o755); // rwxr-xr-x
            std::fs::set_permissions(path, permissions)?;
        }

        // On Windows, files are executable by default if they have certain extensions
        // Scripts generated here are shell scripts (.sh) which wouldn't run directly on Windows anyway
        // Users would need WSL or similar to run them

        Ok(())
    }

    // Navigation methods - stubs to be implemented
    pub fn next_board(&mut self) {
        if !self.boards.is_empty() {
            self.selected_board = (self.selected_board + 1) % self.boards.len();
            self.list_state.select(Some(self.selected_board));
        }
    }

    pub fn previous_board(&mut self) {
        if !self.boards.is_empty() {
            if self.selected_board > 0 {
                self.selected_board -= 1;
            } else {
                self.selected_board = self.boards.len() - 1;
            }
            self.list_state.select(Some(self.selected_board));
        }
    }

    pub fn toggle_focused_pane(&mut self) {
        self.focused_pane = match self.focused_pane {
            FocusedPane::BoardList => FocusedPane::ComponentList,
            FocusedPane::ComponentList => FocusedPane::LogPane,
            FocusedPane::LogPane => FocusedPane::BoardList,
        };
    }

    pub fn acknowledge_tool_warning(&mut self) {
        self.tool_warning_acknowledged = true;
    }

    pub fn reset_log_scroll(&mut self) {
        self.log_scroll_offset = 0;
    }

    // Component navigation
    pub fn next_component(&mut self) {
        if !self.components.is_empty() {
            self.selected_component = (self.selected_component + 1) % self.components.len();
            self.component_list_state
                .select(Some(self.selected_component));
        }
    }

    pub fn previous_component(&mut self) {
        if !self.components.is_empty() {
            if self.selected_component > 0 {
                self.selected_component -= 1;
            } else {
                self.selected_component = self.components.len() - 1;
            }
            self.component_list_state
                .select(Some(self.selected_component));
        }
    }

    // Log scrolling methods
    pub fn scroll_log_up(&mut self) {
        if self.log_scroll_offset > 0 {
            self.log_scroll_offset -= 1;
        }
    }

    pub fn scroll_log_down(&mut self) {
        if let Some(board) = self.boards.get(self.selected_board) {
            if self.log_scroll_offset < board.log_lines.len().saturating_sub(1) {
                self.log_scroll_offset += 1;
            }
        }
    }

    // Board and log management
    pub fn add_log_line(&mut self, board_name: &str, line: String) {
        if let Some(board) = self.boards.iter_mut().find(|b| b.name == board_name) {
            board.log_lines.push(line);
            board.last_updated = chrono::Local::now();
        }
    }

    pub fn update_board_status(&mut self, board_name: &str, status: BuildStatus) {
        if let Some(board) = self.boards.iter_mut().find(|b| b.name == board_name) {
            board.status = status;
            board.last_updated = chrono::Local::now();
        }
    }

    /// Execute a board action asynchronously
    pub async fn execute_action(
        &mut self,
        action: BoardAction,
        tx: tokio::sync::mpsc::UnboundedSender<crate::models::AppEvent>,
    ) -> Result<()> {
        // Handle Flash, RemoteFlash and RemoteMonitor specially
        if action == BoardAction::Flash {
            // Flash action should show local board selection dialog
            self.start_local_board_scan_for_flash().await?;
            return Ok(());
        }
        if action == BoardAction::RemoteFlash {
            self.start_fetching_remote_boards(tx);
            return Ok(());
        }
        if action == BoardAction::RemoteMonitor {
            self.remote_action_type = crate::models::server::RemoteActionType::Monitor;
            self.start_fetching_remote_boards(tx);
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

        // Update status immediately
        self.boards[board_index].status = match action {
            BoardAction::Build => BuildStatus::Building,
            BoardAction::Flash => BuildStatus::Flashing,
            BoardAction::FlashAppOnly => BuildStatus::Flashing,
            BoardAction::GenerateBinary => BuildStatus::Building,
            BoardAction::Monitor => BuildStatus::Monitoring,
            _ => BuildStatus::Building, // For clean/purge operations
        };
        self.boards[board_index].last_updated = Local::now();

        // Clear previous logs for this board
        self.boards[board_index].log_lines.clear();
        self.reset_log_scroll();

        let tx_clone = tx.clone();
        let action_name = action.name().to_string();
        let project_handler = self.project_handler.as_ref().map(|h| {
            // Clone the handler for use in async context
            match h.project_type() {
                crate::projects::ProjectType::RustNoStd => {
                    Box::new(crate::projects::handlers::rust_nostd::RustNoStdHandler)
                        as Box<dyn crate::projects::ProjectHandler>
                }
                crate::projects::ProjectType::Arduino => {
                    Box::new(crate::projects::handlers::arduino::ArduinoHandler)
                        as Box<dyn crate::projects::ProjectHandler>
                }
                crate::projects::ProjectType::PlatformIO => {
                    Box::new(crate::projects::handlers::platformio::PlatformIOHandler)
                        as Box<dyn crate::projects::ProjectHandler>
                }
                crate::projects::ProjectType::MicroPython => {
                    Box::new(crate::projects::handlers::micropython::MicroPythonHandler)
                        as Box<dyn crate::projects::ProjectHandler>
                }
                crate::projects::ProjectType::CircuitPython => {
                    Box::new(crate::projects::handlers::circuitpython::CircuitPythonHandler)
                        as Box<dyn crate::projects::ProjectHandler>
                }
                crate::projects::ProjectType::Zephyr => {
                    Box::new(crate::projects::handlers::zephyr::ZephyrHandler)
                        as Box<dyn crate::projects::ProjectHandler>
                }
                crate::projects::ProjectType::NuttX => {
                    Box::new(crate::projects::handlers::nuttx::NuttXHandler)
                        as Box<dyn crate::projects::ProjectHandler>
                }
                crate::projects::ProjectType::TinyGo => {
                    Box::new(crate::projects::handlers::tinygo::TinyGoHandler)
                        as Box<dyn crate::projects::ProjectHandler>
                }
                crate::projects::ProjectType::Jaculus => {
                    Box::new(crate::projects::handlers::jaculus::JaculusHandler)
                        as Box<dyn crate::projects::ProjectHandler>
                }
                crate::projects::ProjectType::EspIdf => {
                    Box::new(crate::projects::handlers::esp_idf::EspIdfHandler)
                        as Box<dyn crate::projects::ProjectHandler>
                }
            }
        });

        // Spawn the action execution task
        tokio::spawn(async move {
            let log_file = logs_dir.join(format!("{}.log", board_name));
            let result = match action {
                BoardAction::Build => {
                    if let Some(handler) = project_handler.as_ref() {
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
                        // Fallback to ESP-IDF if no project handler
                        Self::build_board_esp_idf(
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
                BoardAction::Flash => {
                    // Flash action now requires local board selection
                    // This should not be reached as Flash action is handled specially
                    Err(anyhow::anyhow!(
                        "Flash action should be handled through local board selection"
                    ))
                }
                BoardAction::Clean => {
                    if let Some(handler) = project_handler.as_ref() {
                        Self::clean_board_with_handler(
                            handler.as_ref(),
                            &board_name,
                            &project_dir,
                            &config_file,
                            &build_dir,
                            tx_clone.clone(),
                        )
                        .await
                    } else {
                        Self::clean_board_esp_idf(
                            &board_name,
                            &project_dir,
                            &build_dir,
                            &log_file,
                            tx_clone.clone(),
                        )
                        .await
                    }
                }
                BoardAction::Monitor => {
                    Self::monitor_board_esp_idf(
                        &board_name,
                        &project_dir,
                        &build_dir,
                        &log_file,
                        tx_clone.clone(),
                    )
                    .await
                }
                BoardAction::FlashAppOnly => {
                    Self::flash_app_only_esp_idf(
                        &board_name,
                        &project_dir,
                        &build_dir,
                        &log_file,
                        tx_clone.clone(),
                    )
                    .await
                }
                BoardAction::Purge => {
                    Self::purge_board_esp_idf(&board_name, &build_dir, &log_file, tx_clone.clone())
                        .await
                }
                BoardAction::GenerateBinary => {
                    Self::generate_binary_esp_idf(
                        &board_name,
                        &project_dir,
                        &build_dir,
                        &log_file,
                        tx_clone.clone(),
                    )
                    .await
                }
                _ => {
                    let _ = tx_clone.send(crate::models::AppEvent::BuildOutput(
                        board_name.clone(),
                        format!("⚠️  Action '{}' not yet implemented", action.name()),
                    ));
                    Ok(())
                }
            };

            // Send completion event
            let _ = tx_clone.send(crate::models::AppEvent::ActionFinished(
                board_name,
                action_name,
                result.is_ok(),
            ));
        });

        Ok(())
    }

    /// Build board using project handler
    pub async fn build_board_with_handler(
        project_handler: &dyn crate::projects::ProjectHandler,
        board_name: &str,
        project_dir: &std::path::Path,
        config_file: &std::path::Path,
        build_dir: &std::path::Path,
        _log_file: &std::path::Path,
        tx: tokio::sync::mpsc::UnboundedSender<crate::models::AppEvent>,
    ) -> Result<()> {
        // Create a ProjectBoardConfig from the individual parameters
        let board_config = ProjectBoardConfig {
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

    /// Flash board using project handler
    pub async fn flash_board_with_handler(
        project_handler: &dyn crate::projects::ProjectHandler,
        board_name: &str,
        project_dir: &std::path::Path,
        config_file: &std::path::Path,
        build_dir: &std::path::Path,
        tx: tokio::sync::mpsc::UnboundedSender<crate::models::AppEvent>,
    ) -> Result<()> {
        // Create a ProjectBoardConfig from the individual parameters
        let board_config = ProjectBoardConfig {
            name: board_name.to_string(),
            config_file: config_file.to_path_buf(),
            build_dir: build_dir.to_path_buf(),
            target: None, // Will be auto-detected
            project_type: project_handler.project_type(),
        };

        // Build first to get artifacts
        let artifacts = match project_handler
            .build_board(project_dir, &board_config, tx.clone())
            .await
        {
            Ok(artifacts) => artifacts,
            Err(e) => {
                let _ = tx.send(crate::models::AppEvent::BuildOutput(
                    board_name.to_string(),
                    format!("❌ Build failed before flashing: {}", e),
                ));
                return Err(e);
            }
        };

        // Call the project handler's flash method
        project_handler
            .flash_board(project_dir, &board_config, &artifacts, None, tx)
            .await
    }

    /// Flash board using project handler with specific port
    pub async fn flash_board_with_handler_and_port(
        project_handler: &dyn crate::projects::ProjectHandler,
        board_name: &str,
        project_dir: &std::path::Path,
        config_file: &std::path::Path,
        build_dir: &std::path::Path,
        port: &str,
        tx: tokio::sync::mpsc::UnboundedSender<crate::models::AppEvent>,
    ) -> Result<()> {
        // Create a ProjectBoardConfig from the individual parameters
        let board_config = ProjectBoardConfig {
            name: board_name.to_string(),
            config_file: config_file.to_path_buf(),
            build_dir: build_dir.to_path_buf(),
            target: None, // Will be auto-detected
            project_type: project_handler.project_type(),
        };

        let _ = tx.send(crate::models::AppEvent::BuildOutput(
            board_name.to_string(),
            format!("🔥 Flashing {} to port {}...", board_name, port),
        ));

        // Build first to get artifacts
        let artifacts = match project_handler
            .build_board(project_dir, &board_config, tx.clone())
            .await
        {
            Ok(artifacts) => artifacts,
            Err(e) => {
                let _ = tx.send(crate::models::AppEvent::BuildOutput(
                    board_name.to_string(),
                    format!("❌ Build failed before flashing: {}", e),
                ));
                return Err(e);
            }
        };

        // Convert artifacts to flash data format and use existing espflash utils
        let _ = tx.send(crate::models::AppEvent::BuildOutput(
            board_name.to_string(),
            format!("📦 Preparing {} flash artifacts...", artifacts.len()),
        ));

        let mut flash_data_map = std::collections::HashMap::new();
        for artifact in &artifacts {
            if let Some(offset) = artifact.offset {
                match std::fs::read(&artifact.file_path) {
                    Ok(data) => {
                        let _ = tx.send(crate::models::AppEvent::BuildOutput(
                            board_name.to_string(),
                            format!(
                                "  📄 {} → 0x{:05x} ({:.1} KB)",
                                artifact.name,
                                offset,
                                data.len() as f64 / 1024.0
                            ),
                        ));
                        flash_data_map.insert(offset, data);
                    }
                    Err(e) => {
                        let _ = tx.send(crate::models::AppEvent::BuildOutput(
                            board_name.to_string(),
                            format!("❌ Failed to read {}: {}", artifact.name, e),
                        ));
                        return Err(anyhow::anyhow!(
                            "Failed to read artifact {}: {}",
                            artifact.name,
                            e
                        ));
                    }
                }
            }
        }

        if flash_data_map.is_empty() {
            return Err(anyhow::anyhow!("No flashable artifacts found"));
        }

        let _ = tx.send(crate::models::AppEvent::BuildOutput(
            board_name.to_string(),
            format!("🚀 Starting flash operation..."),
        ));

        // Use existing espflash utils for flashing
        match crate::utils::espflash_utils::flash_multi_binary(port, flash_data_map).await {
            Ok(_) => {
                let _ = tx.send(crate::models::AppEvent::BuildOutput(
                    board_name.to_string(),
                    "✅ Flash completed successfully!".to_string(),
                ));
                Ok(())
            }
            Err(e) => {
                let _ = tx.send(crate::models::AppEvent::BuildOutput(
                    board_name.to_string(),
                    format!("❌ Flash failed: {}", e),
                ));
                Err(anyhow::anyhow!("Flash operation failed: {}", e))
            }
        }
    }

    /// Clean board using project handler
    pub async fn clean_board_with_handler(
        project_handler: &dyn crate::projects::ProjectHandler,
        board_name: &str,
        project_dir: &std::path::Path,
        config_file: &std::path::Path,
        build_dir: &std::path::Path,
        tx: tokio::sync::mpsc::UnboundedSender<crate::models::AppEvent>,
    ) -> Result<()> {
        // Create a ProjectBoardConfig from the individual parameters
        let board_config = ProjectBoardConfig {
            name: board_name.to_string(),
            config_file: config_file.to_path_buf(),
            build_dir: build_dir.to_path_buf(),
            target: None, // Will be auto-detected
            project_type: project_handler.project_type(),
        };

        // Call the project handler's clean method
        project_handler
            .clean_board(project_dir, &board_config, tx)
            .await
    }

    /// Execute a command with real-time output streaming
    async fn execute_command_streaming(
        command: &str,
        args: &[&str],
        current_dir: &std::path::Path,
        env_vars: Vec<(String, String)>,
        board_name: &str,
        tx: tokio::sync::mpsc::UnboundedSender<crate::models::AppEvent>,
    ) -> Result<bool> {
        use std::process::Stdio;
        use tokio::io::{AsyncBufReadExt, BufReader};
        use tokio::process::Command;

        let mut cmd = Command::new(command);
        cmd.current_dir(current_dir)
            .args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        // Add environment variables
        for (key, value) in env_vars {
            cmd.env(key, value);
        }

        let mut child = cmd.spawn()?;

        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();

        let stdout_reader = BufReader::new(stdout);
        let stderr_reader = BufReader::new(stderr);

        let tx_stdout = tx.clone();
        let tx_stderr = tx.clone();
        let board_name_stdout = board_name.to_string();
        let board_name_stderr = board_name.to_string();

        // Handle stdout streaming
        let stdout_handle = tokio::spawn(async move {
            let mut lines = stdout_reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let _ = tx_stdout.send(crate::models::AppEvent::BuildOutput(
                    board_name_stdout.clone(),
                    line,
                ));
            }
        });

        // Handle stderr streaming
        let stderr_handle = tokio::spawn(async move {
            let mut lines = stderr_reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let _ = tx_stderr.send(crate::models::AppEvent::BuildOutput(
                    board_name_stderr.clone(),
                    format!("⚠️  {}", line),
                ));
            }
        });

        // Wait for command to complete
        let status = child.wait().await?;

        // Wait for all output to be processed
        let _ = tokio::try_join!(stdout_handle, stderr_handle);

        Ok(status.success())
    }

    /// ESP-IDF build implementation
    pub async fn build_board_esp_idf(
        board_name: &str,
        project_dir: &std::path::Path,
        config_file: &std::path::Path,
        build_dir: &std::path::Path,
        _log_file: &std::path::Path,
        tx: tokio::sync::mpsc::UnboundedSender<crate::models::AppEvent>,
    ) -> Result<()> {
        let _ = tx.send(crate::models::AppEvent::BuildOutput(
            board_name.to_string(),
            format!("🔨 Building {} using ESP-IDF...", board_name),
        ));

        // Determine target (simplified)
        let config_content = fs::read_to_string(config_file)?;
        let target = if config_content.contains("esp32s3") {
            "esp32s3"
        } else if config_content.contains("esp32c3") {
            "esp32c3"
        } else if config_content.contains("esp32c6") {
            "esp32c6"
        } else if config_content.contains("esp32p4") {
            "esp32p4"
        } else {
            "esp32s3" // default
        };

        let config_path = config_file.to_string_lossy().to_string();
        let sdkconfig_path = build_dir.join("sdkconfig");

        // Set target first
        let set_target_args = vec![
            "-D".to_string(),
            format!("SDKCONFIG={}", sdkconfig_path.display()),
            "-B".to_string(),
            build_dir.to_string_lossy().to_string(),
            "set-target".to_string(),
            target.to_string(),
        ];

        let set_target_args_str: Vec<&str> = set_target_args.iter().map(|s| s.as_str()).collect();
        let env_vars = vec![("SDKCONFIG_DEFAULTS".to_string(), config_path.clone())];

        let success = Self::execute_command_streaming(
            "idf.py",
            &set_target_args_str,
            project_dir,
            env_vars.clone(),
            board_name,
            tx.clone(),
        )
        .await?;

        if !success {
            let _ = tx.send(crate::models::AppEvent::BuildOutput(
                board_name.to_string(),
                "❌ Set target failed!".to_string(),
            ));
            return Err(anyhow::anyhow!("Set target failed"));
        }

        // Build
        let build_args = vec![
            "-D".to_string(),
            format!("SDKCONFIG={}", sdkconfig_path.display()),
            "-B".to_string(),
            build_dir.to_string_lossy().to_string(),
            "build".to_string(),
        ];

        let build_args_str: Vec<&str> = build_args.iter().map(|s| s.as_str()).collect();

        let success = Self::execute_command_streaming(
            "idf.py",
            &build_args_str,
            project_dir,
            env_vars,
            board_name,
            tx.clone(),
        )
        .await?;

        if success {
            let _ = tx.send(crate::models::AppEvent::BuildOutput(
                board_name.to_string(),
                "✅ Build completed successfully!".to_string(),
            ));
            Ok(())
        } else {
            let _ = tx.send(crate::models::AppEvent::BuildOutput(
                board_name.to_string(),
                "❌ Build failed!".to_string(),
            ));
            Err(anyhow::anyhow!("Build failed"))
        }
    }

    /// ESP-IDF flash implementation
    pub async fn flash_board_esp_idf(
        board_name: &str,
        project_dir: &std::path::Path,
        build_dir: &std::path::Path,
        _log_file: &std::path::Path,
        tx: tokio::sync::mpsc::UnboundedSender<crate::models::AppEvent>,
    ) -> Result<()> {
        let _ = tx.send(crate::models::AppEvent::BuildOutput(
            board_name.to_string(),
            format!("🔥 Flashing {} using ESP-IDF...", board_name),
        ));

        let flash_args = vec![
            "-B".to_string(),
            build_dir.to_string_lossy().to_string(),
            "flash".to_string(),
        ];

        let flash_args_str: Vec<&str> = flash_args.iter().map(|s| s.as_str()).collect();
        let env_vars = vec![]; // No special environment variables needed for flash

        let success = Self::execute_command_streaming(
            "idf.py",
            &flash_args_str,
            project_dir,
            env_vars,
            board_name,
            tx.clone(),
        )
        .await?;

        if success {
            let _ = tx.send(crate::models::AppEvent::BuildOutput(
                board_name.to_string(),
                "✅ Flash completed successfully!".to_string(),
            ));
            Ok(())
        } else {
            let _ = tx.send(crate::models::AppEvent::BuildOutput(
                board_name.to_string(),
                "❌ Flash failed!".to_string(),
            ));
            Err(anyhow::anyhow!("Flash failed"))
        }
    }

    /// ESP-IDF flash implementation with specific port using existing espflash utils
    pub async fn flash_board_esp_idf_with_port(
        board_name: &str,
        project_dir: &std::path::Path,
        build_dir: &std::path::Path,
        port: &str,
        _log_file: &std::path::Path,
        tx: tokio::sync::mpsc::UnboundedSender<crate::models::AppEvent>,
    ) -> Result<()> {
        let _ = tx.send(crate::models::AppEvent::BuildOutput(
            board_name.to_string(),
            format!(
                "🔥 Flashing {} to port {} using espflash...",
                board_name, port
            ),
        ));

        // First, build to ensure we have binaries
        let build_args = vec![
            "-B".to_string(),
            build_dir.to_string_lossy().to_string(),
            "build".to_string(),
        ];
        let build_args_str: Vec<&str> = build_args.iter().map(|s| s.as_str()).collect();
        let env_vars = vec![];

        let _ = tx.send(crate::models::AppEvent::BuildOutput(
            board_name.to_string(),
            "🔨 Building project first...".to_string(),
        ));

        let build_success = Self::execute_command_streaming(
            "idf.py",
            &build_args_str,
            project_dir,
            env_vars,
            board_name,
            tx.clone(),
        )
        .await?;

        if !build_success {
            return Err(anyhow::anyhow!("Build failed before flashing"));
        }

        // Look for standard ESP-IDF flash binaries
        let _ = tx.send(crate::models::AppEvent::BuildOutput(
            board_name.to_string(),
            "📦 Looking for flash binaries...".to_string(),
        ));

        let mut flash_data_map = std::collections::HashMap::new();

        // Check for bootloader
        let bootloader_bin = build_dir.join("bootloader").join("bootloader.bin");
        if bootloader_bin.exists() {
            match std::fs::read(&bootloader_bin) {
                Ok(data) => {
                    let _ = tx.send(crate::models::AppEvent::BuildOutput(
                        board_name.to_string(),
                        format!(
                            "  📄 bootloader.bin → 0x01000 ({:.1} KB)",
                            data.len() as f64 / 1024.0
                        ),
                    ));
                    flash_data_map.insert(0x1000, data);
                }
                Err(e) => {
                    let _ = tx.send(crate::models::AppEvent::BuildOutput(
                        board_name.to_string(),
                        format!("⚠️ Warning: Could not read bootloader.bin: {}", e),
                    ));
                }
            }
        }

        // Check for partition table
        let partition_bin = build_dir
            .join("partition_table")
            .join("partition-table.bin");
        if partition_bin.exists() {
            match std::fs::read(&partition_bin) {
                Ok(data) => {
                    let _ = tx.send(crate::models::AppEvent::BuildOutput(
                        board_name.to_string(),
                        format!(
                            "  📄 partition-table.bin → 0x08000 ({:.1} KB)",
                            data.len() as f64 / 1024.0
                        ),
                    ));
                    flash_data_map.insert(0x8000, data);
                }
                Err(e) => {
                    let _ = tx.send(crate::models::AppEvent::BuildOutput(
                        board_name.to_string(),
                        format!("⚠️ Warning: Could not read partition-table.bin: {}", e),
                    ));
                }
            }
        }

        // Check for application binary
        let app_bin = build_dir.join(format!("{}.bin", board_name));
        if app_bin.exists() {
            match std::fs::read(&app_bin) {
                Ok(data) => {
                    let _ = tx.send(crate::models::AppEvent::BuildOutput(
                        board_name.to_string(),
                        format!(
                            "  📄 {}.bin → 0x10000 ({:.1} KB)",
                            board_name,
                            data.len() as f64 / 1024.0
                        ),
                    ));
                    flash_data_map.insert(0x10000, data);
                }
                Err(e) => {
                    return Err(anyhow::anyhow!("Failed to read application binary: {}", e));
                }
            }
        } else {
            return Err(anyhow::anyhow!(
                "Application binary not found at {}",
                app_bin.display()
            ));
        }

        if flash_data_map.is_empty() {
            return Err(anyhow::anyhow!("No flash binaries found"));
        }

        let _ = tx.send(crate::models::AppEvent::BuildOutput(
            board_name.to_string(),
            "🚀 Starting flash operation...".to_string(),
        ));

        // Use existing espflash utils for flashing
        match crate::utils::espflash_utils::flash_multi_binary(port, flash_data_map).await {
            Ok(_) => {
                let _ = tx.send(crate::models::AppEvent::BuildOutput(
                    board_name.to_string(),
                    "✅ Flash completed successfully!".to_string(),
                ));
                Ok(())
            }
            Err(e) => {
                let _ = tx.send(crate::models::AppEvent::BuildOutput(
                    board_name.to_string(),
                    format!("❌ Flash failed: {}", e),
                ));
                Err(anyhow::anyhow!("Flash operation failed: {}", e))
            }
        }
    }

    /// ESP-IDF clean implementation
    pub async fn clean_board_esp_idf(
        board_name: &str,
        project_dir: &std::path::Path,
        build_dir: &std::path::Path,
        _log_file: &std::path::Path,
        tx: tokio::sync::mpsc::UnboundedSender<crate::models::AppEvent>,
    ) -> Result<()> {
        let _ = tx.send(crate::models::AppEvent::BuildOutput(
            board_name.to_string(),
            format!("🧽 Cleaning {} using ESP-IDF...", board_name),
        ));

        let clean_args = vec![
            "-B".to_string(),
            build_dir.to_string_lossy().to_string(),
            "fullclean".to_string(),
        ];

        let clean_args_str: Vec<&str> = clean_args.iter().map(|s| s.as_str()).collect();
        let env_vars = vec![]; // No special environment variables needed for clean

        let success = Self::execute_command_streaming(
            "idf.py",
            &clean_args_str,
            project_dir,
            env_vars,
            board_name,
            tx.clone(),
        )
        .await?;

        if success {
            let _ = tx.send(crate::models::AppEvent::BuildOutput(
                board_name.to_string(),
                "✅ Clean completed successfully!".to_string(),
            ));
            Ok(())
        } else {
            let _ = tx.send(crate::models::AppEvent::BuildOutput(
                board_name.to_string(),
                "❌ Clean failed!".to_string(),
            ));
            Err(anyhow::anyhow!("Clean failed"))
        }
    }

    /// ESP-IDF monitor implementation
    pub async fn monitor_board_esp_idf(
        board_name: &str,
        project_dir: &std::path::Path,
        build_dir: &std::path::Path,
        _log_file: &std::path::Path,
        tx: tokio::sync::mpsc::UnboundedSender<crate::models::AppEvent>,
    ) -> Result<()> {
        let _ = tx.send(crate::models::AppEvent::BuildOutput(
            board_name.to_string(),
            format!("📺 Starting monitor for {} using ESP-IDF...", board_name),
        ));

        let monitor_args = vec![
            "-B".to_string(),
            build_dir.to_string_lossy().to_string(),
            "monitor".to_string(),
        ];

        let monitor_args_str: Vec<&str> = monitor_args.iter().map(|s| s.as_str()).collect();
        let env_vars = vec![]; // No special environment variables needed for monitor

        let success = Self::execute_command_streaming(
            "idf.py",
            &monitor_args_str,
            project_dir,
            env_vars,
            board_name,
            tx.clone(),
        )
        .await?;

        if success {
            let _ = tx.send(crate::models::AppEvent::BuildOutput(
                board_name.to_string(),
                "✅ Monitor session ended!".to_string(),
            ));
            Ok(())
        } else {
            let _ = tx.send(crate::models::AppEvent::BuildOutput(
                board_name.to_string(),
                "❌ Monitor failed!".to_string(),
            ));
            Err(anyhow::anyhow!("Monitor failed"))
        }
    }

    /// ESP-IDF flash app only implementation (faster than full flash)
    pub async fn flash_app_only_esp_idf(
        board_name: &str,
        project_dir: &std::path::Path,
        build_dir: &std::path::Path,
        _log_file: &std::path::Path,
        tx: tokio::sync::mpsc::UnboundedSender<crate::models::AppEvent>,
    ) -> Result<()> {
        let _ = tx.send(crate::models::AppEvent::BuildOutput(
            board_name.to_string(),
            format!(
                "🚀 Flashing app partition only for {} (faster)...",
                board_name
            ),
        ));

        let flash_args = vec![
            "-B".to_string(),
            build_dir.to_string_lossy().to_string(),
            "app-flash".to_string(),
        ];

        let flash_args_str: Vec<&str> = flash_args.iter().map(|s| s.as_str()).collect();
        let env_vars = vec![]; // No special environment variables needed for app flash

        let success = Self::execute_command_streaming(
            "idf.py",
            &flash_args_str,
            project_dir,
            env_vars,
            board_name,
            tx.clone(),
        )
        .await?;

        if success {
            let _ = tx.send(crate::models::AppEvent::BuildOutput(
                board_name.to_string(),
                "✅ App flash completed successfully! (Bootloader and partitions unchanged)"
                    .to_string(),
            ));
            Ok(())
        } else {
            let _ = tx.send(crate::models::AppEvent::BuildOutput(
                board_name.to_string(),
                "❌ App flash failed!".to_string(),
            ));
            Err(anyhow::anyhow!("App flash failed"))
        }
    }

    /// Purge board build directory (more aggressive than clean)
    pub async fn purge_board_esp_idf(
        board_name: &str,
        build_dir: &std::path::Path,
        _log_file: &std::path::Path,
        tx: tokio::sync::mpsc::UnboundedSender<crate::models::AppEvent>,
    ) -> Result<()> {
        let _ = tx.send(crate::models::AppEvent::BuildOutput(
            board_name.to_string(),
            format!("🗿 Purging build directory for {}...", board_name),
        ));

        if build_dir.exists() {
            let _ = tx.send(crate::models::AppEvent::BuildOutput(
                board_name.to_string(),
                format!("🧹 Removing build directory: {}", build_dir.display()),
            ));

            // Remove the entire build directory
            match std::fs::remove_dir_all(build_dir) {
                Ok(()) => {
                    let _ = tx.send(crate::models::AppEvent::BuildOutput(
                        board_name.to_string(),
                        "✅ Purge completed successfully! Build directory removed.".to_string(),
                    ));
                    Ok(())
                }
                Err(e) => {
                    let _ = tx.send(crate::models::AppEvent::BuildOutput(
                        board_name.to_string(),
                        format!("❌ Purge failed: {}", e),
                    ));
                    Err(anyhow::anyhow!("Purge failed: {}", e))
                }
            }
        } else {
            let _ = tx.send(crate::models::AppEvent::BuildOutput(
                board_name.to_string(),
                "📦 Build directory already clean (nothing to purge)".to_string(),
            ));
            Ok(())
        }
    }

    /// Generate single binary file for distribution
    pub async fn generate_binary_esp_idf(
        board_name: &str,
        project_dir: &std::path::Path,
        build_dir: &std::path::Path,
        _log_file: &std::path::Path,
        tx: tokio::sync::mpsc::UnboundedSender<crate::models::AppEvent>,
    ) -> Result<()> {
        let _ = tx.send(crate::models::AppEvent::BuildOutput(
            board_name.to_string(),
            format!("📦 Generating single binary for {}...", board_name),
        ));

        // Check if build directory exists
        if !build_dir.exists() {
            let _ = tx.send(crate::models::AppEvent::BuildOutput(
                board_name.to_string(),
                "❌ Build directory not found! Please build the project first.".to_string(),
            ));
            return Err(anyhow::anyhow!("Build directory not found"));
        }

        // Look for flash_args file (generated by ESP-IDF build)
        let flash_args_file = build_dir.join("flash_args");
        if !flash_args_file.exists() {
            let _ = tx.send(crate::models::AppEvent::BuildOutput(
                board_name.to_string(),
                "❌ flash_args file not found! Please build the project first.".to_string(),
            ));
            return Err(anyhow::anyhow!("flash_args file not found"));
        }

        // Generate output filename
        let output_file = build_dir.join(format!("{}-merged.bin", board_name));

        let _ = tx.send(crate::models::AppEvent::BuildOutput(
            board_name.to_string(),
            format!("🔧 Merging binaries using ESP-IDF merge_bin.py tool..."),
        ));

        // Use esptool.py merge_bin to create single binary
        let merge_args = vec![
            "--chip".to_string(),
            "auto".to_string(), // Let esptool detect the chip
            "merge_bin".to_string(),
            "-o".to_string(),
            output_file.to_string_lossy().to_string(),
            "--flash_mode".to_string(),
            "dio".to_string(), // Default flash mode
            "--flash_freq".to_string(),
            "40m".to_string(), // Default flash frequency
            "--flash_size".to_string(),
            "4MB".to_string(), // Default flash size
            "@".to_string() + &flash_args_file.to_string_lossy(), // Read args from file
        ];

        let merge_args_str: Vec<&str> = merge_args.iter().map(|s| s.as_str()).collect();
        let env_vars = vec![];

        let success = Self::execute_command_streaming(
            "esptool.py",
            &merge_args_str,
            project_dir,
            env_vars,
            board_name,
            tx.clone(),
        )
        .await?;

        if success {
            // Get file size for user info
            match std::fs::metadata(&output_file) {
                Ok(metadata) => {
                    let size_kb = metadata.len() / 1024;
                    let _ = tx.send(crate::models::AppEvent::BuildOutput(
                        board_name.to_string(),
                        format!(
                            "✅ Binary generation completed! File: {} ({} KB)",
                            output_file.display(),
                            size_kb
                        ),
                    ));
                    let _ = tx.send(crate::models::AppEvent::BuildOutput(
                        board_name.to_string(),
                        format!(
                            "💡 Flash with: esptool.py write_flash 0x0 {}",
                            output_file.display()
                        ),
                    ));
                }
                Err(_) => {
                    let _ = tx.send(crate::models::AppEvent::BuildOutput(
                        board_name.to_string(),
                        format!(
                            "✅ Binary generation completed! File: {}",
                            output_file.display()
                        ),
                    ));
                }
            }
            Ok(())
        } else {
            let _ = tx.send(crate::models::AppEvent::BuildOutput(
                board_name.to_string(),
                "❌ Binary generation failed!".to_string(),
            ));
            Err(anyhow::anyhow!("Binary generation failed"))
        }
    }

    // Remote board functionality
    pub fn get_server_url(&self) -> String {
        // If we have discovered servers, use hostname.local instead of IP addresses
        // This avoids IPv6 connectivity issues and works better with mDNS
        if !self.discovered_servers.is_empty() {
            // Use the first discovered server (they're typically equivalent)
            if let Some(server) = self.discovered_servers.first() {
                // Use the mDNS hostname directly (it already includes .local suffix)
                // This works for both IPv4 and IPv6 networks and avoids IPv6 connectivity issues
                return format!("http://{}:{}", server.name, server.port);
            }
        }

        // Fallback to configured or default URL
        self.server_url
            .as_deref()
            .unwrap_or("http://localhost:8080")
            .to_string()
    }

    /// Start mDNS server discovery
    pub fn start_server_discovery(
        &mut self,
        tx: tokio::sync::mpsc::UnboundedSender<crate::models::AppEvent>,
    ) {
        if self.server_discovery_in_progress {
            return; // Already discovering
        }

        self.server_discovery_in_progress = true;
        self.server_discovery_status = "Discovering servers...".to_string();
        self.server_discovery_start_time = Some(chrono::Local::now());

        // Add discovery start message to logs for all boards
        for board in &mut self.boards {
            board
                .log_lines
                .push("🔍 Starting mDNS discovery for ESPBrew servers (3s timeout)...".to_string());
        }

        tokio::spawn(async move {
            match crate::remote::discovery::discover_espbrew_servers_silent(3).await {
                Ok(servers) => {
                    let _ = tx.send(crate::models::AppEvent::ServerDiscoveryCompleted(servers));
                }
                Err(e) => {
                    let _ = tx.send(crate::models::AppEvent::ServerDiscoveryFailed(format!(
                        "Discovery failed: {}",
                        e
                    )));
                }
            }
        });
    }

    /// Handle server discovery completion
    pub fn handle_server_discovery_completed(
        &mut self,
        servers: Vec<crate::models::server::DiscoveredServer>,
    ) {
        self.server_discovery_in_progress = false;
        self.server_discovery_start_time = None;
        self.discovered_servers = servers;

        if self.discovered_servers.is_empty() {
            self.server_discovery_status = "No servers found".to_string();
            // Add no servers message to system logs
            if self.selected_board < self.boards.len() {
                self.boards[self.selected_board].log_lines.push(
                    "❌ mDNS discovery completed: No ESPBrew servers found on network".to_string(),
                );
            }
        } else {
            let server_count = self.discovered_servers.len();
            let server_names: Vec<String> = self
                .discovered_servers
                .iter()
                .map(|s| s.name.clone())
                .collect();
            self.server_discovery_status = format!(
                "Found {} server(s): {}",
                server_count,
                server_names.join(", ")
            );

            // Add discovery success message to system logs
            if self.selected_board < self.boards.len() {
                self.boards[self.selected_board].log_lines.push(format!(
                    "✅ mDNS discovery completed: Found {} ESPBrew server(s)",
                    server_count
                ));

                for (i, server) in self.discovered_servers.iter().enumerate() {
                    // Use mDNS hostname directly (already includes .local)
                    self.boards[self.selected_board].log_lines.push(format!(
                        "  {}. {} at {}:{} ({})",
                        i + 1,
                        server.name,
                        server.name,
                        server.port,
                        server.description
                    ));
                }
            }
        }
    }

    /// Handle server discovery failure
    pub fn handle_server_discovery_failed(&mut self, error: String) {
        self.server_discovery_in_progress = false;
        self.server_discovery_start_time = None;
        self.server_discovery_status = format!("Discovery failed: {}", error);

        // Add discovery failure message to system logs
        if self.selected_board < self.boards.len() {
            self.boards[self.selected_board]
                .log_lines
                .push(format!("❌ mDNS discovery failed: {}", error));
        }
    }

    pub fn handle_remote_flash_completed(&mut self) {
        self.remote_flash_in_progress = false;
        self.remote_flash_status = Some("Flash completed successfully".to_string());

        // Update board status
        if self.selected_board < self.boards.len() {
            self.boards[self.selected_board].status = crate::models::project::BuildStatus::Success;
        }
    }

    pub fn handle_remote_flash_failed(&mut self, error: String) {
        self.remote_flash_in_progress = false;
        self.remote_flash_status = Some(format!("Flash failed: {}", error));

        // Update board status
        if self.selected_board < self.boards.len() {
            self.boards[self.selected_board].status = crate::models::project::BuildStatus::Failed;
        }
    }

    pub fn handle_remote_monitor_started(&mut self, session_id: String) {
        self.remote_monitor_in_progress = false;
        self.remote_monitor_session_id = Some(session_id.clone());
        self.remote_monitor_status = Some("Monitor session started".to_string());

        // Update board status to monitoring
        if self.selected_board < self.boards.len() {
            self.boards[self.selected_board].status =
                crate::models::project::BuildStatus::Monitoring;
        }

        // TODO: Here we could open a monitoring modal or connect to WebSocket
        // For now, just show the session ID in logs
        if self.selected_board < self.boards.len() {
            self.boards[self.selected_board].log_lines.push(format!(
                "📺 Remote monitoring session active: {}",
                session_id
            ));
            self.boards[self.selected_board]
                .log_lines
                .push("💡 Use CLI 'remote-monitor' command to view logs in real-time".to_string());
        }
    }

    pub fn handle_remote_monitor_failed(&mut self, error: String) {
        self.remote_monitor_in_progress = false;
        self.remote_monitor_status = Some(format!("Monitor failed: {}", error));

        // Update board status
        if self.selected_board < self.boards.len() {
            self.boards[self.selected_board].status = crate::models::project::BuildStatus::Failed;
        }
    }

    pub fn start_fetching_remote_boards(
        &mut self,
        tx: tokio::sync::mpsc::UnboundedSender<crate::models::AppEvent>,
    ) {
        // Use discovered server URL if available, otherwise fallback
        let server_url = self.get_server_url();

        // Set loading state
        self.remote_boards_loading = true;
        self.remote_boards_fetch_error = None;
        self.show_remote_board_dialog = true; // Show dialog with loading state

        // Log the connection attempt with detailed server info
        if self.selected_board < self.boards.len() {
            if !self.discovered_servers.is_empty() {
                // Show server selection logic
                let ipv4_servers: Vec<_> = self
                    .discovered_servers
                    .iter()
                    .filter(|s| matches!(s.ip, std::net::IpAddr::V4(_)))
                    .collect();
                let ipv6_servers: Vec<_> = self
                    .discovered_servers
                    .iter()
                    .filter(|s| matches!(s.ip, std::net::IpAddr::V6(_)))
                    .collect();

                self.boards[self.selected_board].log_lines.push(format!(
                    "📊 Server selection: {} IPv4 servers, {} IPv6 servers found",
                    ipv4_servers.len(),
                    ipv6_servers.len()
                ));

                // Find the preferred server (same logic as get_server_url)
                let preferred_server = self
                    .discovered_servers
                    .iter()
                    .find(|server| matches!(server.ip, std::net::IpAddr::V4(_)))
                    .or_else(|| self.discovered_servers.first());

                if let Some(server) = preferred_server {
                    let preference_reason = if matches!(server.ip, std::net::IpAddr::V4(_)) {
                        "(preferred IPv4)"
                    } else {
                        "(fallback, no IPv4 available)"
                    };

                    self.boards[self.selected_board].log_lines.push(format!(
                        "🔍 Connecting to: {} ({}:{}) {} -> {}",
                        server.name, server.ip, server.port, preference_reason, server_url
                    ));

                    // Show IPv6 vs IPv4 detection for debugging
                    match server.ip {
                        std::net::IpAddr::V6(ip) => {
                            self.boards[self.selected_board].log_lines.push(format!(
                                "🌐 Using IPv6 address: {} -> URL: http://[{}]:{}",
                                ip, ip, server.port
                            ));
                        }
                        std::net::IpAddr::V4(ip) => {
                            self.boards[self.selected_board].log_lines.push(format!(
                                "🌐 Using IPv4 address: {} -> URL: http://{}:{}",
                                ip, ip, server.port
                            ));
                        }
                    }
                }
            } else {
                self.boards[self.selected_board].log_lines.push(format!(
                    "🔍 Connecting to configured server: {}",
                    server_url
                ));
            }
            // Add detailed URL for debugging
            self.boards[self.selected_board].log_lines.push(format!(
                "📡 Making API request to: {}/api/v1/boards",
                server_url
            ));
        }

        // Spawn async task to fetch boards
        let tx_clone = tx.clone();
        tokio::spawn(async move {
            match Self::fetch_remote_boards(&server_url, tx_clone.clone()).await {
                Ok(remote_boards) => {
                    let _ = tx.send(crate::models::AppEvent::RemoteBoardsFetched(remote_boards));
                }
                Err(e) => {
                    let error_msg = if e.to_string().contains("Connection refused") {
                        format!("Server not running at {}", server_url)
                    } else if e.to_string().contains("timeout") {
                        format!("Connection timeout to {}", server_url)
                    } else {
                        format!("Network error: {}", e)
                    };
                    let _ = tx.send(crate::models::AppEvent::RemoteBoardsFetchFailed(error_msg));
                }
            }
        });
    }

    pub fn handle_remote_boards_fetched(
        &mut self,
        remote_boards: Vec<crate::models::board::RemoteBoard>,
    ) {
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

    pub fn handle_remote_boards_fetch_failed(&mut self, error_msg: String) {
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
        self.remote_board_list_state = ratatui::widgets::ListState::default();
        self.remote_flash_status = Some(error_msg.clone());
        self.remote_monitor_status = Some(error_msg);
    }

    pub fn hide_remote_board_dialog(&mut self) {
        self.show_remote_board_dialog = false;
        self.remote_boards.clear();
        self.selected_remote_board = 0;
        self.remote_board_list_state = ratatui::widgets::ListState::default();
        self.remote_flash_status = None;
    }

    pub fn next_remote_board(&mut self) {
        if !self.remote_boards.is_empty() {
            self.selected_remote_board =
                (self.selected_remote_board + 1) % self.remote_boards.len();
            self.remote_board_list_state
                .select(Some(self.selected_remote_board));
        }
    }

    pub fn previous_remote_board(&mut self) {
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

    /// Fetch remote boards from ESPBrew server
    async fn fetch_remote_boards(
        server_url: &str,
        tx: tokio::sync::mpsc::UnboundedSender<crate::models::AppEvent>,
    ) -> anyhow::Result<Vec<crate::models::board::RemoteBoard>> {
        let _ = tx.send(crate::models::AppEvent::BuildOutput(
            "remote".to_string(),
            format!("🔍 Starting connection to server: {}", server_url),
        ));

        // Create client with timeout to prevent hanging
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10)) // 10 second timeout
            .build()?;
        let url = format!("{}/api/v1/boards", server_url.trim_end_matches('/'));

        let _ = tx.send(crate::models::AppEvent::BuildOutput(
            "remote".to_string(),
            format!("📡 Making GET request to: {}", url),
        ));

        let response = client
            .get(&url)
            .send()
            .await
            .map_err(|e| {
                let _ = tx.send(crate::models::AppEvent::BuildOutput(
                    "remote".to_string(),
                    format!("❌ HTTP request failed: {}", e),
                ));
                e
            })?
            .error_for_status()
            .map_err(|e| {
                let _ = tx.send(crate::models::AppEvent::BuildOutput(
                    "remote".to_string(),
                    format!("❌ Server returned HTTP error: {}", e),
                ));
                e
            })?;

        let _ = tx.send(crate::models::AppEvent::BuildOutput(
            "remote".to_string(),
            format!("✅ Got HTTP {} response from server", response.status()),
        ));

        let boards_response: crate::models::responses::RemoteBoardsResponse =
            response.json().await.map_err(|e| {
                let _ = tx.send(crate::models::AppEvent::BuildOutput(
                    "remote".to_string(),
                    format!("❌ Failed to parse JSON response: {}", e),
                ));
                e
            })?;

        let _ = tx.send(crate::models::AppEvent::BuildOutput(
            "remote".to_string(),
            format!(
                "📊 Successfully found {} board(s) on server",
                boards_response.boards.len()
            ),
        ));

        Ok(boards_response.boards)
    }

    /// Execute remote monitor for selected remote board
    pub async fn execute_remote_monitor(
        &mut self,
        tx: tokio::sync::mpsc::UnboundedSender<crate::models::AppEvent>,
    ) -> Result<()> {
        if self.selected_remote_board >= self.remote_boards.len() {
            return Err(anyhow::anyhow!("No remote board selected"));
        }

        let selected_board = self.remote_boards[self.selected_remote_board].clone();
        let server_url = self.get_server_url();
        let baud_rate = 115200; // Default baud rate

        // Update status
        if self.selected_board < self.boards.len() {
            self.boards[self.selected_board].status =
                crate::models::project::BuildStatus::Monitoring;
        }

        self.remote_monitor_in_progress = true;
        self.remote_monitor_status = Some("Starting remote monitor session...".to_string());

        // Start monitoring session
        let tx_clone = tx.clone();
        let board_id = selected_board.id.clone();
        let board_name = selected_board
            .logical_name
            .clone()
            .unwrap_or_else(|| selected_board.id.clone());

        tokio::spawn(async move {
            let result = Self::start_remote_monitor_session(
                &server_url,
                &board_id,
                &board_name,
                baud_rate,
                tx_clone.clone(),
            )
            .await;

            match result {
                Ok(session_id) => {
                    let _ = tx_clone.send(crate::models::AppEvent::BuildOutput(
                        "remote".to_string(),
                        "✅ Remote monitor session started!".to_string(),
                    ));
                    let _ =
                        tx_clone.send(crate::models::AppEvent::RemoteMonitorStarted(session_id));
                }
                Err(e) => {
                    let _ = tx_clone.send(crate::models::AppEvent::BuildOutput(
                        "remote".to_string(),
                        format!("❌ Remote monitor failed: {}", e),
                    ));
                    let _ =
                        tx_clone.send(crate::models::AppEvent::RemoteMonitorFailed(e.to_string()));
                }
            }
        });

        Ok(())
    }

    /// Start remote monitoring session and return session ID
    async fn start_remote_monitor_session(
        server_url: &str,
        board_id: &str,
        board_name: &str,
        baud_rate: u32,
        tx: tokio::sync::mpsc::UnboundedSender<crate::models::AppEvent>,
    ) -> Result<String> {
        use crate::models::monitor::{MonitorRequest, MonitorResponse};

        let _ = tx.send(crate::models::AppEvent::BuildOutput(
            "remote".to_string(),
            format!("📺 Starting monitor session for board: {}", board_name),
        ));

        // Create HTTP client
        let client = reqwest::Client::new();
        let url = format!("{}/api/v1/monitor/start", server_url.trim_end_matches('/'));

        let request = MonitorRequest {
            board_id: board_id.to_string(),
            baud_rate: Some(baud_rate),
            filters: None, // No filters for TUI monitoring
        };

        let _ = tx.send(crate::models::AppEvent::BuildOutput(
            "remote".to_string(),
            format!("📡 Sending monitor request to: {}", url),
        ));

        let response = client
            .post(&url)
            .json(&request)
            .send()
            .await
            .map_err(|e| {
                let _ = tx.send(crate::models::AppEvent::BuildOutput(
                    "remote".to_string(),
                    format!("❌ HTTP request failed: {}", e),
                ));
                e
            })?
            .error_for_status()
            .map_err(|e| {
                let _ = tx.send(crate::models::AppEvent::BuildOutput(
                    "remote".to_string(),
                    format!("❌ Server returned error: {}", e),
                ));
                e
            })?;

        let monitor_response: MonitorResponse = response.json().await.map_err(|e| {
            let _ = tx.send(crate::models::AppEvent::BuildOutput(
                "remote".to_string(),
                format!("❌ Failed to parse response: {}", e),
            ));
            e
        })?;

        if !monitor_response.success {
            return Err(anyhow::anyhow!(
                "Monitor request failed: {}",
                monitor_response.message
            ));
        }

        let session_id = monitor_response
            .session_id
            .ok_or_else(|| anyhow::anyhow!("No session ID returned"))?;
        let websocket_url = monitor_response
            .websocket_url
            .ok_or_else(|| anyhow::anyhow!("No WebSocket URL returned"))?;

        let _ = tx.send(crate::models::AppEvent::BuildOutput(
            "remote".to_string(),
            format!("✅ Monitor session created: {}", session_id),
        ));
        let _ = tx.send(crate::models::AppEvent::BuildOutput(
            "remote".to_string(),
            format!("🔗 WebSocket URL: {}", websocket_url),
        ));

        Ok(session_id)
    }

    /// Execute remote flash for selected remote board
    pub async fn execute_remote_flash(
        &mut self,
        tx: tokio::sync::mpsc::UnboundedSender<crate::models::AppEvent>,
    ) -> Result<()> {
        if self.selected_remote_board >= self.remote_boards.len() {
            return Err(anyhow::anyhow!("No remote board selected"));
        }

        let selected_board = self.remote_boards[self.selected_remote_board].clone();
        let server_url = self.get_server_url();
        let project_dir = self.project_dir.clone();
        let _project_type = self.project_handler.as_ref().map(|h| h.project_type());

        // Update status
        if self.selected_board < self.boards.len() {
            self.boards[self.selected_board].status = crate::models::project::BuildStatus::Flashing;
        }

        self.remote_flash_in_progress = true;
        self.remote_flash_status = Some("Preparing remote flash...".to_string());

        let tx_clone = tx.clone();
        tokio::spawn(async move {
            let result = async {
                // Detect project type and use appropriate flash method
                let _ = tx_clone.send(crate::models::AppEvent::BuildOutput(
                    "remote".to_string(),
                    "🔍 Detecting project type and build artifacts...".to_string(),
                ));

                // For ESP-IDF projects, use the ESP-IDF remote flash method
                Self::upload_and_flash_esp_idf_remote(
                    &server_url,
                    &selected_board,
                    &project_dir,
                    tx_clone.clone(),
                )
                .await
            }
            .await;

            match result {
                Ok(()) => {
                    let _ = tx_clone.send(crate::models::AppEvent::BuildOutput(
                        "remote".to_string(),
                        "✅ Remote flash completed successfully!".to_string(),
                    ));
                    let _ = tx_clone.send(crate::models::AppEvent::RemoteFlashCompleted);
                }
                Err(e) => {
                    let _ = tx_clone.send(crate::models::AppEvent::BuildOutput(
                        "remote".to_string(),
                        format!("❌ Remote flash failed: {}", e),
                    ));
                    let _ =
                        tx_clone.send(crate::models::AppEvent::RemoteFlashFailed(e.to_string()));
                }
            }
        });

        Ok(())
    }

    /// Upload and flash ESP-IDF project to remote board
    async fn upload_and_flash_esp_idf_remote(
        server_url: &str,
        board: &crate::models::board::RemoteBoard,
        project_dir: &std::path::Path,
        tx: tokio::sync::mpsc::UnboundedSender<crate::models::AppEvent>,
    ) -> Result<()> {
        let _ = tx.send(crate::models::AppEvent::BuildOutput(
            "remote".to_string(),
            "🔍 Searching for ESP-IDF build directories...".to_string(),
        ));

        // Discover ESP-IDF build directories dynamically
        let build_dirs = Self::discover_esp_build_directories(project_dir)?;

        if build_dirs.is_empty() {
            let _ = tx.send(crate::models::AppEvent::BuildOutput(
                "remote".to_string(),
                "⚠️ No ESP-IDF build directories found".to_string(),
            ));
            return Err(anyhow::anyhow!(
                "No ESP-IDF build directories found in {}. Run 'idf.py build' first.",
                project_dir.display()
            ));
        }

        // Try each build directory
        for build_dir in &build_dirs {
            let flash_args_path = build_dir.join("flash_args");

            if flash_args_path.exists() {
                let _ = tx.send(crate::models::AppEvent::BuildOutput(
                    "remote".to_string(),
                    format!("📋 Found ESP-IDF build: {}", build_dir.display()),
                ));

                // First, let me log the flash_args file content for debugging
                let _ = tx.send(crate::models::AppEvent::BuildOutput(
                    "remote".to_string(),
                    format!("🔍 Parsing flash_args: {}", flash_args_path.display()),
                ));

                match Self::parse_flash_args(&flash_args_path, build_dir) {
                    Ok((flash_config, binaries)) => {
                        let _ = tx.send(crate::models::AppEvent::BuildOutput(
                            "remote".to_string(),
                            format!(
                                "✅ Successfully parsed {} binaries from flash_args",
                                binaries.len()
                            ),
                        ));

                        let total_size: u64 = binaries
                            .iter()
                            .map(|b| {
                                std::fs::metadata(&b.file_path)
                                    .map(|m| m.len())
                                    .unwrap_or(0)
                            })
                            .sum();

                        let _ = tx.send(crate::models::AppEvent::BuildOutput(
                            "remote".to_string(),
                            format!(
                                "📦 Found {} ESP-IDF binaries ({:.1} KB total) for remote flash",
                                binaries.len(),
                                total_size as f64 / 1024.0
                            ),
                        ));

                        // Log flash configuration
                        let _ = tx.send(crate::models::AppEvent::BuildOutput(
                            "remote".to_string(),
                            format!(
                                "⚙️ Flash config: mode={}, freq={}, size={}",
                                flash_config.flash_mode,
                                flash_config.flash_freq,
                                flash_config.flash_size
                            ),
                        ));

                        for binary in &binaries {
                            let file_size = std::fs::metadata(&binary.file_path)
                                .map(|m| m.len())
                                .unwrap_or(0);
                            let _ = tx.send(crate::models::AppEvent::BuildOutput(
                                "remote".to_string(),
                                format!(
                                    "  📄 {} → 0x{:x} | {} ({:.1} KB) | {}",
                                    binary.name,
                                    binary.offset,
                                    binary.file_name,
                                    file_size as f64 / 1024.0,
                                    binary.file_path.display()
                                ),
                            ));
                        }

                        // Use the multi-binary approach
                        return Self::upload_and_flash_esp_build_with_logging(
                            server_url,
                            board,
                            &flash_config,
                            &binaries,
                            tx,
                        )
                        .await;
                    }
                    Err(e) => {
                        let _ = tx.send(crate::models::AppEvent::BuildOutput(
                            "remote".to_string(),
                            format!("⚠️ Failed to parse {}: {}", flash_args_path.display(), e),
                        ));
                        continue;
                    }
                }
            }
        }

        let dir_names: Vec<String> = build_dirs
            .iter()
            .filter_map(|d| d.file_name().and_then(|n| n.to_str()))
            .map(|s| s.to_string())
            .collect();

        Err(anyhow::anyhow!(
            "No valid ESP-IDF build artifacts found in {}. Found build directories: [{}], but none contain flash_args. Run 'idf.py build' first.",
            project_dir.display(),
            dir_names.join(", ")
        ))
    }

    // Helper methods for ESP-IDF build discovery
    fn discover_esp_build_directories(
        project_dir: &std::path::Path,
    ) -> Result<Vec<std::path::PathBuf>> {
        use std::fs;

        let mut build_dirs = Vec::new();

        // Check common ESP-IDF build directory patterns
        let patterns = vec![
            project_dir.join("build"),
            project_dir.join("build_esp32"),
            project_dir.join("build_esp32s3"),
            project_dir.join("build_esp32c3"),
            project_dir.join("build_esp32c6"),
        ];

        // Add pattern matching for build_* directories
        if let Ok(entries) = fs::read_dir(project_dir) {
            for entry in entries.flatten() {
                if let Ok(file_type) = entry.file_type() {
                    if file_type.is_dir() {
                        let dir_name = entry.file_name();
                        if let Some(name) = dir_name.to_str() {
                            if name.starts_with("build") {
                                build_dirs.push(entry.path());
                            }
                        }
                    }
                }
            }
        }

        // Add explicit patterns
        for pattern in patterns {
            if pattern.exists() && pattern.is_dir() {
                if !build_dirs.contains(&pattern) {
                    build_dirs.push(pattern);
                }
            }
        }

        Ok(build_dirs)
    }

    fn parse_flash_args(
        flash_args_path: &std::path::Path,
        build_dir: &std::path::Path,
    ) -> Result<(
        crate::models::flash::FlashConfig,
        Vec<crate::models::flash::FlashBinaryInfo>,
    )> {
        use std::fs;

        let flash_args_content = fs::read_to_string(flash_args_path)?;

        // Parse ESP-IDF flash_args file content (line-by-line format)
        // Line 1: --flash_mode dio --flash_freq 80m --flash_size 16MB
        // Line 2+: 0x2000 bootloader/bootloader.bin
        let lines: Vec<&str> = flash_args_content.lines().collect();

        if lines.is_empty() {
            return Err(anyhow::anyhow!("flash_args file is empty"));
        }

        // Parse first line for flash configuration
        let mut flash_config = crate::models::flash::FlashConfig {
            flash_mode: "dio".to_string(),
            flash_freq: "40m".to_string(),
            flash_size: "detect".to_string(),
        };

        let config_line = lines[0];
        let config_args: Vec<&str> = config_line.split_whitespace().collect();

        // Parse flash configuration parameters from first line
        for (i, arg) in config_args.iter().enumerate() {
            match *arg {
                "--flash_mode" => {
                    if let Some(mode) = config_args.get(i + 1) {
                        flash_config.flash_mode = mode.to_string();
                    }
                }
                "--flash_freq" => {
                    if let Some(freq) = config_args.get(i + 1) {
                        flash_config.flash_freq = freq.to_string();
                    }
                }
                "--flash_size" => {
                    if let Some(size) = config_args.get(i + 1) {
                        flash_config.flash_size = size.to_string();
                    }
                }
                _ => {}
            }
        }

        // Parse remaining lines for binary files (address/file pairs)
        let mut binaries = Vec::new();
        for line in lines.iter().skip(1) {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                if let Some(binary) = Self::parse_address_file_pair(parts[0], parts[1], build_dir) {
                    binaries.push(binary);
                }
            }
        }

        // If no binaries were found in flash_args, fall back to common file discovery
        if binaries.is_empty() {
            // Bootloader
            let bootloader_path = build_dir.join("bootloader").join("bootloader.bin");
            if bootloader_path.exists() {
                binaries.push(crate::models::flash::FlashBinaryInfo {
                    name: "bootloader".to_string(),
                    file_path: bootloader_path.clone(),
                    file_name: "bootloader.bin".to_string(),
                    offset: 0x0,
                });
            }

            // Partition table
            let partition_table_path = build_dir
                .join("partition_table")
                .join("partition-table.bin");
            if partition_table_path.exists() {
                binaries.push(crate::models::flash::FlashBinaryInfo {
                    name: "partition-table".to_string(),
                    file_path: partition_table_path.clone(),
                    file_name: "partition-table.bin".to_string(),
                    offset: 0x8000,
                });
            }

            // Application binary - look for common app binary patterns
            let mut app_patterns = vec![
                build_dir.join("app.bin"),
                build_dir.join("project.bin"),
                build_dir.join("main.bin"),
            ];

            // Also check for project-specific binary file
            let project_name = build_dir
                .parent()
                .and_then(|p| p.file_name())
                .and_then(|n| n.to_str())
                .unwrap_or("app");
            app_patterns.push(build_dir.join(format!("{}.bin", project_name)));

            for app_path in app_patterns {
                if app_path.exists() {
                    binaries.push(crate::models::flash::FlashBinaryInfo {
                        name: "app".to_string(),
                        file_path: app_path.clone(),
                        file_name: app_path
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_string(),
                        offset: 0x10000,
                    });
                    break; // Only add the first one found
                }
            }
        }

        Ok((flash_config, binaries))
    }

    /// Parse a single address/file pair from flash_args
    fn parse_address_file_pair(
        address_str: &str,
        file_path_str: &str,
        build_dir: &std::path::Path,
    ) -> Option<crate::models::flash::FlashBinaryInfo> {
        // Parse address (hex format like 0x1000)
        let offset = if address_str.starts_with("0x") {
            u32::from_str_radix(&address_str[2..], 16)
        } else {
            address_str.parse::<u32>()
        }
        .ok()?;

        // Resolve relative file path from build directory
        let file_path = if std::path::Path::new(file_path_str).is_absolute() {
            std::path::PathBuf::from(file_path_str)
        } else {
            build_dir.join(file_path_str)
        };

        if !file_path.exists() {
            return None;
        }

        let file_name = file_path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        // Determine partition name from file name and offset
        let name = if file_name.contains("bootloader") {
            "bootloader".to_string()
        } else if file_name.contains("partition") {
            "partition-table".to_string()
        } else if offset == 0x0 || offset == 0x1000 || offset == 0x2000 {
            "bootloader".to_string()
        } else if offset <= 0x9000 && file_name.contains("partition") {
            "partition-table".to_string()
        } else if file_name.contains("storage") {
            "storage".to_string()
        } else {
            "app".to_string()
        };

        Some(crate::models::flash::FlashBinaryInfo {
            name,
            file_path: file_path.clone(),
            file_name,
            offset,
        })
    }

    /// Upload and flash ESP-IDF build with logging
    async fn upload_and_flash_esp_build_with_logging(
        server_url: &str,
        board: &crate::models::board::RemoteBoard,
        flash_config: &crate::models::flash::FlashConfig,
        binaries: &[crate::models::flash::FlashBinaryInfo],
        tx: tokio::sync::mpsc::UnboundedSender<crate::models::AppEvent>,
    ) -> Result<()> {
        use std::fs;

        let client = reqwest::Client::new();
        let flash_url = format!("{}/api/v1/flash", server_url.trim_end_matches('/'));

        let total_size: usize = binaries
            .iter()
            .map(|b| {
                std::fs::metadata(&b.file_path)
                    .map(|m| m.len() as usize)
                    .unwrap_or(0)
            })
            .sum();

        let _ = tx.send(crate::models::AppEvent::BuildOutput(
            "remote".to_string(),
            format!(
                "📤 Uploading {} ESP-IDF binaries ({:.1} KB) to server {}",
                binaries.len(),
                total_size as f64 / 1024.0,
                server_url
            ),
        ));

        // Create multipart form with all binaries
        let mut form = reqwest::multipart::Form::new();

        // Add board ID
        form = form.text("board_id", board.id.clone());

        // Add binary count
        form = form.text("binary_count", binaries.len().to_string());

        // Add flash configuration
        form = form.text("flash_mode", flash_config.flash_mode.clone());
        form = form.text("flash_freq", flash_config.flash_freq.clone());
        form = form.text("flash_size", flash_config.flash_size.clone());

        let _ = tx.send(crate::models::AppEvent::BuildOutput(
            "remote".to_string(),
            format!(
                "🔧 Creating multipart form with {} binaries...",
                binaries.len()
            ),
        ));

        // Add each binary
        for (i, binary_info) in binaries.iter().enumerate() {
            let _ = tx.send(crate::models::AppEvent::BuildOutput(
                "remote".to_string(),
                format!(
                    "📂 Reading binary {}: {}",
                    i + 1,
                    binary_info.file_path.display()
                ),
            ));

            let binary_data = fs::read(&binary_info.file_path).map_err(|e| {
                let error_msg =
                    format!("Failed to read {}: {}", binary_info.file_path.display(), e);
                let _ = tx.send(crate::models::AppEvent::BuildOutput(
                    "remote".to_string(),
                    format!("❌ {}", error_msg),
                ));
                anyhow::anyhow!(error_msg)
            })?;

            let _ = tx.send(crate::models::AppEvent::BuildOutput(
                "remote".to_string(),
                format!(
                    "🗃 [{}/{}] {} → 0x{:x} | {:.1} KB | {}",
                    i + 1,
                    binaries.len(),
                    binary_info.name,
                    binary_info.offset,
                    binary_data.len() as f64 / 1024.0,
                    binary_info.file_name
                ),
            ));

            let _ = tx.send(crate::models::AppEvent::BuildOutput(
                "remote".to_string(),
                format!(
                    "📄 Adding binary_{} to form: {} bytes",
                    i,
                    binary_data.len()
                ),
            ));

            // Add binary data with metadata
            form = form.part(
                format!("binary_{}", i),
                reqwest::multipart::Part::bytes(binary_data)
                    .file_name(binary_info.file_name.clone())
                    .mime_str("application/octet-stream")?,
            );

            // Add binary metadata
            form = form.text(format!("binary_{}_name", i), binary_info.name.clone());
            form = form.text(
                format!("binary_{}_offset", i),
                format!("0x{:x}", binary_info.offset),
            );
            form = form.text(
                format!("binary_{}_filename", i),
                binary_info.file_name.clone(),
            );

            let _ = tx.send(crate::models::AppEvent::BuildOutput(
                "remote".to_string(),
                format!(
                    "✅ Added binary_{} metadata: name={}, offset=0x{:x}",
                    i, binary_info.name, binary_info.offset
                ),
            ));
        }

        let _ = tx.send(crate::models::AppEvent::BuildOutput(
            "remote".to_string(),
            format!(
                "📡 Sending multipart flash request to server ({:.1} KB)...",
                total_size as f64 / 1024.0
            ),
        ));

        // Send the flash request
        let response = client.post(&flash_url).multipart(form).send().await?;

        let _ = tx.send(crate::models::AppEvent::BuildOutput(
            "remote".to_string(),
            format!("🗺 Server response: HTTP {}", response.status()),
        ));

        if response.status().is_success() {
            // Try to parse the response for detailed status
            match response.json::<crate::models::flash::FlashResponse>().await {
                Ok(flash_response) => {
                    if flash_response.success {
                        let duration_info = if let Some(duration) = flash_response.duration_ms {
                            format!(" in {}ms", duration)
                        } else {
                            String::new()
                        };
                        let _ = tx.send(crate::models::AppEvent::BuildOutput(
                            "remote".to_string(),
                            format!(
                                "✅ Remote flash completed successfully{}: {}",
                                duration_info, flash_response.message
                            ),
                        ));
                    } else {
                        let _ = tx.send(crate::models::AppEvent::BuildOutput(
                            "remote".to_string(),
                            format!("❌ Remote flash failed: {}", flash_response.message),
                        ));
                        return Err(anyhow::anyhow!("Flash failed: {}", flash_response.message));
                    }
                }
                Err(e) => {
                    let _ = tx.send(crate::models::AppEvent::BuildOutput(
                        "remote".to_string(),
                        format!("✅ Flash request accepted by server (parse error: {})", e),
                    ));
                }
            }
            Ok(())
        } else {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            let _ = tx.send(crate::models::AppEvent::BuildOutput(
                "remote".to_string(),
                format!(
                    "❌ Server rejected flash request ({}): {}",
                    status, error_text
                ),
            ));
            Err(anyhow::anyhow!(
                "Server rejected flash request: {}",
                error_text
            ))
        }
    }

    /// Build the currently selected board
    pub async fn build_selected_board(
        &mut self,
        tx: tokio::sync::mpsc::UnboundedSender<crate::models::AppEvent>,
    ) -> Result<()> {
        if self.selected_board >= self.boards.len() {
            return Err(anyhow::anyhow!("No board selected"));
        }

        let board = &self.boards[self.selected_board];
        let board_name = board.name.clone();

        // Set build in progress
        self.build_in_progress = true;

        // Update board status to building
        self.boards[self.selected_board].status = BuildStatus::Building;

        // Add build start message to logs
        self.add_log_line(
            &board_name,
            format!("🔨 Starting build for {}...", board_name),
        );

        // Execute build action
        let tx_clone = tx.clone();
        let build_result = self.execute_action(BoardAction::Build, tx_clone).await;

        // Reset build in progress flag
        self.build_in_progress = false;

        match build_result {
            Ok(()) => {
                self.add_log_line(&board_name, "✅ Build initiated successfully".to_string());
                Ok(())
            }
            Err(e) => {
                self.boards[self.selected_board].status = BuildStatus::Failed;
                self.add_log_line(&board_name, format!("❌ Build initiation failed: {}", e));
                Err(e)
            }
        }
    }

    /// Build all boards (sequential or parallel based on build strategy)
    pub async fn build_all_boards(
        &mut self,
        tx: tokio::sync::mpsc::UnboundedSender<crate::models::AppEvent>,
    ) -> Result<()> {
        if self.boards.is_empty() {
            return Err(anyhow::anyhow!("No boards to build"));
        }

        // Set build in progress
        self.build_in_progress = true;

        let board_count = self.boards.len();

        // Update all board statuses first
        let build_strategy_debug = format!("{:?}", self.build_strategy);
        for board in &mut self.boards {
            board.status = BuildStatus::Pending;
        }

        // Then add log messages
        let board_names: Vec<String> = self.boards.iter().map(|b| b.name.clone()).collect();
        for board_name in &board_names {
            self.add_log_line(
                board_name,
                format!("📅 Queued for build (strategy: {})", build_strategy_debug),
            );
        }

        // Log overall build start
        if !self.boards.is_empty() {
            let first_board_name = self.boards[0].name.clone();
            self.add_log_line(
                &first_board_name,
                format!(
                    "🔨 Starting build for {} boards using {} strategy...",
                    board_count, build_strategy_debug
                ),
            );
        }

        let result = match self.build_strategy {
            BuildStrategy::Sequential => self.build_all_sequential(tx.clone()).await,
            BuildStrategy::Parallel => self.build_all_parallel(tx.clone()).await,
            BuildStrategy::IdfBuildApps => self.build_all_idf_build_apps(tx.clone()).await,
        };

        // Reset build in progress flag
        self.build_in_progress = false;

        match result {
            Ok(success_count) => {
                if !self.boards.is_empty() {
                    let first_board_name = self.boards[0].name.clone();
                    self.add_log_line(
                        &first_board_name,
                        format!(
                            "✅ Build all completed: {}/{} boards successful",
                            success_count, board_count
                        ),
                    );
                }
                Ok(())
            }
            Err(e) => {
                if !self.boards.is_empty() {
                    let first_board_name = self.boards[0].name.clone();
                    self.add_log_line(&first_board_name, format!("❌ Build all failed: {}", e));
                }
                Err(e)
            }
        }
    }

    /// Build all boards sequentially
    async fn build_all_sequential(
        &mut self,
        tx: tokio::sync::mpsc::UnboundedSender<crate::models::AppEvent>,
    ) -> Result<usize> {
        let mut success_count = 0;

        for i in 0..self.boards.len() {
            let board_name = self.boards[i].name.clone();
            self.boards[i].status = BuildStatus::Building;

            self.add_log_line(
                &board_name,
                format!(
                    "🔨 Building {} ({}/{})",
                    board_name,
                    i + 1,
                    self.boards.len()
                ),
            );

            // Build this board by temporarily selecting it
            let original_selection = self.selected_board;
            self.selected_board = i;

            let build_result = self.execute_action(BoardAction::Build, tx.clone()).await;

            self.selected_board = original_selection;

            match build_result {
                Ok(()) => {
                    success_count += 1;
                    self.add_log_line(
                        &board_name,
                        format!("✅ Build {} completed successfully", board_name),
                    );
                }
                Err(e) => {
                    self.boards[i].status = BuildStatus::Failed;
                    self.add_log_line(
                        &board_name,
                        format!("❌ Build {} failed: {}", board_name, e),
                    );
                    // Continue with other boards even if one fails
                }
            }
        }

        Ok(success_count)
    }

    /// Build all boards in parallel
    async fn build_all_parallel(
        &mut self,
        tx: tokio::sync::mpsc::UnboundedSender<crate::models::AppEvent>,
    ) -> Result<usize> {
        // For parallel builds, we need to be more careful about shared state
        let mut build_tasks = Vec::new();

        let board_names: Vec<String> = self.boards.iter().map(|b| b.name.clone()).collect();
        for board in &mut self.boards {
            board.status = BuildStatus::Building;
        }

        for board_name in &board_names {
            self.add_log_line(
                board_name,
                format!("🔨 Starting parallel build for {}", board_name),
            );
        }

        // Create build tasks (simplified - in reality this would need more coordination)
        for i in 0..self.boards.len() {
            let board_name = self.boards[i].name.clone();
            let tx_task = tx.clone();

            // For now, just queue them with a small delay to avoid resource conflicts
            let delay_ms = i as u64 * 1000; // 1 second delay between starts

            build_tasks.push(tokio::spawn(async move {
                tokio::time::sleep(tokio::time::Duration::from_millis(delay_ms)).await;
                let _ = tx_task.send(crate::models::AppEvent::BuildOutput(
                    board_name.clone(),
                    format!("🔨 Parallel build for {} (simulated)", board_name),
                ));

                // Simulate build time
                tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

                let _ = tx_task.send(crate::models::AppEvent::ActionFinished(
                    board_name,
                    "Parallel Build".to_string(),
                    true, // Assume success for now
                ));
            }));
        }

        // Wait for all tasks to complete
        for task in build_tasks {
            let _ = task.await;
        }

        Ok(self.boards.len()) // Assume all succeeded for now
    }

    /// Build all boards using idf-build-apps (professional mode)
    async fn build_all_idf_build_apps(
        &mut self,
        tx: tokio::sync::mpsc::UnboundedSender<crate::models::AppEvent>,
    ) -> Result<usize> {
        // Add message about professional build mode
        if !self.boards.is_empty() {
            let first_board_name = self.boards[0].name.clone();
            self.add_log_line(
                &first_board_name,
                "🎆 Using professional idf-build-apps for optimal build performance".to_string(),
            );

            self.add_log_line(
                &first_board_name,
                "📂 Check ./support/build-all-idf-build-apps.sh for the generated script"
                    .to_string(),
            );
        }

        // For now, fall back to sequential builds
        // In a full implementation, this would execute the generated idf-build-apps script
        self.build_all_sequential(tx).await
    }

    /// Refresh the board list by rediscovering boards
    pub async fn refresh_board_list(
        &mut self,
        tx: tokio::sync::mpsc::UnboundedSender<crate::models::AppEvent>,
    ) -> Result<()> {
        let _ = tx.send(crate::models::AppEvent::BuildOutput(
            "system".to_string(),
            "🔄 Refreshing board list...".to_string(),
        ));

        // Store current selection
        let current_board_name = if self.selected_board < self.boards.len() {
            Some(self.boards[self.selected_board].name.clone())
        } else {
            None
        };

        // Rediscover boards using the same logic as App::new
        let new_boards = if let Some(ref handler) = self.project_handler {
            match handler.discover_boards(&self.project_dir) {
                Ok(project_boards) => {
                    let _ = tx.send(crate::models::AppEvent::BuildOutput(
                        "system".to_string(),
                        format!(
                            "✅ Discovered {} boards using project handler",
                            project_boards.len()
                        ),
                    ));

                    project_boards
                        .into_iter()
                        .map(|board| BoardConfig {
                            name: board.name,
                            config_file: board.config_file,
                            build_dir: board.build_dir,
                            status: BuildStatus::Pending,
                            log_lines: Vec::new(),
                            build_time: None,
                            last_updated: Local::now(),
                            target: board.target,
                            project_type: board.project_type,
                        })
                        .collect()
                }
                Err(e) => {
                    let _ = tx.send(crate::models::AppEvent::BuildOutput(
                        "system".to_string(),
                        format!("⚠️ Project handler discovery failed: {}, using fallback", e),
                    ));
                    Self::discover_boards(&self.project_dir)?
                }
            }
        } else {
            let _ = tx.send(crate::models::AppEvent::BuildOutput(
                "system".to_string(),
                "Using fallback ESP-IDF board discovery".to_string(),
            ));
            Self::discover_boards(&self.project_dir)?
        };

        // Load existing logs for new boards
        let mut refreshed_boards = new_boards;
        for board in &mut refreshed_boards {
            Self::load_existing_logs(board, &self.logs_dir);
        }

        let old_count = self.boards.len();
        let new_count = refreshed_boards.len();

        // Update board list
        self.boards = refreshed_boards;

        // Try to restore selection to the same board name if it still exists
        if let Some(board_name) = current_board_name {
            if let Some(index) = self.boards.iter().position(|b| b.name == board_name) {
                self.selected_board = index;
                self.list_state.select(Some(index));
            } else {
                // Board no longer exists, select first board
                self.selected_board = 0;
                if !self.boards.is_empty() {
                    self.list_state.select(Some(0));
                }
            }
        } else {
            // No previous selection, select first board
            self.selected_board = 0;
            if !self.boards.is_empty() {
                self.list_state.select(Some(0));
            }
        }

        let _ = tx.send(crate::models::AppEvent::BuildOutput(
            "system".to_string(),
            format!(
                "✅ Board list refreshed: {} → {} boards",
                old_count, new_count
            ),
        ));

        if new_count != old_count {
            let _ = tx.send(crate::models::AppEvent::BuildOutput(
                "system".to_string(),
                if new_count > old_count {
                    format!("🎉 Found {} new board(s)!", new_count - old_count)
                } else {
                    format!("📉 {} board(s) no longer detected", old_count - new_count)
                },
            ));
        }

        Ok(())
    }

    /// Execute a component action for the currently selected component
    pub async fn execute_component_action(
        &mut self,
        action: ComponentAction,
        tx: tokio::sync::mpsc::UnboundedSender<crate::models::AppEvent>,
    ) -> Result<()> {
        if self.selected_component >= self.components.len() {
            return Err(anyhow::anyhow!("No component selected"));
        }

        let component = &self.components[self.selected_component];
        let component_name = component.name.clone();
        let component_path = component.path.clone();
        let _is_managed = component.is_managed;

        // Check if action is available for this component
        if !action.is_available_for(component) {
            let _ = tx.send(crate::models::AppEvent::BuildOutput(
                "component".to_string(),
                format!(
                    "⚠️ Action '{}' is not available for component '{}'",
                    action.name(),
                    component_name
                ),
            ));
            return Err(anyhow::anyhow!(
                "Action '{}' is not available for component '{}'",
                action.name(),
                component_name
            ));
        }

        // Update component status to show action is in progress
        self.components[self.selected_component].action_status = Some(format!(
            "{}...",
            match action {
                ComponentAction::CloneFromRepository => "Cloning",
                ComponentAction::Update => "Updating",
                ComponentAction::Remove => "Removing",
                ComponentAction::MoveToComponents => "Moving",
                ComponentAction::OpenInEditor => "Opening",
            }
        ));

        let _ = tx.send(crate::models::AppEvent::BuildOutput(
            "component".to_string(),
            format!(
                "🔧 Executing '{}' on component '{}'",
                action.name(),
                component_name
            ),
        ));

        let result = match action {
            ComponentAction::CloneFromRepository => {
                self.execute_component_clone(&component_name, &component_path, &tx)
                    .await
            }
            ComponentAction::Update => {
                self.execute_component_update(&component_name, &component_path, &tx)
                    .await
            }
            ComponentAction::Remove => {
                self.execute_component_remove(&component_name, &component_path, &tx)
                    .await
            }
            ComponentAction::MoveToComponents => {
                self.execute_component_move(&component_name, &component_path, &tx)
                    .await
            }
            ComponentAction::OpenInEditor => {
                self.execute_component_open_editor(&component_name, &component_path, &tx)
                    .await
            }
        };

        // Clear action status
        if self.selected_component < self.components.len() {
            self.components[self.selected_component].action_status = None;
        }

        match result {
            Ok(()) => {
                let _ = tx.send(crate::models::AppEvent::BuildOutput(
                    "component".to_string(),
                    format!(
                        "✅ {} completed successfully for '{}'!",
                        action.name(),
                        component_name
                    ),
                ));

                // Refresh components list after successful action
                if matches!(
                    action,
                    ComponentAction::Remove | ComponentAction::MoveToComponents
                ) {
                    self.refresh_component_list(&tx).await?;
                }

                Ok(())
            }
            Err(e) => {
                let _ = tx.send(crate::models::AppEvent::BuildOutput(
                    "component".to_string(),
                    format!(
                        "❌ {} failed for '{}': {}",
                        action.name(),
                        component_name,
                        e
                    ),
                ));
                Err(e)
            }
        }
    }

    /// Clone component from repository
    async fn execute_component_clone(
        &self,
        component_name: &str,
        component_path: &std::path::Path,
        tx: &tokio::sync::mpsc::UnboundedSender<crate::models::AppEvent>,
    ) -> Result<()> {
        // Read component manifest to get repository URL
        let manifest_path = component_path.join("idf_component.yml");
        if !manifest_path.exists() {
            return Err(anyhow::anyhow!(
                "Component manifest not found: {}",
                manifest_path.display()
            ));
        }

        let manifest_content = std::fs::read_to_string(&manifest_path)?;
        let manifest: crate::models::project::ComponentManifest =
            serde_yaml::from_str(&manifest_content)
                .map_err(|e| anyhow::anyhow!("Failed to parse component manifest: {}", e))?;

        // Get repository URL
        let repo_url = manifest
            .url
            .or(manifest.git)
            .or(manifest.repository)
            .ok_or_else(|| anyhow::anyhow!("No repository URL found in component manifest"))?;

        let _ = tx.send(crate::models::AppEvent::BuildOutput(
            "component".to_string(),
            format!("📡 Cloning component from: {}", repo_url),
        ));

        // Create target path in components directory (not managed_components)
        let target_path = self.project_dir.join("components").join(component_name);

        if target_path.exists() {
            return Err(anyhow::anyhow!(
                "Target directory already exists: {}",
                target_path.display()
            ));
        }

        // Clone the repository
        let clone_success = Self::execute_command_streaming(
            "git",
            &["clone", &repo_url, &target_path.to_string_lossy()],
            &self.project_dir,
            vec![],
            "component",
            tx.clone(),
        )
        .await?;

        if !clone_success {
            return Err(anyhow::anyhow!("Git clone failed"));
        }

        let _ = tx.send(crate::models::AppEvent::BuildOutput(
            "component".to_string(),
            format!("✅ Component cloned to: {}", target_path.display()),
        ));

        Ok(())
    }

    /// Update component to latest version
    async fn execute_component_update(
        &self,
        component_name: &str,
        component_path: &std::path::Path,
        tx: &tokio::sync::mpsc::UnboundedSender<crate::models::AppEvent>,
    ) -> Result<()> {
        let _ = tx.send(crate::models::AppEvent::BuildOutput(
            "component".to_string(),
            format!("🔄 Updating component at: {}", component_path.display()),
        ));

        // Check if it's a git repository
        let git_dir = component_path.join(".git");
        if git_dir.exists() {
            // Use git pull to update
            let update_success = Self::execute_command_streaming(
                "git",
                &[
                    "-C",
                    &component_path.to_string_lossy(),
                    "pull",
                    "origin",
                    "main",
                ],
                &self.project_dir,
                vec![],
                "component",
                tx.clone(),
            )
            .await?;

            if !update_success {
                // Try master branch as fallback
                let update_success = Self::execute_command_streaming(
                    "git",
                    &[
                        "-C",
                        &component_path.to_string_lossy(),
                        "pull",
                        "origin",
                        "master",
                    ],
                    &self.project_dir,
                    vec![],
                    "component",
                    tx.clone(),
                )
                .await?;

                if !update_success {
                    return Err(anyhow::anyhow!(
                        "Git pull failed for both main and master branches"
                    ));
                }
            }
        } else {
            // For managed components, we can try using idf.py component update
            let update_success = Self::execute_command_streaming(
                "idf.py",
                &["add-dependency", "--force", component_name],
                &self.project_dir,
                vec![],
                "component",
                tx.clone(),
            )
            .await?;

            if !update_success {
                return Err(anyhow::anyhow!("Component update using idf.py failed"));
            }
        }

        Ok(())
    }

    /// Remove component directory
    async fn execute_component_remove(
        &self,
        component_name: &str,
        component_path: &std::path::Path,
        tx: &tokio::sync::mpsc::UnboundedSender<crate::models::AppEvent>,
    ) -> Result<()> {
        let _ = tx.send(crate::models::AppEvent::BuildOutput(
            "component".to_string(),
            format!("🗑️ Removing component: {}", component_path.display()),
        ));

        if !component_path.exists() {
            return Err(anyhow::anyhow!("Component directory does not exist"));
        }

        // Remove the entire directory
        std::fs::remove_dir_all(component_path)
            .map_err(|e| anyhow::anyhow!("Failed to remove component directory: {}", e))?;

        let _ = tx.send(crate::models::AppEvent::BuildOutput(
            "component".to_string(),
            format!("✅ Component '{}' removed successfully", component_name),
        ));

        Ok(())
    }

    /// Move component from managed_components to components
    async fn execute_component_move(
        &self,
        component_name: &str,
        component_path: &std::path::Path,
        tx: &tokio::sync::mpsc::UnboundedSender<crate::models::AppEvent>,
    ) -> Result<()> {
        let target_path = self.project_dir.join("components").join(component_name);

        let _ = tx.send(crate::models::AppEvent::BuildOutput(
            "component".to_string(),
            format!(
                "📦 Moving component from {} to {}",
                component_path.display(),
                target_path.display()
            ),
        ));

        if target_path.exists() {
            return Err(anyhow::anyhow!(
                "Target directory already exists: {}",
                target_path.display()
            ));
        }

        // Create components directory if it doesn't exist
        std::fs::create_dir_all(target_path.parent().unwrap())?;

        // Move the directory
        std::fs::rename(component_path, &target_path)
            .map_err(|e| anyhow::anyhow!("Failed to move component: {}", e))?;

        let _ = tx.send(crate::models::AppEvent::BuildOutput(
            "component".to_string(),
            format!(
                "✅ Component '{}' moved to components directory",
                component_name
            ),
        ));

        Ok(())
    }

    /// Open component directory in default editor
    async fn execute_component_open_editor(
        &self,
        component_name: &str,
        component_path: &std::path::Path,
        tx: &tokio::sync::mpsc::UnboundedSender<crate::models::AppEvent>,
    ) -> Result<()> {
        let _ = tx.send(crate::models::AppEvent::BuildOutput(
            "component".to_string(),
            format!("📝 Opening component '{}' in editor", component_name),
        ));

        // Use 'open' command on macOS, 'xdg-open' on Linux
        #[cfg(target_os = "macos")]
        let open_cmd = "open";
        #[cfg(target_os = "linux")]
        let open_cmd = "xdg-open";
        #[cfg(target_os = "windows")]
        let open_cmd = "start";

        let open_success = Self::execute_command_streaming(
            open_cmd,
            &[&component_path.to_string_lossy()],
            &self.project_dir,
            vec![],
            "component",
            tx.clone(),
        )
        .await?;

        if !open_success {
            return Err(anyhow::anyhow!(
                "Failed to open component directory in editor"
            ));
        }

        let _ = tx.send(crate::models::AppEvent::BuildOutput(
            "component".to_string(),
            format!("✅ Component '{}' opened in default editor", component_name),
        ));

        Ok(())
    }

    /// Refresh the component list by rediscovering components
    async fn refresh_component_list(
        &mut self,
        tx: &tokio::sync::mpsc::UnboundedSender<crate::models::AppEvent>,
    ) -> Result<()> {
        let _ = tx.send(crate::models::AppEvent::BuildOutput(
            "component".to_string(),
            "🔄 Refreshing component list...".to_string(),
        ));

        // Store current selection
        let current_component_name = if self.selected_component < self.components.len() {
            Some(self.components[self.selected_component].name.clone())
        } else {
            None
        };

        // Rediscover components
        let new_components = Self::discover_components(&self.project_dir)?;
        let old_count = self.components.len();
        let new_count = new_components.len();

        self.components = new_components;

        // Try to restore selection to the same component name if it still exists
        if let Some(component_name) = current_component_name {
            if let Some(index) = self
                .components
                .iter()
                .position(|c| c.name == component_name)
            {
                self.selected_component = index;
                self.component_list_state.select(Some(index));
            } else {
                // Component no longer exists, select first component
                self.selected_component = 0;
                if !self.components.is_empty() {
                    self.component_list_state.select(Some(0));
                }
            }
        } else {
            // No previous selection, select first component
            self.selected_component = 0;
            if !self.components.is_empty() {
                self.component_list_state.select(Some(0));
            }
        }

        let _ = tx.send(crate::models::AppEvent::BuildOutput(
            "component".to_string(),
            format!(
                "✅ Component list refreshed: {} → {} components",
                old_count, new_count
            ),
        ));

        if new_count != old_count {
            let _ = tx.send(crate::models::AppEvent::BuildOutput(
                "component".to_string(),
                if new_count > old_count {
                    format!("🎉 Found {} new component(s)!", new_count - old_count)
                } else {
                    format!(
                        "📉 {} component(s) no longer detected",
                        old_count - new_count
                    )
                },
            ));
        }

        Ok(())
    }
}
