use crate::AppEvent;
use crate::project::{ArtifactType, BoardConfig, BuildArtifact, ProjectHandler, ProjectType};
use anyhow::{Context, Result};
use async_trait::async_trait;
use std::fs;
use std::path::{Path, PathBuf};
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

    fn can_handle(&self, project_dir: &Path) -> bool {
        let prj_conf = project_dir.join("prj.conf");
        let cmake = project_dir.join("CMakeLists.txt");

        if !prj_conf.exists() || !cmake.exists() {
            return false;
        }

        // Check for Zephyr-specific content
        if let Ok(cmake_content) = fs::read_to_string(&cmake) {
            cmake_content.contains("find_package(Zephyr") || cmake_content.contains("zephyr")
        } else {
            false
        }
    }

    fn discover_boards(&self, project_dir: &Path) -> Result<Vec<BoardConfig>> {
        let project_name = project_dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("zephyr-project")
            .to_string();
        let target = self.detect_target_chip(project_dir)?;

        Ok(vec![BoardConfig {
            name: project_name,
            config_file: project_dir.join("prj.conf"),
            build_dir: project_dir.join("build"),
            target: Some(target),
            project_type: ProjectType::Zephyr,
        }])
    }

    async fn build_board(
        &self,
        project_dir: &Path,
        board_config: &BoardConfig,
        tx: mpsc::UnboundedSender<AppEvent>,
    ) -> Result<Vec<BuildArtifact>> {
        let _ = tx.send(AppEvent::BuildOutput(
            board_config.name.clone(),
            "🔧 Starting Zephyr build...".to_string(),
        ));

        let board_name = self.determine_zephyr_board(&board_config.target)?;

        let mut cmd = Command::new("west");
        cmd.current_dir(project_dir)
            .args(["build", "-b", &board_name]);

        let status = cmd.status().await.context("Failed to run west build")?;

        if status.success() {
            let _ = tx.send(AppEvent::BuildOutput(
                board_config.name.clone(),
                "✅ Zephyr build completed".to_string(),
            ));
            let elf_path = board_config.build_dir.join("zephyr").join("zephyr.elf");
            Ok(vec![BuildArtifact {
                name: "zephyr".to_string(),
                file_path: elf_path,
                artifact_type: ArtifactType::Elf,
                offset: None,
            }])
        } else {
            Err(anyhow::anyhow!("Zephyr build failed"))
        }
    }

    async fn flash_board(
        &self,
        project_dir: &Path,
        board_config: &BoardConfig,
        _artifacts: &[BuildArtifact],
        _port: Option<&str>,
        tx: mpsc::UnboundedSender<AppEvent>,
    ) -> Result<()> {
        let _ = tx.send(AppEvent::BuildOutput(
            board_config.name.clone(),
            "🔥 Starting Zephyr flash...".to_string(),
        ));

        let mut cmd = Command::new("west");
        cmd.current_dir(project_dir).args(["flash"]);

        let status = cmd.status().await.context("Failed to run west flash")?;

        if status.success() {
            let _ = tx.send(AppEvent::BuildOutput(
                board_config.name.clone(),
                "✅ Zephyr flash completed".to_string(),
            ));
            Ok(())
        } else {
            Err(anyhow::anyhow!("Zephyr flash failed"))
        }
    }

    async fn monitor_board(
        &self,
        _project_dir: &Path,
        board_config: &BoardConfig,
        _port: Option<&str>,
        _baud_rate: u32,
        tx: mpsc::UnboundedSender<AppEvent>,
    ) -> Result<()> {
        let _ = tx.send(AppEvent::BuildOutput(
            board_config.name.clone(),
            "📺 Use 'west debug' or serial tools for monitoring".to_string(),
        ));
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
            "🧹 Cleaning Zephyr build...".to_string(),
        ));

        if board_config.build_dir.exists() {
            fs::remove_dir_all(&board_config.build_dir)?;
        }

        Ok(())
    }

    fn get_build_command(&self, _project_dir: &Path, board_config: &BoardConfig) -> String {
        let board_name = self
            .determine_zephyr_board(&board_config.target)
            .unwrap_or("esp32".to_string());
        format!("west build -b {}", board_name)
    }

    fn get_flash_command(
        &self,
        _project_dir: &Path,
        _board_config: &BoardConfig,
        _port: Option<&str>,
    ) -> String {
        "west flash".to_string()
    }

    fn check_tools_available(&self) -> Result<(), String> {
        if !self.is_tool_available("west") {
            return Err("west (Zephyr build tool) not found in PATH".to_string());
        }
        Ok(())
    }

    fn get_missing_tools_message(&self) -> String {
        "⚠️  Zephyr environment is not set up.\n".to_string()
            + "   Install Zephyr SDK and west tool:\n"
            + "   https://docs.zephyrproject.org/latest/getting_started/\n"
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

    fn detect_target_chip(&self, project_dir: &Path) -> Result<String> {
        let prj_conf = project_dir.join("prj.conf");
        if let Ok(content) = fs::read_to_string(&prj_conf) {
            if content.contains("esp32") {
                Ok("ESP32".to_string())
            } else {
                Ok("Generic".to_string())
            }
        } else {
            Ok("Generic".to_string())
        }
    }

    fn determine_zephyr_board(&self, chip_target: &Option<String>) -> Result<String> {
        match chip_target.as_ref().map(|s| s.as_str()) {
            Some("ESP32") => Ok("esp32".to_string()),
            _ => Ok("esp32".to_string()),
        }
    }
}
