use crate::models::{AppEvent, ArtifactType, BuildArtifact, ProjectBoardConfig, ProjectType};
use crate::projects::registry::ProjectHandler;

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

    fn discover_boards(&self, project_dir: &Path) -> Result<Vec<ProjectBoardConfig>> {
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
                        let project_name = self
                            .get_project_name_from_dir(project_dir)
                            .unwrap_or_else(|_| {
                                project_dir
                                    .file_name()
                                    .and_then(|n| n.to_str())
                                    .unwrap_or("rust-project")
                                    .to_string()
                            });
                        let board_name = format!("{}-{}", project_name, chip_info.chip_name);

                        boards.push(ProjectBoardConfig {
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
            let project_name = self
                .get_project_name_from_dir(project_dir)
                .unwrap_or_else(|_| {
                    project_dir
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("rust-project")
                        .to_string()
                });

            boards.push(ProjectBoardConfig {
                name: project_name,
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
        board_config: &ProjectBoardConfig,
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
            match self.find_build_artifacts(project_dir, board_config) {
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
                "❌ Rust no_std build failed".to_string(),
            ));
            Err(anyhow::anyhow!("Cargo build failed"))
        }
    }

    async fn flash_board(
        &self,
        project_dir: &Path,
        board_config: &ProjectBoardConfig,
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
            .find(|artifact| matches!(artifact.artifact_type, ArtifactType::Binary))
            .or_else(|| {
                build_artifacts
                    .iter()
                    .find(|artifact| matches!(artifact.artifact_type, ArtifactType::Elf))
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

        // Use the unified flash service for consistent flashing
        use crate::services::UnifiedFlashService;
        let _flash_service = UnifiedFlashService::new();

        // Determine port to use
        let flash_port = if let Some(p) = port {
            p.to_string()
        } else {
            // If no port specified, try to auto-detect
            crate::utils::espflash_utils::select_esp_port().map_err(|e| {
                let _ = tx.send(AppEvent::BuildOutput(
                    board_config.name.clone(),
                    format!("❌ Failed to auto-detect port: {}", e),
                ));
                e
            })?
        };

        let _ = tx.send(AppEvent::BuildOutput(
            board_config.name.clone(),
            format!("🔌 Using flash port: {}", flash_port),
        ));

        // For Rust no_std projects, use multi-partition flashing with bootloader + partition table + application
        let result = self
            .flash_multi_partition_rust_binary(
                &flash_port,
                &binary_path,
                board_config,
                Some(tx.clone()),
            )
            .await?;

        if result.success {
            let _ = tx.send(AppEvent::BuildOutput(
                board_config.name.clone(),
                "✅ Rust no_std flash completed successfully".to_string(),
            ));
            Ok(())
        } else {
            let _ = tx.send(AppEvent::BuildOutput(
                board_config.name.clone(),
                format!("❌ Rust no_std flash failed: {}", result.message),
            ));
            Err(anyhow::anyhow!("Flash failed: {}", result.message))
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
        board_config: &ProjectBoardConfig,
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

    fn get_build_command(&self, project_dir: &Path, board_config: &ProjectBoardConfig) -> String {
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
        _board_config: &ProjectBoardConfig,
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
        _board_config: &ProjectBoardConfig,
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
                // Look for the project binary using package name from main Cargo.toml
                let project_name = self.get_project_name_from_dir(project_dir)?;
                let binary_path = release_dir.join(&project_name);

                if binary_path.exists() {
                    artifacts.push(BuildArtifact {
                        name: "application".to_string(),
                        file_path: binary_path.clone(),
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

    fn get_project_name_from_dir(&self, project_dir: &Path) -> Result<String> {
        let cargo_toml_path = project_dir.join("Cargo.toml");
        let content =
            std::fs::read_to_string(&cargo_toml_path).context("Failed to read main Cargo.toml")?;

        // Simple parsing to find the name field
        for line in content.lines() {
            let line = line.trim();
            if line.starts_with("name") {
                // Handle various formatting: "name = ", "name=", "name    = "
                if let Some(equals_pos) = line.find('=') {
                    let value_part = &line[equals_pos + 1..].trim();
                    if !value_part.is_empty() {
                        let name = value_part.trim_matches('"').trim_matches('\'');
                        return Ok(name.to_string());
                    }
                }
            }
        }

        // Fallback to directory name
        project_dir
            .file_name()
            .and_then(|n| n.to_str())
            .map(|s| s.to_string())
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "Could not determine project name from {}",
                    cargo_toml_path.display()
                )
            })
    }

    /// Discover boards from .cargo/config_*.toml files (multiconfig pattern)
    fn discover_boards_from_config_files(
        &self,
        project_dir: &Path,
    ) -> Result<Vec<ProjectBoardConfig>> {
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
    fn discover_boards_from_cargo_aliases(
        &self,
        project_dir: &Path,
    ) -> Result<Vec<ProjectBoardConfig>> {
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

    /// Parse a config_*.toml file into a ProjectBoardConfig
    fn parse_config_file_to_board(
        &self,
        config_file: &Path,
        project_dir: &Path,
    ) -> Result<ProjectBoardConfig> {
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

        let mut display_name = config_name.to_uppercase();

        // Extract chip information from [env] section
        if let Some(env) = parsed.get("env").and_then(|v| v.as_table()) {
            if let Some(chip_env) = env.get("ESP_CONFIG_CHIP").and_then(|v| v.as_str()) {
                display_name = chip_env.to_uppercase();
            }
        }

        // Create board configuration
        let project_name = self
            .get_project_name_from_dir(project_dir)
            .unwrap_or_else(|_| {
                project_dir
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("rust-project")
                    .to_string()
            });

        Ok(ProjectBoardConfig {
            name: format!("{}-{}", project_name, config_name),
            config_file: config_file.to_path_buf(),
            build_dir: project_dir.join("target"),
            target: Some(display_name),
            project_type: ProjectType::RustNoStd,
        })
    }

    /// Parse a cargo alias into a ProjectBoardConfig
    fn parse_alias_to_board(
        &self,
        alias_name: &str,
        command: &str,
        project_dir: &Path,
    ) -> Result<ProjectBoardConfig> {
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
                    let project_name =
                        self.get_project_name_from_dir(project_dir)
                            .unwrap_or_else(|_| {
                                project_dir
                                    .file_name()
                                    .and_then(|n| n.to_str())
                                    .unwrap_or("rust-project")
                                    .to_string()
                            });
                    return Ok(ProjectBoardConfig {
                        name: format!(
                            "{}-{}",
                            project_name,
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
            let project_name = self
                .get_project_name_from_dir(project_dir)
                .unwrap_or_else(|_| {
                    project_dir
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("rust-project")
                        .to_string()
                });
            Ok(ProjectBoardConfig {
                name: format!(
                    "{}-{}",
                    project_name,
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
        board_config: &ProjectBoardConfig,
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

    /// Flash Rust no_std binary with multi-partition support (bootloader + partition table + app)
    /// This method creates a complete ESP32 flash image with all required components.
    pub async fn flash_multi_partition_rust_binary(
        &self,
        port: &str,
        binary_path: &Path,
        board_config: &ProjectBoardConfig,
        progress_tx: Option<mpsc::UnboundedSender<AppEvent>>,
    ) -> Result<crate::services::FlashResult> {
        use crate::espflash_local::{default_bootloader, default_partition_table};
        use anyhow::Context;
        use espflash::target::{Chip, XtalFrequency};

        let board_name = board_config.name.clone();

        if let Some(tx) = &progress_tx {
            let _ = tx.send(AppEvent::BuildOutput(
                board_name.clone(),
                "🔧 Preparing multi-partition flash (bootloader + partition table + app)..."
                    .to_string(),
            ));
        }

        // Determine the target chip from board configuration
        let chip = self
            .determine_chip_from_board_config(board_config)
            .context("Failed to determine target chip")?;

        if let Some(tx) = &progress_tx {
            let _ = tx.send(AppEvent::BuildOutput(
                board_name.clone(),
                format!("🎯 Target chip: {:?}", chip),
            ));
        }

        // Use default crystal frequency (40MHz for most chips, 32MHz for H2)
        let xtal_freq = match chip {
            Chip::Esp32h2 => XtalFrequency::_32Mhz,
            _ => XtalFrequency::_40Mhz,
        };

        // Get bootloader binary
        let bootloader_data =
            default_bootloader(chip, xtal_freq).context("Failed to get default bootloader")?;

        if let Some(tx) = &progress_tx {
            let _ = tx.send(AppEvent::BuildOutput(
                board_name.clone(),
                format!(
                    "📥 Bootloader ready: {:.1} KB",
                    bootloader_data.len() as f64 / 1024.0
                ),
            ));
        }

        // Generate partition table
        let partition_table = default_partition_table(chip, None); // Use default flash size
        let partition_table_data = partition_table
            .to_bin()
            .context("Failed to generate partition table binary")?;

        if let Some(tx) = &progress_tx {
            let _ = tx.send(AppEvent::BuildOutput(
                board_name.clone(),
                format!(
                    "📋 Partition table ready: {} bytes",
                    partition_table_data.len()
                ),
            ));
        }

        // Convert ELF to proper ESP32 binary image using espflash save-image
        let app_data = if binary_path.extension().is_none()
            || binary_path.to_string_lossy().ends_with("elf")
            || !binary_path.to_string_lossy().contains(".bin")
        {
            // This is likely an ELF file - convert it to binary image using espflash command
            let temp_dir = std::env::temp_dir();
            let temp_image = temp_dir.join(format!("espbrew_app_{}.bin", std::process::id()));

            if let Some(tx) = &progress_tx {
                let _ = tx.send(AppEvent::BuildOutput(
                    board_name.clone(),
                    "🔧 Converting ELF to binary image using espflash...".to_string(),
                ));
            }

            // Use espflash save-image to convert ELF to binary
            let chip_str = format!("{:?}", chip).to_lowercase();
            let mut cmd = tokio::process::Command::new("espflash");
            cmd.args([
                "save-image",
                "--chip",
                &chip_str,
                binary_path.to_str().unwrap(),
                temp_image.to_str().unwrap(),
            ])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

            let output = cmd
                .output()
                .await
                .with_context(|| "Failed to run espflash save-image")?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(anyhow::anyhow!("espflash save-image failed: {}", stderr));
            }

            // Read the converted binary
            let binary_data = std::fs::read(&temp_image).with_context(|| {
                format!("Failed to read converted binary: {}", temp_image.display())
            })?;

            // Clean up temp file
            let _ = std::fs::remove_file(&temp_image);

            if let Some(tx) = &progress_tx {
                let _ = tx.send(AppEvent::BuildOutput(
                    board_name.clone(),
                    format!(
                        "✅ ELF converted to binary: {:.1} KB",
                        binary_data.len() as f64 / 1024.0
                    ),
                ));
            }

            binary_data
        } else {
            // This is already a binary file
            std::fs::read(binary_path)
                .with_context(|| format!("Failed to read binary file: {}", binary_path.display()))?
        };

        if let Some(tx) = &progress_tx {
            let _ = tx.send(AppEvent::BuildOutput(
                board_name.clone(),
                format!(
                    "📱 Application binary ready: {:.1} KB",
                    app_data.len() as f64 / 1024.0
                ),
            ));
        }

        // Prepare flash data directly from memory using HashMap
        let mut flash_data_map = std::collections::HashMap::new();

        // Add bootloader at 0x0000
        flash_data_map.insert(0x0000, bootloader_data.to_vec());

        // Add partition table at 0x8000
        flash_data_map.insert(0x8000, partition_table_data);

        // Add application at 0x10000
        flash_data_map.insert(0x10000, app_data);

        let total_size: usize = flash_data_map.values().map(|v| v.len()).sum();

        if let Some(tx) = &progress_tx {
            let _ = tx.send(AppEvent::BuildOutput(
                board_name.clone(),
                format!(
                    "🚀 Starting multi-partition flash operation ({:.1} KB total)...",
                    total_size as f64 / 1024.0
                ),
            ));
        }

        // Use espflash utils directly for efficient memory streaming
        let result = crate::utils::espflash_utils::flash_multi_binary(port, flash_data_map).await;

        match result {
            Ok(_) => {
                if let Some(tx) = &progress_tx {
                    let _ = tx.send(AppEvent::BuildOutput(
                        board_name.clone(),
                        "✅ Multi-partition flash completed successfully".to_string(),
                    ));
                }
                Ok(crate::services::FlashResult {
                    success: true,
                    message: format!(
                        "Successfully flashed 3 partitions ({:.1} KB total)",
                        total_size as f64 / 1024.0
                    ),
                    duration_ms: None,
                })
            }
            Err(e) => {
                if let Some(tx) = &progress_tx {
                    let _ = tx.send(AppEvent::BuildOutput(
                        board_name.clone(),
                        format!("❌ Multi-partition flash failed: {}", e),
                    ));
                }
                Ok(crate::services::FlashResult {
                    success: false,
                    message: format!("Flash failed: {}", e),
                    duration_ms: None,
                })
            }
        }
    }

    /// Determine the ESP32 chip type from board configuration
    fn determine_chip_from_board_config(
        &self,
        board_config: &ProjectBoardConfig,
    ) -> Result<espflash::target::Chip> {
        use espflash::target::Chip;

        // First try to extract from the target string if available
        if let Some(ref target_str) = board_config.target {
            let chip = match target_str.to_lowercase().as_str() {
                "esp32" | "esp32-psram" => Chip::Esp32,
                "esp32-s2" => Chip::Esp32s2,
                "esp32-s3" => Chip::Esp32s3,
                "esp32-c3" => Chip::Esp32c3,
                "esp32-c6" => Chip::Esp32c6,
                "esp32-h2" => Chip::Esp32h2,
                "esp32-p4" => Chip::Esp32p4,
                _ => {
                    return Err(anyhow::anyhow!(
                        "Unsupported or unknown chip: {}",
                        target_str
                    ));
                }
            };
            return Ok(chip);
        }

        // If target is not available, try to extract from config file name or board name
        let config_file_name = board_config
            .config_file
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("");

        let board_name_lower = board_config.name.to_lowercase();

        // Check config file name (e.g., config_esp32s3.toml)
        if config_file_name.contains("esp32s3")
            || board_name_lower.contains("esp32s3")
            || board_name_lower.contains("esp32-s3")
        {
            return Ok(Chip::Esp32s3);
        }
        if config_file_name.contains("esp32s2")
            || board_name_lower.contains("esp32s2")
            || board_name_lower.contains("esp32-s2")
        {
            return Ok(Chip::Esp32s2);
        }
        if config_file_name.contains("esp32c6")
            || board_name_lower.contains("esp32c6")
            || board_name_lower.contains("esp32-c6")
        {
            return Ok(Chip::Esp32c6);
        }
        if config_file_name.contains("esp32c3")
            || board_name_lower.contains("esp32c3")
            || board_name_lower.contains("esp32-c3")
        {
            return Ok(Chip::Esp32c3);
        }
        if config_file_name.contains("esp32h2")
            || board_name_lower.contains("esp32h2")
            || board_name_lower.contains("esp32-h2")
        {
            return Ok(Chip::Esp32h2);
        }
        if config_file_name.contains("esp32p4")
            || board_name_lower.contains("esp32p4")
            || board_name_lower.contains("esp32-p4")
        {
            return Ok(Chip::Esp32p4);
        }
        if config_file_name.contains("esp32") || board_name_lower.contains("esp32") {
            return Ok(Chip::Esp32);
        }

        // Default to ESP32 if we can't determine the chip
        Ok(Chip::Esp32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tempfile::NamedTempFile;

    #[test]
    fn test_determine_chip_from_board_config() {
        let handler = RustNoStdHandler;

        // Test with target string
        let config = ProjectBoardConfig {
            name: "test-esp32s3".to_string(),
            config_file: PathBuf::from("Cargo.toml"),
            build_dir: PathBuf::from("target"),
            target: Some("ESP32-S3".to_string()),
            project_type: ProjectType::RustNoStd,
        };

        let chip = handler.determine_chip_from_board_config(&config).unwrap();
        assert!(matches!(chip, espflash::target::Chip::Esp32s3));

        // Test with board name
        let config = ProjectBoardConfig {
            name: "test-esp32c6".to_string(),
            config_file: PathBuf::from("Cargo.toml"),
            build_dir: PathBuf::from("target"),
            target: None,
            project_type: ProjectType::RustNoStd,
        };

        let chip = handler.determine_chip_from_board_config(&config).unwrap();
        assert!(matches!(chip, espflash::target::Chip::Esp32c6));

        // Test default fallback
        let config = ProjectBoardConfig {
            name: "generic-project".to_string(),
            config_file: PathBuf::from("Cargo.toml"),
            build_dir: PathBuf::from("target"),
            target: None,
            project_type: ProjectType::RustNoStd,
        };

        let chip = handler.determine_chip_from_board_config(&config).unwrap();
        assert!(matches!(chip, espflash::target::Chip::Esp32));
    }

    #[tokio::test]
    async fn test_flash_multi_partition_rust_binary_data_preparation() {
        let handler = RustNoStdHandler;

        // Create a temporary binary file for testing
        let mut temp_file = NamedTempFile::new().unwrap();
        let test_app_data = b"test app binary data";
        std::io::Write::write_all(&mut temp_file, test_app_data).unwrap();

        let config = ProjectBoardConfig {
            name: "test-esp32".to_string(),
            config_file: PathBuf::from("Cargo.toml"),
            build_dir: PathBuf::from("target"),
            target: Some("ESP32".to_string()),
            project_type: ProjectType::RustNoStd,
        };

        // Test that chip detection works
        let chip = handler.determine_chip_from_board_config(&config).unwrap();
        assert!(matches!(chip, espflash::target::Chip::Esp32));

        // Verify bootloader and partition table generation works
        use crate::espflash_local::{default_bootloader, default_partition_table};
        use espflash::target::XtalFrequency;

        let bootloader_data = default_bootloader(chip, XtalFrequency::_40Mhz).unwrap();
        assert!(
            !bootloader_data.is_empty(),
            "Bootloader data should not be empty"
        );

        let partition_table = default_partition_table(chip, None);
        let partition_table_data = partition_table.to_bin().unwrap();
        assert!(
            !partition_table_data.is_empty(),
            "Partition table data should not be empty"
        );

        // Verify the application can be read
        let app_data = std::fs::read(temp_file.path()).unwrap();
        assert_eq!(app_data, test_app_data, "Application data should match");

        println!(
            "Multi-partition flash test data prepared: bootloader={} bytes, partition_table={} bytes, app={} bytes",
            bootloader_data.len(),
            partition_table_data.len(),
            app_data.len()
        );
    }
}
