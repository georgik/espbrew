use anyhow::Result;
use chrono::{DateTime, Local};
use clap::Parser;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use glob::glob;
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap},
    Frame, Terminal,
};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io,
    path::{Path, PathBuf},
    process::Stdio,
    time::{Duration, Instant},
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
    /// Path to ESP-IDF project directory
    #[arg(value_name = "PROJECT_DIR")]
    project_dir: PathBuf,
    
    /// Run in CLI mode without TUI - just generate scripts and build all boards
    #[arg(long, help = "Run builds without interactive TUI")]
    cli_only: bool,
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
    BuildOutput(String, String), // board_name, line
    BuildFinished(String, bool), // board_name, success
    Tick,
}

struct App {
    boards: Vec<BoardConfig>,
    selected_board: usize,
    list_state: ListState,
    project_dir: PathBuf,
    logs_dir: PathBuf,
    support_dir: PathBuf,
    show_help: bool,
    start_time: Instant,
}

impl App {
    fn new(project_dir: PathBuf) -> Result<Self> {
        let logs_dir = project_dir.join("logs");
        let support_dir = project_dir.join("support");
        
        // Create directories if they don't exist
        fs::create_dir_all(&logs_dir)?;
        fs::create_dir_all(&support_dir)?;

        let boards = Self::discover_boards(&project_dir)?;
        let mut list_state = ListState::default();
        if !boards.is_empty() {
            list_state.select(Some(0));
        }

        Ok(Self {
            boards,
            selected_board: 0,
            list_state,
            project_dir,
            logs_dir,
            support_dir,
            show_help: false,
            start_time: Instant::now(),
        })
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
        let boards_data: Vec<_> = self.boards.iter().enumerate().map(|(index, board)| {
            (index, board.name.clone(), board.config_file.clone(), board.build_dir.clone())
        }).collect();
        
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
                ).await;
                
                let _ = tx_clone.send(AppEvent::BuildFinished(board_name, result.is_ok()));
            });
        }
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
        
        // Set target command
        let mut cmd = TokioCommand::new("idf.py");
        cmd.current_dir(project_dir)
            .env("SDKCONFIG_DEFAULTS", &*config_path)
            .args(["-B", &build_dir.to_string_lossy(), "set-target", &target])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let output = cmd.output().await?;
        let set_target_log = format!("SET TARGET OUTPUT:\n{}\n{}\n", 
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr));
        
        fs::write(log_file, &set_target_log)?;
        
        if !output.status.success() {
            return Err(anyhow::anyhow!("Failed to set target"));
        }

        // Build command
        let mut cmd = TokioCommand::new("idf.py");
        cmd.current_dir(project_dir)
            .env("SDKCONFIG_DEFAULTS", &*config_path)
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
            let mut log_content = set_target_log.clone();
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

    async fn flash_board(&self, board_index: usize) -> Result<()> {
        if board_index >= self.boards.len() {
            return Err(anyhow::anyhow!("Invalid board index"));
        }
        
        let board = &self.boards[board_index];
        let mut cmd = TokioCommand::new("idf.py");
        cmd.current_dir(&self.project_dir)
            .args(["-B", &board.build_dir.to_string_lossy(), "flash", "monitor"])
            .status().await?;
        
        Ok(())
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
        }
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
}

fn ui(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(f.area());

    // Left panel - Board list
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
                Span::styled(
                    status_symbol,
                    Style::default().fg(board.status.color()),
                ),
                Span::raw(" "),
                Span::raw(&board.name),
                Span::styled(
                    time_info,
                    Style::default().fg(Color::Gray),
                ),
            ]))
        })
        .collect();

    let board_list = List::new(board_items)
        .block(
            Block::default()
                .title("🍺 ESP Boards")
                .borders(Borders::ALL),
        )
        .highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        );

    f.render_stateful_widget(board_list, chunks[0], &mut app.list_state.clone());

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
                    format!("{} {:?}", selected_board.status.symbol(), selected_board.status),
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
            Line::from(""),
            Line::from(vec![
                Span::styled("Controls: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw("↑/↓: Navigate, Enter: Flash, B: Build All, Q: Quit"),
            ]),
        ];

        let details_paragraph = Paragraph::new(details)
            .block(Block::default().title("Board Details").borders(Borders::ALL))
            .wrap(Wrap { trim: true });

        f.render_widget(details_paragraph, right_chunks[0]);

        // Build log
        let log_lines: Vec<Line> = selected_board
            .log_lines
            .iter()
            .rev()
            .take(50)
            .rev()
            .map(|line| Line::from(line.as_str()))
            .collect();

        let log_paragraph = Paragraph::new(log_lines)
            .block(Block::default().title("Build Log").borders(Borders::ALL))
            .wrap(Wrap { trim: true });

        f.render_widget(log_paragraph, right_chunks[1]);
    }

    // Help popup
    if app.show_help {
        let area = centered_rect(60, 20, f.area());
        f.render_widget(Clear, area);
        
        let help_text = vec![
            Line::from("🍺 ESPBrew Help"),
            Line::from(""),
            Line::from("↑/↓ or j/k    Navigate boards"),
            Line::from("Enter         Flash selected board"),
            Line::from("b             Build all boards (does not auto-start)"),
            Line::from("r             Refresh board list"),
            Line::from("h or ?        Toggle this help"),
            Line::from("q             Quit"),
            Line::from(""),
            Line::from("Note: Use --cli-only for automatic builds"),
            Line::from("Logs are saved in ./logs/"),
            Line::from("Build scripts in ./support/"),
        ];
        
        let help_paragraph = Paragraph::new(help_text)
            .block(Block::default().title("Help").borders(Borders::ALL))
            .style(Style::default().bg(Color::Black));
            
        f.render_widget(help_paragraph, area);
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

async fn run_cli_only(mut app: App) -> Result<()> {
    println!("🍺 ESPBrew CLI Mode - Building all boards...");
    println!("Found {} boards:", app.boards.len());
    
    for board in &app.boards {
        println!("  - {} ({})", board.name, board.config_file.display());
    }
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
                        println!("✅ [{}] Build completed successfully! ({}/{} done)", board_name, completed, total_boards);
                    } else {
                        failed += 1;
                        println!("❌ [{}] Build failed! ({}/{} done)", board_name, completed, total_boards);
                    }
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
    
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    
    if !cli.project_dir.exists() {
        return Err(anyhow::anyhow!("Project directory does not exist: {:?}", cli.project_dir));
    }

    let mut app = App::new(cli.project_dir)?;
    
    // Generate support scripts
    println!("🍺 Generating build and flash scripts...");
    app.generate_support_scripts()?;
    println!("✅ Scripts generated in ./support/");
    
    if cli.cli_only {
        return run_cli_only(app).await;
    }

    println!();
    println!("🍺 Starting ESPBrew TUI...");
    println!("Found {} boards. Press 'b' to build all boards.", app.boards.len());
    println!("Press 'h' for help, 'q' to quit.");
    println!();

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
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
                    if let Event::Key(key) = event::read()? {
                        if key.kind == KeyEventKind::Press {
                            match key.code {
                                KeyCode::Char('q') => break Ok(()),
                                KeyCode::Char('h') | KeyCode::Char('?') => {
                                    app.show_help = !app.show_help;
                                }
                                KeyCode::Char('b') => {
                                    app.build_all_boards(tx.clone()).await?;
                                }
                                KeyCode::Char('r') => {
                                    app.boards = App::discover_boards(&app.project_dir)?;
                                    app.selected_board = 0;
                                    app.list_state.select(Some(0));
                                }
                                KeyCode::Up | KeyCode::Char('k') => {
                                    app.previous_board();
                                }
                                KeyCode::Down | KeyCode::Char('j') => {
                                    app.next_board();
                                }
                                KeyCode::Enter => {
                                    if app.selected_board < app.boards.len() &&
                                       matches!(app.boards[app.selected_board].status, BuildStatus::Success) {
                                        let board_name = app.boards[app.selected_board].name.clone();
                                        app.update_board_status(&board_name, BuildStatus::Flashing);
                                        let _board_index = app.selected_board;
                                        // TODO: Implement actual flashing
                                        tokio::spawn(async move {
                                            // Note: This would need proper error handling in a real app
                                            // For now, we'll just update the status
                                        });
                                    }
                                }
                                _ => {}
                            }
                        }
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
                    AppEvent::Tick => {
                        // Regular tick for UI updates
                    }
                }
            }
        }
    };

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    result
}
