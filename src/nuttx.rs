use crate::AppEvent;
use crate::project::{ArtifactType, BoardConfig, BuildArtifact, ProjectHandler, ProjectType};
use anyhow::{Context, Result};
use async_trait::async_trait;
use std::fs;
use std::path::{Path, PathBuf};
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
        // NuttX projects typically have .config and defconfig files
        let has_config = project_dir.join(".config").exists();
        let has_defconfig = project_dir.join("defconfig").exists();
        let has_makefile =
            project_dir.join("Makefile").exists() || project_dir.join("makefile").exists();

        // Also check for nuttx directory structure
        let has_nuttx_dir = project_dir.join("nuttx").exists();

        (has_config || has_defconfig) && (has_makefile || has_nuttx_dir)
    }

    fn discover_boards(&self, project_dir: &Path) -> Result<Vec<BoardConfig>> {
        let project_name = project_dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("nuttx-project")
            .to_string();
        let target = self.detect_target_chip(project_dir)?;

        Ok(vec![BoardConfig {
            name: project_name,
            config_file: project_dir.join(".config"),
            build_dir: project_dir.to_path_buf(),
            target: Some(target),
            project_type: ProjectType::NuttX,
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
            "🔧 Starting NuttX build...".to_string(),
        ));

        let mut cmd = Command::new("make");
        cmd.current_dir(project_dir);

        let status = cmd.status().await.context("Failed to run make")?;

        if status.success() {
            let _ = tx.send(AppEvent::BuildOutput(
                board_config.name.clone(),
                "✅ NuttX build completed".to_string(),
            ));
            let elf_path = project_dir.join("nuttx");
            Ok(vec![BuildArtifact {
                name: "nuttx".to_string(),
                file_path: elf_path,
                artifact_type: ArtifactType::Elf,
                offset: None,
            }])
        } else {
            Err(anyhow::anyhow!("NuttX build failed"))
        }
    }

    async fn flash_board(
        &self,
        project_dir: &Path,
        board_config: &BoardConfig,
        artifacts: &[BuildArtifact],
        _port: Option<&str>,
        tx: mpsc::UnboundedSender<AppEvent>,
    ) -> Result<()> {
        let _ = tx.send(AppEvent::BuildOutput(
            board_config.name.clone(),
            "🔥 Starting NuttX flash...".to_string(),
        ));

        // NuttX flashing depends on the target, use esptool for ESP32
        if let Some(artifact) = artifacts.first() {
            let mut cmd = Command::new("esptool.py");
            cmd.args([
                "--chip",
                "esp32",
                "elf2image",
                &artifact.file_path.to_string_lossy(),
            ]);

            let status = cmd
                .status()
                .await
                .context("Failed to convert ELF to image")?;

            if status.success() {
                let _ = tx.send(AppEvent::BuildOutput(
                    board_config.name.clone(),
                    "✅ NuttX flash preparation completed".to_string(),
                ));
            }
        }

        Ok(())
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
            "📺 Use serial terminal tools for NuttX monitoring".to_string(),
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
            "🧹 Cleaning NuttX build...".to_string(),
        ));

        let mut cmd = Command::new("make");
        cmd.current_dir(project_dir).args(["clean"]);
        cmd.status().await.context("Failed to run make clean")?;

        Ok(())
    }

    fn get_build_command(&self, _project_dir: &Path, _board_config: &BoardConfig) -> String {
        "make".to_string()
    }

    fn get_flash_command(
        &self,
        _project_dir: &Path,
        _board_config: &BoardConfig,
        _port: Option<&str>,
    ) -> String {
        "esptool.py --chip esp32 elf2image nuttx && esptool.py write_flash 0x10000 nuttx.bin"
            .to_string()
    }

    fn check_tools_available(&self) -> Result<(), String> {
        if !self.is_tool_available("make") {
            return Err("make not found in PATH".to_string());
        }
        Ok(())
    }

    fn get_missing_tools_message(&self) -> String {
        "⚠️  NuttX build environment is not set up.\n".to_string()
            + "   Install NuttX build tools:\n"
            + "   https://nuttx.apache.org/docs/latest/quickstart/install.html\n"
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

    fn detect_target_chip(&self, project_dir: &Path) -> Result<String> {
        let config_files = [".config", "defconfig"];

        for config_file in &config_files {
            let config_path = project_dir.join(config_file);
            if let Ok(content) = fs::read_to_string(&config_path) {
                if content.contains("esp32") {
                    return Ok("ESP32".to_string());
                }
            }
        }

        Ok("Generic".to_string())
    }
}
