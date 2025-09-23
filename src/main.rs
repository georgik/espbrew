use anyhow::Result;
use chrono::{DateTime, Local};
use clap::{Parser, Subcommand};
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use glob::glob;
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap},
};
use serde::{Deserialize, Serialize};
use serde_yaml;
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

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
#[command(name = "espbrew")]
#[command(about = "🍺 ESP32 Multi-Board Build Manager - Brew your ESP32 builds with style!")]
struct Cli {
    /// Path to ESP-IDF project directory (defaults to current directory)
    #[arg(global = true, value_name = "PROJECT_DIR")]
    project_dir: Option<PathBuf>,

    /// Run in CLI mode without TUI - just generate scripts and build all boards
    #[arg(long, help = "Run builds without interactive TUI")]
    cli_only: bool,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// List boards and components (default CLI behavior)
    List,
    /// Build all boards
    Build,
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
    ActionFinished(String, String, bool),          // board_name, action, success
    ComponentActionStarted(String, String),        // component_name, action_name
    ComponentActionProgress(String, String),       // component_name, progress_message
    ComponentActionFinished(String, String, bool), // component_name, action_name, success
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
}

#[derive(Debug, Clone)]
struct ComponentConfig {
    name: String,
    path: PathBuf,
    is_managed: bool, // true if in managed_components, false if in components
    action_status: Option<String>, // Current action being performed (e.g., "Cloning...")
}

#[derive(Debug, Clone, PartialEq)]
enum ComponentAction {
    MoveToComponents,
    CloneFromRepository,
    Remove,
    OpenInEditor,
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
    show_help: bool,
    focused_pane: FocusedPane,
    log_scroll_offset: usize,
    show_idf_warning: bool,
    idf_warning_acknowledged: bool,
    show_action_menu: bool,
    show_component_action_menu: bool,
    action_menu_selected: usize,
    component_action_menu_selected: usize,
    available_actions: Vec<BoardAction>,
    available_component_actions: Vec<ComponentAction>,
}

impl App {
    fn new(project_dir: PathBuf) -> Result<Self> {
        let logs_dir = project_dir.join("logs");
        let support_dir = project_dir.join("support");

        // Create directories if they don't exist
        fs::create_dir_all(&logs_dir)?;
        fs::create_dir_all(&support_dir)?;

        let mut boards = Self::discover_boards(&project_dir)?;
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

        // Check if ESP-IDF is available
        let idf_available = Self::check_idf_available();

        let available_actions = vec![
            BoardAction::Build,
            BoardAction::GenerateBinary,
            BoardAction::Clean,
            BoardAction::Purge,
            BoardAction::Flash,
            BoardAction::FlashAppOnly,
            BoardAction::Monitor,
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
            show_help: false,
            focused_pane: FocusedPane::BoardList,
            log_scroll_offset: 0,
            show_idf_warning: !idf_available,
            idf_warning_acknowledged: false,
            show_action_menu: false,
            show_component_action_menu: false,
            action_menu_selected: 0,
            component_action_menu_selected: 0,
            available_actions,
            available_component_actions,
        })
    }

    fn load_existing_logs(board: &mut BoardConfig, logs_dir: &Path) {
        let log_file = logs_dir.join(format!("{}.log", board.name));
        if log_file.exists() {
            if let Ok(content) = fs::read_to_string(&log_file) {
                board.log_lines = content.lines().map(|line| line.to_string()).collect();

                // Update status based on log content
                if board
                    .log_lines
                    .iter()
                    .any(|line| line.contains("Build complete"))
                {
                    board.status = BuildStatus::Success;
                } else if board
                    .log_lines
                    .iter()
                    .any(|line| line.contains("FAILED") || line.contains("Error"))
                {
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
        }
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
# Clean any existing project configuration to ensure clean slate
rm -f sdkconfig

# Set target and build with board-specific defaults
SDKCONFIG_DEFAULTS="{}" idf.py -B "{}" set-target $TARGET
SDKCONFIG_DEFAULTS="{}" idf.py -B "{}" build

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

# Flash the board
idf.py -B "{}" flash monitor

echo "🔥 Flash completed for {}"
"#,
            board.name,
            Local::now().format("%Y-%m-%d %H:%M:%S"),
            board.name,
            board.build_dir.display(),
            self.project_dir.display(),
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

    async fn build_all_boards(&mut self, tx: mpsc::UnboundedSender<AppEvent>) -> Result<()> {
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

    async fn build_selected_board(&mut self, tx: mpsc::UnboundedSender<AppEvent>) -> Result<()> {
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

        // Update status to building
        self.boards[board_index].status = BuildStatus::Building;
        self.boards[board_index].last_updated = Local::now();

        // Clear previous logs for this board
        self.boards[board_index].log_lines.clear();
        self.reset_log_scroll();

        let tx_clone = tx.clone();
        tokio::spawn(async move {
            let log_file = logs_dir.join(format!("{}.log", board_name));
            let result = Self::build_board(
                &board_name,
                &project_dir,
                &config_file,
                &build_dir,
                &log_file,
                tx_clone.clone(),
            )
            .await;

            let _ = tx_clone.send(AppEvent::BuildFinished(board_name, result.is_ok()));
        });

        Ok(())
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

        // Clean any existing project configuration to ensure clean slate
        let sdkconfig_path = project_dir.join("sdkconfig");
        if sdkconfig_path.exists() {
            let _ = fs::remove_file(&sdkconfig_path);
            let _ = tx.send(AppEvent::BuildOutput(
                board_name.to_string(),
                "🧹 Cleaning existing sdkconfig for clean slate".to_string(),
            ));
        }

        // Get current working directory to check if cd is needed
        let current_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let needs_cd = current_dir != *project_dir;

        // Log the set-target command
        let set_target_cmd = if needs_cd {
            format!(
                "cd {} && SDKCONFIG_DEFAULTS='{}' idf.py -B '{}' set-target {}",
                project_dir.display(),
                config_path,
                build_dir.display(),
                target
            )
        } else {
            format!(
                "SDKCONFIG_DEFAULTS='{}' idf.py -B '{}' set-target {}",
                config_path,
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
            .args(["-B", &build_dir.to_string_lossy(), "set-target", &target])
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
                "cd {} && SDKCONFIG_DEFAULTS='{}' idf.py -B '{}' build",
                project_dir.display(),
                config_path,
                build_dir.display()
            )
        } else {
            format!(
                "SDKCONFIG_DEFAULTS='{}' idf.py -B '{}' build",
                config_path,
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
            .args(["-B", &build_dir.to_string_lossy(), "build"])
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

    fn check_idf_available() -> bool {
        std::process::Command::new("which")
            .arg("idf.py")
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    fn acknowledge_idf_warning(&mut self) {
        self.idf_warning_acknowledged = true;
        self.show_idf_warning = false;
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

    async fn execute_action(
        &mut self,
        action: BoardAction,
        tx: mpsc::UnboundedSender<AppEvent>,
    ) -> Result<()> {
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

        tokio::spawn(async move {
            let log_file = logs_dir.join(format!("{}.log", board_name));
            let result = match action {
                BoardAction::Build => {
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
                BoardAction::Clean => {
                    Self::clean_board(
                        &board_name,
                        &project_dir,
                        &build_dir,
                        &log_file,
                        tx_clone.clone(),
                    )
                    .await
                }
                BoardAction::Purge => {
                    Self::purge_board(&board_name, &build_dir, &log_file, tx_clone.clone()).await
                }
                BoardAction::Flash => {
                    Self::flash_board_action(
                        &board_name,
                        &project_dir,
                        &build_dir,
                        &log_file,
                        tx_clone.clone(),
                    )
                    .await
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
                    Self::monitor_board(
                        &board_name,
                        &project_dir,
                        &build_dir,
                        &log_file,
                        tx_clone.clone(),
                    )
                    .await
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

        let clean_cmd = if needs_cd {
            format!(
                "cd {} && idf.py -B '{}' clean",
                project_dir.display(),
                build_dir.display()
            )
        } else {
            format!("idf.py -B '{}' clean", build_dir.display())
        };

        let _ = tx.send(AppEvent::BuildOutput(
            board_name.to_string(),
            format!("🧡 Executing: {}", clean_cmd),
        ));

        let mut cmd = TokioCommand::new("idf.py");
        cmd.current_dir(project_dir)
            .env("PYTHONUNBUFFERED", "1") // Force unbuffered output
            .args(["-B", &build_dir.to_string_lossy(), "clean"])
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

        let flash_cmd = if needs_cd {
            format!(
                "cd {} && idf.py -B '{}' flash",
                project_dir.display(),
                build_dir.display()
            )
        } else {
            format!("idf.py -B '{}' flash", build_dir.display())
        };

        let _ = tx.send(AppEvent::BuildOutput(
            board_name.to_string(),
            format!("🔥 Executing: {}", flash_cmd),
        ));

        let mut cmd = TokioCommand::new("idf.py");
        cmd.current_dir(project_dir)
            .env("PYTHONUNBUFFERED", "1") // Force unbuffered output
            .args(["-B", &build_dir.to_string_lossy(), "flash"])
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

        let flash_cmd = if needs_cd {
            format!(
                "cd {} && idf.py -B '{}' app-flash",
                project_dir.display(),
                build_dir.display()
            )
        } else {
            format!("idf.py -B '{}' app-flash", build_dir.display())
        };

        let _ = tx.send(AppEvent::BuildOutput(
            board_name.to_string(),
            format!("⚡ Executing: {}", flash_cmd),
        ));

        let mut cmd = TokioCommand::new("idf.py");
        cmd.current_dir(project_dir)
            .env("PYTHONUNBUFFERED", "1") // Force unbuffered output
            .args(["-B", &build_dir.to_string_lossy(), "app-flash"])
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

        let monitor_cmd = if needs_cd {
            format!(
                "cd {} && idf.py -B '{}' flash monitor",
                project_dir.display(),
                build_dir.display()
            )
        } else {
            format!("idf.py -B '{}' flash monitor", build_dir.display())
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
            .args(["-B", &build_dir.to_string_lossy(), "flash", "monitor"])
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

    let board_list_title = if app.focused_pane == FocusedPane::BoardList {
        "🍺 ESP Boards [FOCUSED]"
    } else {
        "🍺 ESP Boards"
    };

    let board_list_block = if app.focused_pane == FocusedPane::BoardList {
        Block::default()
            .title(board_list_title)
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan))
    } else {
        Block::default()
            .title(board_list_title)
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

    // ESP-IDF warning modal
    if app.show_idf_warning && !app.idf_warning_acknowledged {
        let area = centered_rect(70, 15, f.area());
        f.render_widget(Clear, area);

        let warning_text = vec![
            Line::from(vec![Span::styled(
                "⚠️  ESP-IDF Environment Not Found",
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            )]),
            Line::from(""),
            Line::from("The 'idf.py' command was not found in your PATH."),
            Line::from("ESP-IDF tools need to be sourced before using ESPBrew."),
            Line::from(""),
            Line::from("To fix this, run one of the following:"),
            Line::from(""),
            Line::from(vec![
                Span::styled("• ", Style::default().fg(Color::Yellow)),
                Span::styled(
                    "source ~/esp/esp-idf/export.sh",
                    Style::default().fg(Color::Cyan),
                ),
            ]),
            Line::from(vec![
                Span::styled("• ", Style::default().fg(Color::Yellow)),
                Span::styled(
                    "source $IDF_PATH/export.sh",
                    Style::default().fg(Color::Cyan),
                ),
            ]),
            Line::from(vec![
                Span::styled("• ", Style::default().fg(Color::Yellow)),
                Span::styled(
                    "get_idf (if using ESP-IDF installer)",
                    Style::default().fg(Color::Cyan),
                ),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled("Press ", Style::default()),
                Span::styled(
                    "Enter",
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(" to continue anyway or ", Style::default()),
                Span::styled(
                    "ESC/q",
                    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                ),
                Span::styled(" to quit", Style::default()),
            ]),
        ];

        let warning_paragraph = Paragraph::new(warning_text)
            .block(
                Block::default()
                    .title("ESP-IDF Environment Warning")
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
    let help_text = if app.focused_pane == FocusedPane::LogPane {
        vec![
            Span::styled("[↑↓]Scroll ", Style::default().fg(Color::Cyan)),
            Span::styled("[PgUp/PgDn]Page ", Style::default().fg(Color::Cyan)),
            Span::styled("[Home/End]Top/Bottom ", Style::default().fg(Color::Cyan)),
            Span::styled("[Tab]Switch Pane ", Style::default().fg(Color::White)),
            Span::styled("[Enter]Actions ", Style::default().fg(Color::Green)),
            Span::styled(
                "[Space/B]Build Selected ",
                Style::default().fg(Color::LightYellow),
            ),
            Span::styled("[X]Build All ", Style::default().fg(Color::Yellow)),
            Span::styled("[H/?]Help ", Style::default().fg(Color::Blue)),
            Span::styled("[Q/Ctrl+C/ESC]Quit", Style::default().fg(Color::Red)),
        ]
    } else {
        vec![
            Span::styled("[↑↓]Navigate ", Style::default().fg(Color::Cyan)),
            Span::styled("[Tab]Switch Pane ", Style::default().fg(Color::White)),
            Span::styled("[Enter]Actions ", Style::default().fg(Color::Green)),
            Span::styled(
                "[Space/B]Build Selected ",
                Style::default().fg(Color::LightYellow),
            ),
            Span::styled("[X]Build All ", Style::default().fg(Color::Yellow)),
            Span::styled("[R]Refresh ", Style::default().fg(Color::Magenta)),
            Span::styled("[H/?]Help ", Style::default().fg(Color::Blue)),
            Span::styled("[Q/Ctrl+C/ESC]Quit", Style::default().fg(Color::Red)),
        ]
    };

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

            if failed > 0 {
                println!("⚠️  Some builds failed. Check the logs for details.");
                std::process::exit(1);
            } else {
                println!("🎆 All builds completed successfully!");
            }
        }
    }

    Ok(())
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

    let mut app = App::new(project_dir)?;

    // Generate support scripts
    println!("🍺 Generating build and flash scripts...");
    app.generate_support_scripts()?;
    println!("✅ Scripts generated in ./support/");

    if cli.cli_only {
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
                                // Handle ESP-IDF warning modal first
                                if app.show_idf_warning && !app.idf_warning_acknowledged {
                                    match key.code {
                                        KeyCode::Enter => {
                                            app.acknowledge_idf_warning();
                                        }
                                        KeyCode::Char('q') | KeyCode::Esc => break Ok(()),
                                        _ => {}
                                    }
                                    continue;
                                }

                                match key.code {
                                    KeyCode::Char('q') => break Ok(()),
                                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => break Ok(()),
                                KeyCode::Esc => {
                                    if app.show_action_menu {
                                        app.hide_action_menu();
                                    } else if app.show_component_action_menu {
                                        app.hide_component_action_menu();
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
                                    app.build_selected_board(tx.clone()).await?;
                                }
                                KeyCode::Char('x') => {
                                    app.build_all_boards(tx.clone()).await?;
                                }
                                KeyCode::Char(' ') => {
                                    app.build_selected_board(tx.clone()).await?;
                                }
                                KeyCode::Char('r') => {
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
                                KeyCode::Up | KeyCode::Char('k') => {
                                    if app.show_action_menu {
                                        app.previous_action();
                                    } else if app.show_component_action_menu {
                                        app.previous_component_action();
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
                                    if app.focused_pane == FocusedPane::LogPane {
                                        app.scroll_log_page_up();
                                    }
                                }
                                KeyCode::PageDown => {
                                    if app.focused_pane == FocusedPane::LogPane {
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
                                                    if let Err(e) = app.execute_component_action_sync(action).await {
                                                        eprintln!("Component action failed: {}", e);
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
                    AppEvent::ComponentActionStarted(component_name, action_name) => {
                        // Component action started - status is already set in the UI thread
                        eprintln!("🧩 [{}] Started: {}", component_name, action_name);
                    }
                    AppEvent::ComponentActionProgress(component_name, message) => {
                        // Show progress in console for now
                        eprintln!("🧩 [{}] {}", component_name, message);
                    }
                    AppEvent::ComponentActionFinished(component_name, action_name, success) => {
                        // Clear component action status and refresh component list
                        if let Some(component) = app.components.iter_mut().find(|c| c.name == component_name) {
                            component.action_status = None;
                        }

                        if success {
                            eprintln!("✅ [{}] {} completed successfully", component_name, action_name);
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
                            eprintln!("❌ [{}] {} failed", component_name, action_name);
                        }
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
