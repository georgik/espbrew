use crate::models::{AppEvent, ArtifactType, BuildArtifact, ProjectBoardConfig, ProjectType};
use crate::projects::registry::ProjectHandler;
use anyhow::{Context, Result, anyhow};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;

#[derive(Debug, Clone, Deserialize, Serialize)]
struct ArduinoProjectBoardConfig {
    name: String,
    fqbn: String,
    description: String,
    target: String,
    #[serde(default)]
    build_properties: std::collections::HashMap<String, String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct ArduinoProjectConfig {
    project_type: String,
    description: Option<String>,
    boards: Vec<ArduinoProjectBoardConfig>,
    #[serde(default)]
    libraries: Vec<String>,
    #[serde(default)]
    build_settings: std::collections::HashMap<String, String>,
}

pub struct ArduinoHandler;

impl ArduinoHandler {
    pub fn new() -> Self {
        Self
    }

    /// Check if arduino-cli is available in PATH
    fn is_arduino_cli_available(&self) -> bool {
        std::process::Command::new("/home/georgik/projects/espbrew/bin/arduino-cli")
            .arg("version")
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    /// Find Arduino sketch files (.ino) in the project directory
    fn find_sketch_files(&self, project_dir: &Path) -> Result<Vec<PathBuf>> {
        let mut sketch_files = Vec::new();

        for entry in std::fs::read_dir(project_dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_file() {
                if let Some(extension) = path.extension() {
                    if extension == "ino" {
                        sketch_files.push(path);
                    }
                }
            }
        }

        if sketch_files.is_empty() {
            return Err(anyhow!(
                "No Arduino sketch files (.ino) found in project directory"
            ));
        }

        Ok(sketch_files)
    }

    /// Parse the Arduino project configuration from boards.json
    fn parse_project_config(&self, project_dir: &Path) -> Result<ArduinoProjectConfig> {
        let config_path = project_dir.join("boards.json");
        if !config_path.exists() {
            // Create a default configuration for single-board projects
            return Ok(ArduinoProjectConfig {
                project_type: "arduino".to_string(),
                description: Some("Arduino project".to_string()),
                boards: vec![ArduinoProjectBoardConfig {
                    name: "default".to_string(),
                    fqbn: "esp32:esp32:esp32c6".to_string(), // Default to ESP32-C6
                    description: "Default ESP32-C6 configuration".to_string(),
                    target: "ESP32-C6".to_string(),
                    build_properties: std::collections::HashMap::new(),
                }],
                libraries: Vec::new(),
                build_settings: std::collections::HashMap::new(),
            });
        }

        let content = std::fs::read_to_string(&config_path)
            .with_context(|| format!("Failed to read {}", config_path.display()))?;

        let config: ArduinoProjectConfig = serde_json::from_str(&content)
            .with_context(|| format!("Failed to parse {}", config_path.display()))?;

        Ok(config)
    }

    /// Find build artifacts after compilation
    fn find_build_artifacts(
        &self,
        project_dir: &Path,
        _board_name: &str,
    ) -> Result<Vec<BuildArtifact>> {
        let mut artifacts = Vec::new();
        let build_dir = project_dir.join("build");

        // Arduino build artifacts have predictable names based on the sketch
        let sketch_files = self.find_sketch_files(project_dir)?;
        if sketch_files.is_empty() {
            return Err(anyhow!("No Arduino sketch files found"));
        }

        let main_sketch = &sketch_files[0];
        let sketch_name = main_sketch
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or_else(|| anyhow!("Invalid sketch filename"))?;

        // Define Arduino build artifacts with their flash offsets
        let app_bin_name = format!("{}.ino.bin", sketch_name);
        let artifact_definitions = vec![
            (
                "bootloader.bin".to_string(),
                ArtifactType::Bootloader,
                Some(0x0),
            ),
            (
                "partitions.bin".to_string(),
                ArtifactType::PartitionTable,
                Some(0x8000),
            ),
            (app_bin_name, ArtifactType::Application, Some(0x10000)),
        ];

        for (filename, artifact_type, offset) in artifact_definitions {
            let artifact_path = build_dir.join(&filename);
            if artifact_path.exists() {
                artifacts.push(BuildArtifact {
                    name: filename,
                    file_path: artifact_path,
                    artifact_type,
                    offset,
                });
            }
        }

        // Also look for ELF file
        let elf_path = build_dir.join(format!("{}.ino.elf", sketch_name));
        if elf_path.exists() {
            artifacts.push(BuildArtifact {
                name: format!("{}.ino.elf", sketch_name),
                file_path: elf_path,
                artifact_type: ArtifactType::Elf,
                offset: None,
            });
        }

        if artifacts.is_empty() {
            return Err(anyhow!(
                "No Arduino build artifacts found in {}",
                build_dir.display()
            ));
        }

        Ok(artifacts)
    }

    /// Get the primary sketch file for compilation
    fn get_main_sketch(&self, project_dir: &Path) -> Result<PathBuf> {
        let sketch_files = self.find_sketch_files(project_dir)?;

        // Prefer sketch file that matches directory name
        let dir_name = project_dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("main");

        for sketch in &sketch_files {
            if let Some(stem) = sketch.file_stem().and_then(|s| s.to_str()) {
                if stem == dir_name {
                    return Ok(sketch.clone());
                }
            }
        }

        // Fallback to first sketch file found
        Ok(sketch_files[0].clone())
    }
}

#[async_trait]
impl ProjectHandler for ArduinoHandler {
    fn project_type(&self) -> ProjectType {
        ProjectType::Arduino
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn can_handle(&self, project_dir: &Path) -> bool {
        // Check for Arduino sketch files (.ino)
        if let Ok(sketch_files) = self.find_sketch_files(project_dir) {
            if !sketch_files.is_empty() {
                return true;
            }
        }

        // Check for Arduino project configuration
        let config_path = project_dir.join("boards.json");
        if config_path.exists() {
            if let Ok(content) = std::fs::read_to_string(&config_path) {
                if let Ok(config) = serde_json::from_str::<ArduinoProjectConfig>(&content) {
                    return config.project_type == "arduino";
                }
            }
        }

        false
    }

    fn discover_boards(&self, project_dir: &Path) -> Result<Vec<ProjectBoardConfig>> {
        let config = self.parse_project_config(project_dir)?;
        let mut boards = Vec::new();

        for board_config in config.boards {
            let config_file = if project_dir.join("boards.json").exists() {
                project_dir.join("boards.json")
            } else {
                // Create a virtual config file path for boards without explicit config
                project_dir.join(format!("{}.json", board_config.name))
            };

            boards.push(ProjectBoardConfig {
                name: format!(
                    "{}-{}",
                    project_dir
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("arduino"),
                    board_config.name
                ),
                config_file,
                build_dir: project_dir.join("build"),
                target: Some(board_config.target),
                project_type: ProjectType::Arduino,
            });
        }

        Ok(boards)
    }

    async fn build_board(
        &self,
        project_dir: &Path,
        board_config: &ProjectBoardConfig,
        tx: mpsc::UnboundedSender<AppEvent>,
    ) -> Result<Vec<BuildArtifact>> {
        let _ = tx.send(AppEvent::BuildOutput(
            board_config.name.clone(),
            "🏗️ Starting Arduino build...".to_string(),
        ));

        if !self.is_arduino_cli_available() {
            return Err(anyhow!("arduino-cli is not available in PATH"));
        }

        // Parse project configuration to get FQBN for this board
        let project_config = self.parse_project_config(project_dir)?;
        // Extract the actual board name from the full board config name
        // Format: "project-name-board-config-name" -> "board-config-name"
        let project_name = project_dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("arduino");
        let board_name = if board_config.name.starts_with(&format!("{}-", project_name)) {
            &board_config.name[project_name.len() + 1..]
        } else {
            board_config.name.split('-').last().unwrap_or("default")
        };

        let arduino_board = project_config
            .boards
            .iter()
            .find(|b| b.name == board_name)
            .ok_or_else(|| anyhow!("Board configuration '{}' not found in config", board_name))?;

        let main_sketch = self.get_main_sketch(project_dir)?;
        let build_dir = board_config.build_dir.clone();

        // Create build directory
        tokio::fs::create_dir_all(&build_dir).await?;

        // Build command
        let mut cmd = Command::new("/home/georgik/projects/espbrew/bin/arduino-cli");
        cmd.current_dir(project_dir)
            .args(["compile", "--fqbn", &arduino_board.fqbn])
            .arg("--build-path")
            .arg(&build_dir)
            .arg("--verbose")
            .arg(&main_sketch)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        // Add build properties if specified
        for (key, value) in &arduino_board.build_properties {
            cmd.args(["--build-property", &format!("{}={}", key, value)]);
        }

        let build_command_str = format!(
            "arduino-cli compile --fqbn {} --build-path {} {}",
            arduino_board.fqbn,
            build_dir.display(),
            main_sketch.display()
        );

        let _ = tx.send(AppEvent::BuildOutput(
            board_config.name.clone(),
            format!("🔨 Executing: {}", build_command_str),
        ));

        let mut child = cmd.spawn().context("Failed to start arduino-cli compile")?;
        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();

        let tx_stdout = tx.clone();
        let tx_stderr = tx.clone();
        let board_name_stdout = board_config.name.clone();
        let board_name_stderr = board_config.name.clone();

        // Handle stdout
        tokio::spawn(async move {
            let mut reader = BufReader::new(stdout);
            let mut buffer = String::new();

            while reader.read_line(&mut buffer).await.unwrap_or(0) > 0 {
                let line = buffer.trim().to_string();
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

        let status = child
            .wait()
            .await
            .context("Failed to wait for arduino-cli compile")?;

        if status.success() {
            let _ = tx.send(AppEvent::BuildOutput(
                board_config.name.clone(),
                "✅ Arduino build completed successfully".to_string(),
            ));

            // Find build artifacts
            match self.find_build_artifacts(project_dir, &board_config.name) {
                Ok(artifacts) => {
                    let _ = tx.send(AppEvent::BuildOutput(
                        board_config.name.clone(),
                        format!("🎯 Found {} build artifact(s)", artifacts.len()),
                    ));
                    Ok(artifacts)
                }
                Err(e) => {
                    let _ = tx.send(AppEvent::BuildOutput(
                        board_config.name.clone(),
                        format!("⚠️ Failed to find build artifacts: {}", e),
                    ));
                    Err(e)
                }
            }
        } else {
            let _ = tx.send(AppEvent::BuildOutput(
                board_config.name.clone(),
                "❌ Arduino build failed".to_string(),
            ));
            Err(anyhow!("arduino-cli compile failed"))
        }
    }

    async fn flash_board(
        &self,
        project_dir: &Path,
        board_config: &ProjectBoardConfig,
        _artifacts: &[BuildArtifact],
        port: Option<&str>,
        tx: mpsc::UnboundedSender<AppEvent>,
    ) -> Result<()> {
        let _ = tx.send(AppEvent::BuildOutput(
            board_config.name.clone(),
            "🔥 Starting Arduino flash...".to_string(),
        ));

        if !self.is_arduino_cli_available() {
            return Err(anyhow!("arduino-cli is not available in PATH"));
        }

        // Parse project configuration to get FQBN for this board
        let project_config = self.parse_project_config(project_dir)?;
        // Extract the actual board name from the full board config name
        let project_name = project_dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("arduino");
        let board_name = if board_config.name.starts_with(&format!("{}-", project_name)) {
            &board_config.name[project_name.len() + 1..]
        } else {
            board_config.name.split('-').last().unwrap_or("default")
        };

        let arduino_board = project_config
            .boards
            .iter()
            .find(|b| b.name == board_name)
            .ok_or_else(|| anyhow!("Board configuration '{}' not found in config", board_name))?;

        let main_sketch = self.get_main_sketch(project_dir)?;

        // Upload command
        let mut cmd = Command::new("/home/georgik/projects/espbrew/bin/arduino-cli");
        cmd.current_dir(project_dir)
            .args(["upload", "--fqbn", &arduino_board.fqbn])
            .arg("--verbose");

        if let Some(port_str) = port {
            cmd.args(["--port", port_str]);
        }

        cmd.arg(&main_sketch)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let upload_command_str = format!(
            "arduino-cli upload --fqbn {} {} {}",
            arduino_board.fqbn,
            port.map(|p| format!("--port {}", p)).unwrap_or_default(),
            main_sketch.display()
        );

        let _ = tx.send(AppEvent::BuildOutput(
            board_config.name.clone(),
            format!("🔨 Executing: {}", upload_command_str),
        ));

        let mut child = cmd.spawn().context("Failed to start arduino-cli upload")?;
        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();

        let tx_stdout = tx.clone();
        let tx_stderr = tx.clone();
        let board_name_stdout = board_config.name.clone();
        let board_name_stderr = board_config.name.clone();

        // Handle stdout
        tokio::spawn(async move {
            let mut reader = BufReader::new(stdout);
            let mut buffer = String::new();

            while reader.read_line(&mut buffer).await.unwrap_or(0) > 0 {
                let line = buffer.trim().to_string();
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

        let status = child
            .wait()
            .await
            .context("Failed to wait for arduino-cli upload")?;

        if status.success() {
            let _ = tx.send(AppEvent::BuildOutput(
                board_config.name.clone(),
                "✅ Arduino flash completed successfully".to_string(),
            ));
            Ok(())
        } else {
            let _ = tx.send(AppEvent::BuildOutput(
                board_config.name.clone(),
                "❌ Arduino flash failed".to_string(),
            ));
            Err(anyhow!("arduino-cli upload failed"))
        }
    }

    async fn monitor_board(
        &self,
        _project_dir: &Path,
        board_config: &ProjectBoardConfig,
        port: Option<&str>,
        baud_rate: u32,
        tx: mpsc::UnboundedSender<AppEvent>,
    ) -> Result<()> {
        let _ = tx.send(AppEvent::BuildOutput(
            board_config.name.clone(),
            format!(
                "📺 Starting Arduino serial monitor at {} baud...",
                baud_rate
            ),
        ));

        if !self.is_arduino_cli_available() {
            return Err(anyhow!("arduino-cli is not available in PATH"));
        }

        let port_str = port.ok_or_else(|| anyhow!("Port must be specified for monitoring"))?;

        let mut cmd = Command::new("/home/georgik/projects/espbrew/bin/arduino-cli");
        cmd.args(["monitor", "--port", port_str])
            .args(["--config", &format!("baudrate={}", baud_rate)])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = cmd.spawn().context("Failed to start arduino-cli monitor")?;
        let stdout = child.stdout.take().unwrap();

        let tx_stdout = tx.clone();
        let board_name = board_config.name.clone();

        // Handle stdout
        tokio::spawn(async move {
            let mut reader = BufReader::new(stdout);
            let mut buffer = String::new();

            while reader.read_line(&mut buffer).await.unwrap_or(0) > 0 {
                let line = buffer.trim().to_string();
                let _ = tx_stdout.send(AppEvent::BuildOutput(board_name.clone(), line));
                buffer.clear();
            }
        });

        let _status = child
            .wait()
            .await
            .context("Failed to wait for arduino-cli monitor")?;
        Ok(())
    }

    async fn clean_board(
        &self,
        project_dir: &Path,
        board_config: &ProjectBoardConfig,
        tx: mpsc::UnboundedSender<AppEvent>,
    ) -> Result<()> {
        let _ = tx.send(AppEvent::BuildOutput(
            board_config.name.clone(),
            "🧹 Cleaning Arduino build artifacts...".to_string(),
        ));

        let build_dir = &board_config.build_dir;
        if build_dir.exists() {
            tokio::fs::remove_dir_all(build_dir)
                .await
                .with_context(|| {
                    format!("Failed to remove build directory {}", build_dir.display())
                })?;
        }

        // Also remove any .bin files in the project directory
        if let Ok(entries) = std::fs::read_dir(project_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() {
                    if let Some(extension) = path.extension() {
                        if extension == "bin" {
                            let _ = std::fs::remove_file(&path);
                        }
                    }
                }
            }
        }

        let _ = tx.send(AppEvent::BuildOutput(
            board_config.name.clone(),
            "✅ Clean completed successfully".to_string(),
        ));

        Ok(())
    }

    fn get_build_command(&self, project_dir: &Path, board_config: &ProjectBoardConfig) -> String {
        if let Ok(project_config) = self.parse_project_config(project_dir) {
            if let Ok(main_sketch) = self.get_main_sketch(project_dir) {
                let board_name = board_config.name.split('-').last().unwrap_or("default");
                if let Some(arduino_board) =
                    project_config.boards.iter().find(|b| b.name == board_name)
                {
                    return format!(
                        "arduino-cli compile --fqbn {} --build-path {} {}",
                        arduino_board.fqbn,
                        board_config.build_dir.display(),
                        main_sketch.display()
                    );
                }
            }
        }
        "arduino-cli compile <sketch.ino>".to_string()
    }

    fn get_flash_command(
        &self,
        project_dir: &Path,
        board_config: &ProjectBoardConfig,
        port: Option<&str>,
    ) -> String {
        if let Ok(project_config) = self.parse_project_config(project_dir) {
            if let Ok(main_sketch) = self.get_main_sketch(project_dir) {
                let board_name = board_config.name.split('-').last().unwrap_or("default");
                if let Some(arduino_board) =
                    project_config.boards.iter().find(|b| b.name == board_name)
                {
                    return format!(
                        "arduino-cli upload --fqbn {} {} {}",
                        arduino_board.fqbn,
                        port.map(|p| format!("--port {}", p)).unwrap_or_default(),
                        main_sketch.display()
                    );
                }
            }
        }
        "arduino-cli upload <sketch.ino>".to_string()
    }

    fn check_tools_available(&self) -> Result<(), String> {
        if !self.is_arduino_cli_available() {
            return Err("arduino-cli not found in PATH. Please install arduino-cli: https://arduino.github.io/arduino-cli/latest/installation/".to_string());
        }
        Ok(())
    }

    fn get_missing_tools_message(&self) -> String {
        "⚠️  Arduino development tools are not properly set up.\n".to_string()
            + "   Please ensure arduino-cli is installed:\n"
            + "   - Install arduino-cli: https://arduino.github.io/arduino-cli/latest/installation/\n"
            + "   - Run: arduino-cli core update-index\n"
            + "   - Install ESP32 core: arduino-cli core install esp32:esp32\n"
            + "   Press Enter to continue anyway, or 'q' to quit."
    }
}
