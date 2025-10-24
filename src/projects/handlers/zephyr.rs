use crate::models::{AppEvent, ArtifactType, BuildArtifact, ProjectBoardConfig, ProjectType};
use crate::projects::registry::ProjectHandler;

use anyhow::{Context, Result};
use async_trait::async_trait;
use std::fs;
use std::path::{Path, PathBuf};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;

/// Handler for Zephyr RTOS projects
pub struct ZephyrHandler;

#[async_trait]
impl ProjectHandler for ZephyrHandler {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn project_type(&self) -> ProjectType {
        ProjectType::Zephyr
    }

    fn check_artifacts_exist(
        &self,
        _project_dir: &Path,
        board_config: &ProjectBoardConfig,
    ) -> bool {
        // Check if build directory exists as basic artifact validation
        board_config.build_dir.exists()
    }

    fn can_handle(&self, project_dir: &Path) -> bool {
        // Look for prj.conf and CMakeLists.txt with Zephyr-specific content
        let prj_conf = project_dir.join("prj.conf");
        let cmake_lists = project_dir.join("CMakeLists.txt");

        if !prj_conf.exists() || !cmake_lists.exists() {
            return false;
        }

        // Check for Zephyr-specific content in CMakeLists.txt
        if let Ok(content) = fs::read_to_string(&cmake_lists) {
            content.contains("find_package(Zephyr") || content.contains("CONFIG_")
        } else {
            false
        }
    }

    fn discover_boards(&self, project_dir: &Path) -> Result<Vec<ProjectBoardConfig>> {
        let mut boards = Vec::new();

        // Look for board configurations in various places:
        // 1. boards/ directory with board definitions
        // 2. prj.conf for default board hints
        // 3. west.yml for board references

        // Check prj.conf for board hints
        let prj_conf = project_dir.join("prj.conf");
        if let Ok(content) = fs::read_to_string(&prj_conf) {
            let detected_boards = self.detect_boards_from_config(&content, project_dir)?;
            boards.extend(detected_boards);
        }

        // Look for board-specific configurations
        let boards_dir = project_dir.join("boards");
        if boards_dir.is_dir() {
            let board_configs = self.find_board_configurations(&boards_dir, project_dir)?;
            boards.extend(board_configs);
        }

        // If no specific boards found, create a default ESP32 configuration
        if boards.is_empty() {
            boards.push(ProjectBoardConfig {
                name: "esp32".to_string(),
                config_file: prj_conf,
                build_dir: project_dir.join("build"),
                target: Some("ESP32".to_string()),
                project_type: ProjectType::Zephyr,
            });
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
            "🏗️  Starting Zephyr build...".to_string(),
        ));

        let build_command = self.get_build_command(project_dir, board_config);
        let _ = tx.send(AppEvent::BuildOutput(
            board_config.name.clone(),
            format!("🔨 Executing: {}", build_command),
        ));

        // Build with west
        let mut cmd = Command::new("west");
        cmd.current_dir(project_dir)
            .args(["build", "-b", &board_config.name])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        let mut child = cmd.spawn().context("Failed to start west build")?;
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
            .context("Failed to wait for west build")?;

        if status.success() {
            let _ = tx.send(AppEvent::BuildOutput(
                board_config.name.clone(),
                "✅ Zephyr build completed successfully".to_string(),
            ));

            // Find build artifacts
            self.find_build_artifacts(project_dir, board_config)
        } else {
            let _ = tx.send(AppEvent::BuildOutput(
                board_config.name.clone(),
                "❌ Zephyr build failed".to_string(),
            ));
            Err(anyhow::anyhow!("Zephyr build failed"))
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
            "🔥 Starting Zephyr flash...".to_string(),
        ));

        let flash_command = self.get_flash_command(project_dir, board_config, port);
        let _ = tx.send(AppEvent::BuildOutput(
            board_config.name.clone(),
            format!("🔨 Executing: {}", flash_command),
        ));

        let mut cmd = Command::new("west");
        cmd.current_dir(project_dir).args(["flash"]);

        if let Some(port) = port {
            // Some boards support specifying the serial port
            if board_config.name.contains("esp32") {
                cmd.args(["--esp-device", port]);
            }
        }

        cmd.stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        let mut child = cmd.spawn().context("Failed to start west flash")?;
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
            .context("Failed to wait for west flash")?;

        if status.success() {
            let _ = tx.send(AppEvent::BuildOutput(
                board_config.name.clone(),
                "✅ Zephyr flash completed successfully".to_string(),
            ));
            Ok(())
        } else {
            let _ = tx.send(AppEvent::BuildOutput(
                board_config.name.clone(),
                "❌ Zephyr flash failed".to_string(),
            ));
            Err(anyhow::anyhow!("Zephyr flash failed"))
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
                "📺 Starting Zephyr console monitor on {} at {} baud",
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
            "🧹 Cleaning Zephyr build artifacts...".to_string(),
        ));

        // Remove build directory
        let build_dir = project_dir.join("build");
        if build_dir.exists() {
            fs::remove_dir_all(&build_dir).context("Failed to remove build directory")?;
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
                "cd {} && west build -b {}",
                project_dir.display(),
                board_config.name
            )
        } else {
            format!("west build -b {}", board_config.name)
        }
    }

    fn get_flash_command(
        &self,
        project_dir: &Path,
        board_config: &ProjectBoardConfig,
        port: Option<&str>,
    ) -> String {
        let port_arg = if let Some(port) = port {
            if board_config.name.contains("esp32") {
                format!(" --esp-device {}", port)
            } else {
                String::new()
            }
        } else {
            String::new()
        };

        if std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")) != *project_dir {
            format!("cd {} && west flash{}", project_dir.display(), port_arg)
        } else {
            format!("west flash{}", port_arg)
        }
    }

    fn check_tools_available(&self) -> Result<(), String> {
        // Check for west command
        if !self.is_tool_available("west") {
            return Err("west (Zephyr meta-tool) not found in PATH".to_string());
        }

        Ok(())
    }

    fn get_missing_tools_message(&self) -> String {
        "⚠️  Zephyr development environment is not properly set up.\n".to_string()
            + "   Please ensure the following are installed:\n"
            + "   - Zephyr SDK: https://docs.zephyrproject.org/latest/develop/getting_started/index.html\n"
            + "   - west: pip install west\n"
            + "   - CMake and other build tools\n"
            + "   - For monitoring: screen, minicom, or other serial terminal\n"
            + "   Press Enter to continue anyway, or 'q' to quit."
    }
}

impl ZephyrHandler {
    fn is_tool_available(&self, tool: &str) -> bool {
        std::process::Command::new("which")
            .arg(tool)
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    fn detect_boards_from_config(
        &self,
        config_content: &str,
        project_dir: &Path,
    ) -> Result<Vec<ProjectBoardConfig>> {
        let mut boards = Vec::new();

        // Look for board-specific configuration hints
        for line in config_content.lines() {
            if line.starts_with("CONFIG_BOARD_") || line.starts_with("#CONFIG_BOARD_") {
                if let Some(board_hint) = self.extract_board_from_config_line(line) {
                    let target = self.board_to_target(&board_hint);

                    boards.push(ProjectBoardConfig {
                        name: board_hint,
                        config_file: project_dir.join("prj.conf"),
                        build_dir: project_dir.join("build"),
                        target: Some(target),
                        project_type: ProjectType::Zephyr,
                    });
                }
            }
        }

        Ok(boards)
    }

    fn extract_board_from_config_line(&self, line: &str) -> Option<String> {
        // Extract board name from CONFIG_BOARD_BOARDNAME=y
        if let Some(pos) = line.find("CONFIG_BOARD_") {
            let after_prefix = &line[pos + 13..]; // Skip "CONFIG_BOARD_"
            if let Some(eq_pos) = after_prefix.find('=') {
                let board_name = &after_prefix[..eq_pos];
                return Some(board_name.to_lowercase());
            }
        }
        None
    }

    fn find_board_configurations(
        &self,
        boards_dir: &Path,
        project_dir: &Path,
    ) -> Result<Vec<ProjectBoardConfig>> {
        let mut boards = Vec::new();

        if let Ok(entries) = boards_dir.read_dir() {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    if let Some(board_name) = path.file_name().and_then(|n| n.to_str()) {
                        let target = self.board_to_target(board_name);

                        boards.push(ProjectBoardConfig {
                            name: board_name.to_string(),
                            config_file: project_dir.join("prj.conf"),
                            build_dir: project_dir.join("build"),
                            target: Some(target),
                            project_type: ProjectType::Zephyr,
                        });
                    }
                }
            }
        }

        Ok(boards)
    }

    fn board_to_target(&self, board_name: &str) -> String {
        // Map Zephyr board names to ESP32 targets
        if board_name.contains("esp32s3") {
            "ESP32-S3".to_string()
        } else if board_name.contains("esp32c6") {
            "ESP32-C6".to_string()
        } else if board_name.contains("esp32c3") {
            "ESP32-C3".to_string()
        } else if board_name.contains("esp32p4") {
            "ESP32-P4".to_string()
        } else if board_name.contains("esp32") {
            "ESP32".to_string()
        } else {
            board_name.to_uppercase()
        }
    }

    fn find_build_artifacts(
        &self,
        project_dir: &Path,
        _board_config: &ProjectBoardConfig,
    ) -> Result<Vec<BuildArtifact>> {
        let mut artifacts = Vec::new();

        // Zephyr build artifacts are in build/zephyr/
        let build_dir = project_dir.join("build").join("zephyr");

        if !build_dir.exists() {
            return Err(anyhow::anyhow!(
                "No build directory found in {}. Build the project first.",
                build_dir.display()
            ));
        }

        // Look for zephyr.bin
        let zephyr_bin = build_dir.join("zephyr.bin");
        if zephyr_bin.exists() {
            artifacts.push(BuildArtifact {
                name: "zephyr".to_string(),
                file_path: zephyr_bin,
                artifact_type: ArtifactType::Binary,
                offset: Some(0x10000), // Default app offset for ESP32
            });
        }

        // Look for zephyr.elf
        let zephyr_elf = build_dir.join("zephyr.elf");
        if zephyr_elf.exists() {
            artifacts.push(BuildArtifact {
                name: "zephyr".to_string(),
                file_path: zephyr_elf,
                artifact_type: ArtifactType::Elf,
                offset: None,
            });
        }

        // Look for merged.bin (for ESP32 targets)
        let merged_bin = build_dir.join("merged.bin");
        if merged_bin.exists() {
            artifacts.push(BuildArtifact {
                name: "merged".to_string(),
                file_path: merged_bin,
                artifact_type: ArtifactType::Binary,
                offset: Some(0x0),
            });
        }

        if artifacts.is_empty() {
            return Err(anyhow::anyhow!(
                "No build artifacts found in {}. Build the project first.",
                build_dir.display()
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
}
