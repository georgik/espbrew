use crate::AppEvent;
use crate::project::{ArtifactType, BoardConfig, BuildArtifact, ProjectHandler, ProjectType};
use anyhow::{Context, Result};
use async_trait::async_trait;
use std::path::Path;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;

/// Information about an ESP chip target
#[derive(Debug, Clone)]
struct ChipInfo {
    chip_name: String,
    display_name: String,
}

/// Information needed to build a specific board configuration
#[derive(Debug, Clone)]
struct BuildInfo {
    target: Option<String>,
    features: Vec<String>,
    config_file: Option<std::path::PathBuf>,
}

/// Handler for Rust no_std embedded projects
pub struct RustNoStdHandler;

#[async_trait]
impl ProjectHandler for RustNoStdHandler {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn project_type(&self) -> ProjectType {
        ProjectType::RustNoStd
    }

    fn can_handle(&self, project_dir: &Path) -> bool {
        let cargo_toml = project_dir.join("Cargo.toml");
        if !cargo_toml.exists() {
            return false;
        }

        // Check if it's an embedded Rust project
        if let Ok(content) = std::fs::read_to_string(&cargo_toml) {
            // Look for common embedded Rust dependencies
            content.contains("esp-hal")
                || content.contains("esp-backtrace")
                || content.contains("esp-println")
                || content.contains("embedded-hal")
                || (content.contains("no_std")
                    && (content.contains("esp32") || content.contains("esp")))
        } else {
            false
        }
    }

    fn discover_boards(&self, project_dir: &Path) -> Result<Vec<BoardConfig>> {
        let cargo_toml = project_dir.join("Cargo.toml");
        if !cargo_toml.exists() {
            return Ok(Vec::new());
        }

        let mut boards = Vec::new();
        let build_dir = project_dir.join("target");

        // First, try to discover boards from .cargo/config_*.toml files (multiconfig pattern)
        if let Ok(config_boards) = self.discover_boards_from_config_files(project_dir) {
            if !config_boards.is_empty() {
                boards.extend(config_boards);
            }
        }

        // Next, try to discover from cargo aliases in main config.toml (multitarget pattern)
        if boards.is_empty() {
            if let Ok(alias_boards) = self.discover_boards_from_cargo_aliases(project_dir) {
                if !alias_boards.is_empty() {
                    boards.extend(alias_boards);
                }
            }
        }

        // Check .cargo/config.toml for target configurations (legacy support)
        if boards.is_empty() {
            let cargo_config = project_dir.join(".cargo").join("config.toml");
            if cargo_config.exists() {
                if let Ok(targets) = self.parse_cargo_config_targets(&cargo_config) {
                    for (_target_name, chip_info) in targets {
                        let board_name = format!(
                            "{}-{}",
                            project_dir
                                .file_name()
                                .and_then(|n| n.to_str())
                                .unwrap_or("rust-project"),
                            chip_info.chip_name
                        );

                        boards.push(BoardConfig {
                            name: board_name,
                            config_file: cargo_toml.clone(),
                            build_dir: build_dir.clone(),
                            target: Some(chip_info.display_name),
                            project_type: ProjectType::RustNoStd,
                        });
                    }
                }
            }
        }

        // Fallback: create a single board configuration based on Cargo.toml
        if boards.is_empty() {
            let target_chip = self.detect_target_chip(&cargo_toml)?;
            let board_name = project_dir
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("rust-project")
                .to_string();

            boards.push(BoardConfig {
                name: board_name,
                config_file: cargo_toml,
                build_dir,
                target: Some(target_chip),
                project_type: ProjectType::RustNoStd,
            });
        }

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
            "🦀 Starting Rust no_std build...".to_string(),
        ));

        let build_command = self.get_build_command(project_dir, board_config);
        let _ = tx.send(AppEvent::BuildOutput(
            board_config.name.clone(),
            format!("🔨 Executing: {}", build_command),
        ));

        // Build using the proper command for this board configuration
        let mut cmd =
            if let Ok(build_info) = self.extract_build_info_from_board(project_dir, board_config) {
                let mut cmd = Command::new("cargo");
                cmd.current_dir(project_dir)
                    .args(["build", "--release"])
                    .stdout(std::process::Stdio::piped())
                    .stderr(std::process::Stdio::piped());

                // Add config file if it's not the default Cargo.toml
                if board_config.config_file != project_dir.join("Cargo.toml") {
                    if let Some(config_path_str) = board_config.config_file.to_str() {
                        cmd.args(["--config", config_path_str]);
                    }
                }

                // Add target if specified
                if let Some(ref target) = build_info.target {
                    cmd.args(["--target", target]);
                }

                // Add features if specified
                if !build_info.features.is_empty() {
                    cmd.args(["--features", &build_info.features.join(",")]);
                }

                cmd
            } else {
                // Fallback to simple build command
                let mut cmd = Command::new("cargo");
                cmd.current_dir(project_dir)
                    .args(["build", "--release"])
                    .stdout(std::process::Stdio::piped())
                    .stderr(std::process::Stdio::piped());
                cmd
            };

        let mut child = cmd.spawn().context("Failed to start cargo build")?;
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
            .context("Failed to wait for cargo build")?;

        if status.success() {
            let _ = tx.send(AppEvent::BuildOutput(
                board_config.name.clone(),
                "✅ Rust no_std build completed successfully".to_string(),
            ));

            // Find build artifacts
            self.find_build_artifacts(project_dir, board_config)
        } else {
            let _ = tx.send(AppEvent::BuildOutput(
                board_config.name.clone(),
                "❌ Rust no_std build failed".to_string(),
            ));
            Err(anyhow::anyhow!("Cargo build failed"))
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
            "🔥 Starting Rust no_std flash...".to_string(),
        ));

        // First, try to use existing artifacts if available
        let build_artifacts = if !artifacts.is_empty() {
            let _ = tx.send(AppEvent::BuildOutput(
                board_config.name.clone(),
                "📁 Using existing build artifacts".to_string(),
            ));
            artifacts.to_vec()
        } else {
            // Try to find existing build artifacts without building
            let _ = tx.send(AppEvent::BuildOutput(
                board_config.name.clone(),
                "🔍 Searching for existing build artifacts...".to_string(),
            ));

            match self.find_build_artifacts(project_dir, board_config) {
                Ok(found_artifacts) => {
                    let _ = tx.send(AppEvent::BuildOutput(
                        board_config.name.clone(),
                        format!("✅ Found {} existing artifact(s)", found_artifacts.len()),
                    ));
                    found_artifacts
                }
                Err(_) => {
                    // No existing artifacts found, need to build
                    let _ = tx.send(AppEvent::BuildOutput(
                        board_config.name.clone(),
                        "🔨 No existing binary found, building project...".to_string(),
                    ));
                    self.build_board(project_dir, board_config, tx.clone())
                        .await?
                }
            }
        };

        // Find the binary artifact to flash
        let binary_artifact = build_artifacts
            .iter()
            .find(|artifact| matches!(artifact.artifact_type, crate::project::ArtifactType::Binary))
            .or_else(|| {
                build_artifacts.iter().find(|artifact| {
                    matches!(artifact.artifact_type, crate::project::ArtifactType::Elf)
                })
            })
            .or_else(|| {
                build_artifacts.iter().find(|artifact| {
                    artifact
                        .file_path
                        .extension()
                        .map(|ext| ext.to_str().unwrap_or(""))
                        .unwrap_or("")
                        == "bin"
                })
            });

        let binary_path = if let Some(artifact) = binary_artifact {
            let _ = tx.send(AppEvent::BuildOutput(
                board_config.name.clone(),
                format!("✅ Using binary: {}", artifact.file_path.display()),
            ));
            artifact.file_path.clone()
        } else {
            let _ = tx.send(AppEvent::BuildOutput(
                board_config.name.clone(),
                "❌ No suitable binary artifact found in build artifacts".to_string(),
            ));
            return Err(anyhow::anyhow!(
                "No suitable binary found in build artifacts. Found {} artifacts: {:?}",
                build_artifacts.len(),
                build_artifacts
                    .iter()
                    .map(|a| &a.file_path)
                    .collect::<Vec<_>>()
            ));
        };

        // Use internal espflash for TUI-compatible flashing
        let tx_clone = tx.clone();
        let board_name_clone = board_config.name.clone();
        let port_clone = port.map(|s| s.to_string());

        let flash_result = tokio::spawn(async move {
            match Self::flash_binary_internal(&binary_path, port_clone.as_deref(), tx_clone.clone())
                .await
            {
                Ok(_) => {
                    let _ = tx_clone.send(AppEvent::BuildOutput(
                        board_name_clone.clone(),
                        "✅ Rust no_std flash completed successfully".to_string(),
                    ));
                    Ok(())
                }
                Err(e) => {
                    let _ = tx_clone.send(AppEvent::BuildOutput(
                        board_name_clone.clone(),
                        format!("❌ Rust no_std flash failed: {}", e),
                    ));
                    Err(e)
                }
            }
        })
        .await;

        match flash_result {
            Ok(result) => result,
            Err(e) => Err(anyhow::anyhow!("Flash task failed: {}", e)),
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
                "📺 Starting serial monitor on {} at {} baud",
                port.unwrap_or("auto-detect"),
                baud_rate
            ),
        ));

        // For Rust projects, we can use espflash monitor or cargo-espflash
        let mut cmd = Command::new("cargo");
        cmd.current_dir(project_dir)
            .args(["run", "--release"]) // This will flash and monitor
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        if let Some(port) = port {
            cmd.env("ESPFLASH_PORT", port);
        }

        let mut child = cmd
            .spawn()
            .context("Failed to start cargo run for monitoring")?;
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

        let status = child.wait().await.context("Failed to wait for cargo run")?;

        if status.success() {
            let _ = tx.send(AppEvent::BuildOutput(
                board_config.name.clone(),
                "✅ Serial monitoring session completed".to_string(),
            ));
        } else {
            let _ = tx.send(AppEvent::BuildOutput(
                board_config.name.clone(),
                "❌ Serial monitoring failed".to_string(),
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
            "🧹 Cleaning Rust build artifacts...".to_string(),
        ));

        let mut cmd = Command::new("cargo");
        cmd.current_dir(project_dir)
            .args(["clean"])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        let output = cmd.output().await.context("Failed to run cargo clean")?;

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
            Err(anyhow::anyhow!("Cargo clean failed"))
        }
    }

    fn get_build_command(&self, project_dir: &Path, board_config: &BoardConfig) -> String {
        let mut command = vec![
            "cargo".to_string(),
            "build".to_string(),
            "--release".to_string(),
        ];

        // Check if this board config uses a specific config file
        if board_config.config_file != project_dir.join("Cargo.toml") {
            // This is a config_*.toml file, add --config flag
            if let Some(config_path_str) = board_config.config_file.to_str() {
                command.push("--config".to_string());
                command.push(config_path_str.to_string());
            }
        }

        // Try to determine target and features from board name or config
        if let Ok(build_info) = self.extract_build_info_from_board(project_dir, board_config) {
            if let Some(target) = build_info.target {
                command.push("--target".to_string());
                command.push(target);
            }
            if !build_info.features.is_empty() {
                command.push("--features".to_string());
                command.push(build_info.features.join(","));
            }
        }

        command.join(" ")
    }

    fn get_flash_command(
        &self,
        _project_dir: &Path,
        _board_config: &BoardConfig,
        port: Option<&str>,
    ) -> String {
        if let Some(port) = port {
            format!("espflash flash --port {} --non-interactive <binary>", port)
        } else {
            "espflash flash --non-interactive <binary> (auto-detect port)".to_string()
        }
    }

    fn check_tools_available(&self) -> Result<(), String> {
        // Check for cargo
        if !self.is_tool_available("cargo") {
            return Err("cargo (Rust toolchain) not found in PATH".to_string());
        }

        // Check for espflash (used by cargo-espflash for flashing)
        if !self.is_tool_available("espflash") {
            return Err(
                "espflash not found in PATH. Install with: cargo install espflash".to_string(),
            );
        }

        Ok(())
    }

    fn get_missing_tools_message(&self) -> String {
        "⚠️  Rust embedded development tools are not properly set up.\n".to_string()
            + "   Please ensure the following are installed:\n"
            + "   - Rust toolchain (cargo): https://rustup.rs/\n"
            + "   - espflash: cargo install espflash\n"
            + "   - Required targets: rustup target add xtensa-esp32s3-none-elf (or similar)\n"
            + "   Press Enter to continue anyway, or 'q' to quit."
    }
}

impl RustNoStdHandler {
    /// Check if the required tools are available for a specific project
    pub fn check_tools_for_project(&self, project_dir: &Path) -> Result<(), String> {
        // First run the general tool checks
        self.check_tools_available()?;

        // Check if this project uses Xtensa architecture targets
        if self.project_uses_xtensa(project_dir) {
            // Check if esp toolchain is installed
            if !self.is_esp_toolchain_available() {
                return Err(
                    "Xtensa Rust toolchain not found. This project targets Xtensa architecture (ESP32/ESP32-S2/ESP32-S3).\n".to_string() +
                    "Please install the ESP Rust toolchain:\n" +
                    "  cargo install espup\n" +
                    "  espup install\n" +
                    "This will install the required Xtensa toolchain to ~/.rustup/toolchains/esp"
                );
            }
        }

        Ok(())
    }

    fn is_tool_available(&self, tool: &str) -> bool {
        std::process::Command::new("which")
            .arg(tool)
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    /// Check if the project uses Xtensa architecture targets (ESP32, ESP32-S2, ESP32-S3)
    fn project_uses_xtensa(&self, project_dir: &Path) -> bool {
        // Check .cargo/config.toml for Xtensa targets
        let cargo_config = project_dir.join(".cargo").join("config.toml");
        if cargo_config.exists() {
            if let Ok(content) = std::fs::read_to_string(&cargo_config) {
                if content.contains("xtensa-esp32-none-elf")
                    || content.contains("xtensa-esp32s2-none-elf")
                    || content.contains("xtensa-esp32s3-none-elf")
                {
                    return true;
                }
            }
        }

        // Check Cargo.toml for ESP32/S2/S3 indicators
        let cargo_toml = project_dir.join("Cargo.toml");
        if cargo_toml.exists() {
            if let Ok(content) = std::fs::read_to_string(&cargo_toml) {
                // Check for ESP32, ESP32-S2, ESP32-S3 (but not C3, C6, H2 which use RISC-V)
                if (content.contains("esp32")
                    && !content.contains("esp32c")
                    && !content.contains("esp32h"))
                    || content.contains("esp32s2")
                    || content.contains("esp32s3")
                {
                    return true;
                }
            }
        }

        false
    }

    /// Check if the ESP Rust toolchain is available
    fn is_esp_toolchain_available(&self) -> bool {
        // Check if ~/.rustup/toolchains/esp directory exists
        if let Some(home_dir) = dirs::home_dir() {
            let esp_toolchain_path = home_dir.join(".rustup").join("toolchains").join("esp");
            if esp_toolchain_path.exists() && esp_toolchain_path.is_dir() {
                return true;
            }
        }

        // Alternative: Check if rustc can compile for xtensa targets
        // This is a more thorough check but might be slower
        std::process::Command::new("rustup")
            .args(["target", "list", "--installed"])
            .output()
            .map(|output| {
                if output.status.success() {
                    let installed_targets = String::from_utf8_lossy(&output.stdout);
                    installed_targets.contains("xtensa-esp32")
                        || installed_targets.contains("xtensa-esp32s2")
                        || installed_targets.contains("xtensa-esp32s3")
                } else {
                    false
                }
            })
            .unwrap_or(false)
    }

    /// TUI-compatible internal flash function using esptool but with proper logging
    async fn flash_binary_internal(
        binary_path: &Path,
        port: Option<&str>,
        tx: mpsc::UnboundedSender<AppEvent>,
    ) -> Result<()> {
        use std::process::Stdio;
        use tokio::process::Command;

        // Determine the target port
        let target_port = match port {
            Some(p) => p.to_string(),
            None => {
                let _ = tx.send(AppEvent::BuildOutput(
                    "flash".to_string(),
                    "🔍 Auto-detecting ESP32 serial port...".to_string(),
                ));
                Self::select_esp_port_internal(tx.clone()).await?
            }
        };

        // Get detailed file information
        let file_metadata = std::fs::metadata(&binary_path)
            .map_err(|e| anyhow::anyhow!("Failed to get file metadata: {}", e))?;

        let _ = tx.send(AppEvent::BuildOutput(
            "flash".to_string(),
            format!(
                "🔥 Starting detailed TUI flash operation on port: {}",
                target_port
            ),
        ));

        let _ = tx.send(AppEvent::BuildOutput(
            "flash".to_string(),
            format!("📁 Target ELF file: {}", binary_path.display()),
        ));

        let _ = tx.send(AppEvent::BuildOutput(
            "flash".to_string(),
            format!(
                "💾 File size: {} bytes ({:.2} KB)",
                file_metadata.len(),
                file_metadata.len() as f64 / 1024.0
            ),
        ));

        let _ = tx.send(AppEvent::BuildOutput(
            "flash".to_string(),
            format!(
                "📅 File modified: {:?}",
                file_metadata
                    .modified()
                    .unwrap_or_else(|_| std::time::SystemTime::now())
            ),
        ));

        // Verify the file is indeed an ELF binary
        if let Ok(file_content) = std::fs::read(&binary_path) {
            if file_content.len() >= 4 {
                if &file_content[0..4] == b"\x7fELF" {
                    let _ = tx.send(AppEvent::BuildOutput(
                        "flash".to_string(),
                        "✅ Confirmed: File is a valid ELF binary".to_string(),
                    ));

                    // Show ELF header info
                    if file_content.len() >= 16 {
                        let class = match file_content[4] {
                            1 => "32-bit",
                            2 => "64-bit",
                            _ => "Unknown",
                        };
                        let endian = match file_content[5] {
                            1 => "Little-endian",
                            2 => "Big-endian",
                            _ => "Unknown",
                        };
                        let _ = tx.send(AppEvent::BuildOutput(
                            "flash".to_string(),
                            format!("📦 ELF info: {} {}", class, endian),
                        ));
                    }
                } else {
                    let _ = tx.send(AppEvent::BuildOutput(
                        "flash".to_string(),
                        "⚠️ WARNING: File doesn't appear to be a valid ELF binary!".to_string(),
                    ));
                    let _ = tx.send(AppEvent::BuildOutput(
                        "flash".to_string(),
                        format!(
                            "🔍 File header: {:02X} {:02X} {:02X} {:02X}",
                            file_content.get(0).unwrap_or(&0),
                            file_content.get(1).unwrap_or(&0),
                            file_content.get(2).unwrap_or(&0),
                            file_content.get(3).unwrap_or(&0)
                        ),
                    ));
                }
            }
        }

        // Construct espflash command with detailed logging
        let espflash_args = [
            "flash",
            "--port",
            &target_port,
            "--no-stub", // Use ROM bootloader only to avoid issues
            binary_path.to_str().unwrap(),
        ];

        let _ = tx.send(AppEvent::BuildOutput(
            "flash".to_string(),
            format!("🛠️ Command: espflash {}", espflash_args.join(" ")),
        ));

        let mut cmd = Command::new("espflash")
            .args(&espflash_args)
            .env("RUST_LOG", "info") // Show more logging to capture what's happening
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .context("Failed to spawn espflash command")?;

        let _ = tx.send(AppEvent::BuildOutput(
            "flash".to_string(),
            "🚀 Executing espflash - this may take a few minutes...".to_string(),
        ));

        // Wait for completion with timeout
        let timeout_dur = std::time::Duration::from_secs(180); // 3 minute timeout for ELF flashing
        let start_time = std::time::Instant::now();
        let result = tokio::time::timeout(timeout_dur, cmd.wait_with_output()).await;
        let elapsed = start_time.elapsed();

        match result {
            Ok(Ok(output)) => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);

                let _ = tx.send(AppEvent::BuildOutput(
                    "flash".to_string(),
                    format!(
                        "⏱️ Flash operation took: {:.2} seconds",
                        elapsed.as_secs_f64()
                    ),
                ));

                // Show espflash output for debugging
                if !stdout.trim().is_empty() {
                    let _ = tx.send(AppEvent::BuildOutput(
                        "flash".to_string(),
                        "📜 espflash stdout:".to_string(),
                    ));
                    for line in stdout.lines() {
                        if !line.trim().is_empty() {
                            let _ = tx.send(AppEvent::BuildOutput(
                                "flash".to_string(),
                                format!("  {}", line.trim()),
                            ));
                        }
                    }
                }

                if !stderr.trim().is_empty() {
                    let _ = tx.send(AppEvent::BuildOutput(
                        "flash".to_string(),
                        "📜 espflash stderr:".to_string(),
                    ));
                    for line in stderr.lines() {
                        if !line.trim().is_empty() {
                            let _ = tx.send(AppEvent::BuildOutput(
                                "flash".to_string(),
                                format!("  {}", line.trim()),
                            ));
                        }
                    }
                }

                if output.status.success() {
                    let _ = tx.send(AppEvent::BuildOutput(
                        "flash".to_string(),
                        format!(
                            "✨ ELF flash operation completed successfully in {:.2}s!",
                            elapsed.as_secs_f64()
                        ),
                    ));
                    Ok(())
                } else {
                    let error_msg = stderr.trim();

                    let _ = tx.send(AppEvent::BuildOutput(
                        "flash".to_string(),
                        format!(
                            "❌ Flash operation failed after {:.2}s",
                            elapsed.as_secs_f64()
                        ),
                    ));
                    let _ = tx.send(AppEvent::BuildOutput(
                        "flash".to_string(),
                        format!("❌ Error details: {}", error_msg),
                    ));

                    // Check for specific error patterns and provide helpful suggestions
                    if error_msg.contains("Permission denied")
                        || error_msg.contains("Failed to open")
                    {
                        let _ = tx.send(AppEvent::BuildOutput(
                            "flash".to_string(),
                            "💡 Tip: Check serial port permissions or try running with sudo"
                                .to_string(),
                        ));
                    } else if error_msg.contains("No such file or directory") {
                        let _ = tx.send(AppEvent::BuildOutput(
                            "flash".to_string(),
                            "💡 Tip: Install espflash with: cargo install espflash".to_string(),
                        ));
                    } else if error_msg.contains("No serial ports found") {
                        let _ = tx.send(AppEvent::BuildOutput(
                            "flash".to_string(),
                            "💡 Tip: Check that your ESP32 board is connected via USB".to_string(),
                        ));
                    }

                    Err(anyhow::anyhow!(
                        "ESP flash command failed (exit code: {}): {}",
                        output.status.code().unwrap_or(-1),
                        error_msg
                    ))
                }
            }
            Ok(Err(e)) => {
                let _ = tx.send(AppEvent::BuildOutput(
                    "flash".to_string(),
                    format!(
                        "❌ Failed to execute flash command after {:.2}s: {}",
                        elapsed.as_secs_f64(),
                        e
                    ),
                ));
                Err(anyhow::anyhow!("Failed to run flash command: {}", e))
            }
            Err(_) => {
                let _ = tx.send(AppEvent::BuildOutput(
                    "flash".to_string(),
                    format!(
                        "❌ Flash operation timed out after {:.2}s (3 minute limit)",
                        elapsed.as_secs_f64()
                    ),
                ));
                Err(anyhow::anyhow!("Flash operation timed out after 3 minutes"))
            }
        }
    }

    /// TUI-compatible port selection function
    async fn select_esp_port_internal(tx: mpsc::UnboundedSender<AppEvent>) -> Result<String> {
        // Check if user specified a port via environment variable
        if let Ok(port) = std::env::var("ESPFLASH_PORT") {
            let _ = tx.send(AppEvent::BuildOutput(
                "flash".to_string(),
                format!(
                    "🎯 Using port from ESPFLASH_PORT environment variable: {}",
                    port
                ),
            ));
            return Ok(port);
        }

        // Find available ports
        let ports = Self::find_esp_ports_internal(tx.clone()).await?;

        if ports.is_empty() {
            return Err(anyhow::anyhow!(
                "No ESP32-compatible serial ports found. Please connect your development board via USB."
            ));
        }

        if ports.len() == 1 {
            let port = ports[0].clone();
            let _ = tx.send(AppEvent::BuildOutput(
                "flash".to_string(),
                format!("🎯 Auto-selected single available port: {}", port),
            ));
            return Ok(port);
        }

        // Multiple ports available - for now, select the first one
        let port = ports[0].clone();
        let _ = tx.send(AppEvent::BuildOutput(
            "flash".to_string(),
            format!("🎯 Multiple ports available, auto-selected first: {} (set ESPFLASH_PORT to override)", port),
        ));
        let _ = tx.send(AppEvent::BuildOutput(
            "flash".to_string(),
            format!("   Available ports: {}", ports.join(", ")),
        ));

        Ok(port)
    }

    /// TUI-compatible port discovery function
    async fn find_esp_ports_internal(tx: mpsc::UnboundedSender<AppEvent>) -> Result<Vec<String>> {
        let _ = tx.send(AppEvent::BuildOutput(
            "flash".to_string(),
            "🔍 Scanning for ESP32-compatible serial ports...".to_string(),
        ));

        let ports = serialport::available_ports()?;

        // Filter for relevant USB ports on macOS and Linux
        let esp_ports: Vec<String> = ports
            .into_iter()
            .filter_map(|port_info| {
                let port_name = &port_info.port_name;
                // On macOS, focus on USB modem and USB serial ports
                if port_name.contains("/dev/cu.usbmodem")
                    || port_name.contains("/dev/cu.usbserial")
                    || port_name.contains("/dev/tty.usbmodem")
                    || port_name.contains("/dev/tty.usbserial")
                    // On Linux, ESP32 devices typically appear as ttyUSB* or ttyACM*
                    || port_name.contains("/dev/ttyUSB")
                    || port_name.contains("/dev/ttyACM")
                {
                    Some(port_name.clone())
                } else {
                    None
                }
            })
            .collect();

        let _ = tx.send(AppEvent::BuildOutput(
            "flash".to_string(),
            format!("📡 Found {} ESP32-compatible serial ports", esp_ports.len()),
        ));

        for port in &esp_ports {
            let _ = tx.send(AppEvent::BuildOutput(
                "flash".to_string(),
                format!("  🔌 {}", port),
            ));
        }

        Ok(esp_ports)
    }

    fn parse_cargo_config_targets(&self, config_path: &Path) -> Result<Vec<(String, ChipInfo)>> {
        let content =
            std::fs::read_to_string(config_path).context("Failed to read .cargo/config.toml")?;

        let mut targets = Vec::new();
        let mut current_target = None;

        for line in content.lines() {
            let line = line.trim();

            // Look for target sections like [target.xtensa-esp32s3-none-elf]
            if line.starts_with("[target.") && line.ends_with("]") {
                let target_name = line
                    .strip_prefix("[target.")
                    .and_then(|s| s.strip_suffix("]"))
                    .unwrap_or("")
                    .to_string();

                if let Some(chip_info) = self.target_to_chip_info(&target_name) {
                    current_target = Some((target_name, chip_info));
                }
            }
            // Look for default target in [build] section
            else if line.starts_with("target = ") {
                let target_name = line
                    .strip_prefix("target = ")
                    .unwrap_or("")
                    .trim_matches('"')
                    .to_string();

                if let Some(chip_info) = self.target_to_chip_info(&target_name) {
                    targets.push((target_name, chip_info));
                }
            }
        }

        // Add any target from target sections
        if let Some((target_name, chip_info)) = current_target {
            if !targets.iter().any(|(name, _)| name == &target_name) {
                targets.push((target_name, chip_info));
            }
        }

        Ok(targets)
    }

    fn target_to_chip_info(&self, target: &str) -> Option<ChipInfo> {
        match target {
            "xtensa-esp32-none-elf" => Some(ChipInfo {
                chip_name: "esp32".to_string(),
                display_name: "ESP32".to_string(),
            }),
            "xtensa-esp32s2-none-elf" => Some(ChipInfo {
                chip_name: "esp32s2".to_string(),
                display_name: "ESP32-S2".to_string(),
            }),
            "xtensa-esp32s3-none-elf" => Some(ChipInfo {
                chip_name: "esp32s3".to_string(),
                display_name: "ESP32-S3".to_string(),
            }),
            "riscv32imc-esp-espidf" => Some(ChipInfo {
                chip_name: "esp32c3".to_string(),
                display_name: "ESP32-C3".to_string(),
            }),
            "riscv32imac-esp-espidf" => Some(ChipInfo {
                chip_name: "esp32c6".to_string(),
                display_name: "ESP32-C6".to_string(),
            }),
            target if target.contains("riscv32") && target.contains("esp32c3") => Some(ChipInfo {
                chip_name: "esp32c3".to_string(),
                display_name: "ESP32-C3".to_string(),
            }),
            target if target.contains("riscv32") && target.contains("esp32c6") => Some(ChipInfo {
                chip_name: "esp32c6".to_string(),
                display_name: "ESP32-C6".to_string(),
            }),
            target if target.contains("riscv32") && target.contains("esp32h2") => Some(ChipInfo {
                chip_name: "esp32h2".to_string(),
                display_name: "ESP32-H2".to_string(),
            }),
            _ => None,
        }
    }

    fn detect_target_chip(&self, cargo_toml_path: &Path) -> Result<String> {
        let content =
            std::fs::read_to_string(cargo_toml_path).context("Failed to read Cargo.toml")?;

        // Look for ESP chip indicators in features or dependencies
        if content.contains("esp32s3") {
            Ok("ESP32-S3".to_string())
        } else if content.contains("esp32c6") {
            Ok("ESP32-C6".to_string())
        } else if content.contains("esp32c3") {
            Ok("ESP32-C3".to_string())
        } else if content.contains("esp32h2") {
            Ok("ESP32-H2".to_string())
        } else if content.contains("esp32p4") {
            Ok("ESP32-P4".to_string())
        } else if content.contains("esp32") {
            Ok("ESP32".to_string())
        } else {
            // Default to ESP32-S3 if we can't determine
            Ok("ESP32-S3".to_string())
        }
    }

    pub fn find_build_artifacts(
        &self,
        project_dir: &Path,
        board_config: &BoardConfig,
    ) -> Result<Vec<BuildArtifact>> {
        let mut artifacts = Vec::new();

        // Look for the compiled binary in target/xtensa-*/release/ or target/riscv32*/release/
        let target_dir = project_dir.join("target");
        let release_dirs = vec![
            target_dir.join("xtensa-esp32s3-none-elf/release"),
            target_dir.join("xtensa-esp32-none-elf/release"),
            target_dir.join("riscv32imc-unknown-none-elf/release"),
            target_dir.join("riscv32imac-unknown-none-elf/release"),
            target_dir.join("riscv32imc-esp-espidf/release"),
            target_dir.join("riscv32imac-esp-espidf/release"),
            // Add more target architectures as needed
        ];

        for release_dir in release_dirs {
            if release_dir.exists() {
                // Look for the project binary using package name from Cargo.toml
                let project_name = self.get_project_name(&board_config.config_file)?;
                let binary_path = release_dir.join(&project_name);

                if binary_path.exists() {
                    artifacts.push(BuildArtifact {
                        name: "application".to_string(),
                        file_path: binary_path,
                        artifact_type: ArtifactType::Elf,
                        offset: Some(0x10000), // Default app offset
                    });
                    break;
                }
            }
        }

        if artifacts.is_empty() {
            return Err(anyhow::anyhow!(
                "No build artifacts found. Make sure the project builds successfully."
            ));
        }

        Ok(artifacts)
    }

    fn get_project_name(&self, cargo_toml_path: &Path) -> Result<String> {
        let content =
            std::fs::read_to_string(cargo_toml_path).context("Failed to read Cargo.toml")?;

        // Simple parsing to find the name field
        for line in content.lines() {
            if let Some(name_line) = line.strip_prefix("name = ") {
                let name = name_line.trim_matches('"').trim_matches('\'');
                return Ok(name.to_string());
            }
        }

        // Fallback to directory name
        cargo_toml_path
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .map(|s| s.to_string())
            .ok_or_else(|| anyhow::anyhow!("Could not determine project name"))
    }

    /// Discover boards from .cargo/config_*.toml files (multiconfig pattern)
    fn discover_boards_from_config_files(&self, project_dir: &Path) -> Result<Vec<BoardConfig>> {
        let mut boards = Vec::new();
        let cargo_dir = project_dir.join(".cargo");

        if !cargo_dir.exists() {
            return Ok(Vec::new());
        }

        // Find all config_*.toml files
        let config_files = match std::fs::read_dir(&cargo_dir) {
            Ok(entries) => entries
                .filter_map(|entry| entry.ok())
                .map(|entry| entry.path())
                .filter(|path| {
                    path.file_name()
                        .and_then(|name| name.to_str())
                        .map(|name| name.starts_with("config_") && name.ends_with(".toml"))
                        .unwrap_or(false)
                })
                .collect::<Vec<_>>(),
            Err(_) => return Ok(Vec::new()),
        };

        for config_file in config_files {
            if let Ok(board_config) = self.parse_config_file_to_board(&config_file, project_dir) {
                boards.push(board_config);
            }
        }

        Ok(boards)
    }

    /// Discover boards from cargo aliases in main config.toml (multitarget pattern)
    fn discover_boards_from_cargo_aliases(&self, project_dir: &Path) -> Result<Vec<BoardConfig>> {
        let mut boards = Vec::new();
        let main_config = project_dir.join(".cargo").join("config.toml");

        if !main_config.exists() {
            return Ok(Vec::new());
        }

        let content = std::fs::read_to_string(&main_config)
            .context("Failed to read main .cargo/config.toml")?;

        // Parse TOML using the toml crate
        let parsed: toml::Value = content.parse().context("Failed to parse config.toml")?;

        if let Some(aliases) = parsed.get("alias").and_then(|v| v.as_table()) {
            for (alias_name, command) in aliases {
                if let Some(command_str) = command.as_str() {
                    if let Ok(board_config) =
                        self.parse_alias_to_board(alias_name, command_str, project_dir)
                    {
                        boards.push(board_config);
                    }
                }
            }
        }

        Ok(boards)
    }

    /// Parse a config_*.toml file into a BoardConfig
    fn parse_config_file_to_board(
        &self,
        config_file: &Path,
        project_dir: &Path,
    ) -> Result<BoardConfig> {
        let content = std::fs::read_to_string(config_file).context("Failed to read config file")?;

        // Extract board name from filename (config_esp32.toml -> esp32)
        let config_name = config_file
            .file_stem()
            .and_then(|stem| stem.to_str())
            .and_then(|name| name.strip_prefix("config_"))
            .unwrap_or("unknown")
            .to_string();

        // Parse TOML to extract environment variables
        let parsed: toml::Value = content.parse().context("Failed to parse config file")?;

        let mut chip_name = config_name.clone();
        let mut display_name = config_name.to_uppercase();

        // Extract chip information from [env] section
        if let Some(env) = parsed.get("env").and_then(|v| v.as_table()) {
            if let Some(chip_env) = env.get("ESP_CONFIG_CHIP").and_then(|v| v.as_str()) {
                chip_name = chip_env.to_string();
                display_name = chip_env.to_uppercase();
            }
        }

        // Create board configuration
        Ok(BoardConfig {
            name: format!(
                "{}-{}",
                project_dir
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("rust-project"),
                config_name
            ),
            config_file: config_file.to_path_buf(),
            build_dir: project_dir.join("target"),
            target: Some(display_name),
            project_type: ProjectType::RustNoStd,
        })
    }

    /// Parse a cargo alias into a BoardConfig
    fn parse_alias_to_board(
        &self,
        alias_name: &str,
        command: &str,
        project_dir: &Path,
    ) -> Result<BoardConfig> {
        // Extract target and features from cargo run command
        // Example: "run --release --target riscv32imac-unknown-none-elf --config=./.cargo/config_esp32c6.toml --features=esp32c6"

        let mut target = None;
        let mut features = Vec::new();
        let mut config_file = None;

        let parts: Vec<&str> = command.split_whitespace().collect();
        let mut i = 0;
        while i < parts.len() {
            match parts[i] {
                "--target" => {
                    if i + 1 < parts.len() {
                        target = Some(parts[i + 1].to_string());
                        i += 1;
                    }
                }
                "--features" => {
                    if i + 1 < parts.len() {
                        features.push(parts[i + 1].to_string());
                        i += 1;
                    }
                }
                arg if arg.starts_with("--config=") => {
                    if let Some(config_path) = arg.strip_prefix("--config=") {
                        // Convert relative path to absolute
                        let config_path = if config_path.starts_with("./") {
                            project_dir.join(config_path.strip_prefix("./").unwrap_or(config_path))
                        } else {
                            std::path::PathBuf::from(config_path)
                        };
                        config_file = Some(config_path);
                    }
                }
                arg if arg.starts_with("--features=") => {
                    if let Some(feature_list) = arg.strip_prefix("--features=") {
                        features.push(feature_list.to_string());
                    }
                }
                _ => {}
            }
            i += 1;
        }

        // Determine chip information from target or features
        let chip_info = if let Some(target_str) = &target {
            self.target_to_chip_info(target_str)
        } else if !features.is_empty() {
            // Try to extract chip from features
            for feature in &features {
                if let Some(info) = self.feature_to_chip_info(feature) {
                    return Ok(BoardConfig {
                        name: format!(
                            "{}-{}",
                            project_dir
                                .file_name()
                                .and_then(|n| n.to_str())
                                .unwrap_or("rust-project"),
                            alias_name.strip_prefix("run-").unwrap_or(alias_name)
                        ),
                        config_file: config_file.unwrap_or_else(|| project_dir.join("Cargo.toml")),
                        build_dir: project_dir.join("target"),
                        target: Some(info.display_name),
                        project_type: ProjectType::RustNoStd,
                    });
                }
            }
            None
        } else {
            None
        };

        if let Some(info) = chip_info {
            Ok(BoardConfig {
                name: format!(
                    "{}-{}",
                    project_dir
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("rust-project"),
                    alias_name.strip_prefix("run-").unwrap_or(alias_name)
                ),
                config_file: config_file.unwrap_or_else(|| project_dir.join("Cargo.toml")),
                build_dir: project_dir.join("target"),
                target: Some(info.display_name),
                project_type: ProjectType::RustNoStd,
            })
        } else {
            Err(anyhow::anyhow!(
                "Could not determine chip information from alias: {}",
                alias_name
            ))
        }
    }

    /// Convert feature name to chip information
    fn feature_to_chip_info(&self, feature: &str) -> Option<ChipInfo> {
        match feature {
            "esp32" => Some(ChipInfo {
                chip_name: "esp32".to_string(),
                display_name: "ESP32".to_string(),
            }),
            "esp32s2" => Some(ChipInfo {
                chip_name: "esp32s2".to_string(),
                display_name: "ESP32-S2".to_string(),
            }),
            "esp32s3" => Some(ChipInfo {
                chip_name: "esp32s3".to_string(),
                display_name: "ESP32-S3".to_string(),
            }),
            "esp32c3" => Some(ChipInfo {
                chip_name: "esp32c3".to_string(),
                display_name: "ESP32-C3".to_string(),
            }),
            "esp32c6" => Some(ChipInfo {
                chip_name: "esp32c6".to_string(),
                display_name: "ESP32-C6".to_string(),
            }),
            "esp32h2" => Some(ChipInfo {
                chip_name: "esp32h2".to_string(),
                display_name: "ESP32-H2".to_string(),
            }),
            "esp32p4" => Some(ChipInfo {
                chip_name: "esp32p4".to_string(),
                display_name: "ESP32-P4".to_string(),
            }),
            feature if feature.contains("esp32") && feature.contains("psram") => {
                // Handle special PSRAM variants like "esp32-psram"
                Some(ChipInfo {
                    chip_name: "esp32".to_string(),
                    display_name: "ESP32-PSRAM".to_string(),
                })
            }
            _ => None,
        }
    }

    /// Extract build information from a board configuration
    fn extract_build_info_from_board(
        &self,
        project_dir: &Path,
        board_config: &BoardConfig,
    ) -> Result<BuildInfo> {
        let mut build_info = BuildInfo {
            target: None,
            features: Vec::new(),
            config_file: None,
        };

        // If using a config_*.toml file, parse it for build information
        if board_config.config_file != project_dir.join("Cargo.toml") {
            build_info.config_file = Some(board_config.config_file.clone());

            // Try to read the config file and extract target/features
            if let Ok(content) = std::fs::read_to_string(&board_config.config_file) {
                if let Ok(parsed) = content.parse::<toml::Value>() {
                    // Extract target information from [env] section
                    if let Some(env) = parsed.get("env").and_then(|v| v.as_table()) {
                        if let Some(chip) = env.get("ESP_CONFIG_CHIP").and_then(|v| v.as_str()) {
                            // Map chip to target and features
                            match chip {
                                "esp32" => {
                                    build_info.target = Some("xtensa-esp32-none-elf".to_string());
                                    build_info.features.push("esp32".to_string());
                                }
                                "esp32s2" => {
                                    build_info.target = Some("xtensa-esp32s2-none-elf".to_string());
                                    build_info.features.push("esp32s2".to_string());
                                }
                                "esp32s3" => {
                                    build_info.target = Some("xtensa-esp32s3-none-elf".to_string());
                                    build_info.features.push("esp32s3".to_string());
                                }
                                "esp32c3" => {
                                    build_info.target =
                                        Some("riscv32imc-unknown-none-elf".to_string());
                                    build_info.features.push("esp32c3".to_string());
                                }
                                "esp32c6" => {
                                    build_info.target =
                                        Some("riscv32imac-unknown-none-elf".to_string());
                                    build_info.features.push("esp32c6".to_string());
                                }
                                "esp32h2" => {
                                    build_info.target =
                                        Some("riscv32imac-unknown-none-elf".to_string());
                                    build_info.features.push("esp32h2".to_string());
                                }
                                _ => {}
                            }
                        }
                    }
                }
            }
        }

        // If we couldn't extract from config file, try to infer from board name/target
        if build_info.target.is_none() && build_info.features.is_empty() {
            if let Some(ref target_str) = board_config.target {
                // Try to map the target back to build info
                match target_str.to_lowercase().as_str() {
                    "esp32" => {
                        build_info.target = Some("xtensa-esp32-none-elf".to_string());
                        build_info.features.push("esp32".to_string());
                    }
                    "esp32-s2" => {
                        build_info.target = Some("xtensa-esp32s2-none-elf".to_string());
                        build_info.features.push("esp32s2".to_string());
                    }
                    "esp32-s3" => {
                        build_info.target = Some("xtensa-esp32s3-none-elf".to_string());
                        build_info.features.push("esp32s3".to_string());
                    }
                    "esp32-c3" => {
                        build_info.target = Some("riscv32imc-unknown-none-elf".to_string());
                        build_info.features.push("esp32c3".to_string());
                    }
                    "esp32-c6" => {
                        build_info.target = Some("riscv32imac-unknown-none-elf".to_string());
                        build_info.features.push("esp32c6".to_string());
                    }
                    "esp32-h2" => {
                        build_info.target = Some("riscv32imac-unknown-none-elf".to_string());
                        build_info.features.push("esp32h2".to_string());
                    }
                    "esp32-psram" => {
                        build_info.target = Some("xtensa-esp32-none-elf".to_string());
                        build_info.features.push("esp32-psram".to_string());
                    }
                    _ => {}
                }
            }
        }

        Ok(build_info)
    }
}
