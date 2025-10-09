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

    pub fn generate_support_scripts(&self) -> Result<()> {
        // TODO: Move implementation from main_backup.rs
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
        // Handle RemoteFlash and RemoteMonitor specially
        if action == BoardAction::RemoteFlash {
            self.start_fetching_remote_boards(tx);
            return Ok(());
        }
        if action == BoardAction::RemoteMonitor {
            // TODO: Implement remote monitor functionality
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
        let _project_handler = self.project_handler.as_ref().map(|h| h.project_type());

        // Spawn the action execution task
        tokio::spawn(async move {
            let log_file = logs_dir.join(format!("{}.log", board_name));
            let result = match action {
                BoardAction::Build => {
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
                BoardAction::Flash => {
                    Self::flash_board_esp_idf(
                        &board_name,
                        &project_dir,
                        &build_dir,
                        &log_file,
                        tx_clone.clone(),
                    )
                    .await
                }
                BoardAction::Clean => {
                    Self::clean_board_esp_idf(
                        &board_name,
                        &project_dir,
                        &build_dir,
                        &log_file,
                        tx_clone.clone(),
                    )
                    .await
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
                _ => {
                    // TODO: Implement other actions (FlashAppOnly, Purge, GenerateBinary)
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
    async fn flash_board_with_handler(
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

        // First build to get artifacts
        let artifacts = project_handler
            .build_board(project_dir, &board_config, tx.clone())
            .await?;

        // Then flash the artifacts
        project_handler
            .flash_board(project_dir, &board_config, &artifacts, None, tx)
            .await
    }

    /// Clean board using project handler
    async fn clean_board_with_handler(
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

        // Clean the board
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

    // Remote board functionality
    pub fn get_server_url(&self) -> String {
        // If we have discovered servers, prefer IPv4 over IPv6
        if !self.discovered_servers.is_empty() {
            // First try to find an IPv4 server
            let preferred_server = self
                .discovered_servers
                .iter()
                .find(|server| matches!(server.ip, std::net::IpAddr::V4(_)))
                .or_else(|| self.discovered_servers.first()); // Fallback to first server if no IPv4

            if let Some(server) = preferred_server {
                // Properly format IPv6 addresses with square brackets
                let ip_str = match server.ip {
                    std::net::IpAddr::V6(_) => format!("[{}]", server.ip),
                    std::net::IpAddr::V4(_) => server.ip.to_string(),
                };
                return format!("http://{}:{}", ip_str, server.port);
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
                    self.boards[self.selected_board].log_lines.push(format!(
                        "  {}. {} at {}:{} ({})",
                        i + 1,
                        server.name,
                        server.ip,
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
        let project_type = self.project_handler.as_ref().map(|h| h.project_type());

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
}
