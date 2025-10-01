use crate::AppEvent;
use crate::project::{ArtifactType, BoardConfig, BuildArtifact, ProjectHandler, ProjectType};
use anyhow::{Context, Result};
use async_trait::async_trait;
use glob::glob;
use std::{
    fs,
    path::{Path, PathBuf},
};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;

/// Handler for ESP-IDF projects with CMake build system
pub struct EspIdfHandler;

#[async_trait]
impl ProjectHandler for EspIdfHandler {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn project_type(&self) -> ProjectType {
        ProjectType::EspIdf
    }

    fn can_handle(&self, project_dir: &Path) -> bool {
        let cmake_file = project_dir.join("CMakeLists.txt");
        let sdkconfig_exists = project_dir.join("sdkconfig").exists()
            || project_dir
                .read_dir()
                .map(|mut entries| {
                    entries.any(|entry| {
                        entry
                            .map(|e| {
                                e.file_name()
                                    .to_string_lossy()
                                    .starts_with("sdkconfig.defaults")
                            })
                            .unwrap_or(false)
                    })
                })
                .unwrap_or(false);

        cmake_file.exists() && sdkconfig_exists
    }

    fn discover_boards(&self, project_dir: &Path) -> Result<Vec<BoardConfig>> {
        let pattern = project_dir.join("sdkconfig.defaults.*");
        let mut boards = Vec::new();

        for entry in glob(&pattern.to_string_lossy())? {
            let config_file = entry?;
            if let Some(file_name) = config_file.file_name() {
                if let Some(name) = file_name.to_str() {
                    if let Some(board_name) = name.strip_prefix("sdkconfig.defaults.") {
                        let build_dir = project_dir.join(format!("build.{}", board_name));
                        let target = self.determine_target(&config_file).ok();

                        boards.push(BoardConfig {
                            name: board_name.to_string(),
                            config_file: config_file.clone(),
                            build_dir,
                            target,
                            project_type: ProjectType::EspIdf,
                        });
                    }
                }
            }
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
            "🏗️  Starting ESP-IDF build...".to_string(),
        ));

        let build_command = self.get_build_command(project_dir, board_config);
        let _ = tx.send(AppEvent::BuildOutput(
            board_config.name.clone(),
            format!("🔨 Executing: {}", build_command),
        ));

        // First determine target
        let target = self.determine_target(&board_config.config_file)?;
        let config_path = board_config.config_file.to_string_lossy();

        // Use board-specific sdkconfig file to avoid conflicts
        let sdkconfig_path = board_config.build_dir.join("sdkconfig");

        // Set target command
        let mut cmd = Command::new("idf.py");
        cmd.current_dir(project_dir)
            .env("SDKCONFIG_DEFAULTS", &*config_path)
            .args([
                "-D",
                &format!("SDKCONFIG={}", sdkconfig_path.display()),
                "-B",
                &board_config.build_dir.to_string_lossy(),
                "set-target",
                &target,
            ])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        let output = cmd
            .output()
            .await
            .context("Failed to run idf.py set-target")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let _ = tx.send(AppEvent::BuildOutput(
                board_config.name.clone(),
                format!("❌ Set target failed: {}", stderr.trim()),
            ));
            return Err(anyhow::anyhow!("Failed to set target"));
        }

        // Build command with unbuffered output for real-time streaming
        let mut cmd = Command::new("idf.py");
        cmd.current_dir(project_dir)
            .env("SDKCONFIG_DEFAULTS", &*config_path)
            .env("PYTHONUNBUFFERED", "1") // Force Python to not buffer output
            .args([
                "-D",
                &format!("SDKCONFIG={}", sdkconfig_path.display()),
                "-B",
                &board_config.build_dir.to_string_lossy(),
                "build",
            ])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        let mut child = cmd.spawn().context("Failed to start idf.py build")?;
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
            .context("Failed to wait for idf.py build")?;

        if status.success() {
            let _ = tx.send(AppEvent::BuildOutput(
                board_config.name.clone(),
                "✅ ESP-IDF build completed successfully".to_string(),
            ));

            // Find build artifacts
            self.find_build_artifacts(project_dir, board_config)
        } else {
            let _ = tx.send(AppEvent::BuildOutput(
                board_config.name.clone(),
                "❌ ESP-IDF build failed".to_string(),
            ));
            Err(anyhow::anyhow!("ESP-IDF build failed"))
        }
    }

    async fn flash_board(
        &self,
        project_dir: &Path,
        board_config: &BoardConfig,
        _artifacts: &[BuildArtifact],
        port: Option<&str>,
        tx: mpsc::UnboundedSender<AppEvent>,
    ) -> Result<()> {
        let _ = tx.send(AppEvent::BuildOutput(
            board_config.name.clone(),
            "🔥 Starting ESP-IDF flash...".to_string(),
        ));

        let flash_command = self.get_flash_command(project_dir, board_config, port);
        let _ = tx.send(AppEvent::BuildOutput(
            board_config.name.clone(),
            format!("🔨 Executing: {}", flash_command),
        ));

        let config_path = board_config.config_file.to_string_lossy();
        let sdkconfig_path = board_config.build_dir.join("sdkconfig");

        let mut cmd = Command::new("idf.py");
        cmd.current_dir(project_dir)
            .env("SDKCONFIG_DEFAULTS", &*config_path)
            .args([
                "-D",
                &format!("SDKCONFIG={}", sdkconfig_path.display()),
                "-B",
                &board_config.build_dir.to_string_lossy(),
                "flash",
            ]);

        if let Some(port) = port {
            cmd.args(["-p", port]);
        }

        cmd.stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        let mut child = cmd.spawn().context("Failed to start idf.py flash")?;
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
            .context("Failed to wait for idf.py flash")?;

        if status.success() {
            let _ = tx.send(AppEvent::BuildOutput(
                board_config.name.clone(),
                "✅ ESP-IDF flash completed successfully".to_string(),
            ));
            Ok(())
        } else {
            let _ = tx.send(AppEvent::BuildOutput(
                board_config.name.clone(),
                "❌ ESP-IDF flash failed".to_string(),
            ));
            Err(anyhow::anyhow!("ESP-IDF flash failed"))
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
                "📺 Starting ESP-IDF monitor on {} at {} baud",
                port.unwrap_or("auto-detect"),
                baud_rate
            ),
        ));

        let config_path = board_config.config_file.to_string_lossy();
        let sdkconfig_path = board_config.build_dir.join("sdkconfig");

        let mut cmd = Command::new("idf.py");
        cmd.current_dir(project_dir)
            .env("SDKCONFIG_DEFAULTS", &*config_path)
            .args([
                "-D",
                &format!("SDKCONFIG={}", sdkconfig_path.display()),
                "-B",
                &board_config.build_dir.to_string_lossy(),
                "monitor",
            ]);

        if let Some(port) = port {
            cmd.args(["-p", port]);
        }

        cmd.stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        let mut child = cmd.spawn().context("Failed to start idf.py monitor")?;
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
            .context("Failed to wait for idf.py monitor")?;

        if status.success() {
            let _ = tx.send(AppEvent::BuildOutput(
                board_config.name.clone(),
                "✅ ESP-IDF monitoring session completed".to_string(),
            ));
        } else {
            let _ = tx.send(AppEvent::BuildOutput(
                board_config.name.clone(),
                "❌ ESP-IDF monitoring failed".to_string(),
            ));
        }

        Ok(())
    }

    async fn clean_board(
        &self,
        project_dir: &Path,
        board_config: &BoardConfig,
        tx: mpsc::UnboundedSender<AppEvent>,
    ) -> Result<()> {
        let _ = tx.send(AppEvent::BuildOutput(
            board_config.name.clone(),
            "🧹 Cleaning ESP-IDF build artifacts...".to_string(),
        ));

        let config_path = board_config.config_file.to_string_lossy();
        let sdkconfig_path = board_config.build_dir.join("sdkconfig");

        let mut cmd = Command::new("idf.py");
        cmd.current_dir(project_dir)
            .env("SDKCONFIG_DEFAULTS", &*config_path)
            .args([
                "-D",
                &format!("SDKCONFIG={}", sdkconfig_path.display()),
                "-B",
                &board_config.build_dir.to_string_lossy(),
                "clean",
            ]);

        let output = cmd.output().await.context("Failed to run idf.py clean")?;

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
            Err(anyhow::anyhow!("ESP-IDF clean failed"))
        }
    }

    fn get_build_command(&self, project_dir: &Path, board_config: &BoardConfig) -> String {
        let config_path = board_config.config_file.display();
        let build_dir = board_config.build_dir.display();
        let sdkconfig_file = board_config.build_dir.join("sdkconfig");
        let sdkconfig_path = sdkconfig_file.display();

        if std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")) != *project_dir {
            format!(
                "cd {} && SDKCONFIG_DEFAULTS='{}' idf.py -D SDKCONFIG='{}' -B '{}' build",
                project_dir.display(),
                config_path,
                sdkconfig_path,
                build_dir
            )
        } else {
            format!(
                "SDKCONFIG_DEFAULTS='{}' idf.py -D SDKCONFIG='{}' -B '{}' build",
                config_path, sdkconfig_path, build_dir
            )
        }
    }

    fn get_flash_command(
        &self,
        project_dir: &Path,
        board_config: &BoardConfig,
        port: Option<&str>,
    ) -> String {
        let config_path = board_config.config_file.display();
        let build_dir = board_config.build_dir.display();
        let sdkconfig_file = board_config.build_dir.join("sdkconfig");
        let sdkconfig_path = sdkconfig_file.display();

        let port_arg = port.map(|p| format!(" -p {}", p)).unwrap_or_default();

        if std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")) != *project_dir {
            format!(
                "cd {} && SDKCONFIG_DEFAULTS='{}' idf.py -D SDKCONFIG='{}' -B '{}' flash{}",
                project_dir.display(),
                config_path,
                sdkconfig_path,
                build_dir,
                port_arg
            )
        } else {
            format!(
                "SDKCONFIG_DEFAULTS='{}' idf.py -D SDKCONFIG='{}' -B '{}' flash{}",
                config_path, sdkconfig_path, build_dir, port_arg
            )
        }
    }

    fn check_tools_available(&self) -> Result<(), String> {
        // Check for idf.py
        if !self.is_tool_available("idf.py") {
            return Err(
                "idf.py not found in PATH. ESP-IDF environment not properly set up".to_string(),
            );
        }

        Ok(())
    }

    fn get_missing_tools_message(&self) -> String {
        "⚠️  ESP-IDF development environment is not properly set up.\n".to_string()
            + "   Please ensure the following are done:\n"
            + "   - ESP-IDF is installed: https://docs.espressif.com/projects/esp-idf/en/latest/get-started/\n"
            + "   - ESP-IDF environment is activated: source ~/esp/esp-idf/export.sh\n"
            + "   - idf.py is available in PATH\n"
            + "   Press Enter to continue anyway, or 'q' to quit."
    }
}

impl EspIdfHandler {
    fn is_tool_available(&self, tool: &str) -> bool {
        std::process::Command::new("which")
            .arg(tool)
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    fn determine_target(&self, config_file: &Path) -> Result<String> {
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

    fn find_build_artifacts(
        &self,
        _project_dir: &Path,
        board_config: &BoardConfig,
    ) -> Result<Vec<BuildArtifact>> {
        let mut artifacts = Vec::new();

        // Parse flash_args file to get all binaries and their offsets
        let flash_args_path = board_config.build_dir.join("flash_args");
        if flash_args_path.exists() {
            let (_, binaries) = self.parse_flash_args(&flash_args_path, &board_config.build_dir)?;
            for binary_info in binaries {
                let artifact_type = match binary_info.offset {
                    0x0 => ArtifactType::Bootloader,
                    0x8000 => ArtifactType::PartitionTable,
                    0x10000 => ArtifactType::Application,
                    _ => ArtifactType::Binary,
                };

                artifacts.push(BuildArtifact {
                    name: binary_info.name,
                    file_path: binary_info.file_path,
                    artifact_type,
                    offset: Some(binary_info.offset),
                });
            }
        } else {
            return Err(anyhow::anyhow!(
                "No flash_args file found in {}. Build the project first.",
                board_config.build_dir.display()
            ));
        }

        Ok(artifacts)
    }

    fn parse_flash_args(
        &self,
        flash_args_path: &Path,
        build_dir: &Path,
    ) -> Result<(FlashConfig, Vec<FlashBinaryInfo>)> {
        let content = fs::read_to_string(flash_args_path).context("Failed to read flash_args")?;

        let lines: Vec<&str> = content.lines().collect();
        if lines.is_empty() {
            return Err(anyhow::anyhow!("flash_args file is empty"));
        }

        // Parse first line for flash configuration
        let config_line = lines[0];
        let mut flash_mode = "dio".to_string();
        let mut flash_freq = "40m".to_string();
        let mut flash_size = "4MB".to_string();

        for part in config_line.split_whitespace() {
            if part.starts_with("--flash_mode") {
                if let Some(mode) = part.split(' ').nth(1) {
                    flash_mode = mode.to_string();
                }
            } else if part.starts_with("--flash_freq") {
                if let Some(freq) = part.split(' ').nth(1) {
                    flash_freq = freq.to_string();
                }
            } else if part.starts_with("--flash_size") {
                if let Some(size) = part.split(' ').nth(1) {
                    flash_size = size.to_string();
                }
            }
        }

        // Parse remaining lines for binary files
        let mut binaries = Vec::new();
        for line in lines.iter().skip(1) {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                let offset_str = parts[0];
                let file_path = parts[1];

                // Parse hex offset
                let offset = if offset_str.starts_with("0x") {
                    u32::from_str_radix(&offset_str[2..], 16).context("Invalid hex offset")?
                } else {
                    offset_str.parse::<u32>().context("Invalid offset")?
                };

                // Determine binary type based on offset
                let name = match offset {
                    0x0 => "bootloader".to_string(),
                    0x8000 => "partition_table".to_string(),
                    0x10000 => "application".to_string(),
                    _ => format!("binary_at_0x{:x}", offset),
                };

                let full_path = build_dir.join(file_path);
                binaries.push(FlashBinaryInfo {
                    offset,
                    file_path: full_path,
                    name,
                    file_name: file_path.to_string(),
                });
            }
        }

        let flash_config = FlashConfig {
            flash_mode,
            flash_freq,
            flash_size,
        };

        Ok((flash_config, binaries))
    }
}

/// Information about a binary to be flashed
#[derive(Debug, Clone)]
pub struct FlashBinaryInfo {
    pub offset: u32,
    pub file_path: PathBuf,
    pub name: String,
    pub file_name: String,
}

#[derive(Debug, Clone)]
pub struct FlashConfig {
    pub flash_mode: String,
    pub flash_freq: String,
    pub flash_size: String,
}
