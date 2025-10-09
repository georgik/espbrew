use crate::models::{AppEvent, ArtifactType, BuildArtifact, ProjectBoardConfig, ProjectType};
use crate::projects::registry::ProjectHandler;

use anyhow::{Context, Result};
use async_trait::async_trait;
use std::fs;
use std::path::{Path, PathBuf};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;

/// Handler for PlatformIO projects
pub struct PlatformIOHandler;

#[async_trait]
impl ProjectHandler for PlatformIOHandler {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn project_type(&self) -> ProjectType {
        ProjectType::PlatformIO
    }

    fn can_handle(&self, project_dir: &Path) -> bool {
        project_dir.join("platformio.ini").exists()
    }

    fn discover_boards(&self, project_dir: &Path) -> Result<Vec<ProjectBoardConfig>> {
        let platformio_ini = project_dir.join("platformio.ini");
        if !platformio_ini.exists() {
            return Ok(Vec::new());
        }

        let mut boards = Vec::new();
        let content = fs::read_to_string(&platformio_ini)?;

        // Parse [env:board_name] sections
        let mut current_env: Option<String> = None;
        let mut current_board: Option<String> = None;
        let mut current_platform: Option<String> = None;

        for line in content.lines() {
            let line = line.trim();

            // Check for environment section
            if line.starts_with("[env:") && line.ends_with("]") {
                // Save previous environment if it exists
                if let Some(env_name) = current_env.take() {
                    let build_dir = project_dir.join(".pio").join("build").join(&env_name);
                    let target = current_board
                        .as_ref()
                        .map(|board| self.board_to_target(board))
                        .unwrap_or_else(|| "Unknown".to_string());

                    boards.push(ProjectBoardConfig {
                        name: env_name,
                        config_file: platformio_ini.clone(),
                        build_dir,
                        target: Some(target),
                        project_type: ProjectType::PlatformIO,
                    });
                }

                // Extract environment name
                current_env = line
                    .strip_prefix("[env:")
                    .and_then(|s| s.strip_suffix("]"))
                    .map(|s| s.to_string());
                current_board = None;
                current_platform = None;
            }
            // Parse board parameter
            else if line.starts_with("board = ") && current_env.is_some() {
                current_board = line.strip_prefix("board = ").map(|s| s.to_string());
            }
            // Parse platform parameter
            else if line.starts_with("platform = ") && current_env.is_some() {
                current_platform = line.strip_prefix("platform = ").map(|s| s.to_string());
            }
        }

        // Handle the last environment
        if let Some(env_name) = current_env {
            let build_dir = project_dir.join(".pio").join("build").join(&env_name);
            let target = current_board
                .as_ref()
                .map(|board| self.board_to_target(board))
                .unwrap_or_else(|| "Unknown".to_string());

            boards.push(ProjectBoardConfig {
                name: env_name,
                config_file: platformio_ini,
                build_dir,
                target: Some(target),
                project_type: ProjectType::PlatformIO,
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
            "🏗️  Starting PlatformIO build...".to_string(),
        ));

        let build_command = self.get_build_command(project_dir, board_config);
        let _ = tx.send(AppEvent::BuildOutput(
            board_config.name.clone(),
            format!("🔨 Executing: {}", build_command),
        ));

        // Build with PlatformIO
        let mut cmd = Command::new("pio");
        cmd.current_dir(project_dir)
            .args(["run", "-e", &board_config.name])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        let mut child = cmd.spawn().context("Failed to start pio run")?;
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

        let status = child.wait().await.context("Failed to wait for pio run")?;

        if status.success() {
            let _ = tx.send(AppEvent::BuildOutput(
                board_config.name.clone(),
                "✅ PlatformIO build completed successfully".to_string(),
            ));

            // Find build artifacts
            self.find_build_artifacts(project_dir, board_config)
        } else {
            let _ = tx.send(AppEvent::BuildOutput(
                board_config.name.clone(),
                "❌ PlatformIO build failed".to_string(),
            ));
            Err(anyhow::anyhow!("PlatformIO build failed"))
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
            "🔥 Starting PlatformIO flash...".to_string(),
        ));

        let flash_command = self.get_flash_command(project_dir, board_config, port);
        let _ = tx.send(AppEvent::BuildOutput(
            board_config.name.clone(),
            format!("🔨 Executing: {}", flash_command),
        ));

        let mut cmd = Command::new("pio");
        cmd.current_dir(project_dir)
            .args(["run", "-e", &board_config.name, "--target", "upload"]);

        if let Some(port) = port {
            cmd.args(["--upload-port", port]);
        }

        cmd.stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        let mut child = cmd.spawn().context("Failed to start pio run upload")?;
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
            .context("Failed to wait for pio upload")?;

        if status.success() {
            let _ = tx.send(AppEvent::BuildOutput(
                board_config.name.clone(),
                "✅ PlatformIO flash completed successfully".to_string(),
            ));
            Ok(())
        } else {
            let _ = tx.send(AppEvent::BuildOutput(
                board_config.name.clone(),
                "❌ PlatformIO flash failed".to_string(),
            ));
            Err(anyhow::anyhow!("PlatformIO flash failed"))
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
                "📺 Starting PlatformIO monitor on {} at {} baud",
                port.unwrap_or("auto-detect"),
                baud_rate
            ),
        ));

        let mut cmd = Command::new("pio");
        cmd.current_dir(project_dir)
            .args(["device", "monitor", "-e", &board_config.name]);

        if let Some(port) = port {
            cmd.args(["--port", port]);
        }

        cmd.args(["--baud", &baud_rate.to_string()])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        let mut child = cmd.spawn().context("Failed to start pio device monitor")?;
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
            .context("Failed to wait for pio device monitor")?;

        if status.success() {
            let _ = tx.send(AppEvent::BuildOutput(
                board_config.name.clone(),
                "✅ PlatformIO monitoring session completed".to_string(),
            ));
        } else {
            let _ = tx.send(AppEvent::BuildOutput(
                board_config.name.clone(),
                "❌ PlatformIO monitoring failed".to_string(),
            ));
        }

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
            "🧹 Cleaning PlatformIO build artifacts...".to_string(),
        ));

        let mut cmd = Command::new("pio");
        cmd.current_dir(project_dir)
            .args(["run", "-e", &board_config.name, "--target", "clean"]);

        let output = cmd.output().await.context("Failed to run pio clean")?;

        if output.status.success() {
            let _ = tx.send(AppEvent::BuildOutput(
                board_config.name.clone(),
                "✅ Clean completed successfully".to_string(),
            ));
            Ok(())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let _ = tx.send(AppEvent::BuildOutput(
                board_config.name.clone(),
                format!("❌ Clean failed: {}", stderr.trim()),
            ));
            Err(anyhow::anyhow!("PlatformIO clean failed"))
        }
    }

    fn get_build_command(&self, project_dir: &Path, board_config: &ProjectBoardConfig) -> String {
        if std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")) != *project_dir {
            format!(
                "cd {} && pio run -e {}",
                project_dir.display(),
                board_config.name
            )
        } else {
            format!("pio run -e {}", board_config.name)
        }
    }

    fn get_flash_command(
        &self,
        project_dir: &Path,
        board_config: &ProjectBoardConfig,
        port: Option<&str>,
    ) -> String {
        let port_arg = port
            .map(|p| format!(" --upload-port {}", p))
            .unwrap_or_default();

        if std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")) != *project_dir {
            format!(
                "cd {} && pio run -e {} --target upload{}",
                project_dir.display(),
                board_config.name,
                port_arg
            )
        } else {
            format!(
                "pio run -e {} --target upload{}",
                board_config.name, port_arg
            )
        }
    }

    fn check_tools_available(&self) -> Result<(), String> {
        // Check for pio command
        if !self.is_tool_available("pio") {
            return Err("pio (PlatformIO Core) not found in PATH".to_string());
        }

        Ok(())
    }

    fn get_missing_tools_message(&self) -> String {
        "⚠️  PlatformIO development environment is not properly set up.\n".to_string()
            + "   Please ensure the following are installed:\n"
            + "   - PlatformIO Core: https://platformio.org/install/cli\n"
            + "   - Install with: pip install platformio\n"
            + "   - Or via installer: curl -fsSL https://raw.githubusercontent.com/platformio/platformio-core/develop/scripts/get-platformio.py | python3\n"
            + "   Press Enter to continue anyway, or 'q' to quit."
    }
}

impl PlatformIOHandler {
    fn is_tool_available(&self, tool: &str) -> bool {
        std::process::Command::new("which")
            .arg(tool)
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    fn board_to_target(&self, board: &str) -> String {
        // Map common PlatformIO board names to ESP32 targets
        if board.contains("esp32s3") {
            "ESP32-S3".to_string()
        } else if board.contains("esp32c6") {
            "ESP32-C6".to_string()
        } else if board.contains("esp32c3") {
            "ESP32-C3".to_string()
        } else if board.contains("esp32p4") {
            "ESP32-P4".to_string()
        } else if board.contains("esp32") {
            "ESP32".to_string()
        } else {
            board.to_string()
        }
    }

    fn find_build_artifacts(
        &self,
        project_dir: &Path,
        board_config: &ProjectBoardConfig,
    ) -> Result<Vec<BuildArtifact>> {
        let mut artifacts = Vec::new();

        // PlatformIO build artifacts are in .pio/build/{environment}/
        let build_dir = project_dir
            .join(".pio")
            .join("build")
            .join(&board_config.name);

        if !build_dir.exists() {
            return Err(anyhow::anyhow!(
                "No build directory found in {}. Build the project first.",
                build_dir.display()
            ));
        }

        // Look for firmware.bin (common PlatformIO output)
        let firmware_bin = build_dir.join("firmware.bin");
        if firmware_bin.exists() {
            artifacts.push(BuildArtifact {
                name: "firmware".to_string(),
                file_path: firmware_bin,
                artifact_type: ArtifactType::Binary,
                offset: Some(0x10000), // Default app offset
            });
        }

        // Look for bootloader.bin
        let bootloader_bin = build_dir.join("bootloader.bin");
        if bootloader_bin.exists() {
            artifacts.push(BuildArtifact {
                name: "bootloader".to_string(),
                file_path: bootloader_bin,
                artifact_type: ArtifactType::Bootloader,
                offset: Some(0x0),
            });
        }

        // Look for partitions.bin
        let partitions_bin = build_dir.join("partitions.bin");
        if partitions_bin.exists() {
            artifacts.push(BuildArtifact {
                name: "partitions".to_string(),
                file_path: partitions_bin,
                artifact_type: ArtifactType::PartitionTable,
                offset: Some(0x8000),
            });
        }

        // Look for .elf file
        if let Ok(entries) = build_dir.read_dir() {
            for entry in entries.flatten() {
                if let Some(extension) = entry.path().extension() {
                    if extension == "elf" {
                        artifacts.push(BuildArtifact {
                            name: "application".to_string(),
                            file_path: entry.path(),
                            artifact_type: ArtifactType::Elf,
                            offset: None,
                        });
                        break;
                    }
                }
            }
        }

        if artifacts.is_empty() {
            return Err(anyhow::anyhow!(
                "No build artifacts found in {}. Build the project first.",
                build_dir.display()
            ));
        }

        Ok(artifacts)
    }
}
