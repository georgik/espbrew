use crate::AppEvent;
use crate::project::{ArtifactType, BoardConfig, BuildArtifact, ProjectHandler, ProjectType};
use anyhow::{Context, Result};
use async_trait::async_trait;
use std::fs;
use std::path::{Path, PathBuf};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;

/// Handler for NuttX RTOS projects
pub struct NuttXHandler;

#[async_trait]
impl ProjectHandler for NuttXHandler {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn project_type(&self) -> ProjectType {
        ProjectType::NuttX
    }

    fn can_handle(&self, project_dir: &Path) -> bool {
        // Look for NuttX-specific files: .config, defconfig, Makefile, and nuttx directory
        let config_file = project_dir.join(".config");
        let defconfig = project_dir.join("defconfig");
        let makefile = project_dir.join("Makefile");
        let nuttx_dir = project_dir.join("nuttx");

        // Check for basic NuttX structure
        let has_config = config_file.exists() || defconfig.exists();
        let has_makefile = makefile.exists();
        let has_nuttx_dir = nuttx_dir.is_dir();

        // Also check for NuttX-specific content in Makefile
        let has_nuttx_makefile = if makefile.exists() {
            if let Ok(content) = fs::read_to_string(&makefile) {
                content.contains("TOPDIR")
                    || content.contains("nuttx")
                    || content.contains("CONFIG_")
            } else {
                false
            }
        } else {
            false
        };

        (has_config && has_makefile) || has_nuttx_dir || has_nuttx_makefile
    }

    fn discover_boards(&self, project_dir: &Path) -> Result<Vec<BoardConfig>> {
        let mut boards = Vec::new();

        // Look for board configurations in various places:
        // 1. .config file for current configuration
        // 2. configs/ directory for available board configurations
        // 3. defconfig files

        // Check .config for current board configuration
        let config_file = project_dir.join(".config");
        if config_file.exists() {
            if let Ok(content) = fs::read_to_string(&config_file) {
                let detected_boards = self.detect_boards_from_config(&content, project_dir)?;
                boards.extend(detected_boards);
            }
        }

        // Look for configs directory with board definitions
        let configs_dir = project_dir.join("configs");
        if configs_dir.is_dir() {
            let config_boards = self.find_board_configurations(&configs_dir, project_dir)?;
            boards.extend(config_boards);
        }

        // Look in nuttx/configs if present
        let nuttx_configs_dir = project_dir.join("nuttx").join("configs");
        if nuttx_configs_dir.is_dir() {
            let nuttx_config_boards =
                self.find_board_configurations(&nuttx_configs_dir, project_dir)?;
            boards.extend(nuttx_config_boards);
        }

        // If no specific boards found, create a default ESP32 configuration
        if boards.is_empty() {
            let config_file = if config_file.exists() {
                config_file
            } else {
                project_dir.join("defconfig")
            };

            boards.push(BoardConfig {
                name: "esp32-core".to_string(),
                config_file,
                build_dir: project_dir.to_path_buf(),
                target: Some("ESP32".to_string()),
                project_type: ProjectType::NuttX,
            });
        }

        boards.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(boards)
    }

    async fn build_board(
        &self,
        project_dir: &Path,
        board_config: &BoardConfig,
        tx: mpsc::UnboundedSender<AppEvent>,
    ) -> Result<Vec<BuildArtifact>> {
        let _ = tx.send(AppEvent::BuildOutput(
            board_config.name.clone(),
            "🏗️  Starting NuttX build...".to_string(),
        ));

        let build_command = self.get_build_command(project_dir, board_config);
        let _ = tx.send(AppEvent::BuildOutput(
            board_config.name.clone(),
            format!("🔨 Executing: {}", build_command),
        ));

        // Build with make
        let mut cmd = Command::new("make");
        cmd.current_dir(project_dir)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        let mut child = cmd.spawn().context("Failed to start make")?;
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

        let status = child.wait().await.context("Failed to wait for make")?;

        if status.success() {
            let _ = tx.send(AppEvent::BuildOutput(
                board_config.name.clone(),
                "✅ NuttX build completed successfully".to_string(),
            ));

            // Find build artifacts
            self.find_build_artifacts(project_dir, board_config)
        } else {
            let _ = tx.send(AppEvent::BuildOutput(
                board_config.name.clone(),
                "❌ NuttX build failed".to_string(),
            ));
            Err(anyhow::anyhow!("NuttX build failed"))
        }
    }

    async fn flash_board(
        &self,
        project_dir: &Path,
        board_config: &BoardConfig,
        artifacts: &[BuildArtifact],
        port: Option<&str>,
        tx: mpsc::UnboundedSender<AppEvent>,
    ) -> Result<()> {
        let _ = tx.send(AppEvent::BuildOutput(
            board_config.name.clone(),
            "🔥 Starting NuttX flash...".to_string(),
        ));

        // NuttX flashing depends on the target board
        // For ESP32, we'll use esptool
        if board_config.name.contains("esp32") {
            self.flash_esp32(project_dir, board_config, artifacts, port, tx)
                .await
        } else {
            // For other boards, try generic flash command or provide guidance
            let _ = tx.send(AppEvent::BuildOutput(
                board_config.name.clone(),
                format!(
                    "⚠️  Flash method for {} not implemented. Please flash manually.",
                    board_config.name
                ),
            ));

            let _ = tx.send(AppEvent::BuildOutput(
                board_config.name.clone(),
                "💡 Check NuttX documentation for your specific board flashing instructions."
                    .to_string(),
            ));

            Ok(())
        }
    }

    async fn monitor_board(
        &self,
        project_dir: &Path,
        board_config: &BoardConfig,
        port: Option<&str>,
        baud_rate: u32,
        tx: mpsc::UnboundedSender<AppEvent>,
    ) -> Result<()> {
        let _ = tx.send(AppEvent::BuildOutput(
            board_config.name.clone(),
            format!(
                "📺 Starting NuttX console monitor on {} at {} baud",
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
        board_config: &BoardConfig,
        tx: mpsc::UnboundedSender<AppEvent>,
    ) -> Result<()> {
        let _ = tx.send(AppEvent::BuildOutput(
            board_config.name.clone(),
            "🧹 Cleaning NuttX build artifacts...".to_string(),
        ));

        let mut cmd = Command::new("make");
        cmd.current_dir(project_dir).args(["clean"]);

        let output = cmd.output().await.context("Failed to run make clean")?;

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
            Err(anyhow::anyhow!("NuttX clean failed"))
        }
    }

    fn get_build_command(&self, project_dir: &Path, board_config: &BoardConfig) -> String {
        if std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")) != *project_dir {
            format!("cd {} && make", project_dir.display())
        } else {
            "make".to_string()
        }
    }

    fn get_flash_command(
        &self,
        project_dir: &Path,
        board_config: &BoardConfig,
        port: Option<&str>,
    ) -> String {
        let port_str = port.unwrap_or("/dev/ttyUSB0");
        let project_dir_str =
            if std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")) != *project_dir {
                format!("cd {} && ", project_dir.display())
            } else {
                String::new()
            };

        if board_config.name.contains("esp32") {
            format!(
                "{}esptool.py --chip esp32 --port {} --baud 921600 write_flash -z 0x1000 nuttx.bin",
                project_dir_str, port_str
            )
        } else {
            format!(
                "{}# Flash command depends on target board - check NuttX documentation",
                project_dir_str
            )
        }
    }

    fn check_tools_available(&self) -> Result<(), String> {
        // Check for make command (essential for NuttX)
        if !self.is_tool_available("make") {
            return Err("make not found in PATH".to_string());
        }

        Ok(())
    }

    fn get_missing_tools_message(&self) -> String {
        "⚠️  NuttX development environment is not properly set up.\n".to_string()
            + "   Please ensure the following are installed:\n"
            + "   - NuttX toolchain for your target architecture\n"
            + "   - make (build system)\n"
            + "   - For ESP32: esptool.py (pip install esptool)\n"
            + "   - For monitoring: screen, minicom, or other serial terminal\n"
            + "   - Check: https://nuttx.apache.org/docs/latest/quickstart/install.html\n"
            + "   Press Enter to continue anyway, or 'q' to quit."
    }
}

impl NuttXHandler {
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
    ) -> Result<Vec<BoardConfig>> {
        let mut boards = Vec::new();

        // Look for board-specific configuration hints in .config
        for line in config_content.lines() {
            if line.starts_with("CONFIG_ARCH_BOARD=") {
                if let Some(board_name) = line.split('=').nth(1) {
                    let board_name = board_name.trim_matches('"');
                    let target = self.board_to_target(board_name);

                    boards.push(BoardConfig {
                        name: board_name.to_string(),
                        config_file: project_dir.join(".config"),
                        build_dir: project_dir.to_path_buf(),
                        target: Some(target),
                        project_type: ProjectType::NuttX,
                    });
                    break; // Usually only one board per config
                }
            }
        }

        Ok(boards)
    }

    fn find_board_configurations(
        &self,
        configs_dir: &Path,
        project_dir: &Path,
    ) -> Result<Vec<BoardConfig>> {
        let mut boards = Vec::new();

        if let Ok(entries) = configs_dir.read_dir() {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    if let Some(board_name) = path.file_name().and_then(|n| n.to_str()) {
                        // Look for defconfig in this board directory
                        let defconfig_path = path.join("defconfig");
                        let config_file = if defconfig_path.exists() {
                            defconfig_path
                        } else {
                            project_dir.join(".config")
                        };

                        let target = self.board_to_target(board_name);

                        boards.push(BoardConfig {
                            name: board_name.to_string(),
                            config_file,
                            build_dir: project_dir.to_path_buf(),
                            target: Some(target),
                            project_type: ProjectType::NuttX,
                        });
                    }
                }
            }
        }

        Ok(boards)
    }

    fn board_to_target(&self, board_name: &str) -> String {
        // Map NuttX board names to ESP32 targets
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
        board_config: &BoardConfig,
    ) -> Result<Vec<BuildArtifact>> {
        let mut artifacts = Vec::new();

        // NuttX typically produces nuttx.bin and nuttx.elf in the root directory
        let nuttx_bin = project_dir.join("nuttx.bin");
        if nuttx_bin.exists() {
            artifacts.push(BuildArtifact {
                name: "nuttx".to_string(),
                file_path: nuttx_bin,
                artifact_type: ArtifactType::Binary,
                offset: Some(0x1000), // Common offset for ESP32
            });
        }

        let nuttx_elf = project_dir.join("nuttx");
        if nuttx_elf.exists() {
            artifacts.push(BuildArtifact {
                name: "nuttx".to_string(),
                file_path: nuttx_elf,
                artifact_type: ArtifactType::Elf,
                offset: None,
            });
        }

        // Also check in nuttx subdirectory if it exists
        let nuttx_dir = project_dir.join("nuttx");
        if nuttx_dir.is_dir() {
            let nuttx_bin_sub = nuttx_dir.join("nuttx.bin");
            if nuttx_bin_sub.exists()
                && !artifacts
                    .iter()
                    .any(|a| a.name == "nuttx" && a.artifact_type == ArtifactType::Binary)
            {
                artifacts.push(BuildArtifact {
                    name: "nuttx".to_string(),
                    file_path: nuttx_bin_sub,
                    artifact_type: ArtifactType::Binary,
                    offset: Some(0x1000),
                });
            }

            let nuttx_elf_sub = nuttx_dir.join("nuttx");
            if nuttx_elf_sub.exists()
                && !artifacts
                    .iter()
                    .any(|a| a.name == "nuttx" && a.artifact_type == ArtifactType::Elf)
            {
                artifacts.push(BuildArtifact {
                    name: "nuttx".to_string(),
                    file_path: nuttx_elf_sub,
                    artifact_type: ArtifactType::Elf,
                    offset: None,
                });
            }
        }

        if artifacts.is_empty() {
            return Err(anyhow::anyhow!(
                "No build artifacts found in {}. Build the project first.",
                project_dir.display()
            ));
        }

        Ok(artifacts)
    }

    async fn flash_esp32(
        &self,
        project_dir: &Path,
        board_config: &BoardConfig,
        artifacts: &[BuildArtifact],
        port: Option<&str>,
        tx: mpsc::UnboundedSender<AppEvent>,
    ) -> Result<()> {
        let port_str = port.unwrap_or("/dev/ttyUSB0");

        // Find the binary artifact to flash
        let binary_artifact = artifacts
            .iter()
            .find(|a| a.artifact_type == ArtifactType::Binary)
            .ok_or_else(|| anyhow::anyhow!("No binary artifact found for flashing"))?;

        let _ = tx.send(AppEvent::BuildOutput(
            board_config.name.clone(),
            format!(
                "📤 Flashing {} to ESP32",
                binary_artifact.file_path.display()
            ),
        ));

        let mut cmd = Command::new("esptool.py");
        cmd.current_dir(project_dir)
            .args(["--chip", "esp32"])
            .args(["--port", port_str])
            .args(["--baud", "921600"])
            .args(["write_flash", "-z"])
            .args([&format!("0x{:x}", binary_artifact.offset.unwrap_or(0x1000))])
            .args([&binary_artifact.file_path.to_string_lossy().to_string()])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        let mut child = cmd.spawn().context("Failed to start esptool.py")?;
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
            .context("Failed to wait for esptool.py")?;

        if status.success() {
            let _ = tx.send(AppEvent::BuildOutput(
                board_config.name.clone(),
                "✅ NuttX flash completed successfully".to_string(),
            ));
            Ok(())
        } else {
            let _ = tx.send(AppEvent::BuildOutput(
                board_config.name.clone(),
                "❌ NuttX flash failed".to_string(),
            ));
            Err(anyhow::anyhow!("NuttX flash failed"))
        }
    }

    async fn monitor_with_screen(
        &self,
        _project_dir: &Path,
        board_config: &BoardConfig,
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
        board_config: &BoardConfig,
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
