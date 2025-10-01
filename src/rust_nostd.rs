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

        // Check .cargo/config.toml for target configurations
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

        // Build with cargo build --release (as per user rules)
        let mut cmd = Command::new("cargo");
        cmd.current_dir(project_dir)
            .args(["build", "--release"])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

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

        // Use our espflash utilities to flash the binary
        let tx_clone = tx.clone();
        let board_name_clone = board_config.name.clone();
        let port_clone = port.map(|s| s.to_string());

        let flash_result = tokio::spawn(async move {
            match crate::espflash_utils::flash_binary_to_esp(&binary_path, port_clone.as_deref())
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

    fn get_build_command(&self, _project_dir: &Path, _board_config: &BoardConfig) -> String {
        // Always use --release as per user rules
        "cargo build --release".to_string()
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

    fn find_build_artifacts(
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
}
