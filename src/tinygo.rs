use crate::AppEvent;
use crate::project::{ArtifactType, BoardConfig, BuildArtifact, ProjectHandler, ProjectType};
use anyhow::{Context, Result};
use async_trait::async_trait;
use std::fs;
use std::path::{Path, PathBuf};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;

/// Handler for TinyGo embedded projects
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
        let go_mod = project_dir.join("go.mod");
        if !go_mod.exists() {
            return false;
        }

        // Check if it's a TinyGo embedded project
        if let Ok(content) = fs::read_to_string(&go_mod) {
            // Look for embedded/TinyGo indicators
            content.contains("tinygo.org")
                || content.contains("machine")
                || self.has_tinygo_imports(project_dir)
        } else {
            false
        }
    }

    fn discover_boards(&self, project_dir: &Path) -> Result<Vec<BoardConfig>> {
        let mut boards = Vec::new();
        let project_name = project_dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("tinygo-project")
            .to_string();

        let target = self.detect_target_chip(project_dir)?;

        boards.push(BoardConfig {
            name: project_name,
            config_file: project_dir.join("go.mod"),
            build_dir: project_dir.to_path_buf(),
            target: Some(target),
            project_type: ProjectType::TinyGo,
        });

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
            "🔧 Starting TinyGo build...".to_string(),
        ));

        let target = self.determine_tinygo_target(&board_config.target)?;

        let mut cmd = Command::new("tinygo");
        cmd.current_dir(project_dir)
            .args(["build", "-target", &target, "-o", "main.elf", "."])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        let mut child = cmd.spawn().context("Failed to start tinygo build")?;
        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();

        // Handle output...
        let status = child
            .wait()
            .await
            .context("Failed to wait for tinygo build")?;

        if status.success() {
            let _ = tx.send(AppEvent::BuildOutput(
                board_config.name.clone(),
                "✅ TinyGo build completed successfully".to_string(),
            ));

            let elf_path = project_dir.join("main.elf");
            Ok(vec![BuildArtifact {
                name: "application".to_string(),
                file_path: elf_path,
                artifact_type: ArtifactType::Elf,
                offset: None,
            }])
        } else {
            Err(anyhow::anyhow!("TinyGo build failed"))
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
            "🔥 Starting TinyGo flash...".to_string(),
        ));

        let target = self.determine_tinygo_target(&board_config.target)?;

        let mut cmd = Command::new("tinygo");
        cmd.current_dir(project_dir)
            .args(["flash", "-target", &target]);

        if let Some(port) = port {
            cmd.args(["-port", port]);
        }

        let status = cmd.status().await.context("Failed to run tinygo flash")?;

        if status.success() {
            let _ = tx.send(AppEvent::BuildOutput(
                board_config.name.clone(),
                "✅ TinyGo flash completed successfully".to_string(),
            ));
            Ok(())
        } else {
            Err(anyhow::anyhow!("TinyGo flash failed"))
        }
    }

    async fn monitor_board(
        &self,
        _project_dir: &Path,
        board_config: &BoardConfig,
        port: Option<&str>,
        baud_rate: u32,
        tx: mpsc::UnboundedSender<AppEvent>,
    ) -> Result<()> {
        let _ = tx.send(AppEvent::BuildOutput(
            board_config.name.clone(),
            format!("📺 TinyGo monitoring via serial at {} baud", baud_rate),
        ));

        // Use basic serial monitoring
        if self.is_tool_available("miniterm.py") {
            let mut cmd = Command::new("miniterm.py");
            if let Some(port) = port {
                cmd.arg(port);
            }
            cmd.arg(baud_rate.to_string());
            cmd.status().await.context("Failed to run miniterm.py")?;
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
            "🧹 Cleaning TinyGo build artifacts...".to_string(),
        ));

        // Remove build artifacts
        let artifacts_to_remove = ["main.elf", "main.bin", "main.hex"];
        for artifact in &artifacts_to_remove {
            let path = project_dir.join(artifact);
            if path.exists() {
                fs::remove_file(&path)?;
            }
        }

        Ok(())
    }

    fn get_build_command(&self, _project_dir: &Path, board_config: &BoardConfig) -> String {
        let target = self
            .determine_tinygo_target(&board_config.target)
            .unwrap_or("esp32".to_string());
        format!("tinygo build -target {} -o main.elf .", target)
    }

    fn get_flash_command(
        &self,
        _project_dir: &Path,
        board_config: &BoardConfig,
        port: Option<&str>,
    ) -> String {
        let target = self
            .determine_tinygo_target(&board_config.target)
            .unwrap_or("esp32".to_string());
        let port_arg = port.map(|p| format!(" -port {}", p)).unwrap_or_default();
        format!("tinygo flash -target {}{}", target, port_arg)
    }

    fn check_tools_available(&self) -> Result<(), String> {
        if !self.is_tool_available("tinygo") {
            return Err("tinygo not found in PATH".to_string());
        }
        Ok(())
    }

    fn get_missing_tools_message(&self) -> String {
        "⚠️  TinyGo is not installed.\n".to_string()
            + "   Install from: https://tinygo.org/getting-started/install/\n"
            + "   Press Enter to continue anyway, or 'q' to quit."
    }
}

impl TinyGoHandler {
    fn is_tool_available(&self, tool: &str) -> bool {
        std::process::Command::new("which")
            .arg(tool)
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    fn has_tinygo_imports(&self, project_dir: &Path) -> bool {
        // Check Go files for TinyGo/embedded imports
        if let Ok(entries) = project_dir.read_dir() {
            for entry in entries.flatten() {
                if let Some(ext) = entry.path().extension() {
                    if ext == "go" {
                        if let Ok(content) = fs::read_to_string(entry.path()) {
                            if content.contains("import \"machine\"")
                                || content.contains("tinygo.org")
                            {
                                return true;
                            }
                        }
                    }
                }
            }
        }
        false
    }

    fn detect_target_chip(&self, project_dir: &Path) -> Result<String> {
        // Look for target indicators in Go files or comments
        if let Ok(entries) = project_dir.read_dir() {
            for entry in entries.flatten() {
                if let Some(ext) = entry.path().extension() {
                    if ext == "go" {
                        if let Ok(content) = fs::read_to_string(entry.path()) {
                            if content.contains("esp32") {
                                return Ok("ESP32".to_string());
                            } else if content.contains("pico") {
                                return Ok("RP2040".to_string());
                            }
                        }
                    }
                }
            }
        }
        Ok("ESP32".to_string())
    }

    fn determine_tinygo_target(&self, chip_target: &Option<String>) -> Result<String> {
        match chip_target.as_ref().map(|s| s.as_str()) {
            Some("ESP32") => Ok("esp32-coreboard-v2".to_string()),
            Some("RP2040") => Ok("pico".to_string()),
            _ => Ok("esp32-coreboard-v2".to_string()),
        }
    }
}
