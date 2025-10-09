use crate::models::{AppEvent, ArtifactType, BuildArtifact, ProjectBoardConfig, ProjectType};
use crate::projects::registry::ProjectHandler;

use anyhow::{Context, Result};
use async_trait::async_trait;
use std::fs;
use std::path::{Path, PathBuf};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;

/// Handler for TinyGo projects
pub struct TinyGoHandler;

#[async_trait]
impl ProjectHandler for TinyGoHandler {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn project_type(&self) -> ProjectType {
        ProjectType::TinyGo
    }

    fn can_handle(&self, project_dir: &Path) -> bool {
        // Look for go.mod and TinyGo-specific imports
        let go_mod = project_dir.join("go.mod");
        if !go_mod.exists() {
            return false;
        }

        // Check for TinyGo-specific imports in .go files
        self.has_tinygo_imports(project_dir)
    }

    fn discover_boards(&self, project_dir: &Path) -> Result<Vec<ProjectBoardConfig>> {
        let mut boards = Vec::new();

        // TinyGo doesn't have strict board configuration files
        // We'll discover based on Go files and attempt to detect target
        let go_files = self.find_go_files(project_dir)?;

        if go_files.is_empty() {
            return Ok(Vec::new());
        }

        // Check for board-specific configurations or target hints
        let detected_boards = self.detect_boards_from_files(project_dir, &go_files)?;

        if detected_boards.is_empty() {
            // Default to ESP32 configuration
            boards.push(ProjectBoardConfig {
                name: "esp32-coreboard-v2".to_string(),
                config_file: project_dir.join("go.mod"),
                build_dir: project_dir.to_path_buf(),
                target: Some("ESP32".to_string()),
                project_type: ProjectType::TinyGo,
            });
        } else {
            boards.extend(detected_boards);
        }

        boards.sort_by(|a, b| a.name.cmp(&b.name));
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
            "🏗️  Starting TinyGo build...".to_string(),
        ));

        let build_command = self.get_build_command(project_dir, board_config);
        let _ = tx.send(AppEvent::BuildOutput(
            board_config.name.clone(),
            format!("🔨 Executing: {}", build_command),
        ));

        // Build with TinyGo
        let mut cmd = Command::new("tinygo");
        cmd.current_dir(project_dir)
            .args(["build", "-target", &board_config.name, "-o", "firmware.bin"])
            .args(["."])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        let mut child = cmd.spawn().context("Failed to start tinygo build")?;
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
            .context("Failed to wait for tinygo build")?;

        if status.success() {
            let _ = tx.send(AppEvent::BuildOutput(
                board_config.name.clone(),
                "✅ TinyGo build completed successfully".to_string(),
            ));

            // Find build artifacts
            self.find_build_artifacts(project_dir, board_config)
        } else {
            let _ = tx.send(AppEvent::BuildOutput(
                board_config.name.clone(),
                "❌ TinyGo build failed".to_string(),
            ));
            Err(anyhow::anyhow!("TinyGo build failed"))
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
            "🔥 Starting TinyGo flash...".to_string(),
        ));

        let flash_command = self.get_flash_command(project_dir, board_config, port);
        let _ = tx.send(AppEvent::BuildOutput(
            board_config.name.clone(),
            format!("🔨 Executing: {}", flash_command),
        ));

        let mut cmd = Command::new("tinygo");
        cmd.current_dir(project_dir)
            .args(["flash", "-target", &board_config.name]);

        if let Some(port) = port {
            cmd.args(["-port", port]);
        }

        cmd.args(["."])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        let mut child = cmd.spawn().context("Failed to start tinygo flash")?;
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
            .context("Failed to wait for tinygo flash")?;

        if status.success() {
            let _ = tx.send(AppEvent::BuildOutput(
                board_config.name.clone(),
                "✅ TinyGo flash completed successfully".to_string(),
            ));
            Ok(())
        } else {
            let _ = tx.send(AppEvent::BuildOutput(
                board_config.name.clone(),
                "❌ TinyGo flash failed".to_string(),
            ));
            Err(anyhow::anyhow!("TinyGo flash failed"))
        }
    }

    async fn monitor_board(
        &self,
        project_dir: &Path,
        board_config: &ProjectBoardConfig,
        port: Option<&str>,
        baud_rate: u32,
        tx: mpsc::UnboundedSender<AppEvent>,
    ) -> Result<()> {
        let _ = tx.send(AppEvent::BuildOutput(
            board_config.name.clone(),
            format!(
                "📺 Starting TinyGo monitor on {} at {} baud",
                port.unwrap_or("auto-detect"),
                baud_rate
            ),
        ));

        // Use appropriate monitoring tool based on availability
        if self.is_tool_available("screen") {
            self.monitor_with_screen(project_dir, board_config, port, baud_rate, tx)
                .await
        } else if self.is_tool_available("minicom") {
            self.monitor_with_minicom(project_dir, board_config, port, baud_rate, tx)
                .await
        } else {
            let _ = tx.send(AppEvent::BuildOutput(
                board_config.name.clone(),
                "❌ No suitable monitoring tool available (screen or minicom)".to_string(),
            ));
            Err(anyhow::anyhow!("No suitable monitoring tool available"))
        }
    }

    async fn clean_board(
        &self,
        project_dir: &Path,
        board_config: &ProjectBoardConfig,
        tx: mpsc::UnboundedSender<AppEvent>,
    ) -> Result<()> {
        let _ = tx.send(AppEvent::BuildOutput(
            board_config.name.clone(),
            "🧹 Cleaning TinyGo build artifacts...".to_string(),
        ));

        // Remove common build artifacts
        let artifacts_to_remove = ["firmware.bin", "firmware.elf", "main"];

        for artifact in &artifacts_to_remove {
            let artifact_path = project_dir.join(artifact);
            if artifact_path.exists() {
                fs::remove_file(&artifact_path).context("Failed to remove build artifact")?;
            }
        }

        let _ = tx.send(AppEvent::BuildOutput(
            board_config.name.clone(),
            "✅ Clean completed successfully".to_string(),
        ));

        Ok(())
    }

    fn get_build_command(&self, project_dir: &Path, board_config: &ProjectBoardConfig) -> String {
        if std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")) != *project_dir {
            format!(
                "cd {} && tinygo build -target {} -o firmware.bin .",
                project_dir.display(),
                board_config.name
            )
        } else {
            format!(
                "tinygo build -target {} -o firmware.bin .",
                board_config.name
            )
        }
    }

    fn get_flash_command(
        &self,
        project_dir: &Path,
        board_config: &ProjectBoardConfig,
        port: Option<&str>,
    ) -> String {
        let port_arg = port.map(|p| format!(" -port {}", p)).unwrap_or_default();

        if std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")) != *project_dir {
            format!(
                "cd {} && tinygo flash -target {}{} .",
                project_dir.display(),
                board_config.name,
                port_arg
            )
        } else {
            format!("tinygo flash -target {}{} .", board_config.name, port_arg)
        }
    }

    fn check_tools_available(&self) -> Result<(), String> {
        // Check for tinygo command
        if !self.is_tool_available("tinygo") {
            return Err("tinygo not found in PATH".to_string());
        }

        Ok(())
    }

    fn get_missing_tools_message(&self) -> String {
        "⚠️  TinyGo development environment is not properly set up.\n".to_string()
            + "   Please ensure the following are installed:\n"
            + "   - TinyGo: https://tinygo.org/getting-started/install/\n"
            + "   - Go toolchain\n"
            + "   - For monitoring: screen, minicom, or other serial terminal\n"
            + "   Press Enter to continue anyway, or 'q' to quit."
    }
}

impl TinyGoHandler {
    fn has_tinygo_imports(&self, project_dir: &Path) -> bool {
        let tinygo_imports = [
            "\"machine\"",
            "\"device/",
            "\"runtime/interrupt\"",
            "\"tinygo.org/x/",
            "machine.",
            "interrupt.",
        ];

        if let Ok(entries) = project_dir.read_dir() {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().map_or(false, |ext| ext == "go") {
                    if let Ok(content) = fs::read_to_string(&path) {
                        for import in &tinygo_imports {
                            if content.contains(import) {
                                return true;
                            }
                        }
                    }
                }
            }
        }

        false
    }

    fn find_go_files(&self, project_dir: &Path) -> Result<Vec<PathBuf>> {
        let mut go_files = Vec::new();

        if let Ok(entries) = project_dir.read_dir() {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() && path.extension().map_or(false, |ext| ext == "go") {
                    go_files.push(path);
                }
            }
        }

        Ok(go_files)
    }

    fn detect_boards_from_files(
        &self,
        project_dir: &Path,
        go_files: &[PathBuf],
    ) -> Result<Vec<ProjectBoardConfig>> {
        let mut boards = Vec::new();

        // Look for board-specific configurations in comments or build tags
        for go_file in go_files {
            if let Ok(content) = fs::read_to_string(go_file) {
                // Check for board hints in build tags or comments
                let target = if content.contains("esp32-c6") || content.contains("ESP32-C6") {
                    ("esp32-c6-generic", "ESP32-C6")
                } else if content.contains("esp32s3") || content.contains("ESP32-S3") {
                    ("esp32-s3-usb-otg", "ESP32-S3")
                } else if content.contains("esp32-c3") || content.contains("ESP32-C3") {
                    ("esp32-c3-mini", "ESP32-C3")
                } else if content.contains("esp32") || content.contains("ESP32") {
                    ("esp32-coreboard-v2", "ESP32")
                } else {
                    continue; // Skip if no specific target found
                };

                // Avoid duplicates
                if !boards
                    .iter()
                    .any(|b: &ProjectBoardConfig| b.name == target.0)
                {
                    boards.push(ProjectBoardConfig {
                        name: target.0.to_string(),
                        config_file: go_file.clone(),
                        build_dir: project_dir.to_path_buf(),
                        target: Some(target.1.to_string()),
                        project_type: ProjectType::TinyGo,
                    });
                }
            }
        }

        Ok(boards)
    }

    fn find_build_artifacts(
        &self,
        project_dir: &Path,
        _board_config: &ProjectBoardConfig,
    ) -> Result<Vec<BuildArtifact>> {
        let mut artifacts = Vec::new();

        // TinyGo typically produces firmware.bin
        let firmware_bin = project_dir.join("firmware.bin");
        if firmware_bin.exists() {
            artifacts.push(BuildArtifact {
                name: "firmware".to_string(),
                file_path: firmware_bin,
                artifact_type: ArtifactType::Binary,
                offset: Some(0x10000), // Default app offset for ESP32
            });
        }

        // Look for firmware.elf
        let firmware_elf = project_dir.join("firmware.elf");
        if firmware_elf.exists() {
            artifacts.push(BuildArtifact {
                name: "firmware".to_string(),
                file_path: firmware_elf,
                artifact_type: ArtifactType::Elf,
                offset: None,
            });
        }

        // Look for main executable (no extension)
        let main_executable = project_dir.join("main");
        if main_executable.exists() {
            artifacts.push(BuildArtifact {
                name: "main".to_string(),
                file_path: main_executable,
                artifact_type: ArtifactType::Elf,
                offset: None,
            });
        }

        if artifacts.is_empty() {
            return Err(anyhow::anyhow!(
                "No build artifacts found in {}. Build the project first.",
                project_dir.display()
            ));
        }

        Ok(artifacts)
    }

    async fn monitor_with_screen(
        &self,
        _project_dir: &Path,
        board_config: &ProjectBoardConfig,
        port: Option<&str>,
        baud_rate: u32,
        tx: mpsc::UnboundedSender<AppEvent>,
    ) -> Result<()> {
        let port_str = port.unwrap_or("/dev/ttyUSB0");

        let _ = tx.send(AppEvent::BuildOutput(
            board_config.name.clone(),
            format!(
                "📺 Starting screen session: screen {} {}",
                port_str, baud_rate
            ),
        ));

        let mut cmd = Command::new("screen");
        cmd.args([port_str, &baud_rate.to_string()])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        let mut child = cmd.spawn().context("Failed to start screen")?;
        let _ = child.wait().await.context("Failed to wait for screen")?;

        Ok(())
    }

    async fn monitor_with_minicom(
        &self,
        _project_dir: &Path,
        board_config: &ProjectBoardConfig,
        port: Option<&str>,
        baud_rate: u32,
        tx: mpsc::UnboundedSender<AppEvent>,
    ) -> Result<()> {
        let port_str = port.unwrap_or("/dev/ttyUSB0");

        let _ = tx.send(AppEvent::BuildOutput(
            board_config.name.clone(),
            format!(
                "📺 Starting minicom session: minicom -D {} -b {}",
                port_str, baud_rate
            ),
        ));

        let mut cmd = Command::new("minicom");
        cmd.args(["-D", port_str, "-b", &baud_rate.to_string()])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        let mut child = cmd.spawn().context("Failed to start minicom")?;
        let _ = child.wait().await.context("Failed to wait for minicom")?;

        Ok(())
    }

    fn is_tool_available(&self, tool: &str) -> bool {
        std::process::Command::new("which")
            .arg(tool)
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }
}
