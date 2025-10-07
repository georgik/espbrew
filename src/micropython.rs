use crate::AppEvent;
use crate::project::{ArtifactType, BoardConfig, BuildArtifact, ProjectHandler, ProjectType};
use anyhow::{Context, Result};
use async_trait::async_trait;
use std::fs;
use std::path::{Path, PathBuf};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;

/// Handler for MicroPython projects
pub struct MicroPythonHandler;

#[async_trait]
impl ProjectHandler for MicroPythonHandler {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn project_type(&self) -> ProjectType {
        ProjectType::MicroPython
    }

    fn can_handle(&self, project_dir: &Path) -> bool {
        // Check for main.py or boot.py (standard MicroPython files)
        let has_main_py = project_dir.join("main.py").exists();
        let has_boot_py = project_dir.join("boot.py").exists();

        // Also check if there's a micropython-specific indicator
        let has_micropython_indicator = self.has_micropython_imports(project_dir);

        has_main_py || has_boot_py || has_micropython_indicator
    }

    fn discover_boards(&self, project_dir: &Path) -> Result<Vec<BoardConfig>> {
        let mut boards = Vec::new();

        // MicroPython doesn't have traditional "build configurations" like ESP-IDF
        // Instead, we create a single configuration representing the project
        let project_name = project_dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("micropython-project")
            .to_string();

        let target = self.detect_target_chip(project_dir)?;

        boards.push(BoardConfig {
            name: project_name,
            config_file: project_dir.join("main.py"), // Use main.py as config reference
            build_dir: project_dir.to_path_buf(),     // No separate build dir for MicroPython
            target: Some(target),
            project_type: ProjectType::MicroPython,
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
            "🐍 MicroPython project - no compilation needed".to_string(),
        ));

        let _ = tx.send(AppEvent::BuildOutput(
            board_config.name.clone(),
            "📁 Scanning for Python source files...".to_string(),
        ));

        // Find all Python files in the project
        self.find_python_files(project_dir, board_config, tx.clone())
            .await
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
            "🔥 Starting MicroPython file upload...".to_string(),
        ));

        // Determine which tool to use
        let upload_tool = self.detect_upload_tool(tx.clone()).await?;

        match upload_tool.as_str() {
            "mpremote" => {
                self.upload_with_mpremote(project_dir, board_config, artifacts, port, tx)
                    .await
            }
            "ampy" => {
                self.upload_with_ampy(project_dir, board_config, artifacts, port, tx)
                    .await
            }
            _ => Err(anyhow::anyhow!("No suitable MicroPython upload tool found")),
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
                "📺 Starting MicroPython REPL monitor on {} at {} baud",
                port.unwrap_or("auto-detect"),
                baud_rate
            ),
        ));

        // Try mpremote first, then fallback to direct serial connection
        if self.is_tool_available("mpremote") {
            self.monitor_with_mpremote(project_dir, board_config, port, baud_rate, tx)
                .await
        } else {
            self.monitor_with_serial(project_dir, board_config, port, baud_rate, tx)
                .await
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
            "🧹 Cleaning MicroPython files from device...".to_string(),
        ));

        // For MicroPython, "clean" means removing files from the device
        // This is optional functionality since it's destructive
        let _ = tx.send(AppEvent::BuildOutput(
            board_config.name.clone(),
            "⚠️  Clean operation for MicroPython would remove files from device".to_string(),
        ));

        let _ = tx.send(AppEvent::BuildOutput(
            board_config.name.clone(),
            "💡 Use 'mpremote fs rm :main.py' to manually remove specific files".to_string(),
        ));

        Ok(())
    }

    fn get_build_command(&self, _project_dir: &Path, _board_config: &BoardConfig) -> String {
        "# MicroPython projects don't require compilation".to_string()
    }

    fn get_flash_command(
        &self,
        project_dir: &Path,
        board_config: &BoardConfig,
        port: Option<&str>,
    ) -> String {
        let port_arg = port.map(|p| format!(" connect {}", p)).unwrap_or_default();

        if std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")) != *project_dir {
            format!(
                "cd {} && mpremote{} cp *.py :",
                project_dir.display(),
                port_arg
            )
        } else {
            format!("mpremote{} cp *.py :", port_arg)
        }
    }

    fn check_tools_available(&self) -> Result<(), String> {
        // Check for at least one upload tool
        if !self.is_tool_available("mpremote") && !self.is_tool_available("ampy") {
            return Err("No MicroPython upload tool found (mpremote or ampy)".to_string());
        }

        Ok(())
    }

    fn get_missing_tools_message(&self) -> String {
        "⚠️  MicroPython development tools are not properly set up.\n".to_string()
            + "   Please install at least one of the following:\n"
            + "   - mpremote (recommended): pip install mpremote\n"
            + "   - ampy (legacy): pip install adafruit-ampy\n"
            + "   - For monitoring: pip install pyserial\n"
            + "   Press Enter to continue anyway, or 'q' to quit."
    }
}

impl MicroPythonHandler {
    fn is_tool_available(&self, tool: &str) -> bool {
        std::process::Command::new("which")
            .arg(tool)
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    fn has_micropython_imports(&self, project_dir: &Path) -> bool {
        // Check if any Python files contain MicroPython-specific imports
        let python_files = self.find_python_files_sync(project_dir);

        for file_path in python_files {
            if let Ok(content) = fs::read_to_string(&file_path) {
                if content.contains("import machine")
                    || content.contains("from machine import")
                    || content.contains("import micropython")
                    || content.contains("import esp")
                    || content.contains("from esp import")
                    || content.contains("import network")
                    || content.contains("import ubinascii")
                    || content.contains("import utime")
                {
                    return true;
                }
            }
        }

        false
    }

    fn find_python_files_sync(&self, project_dir: &Path) -> Vec<PathBuf> {
        let mut python_files = Vec::new();

        if let Ok(entries) = project_dir.read_dir() {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() {
                    if let Some(extension) = path.extension() {
                        if extension == "py" {
                            python_files.push(path);
                        }
                    }
                }
            }
        }

        python_files.sort();
        python_files
    }

    async fn find_python_files(
        &self,
        project_dir: &Path,
        board_config: &BoardConfig,
        tx: mpsc::UnboundedSender<AppEvent>,
    ) -> Result<Vec<BuildArtifact>> {
        let mut artifacts = Vec::new();
        let python_files = self.find_python_files_sync(project_dir);

        let _ = tx.send(AppEvent::BuildOutput(
            board_config.name.clone(),
            format!("✅ Found {} Python files", python_files.len()),
        ));

        for file_path in python_files {
            let file_name = file_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown.py")
                .to_string();

            let _ = tx.send(AppEvent::BuildOutput(
                board_config.name.clone(),
                format!("  📄 {}", file_name),
            ));

            artifacts.push(BuildArtifact {
                name: file_name.clone(),
                file_path: file_path.clone(),
                artifact_type: ArtifactType::Binary, // Python source files
                offset: None,
            });
        }

        if artifacts.is_empty() {
            return Err(anyhow::anyhow!(
                "No Python files found in project directory"
            ));
        }

        Ok(artifacts)
    }

    fn detect_target_chip(&self, project_dir: &Path) -> Result<String> {
        // Try to detect target from comments or file content
        let python_files = self.find_python_files_sync(project_dir);

        for file_path in python_files {
            if let Ok(content) = fs::read_to_string(&file_path) {
                // Look for ESP32 chip indicators in comments or code
                if content.contains("ESP32-S3") || content.contains("esp32s3") {
                    return Ok("ESP32-S3".to_string());
                } else if content.contains("ESP32-C6") || content.contains("esp32c6") {
                    return Ok("ESP32-C6".to_string());
                } else if content.contains("ESP32-C3") || content.contains("esp32c3") {
                    return Ok("ESP32-C3".to_string());
                } else if content.contains("ESP32-P4") || content.contains("esp32p4") {
                    return Ok("ESP32-P4".to_string());
                } else if content.contains("ESP32") || content.contains("esp32") {
                    return Ok("ESP32".to_string());
                }
            }
        }

        // Default to ESP32 if we can't determine
        Ok("ESP32".to_string())
    }

    async fn detect_upload_tool(&self, tx: mpsc::UnboundedSender<AppEvent>) -> Result<String> {
        if self.is_tool_available("mpremote") {
            let _ = tx.send(AppEvent::BuildOutput(
                "upload".to_string(),
                "🔧 Using mpremote for file upload".to_string(),
            ));
            Ok("mpremote".to_string())
        } else if self.is_tool_available("ampy") {
            let _ = tx.send(AppEvent::BuildOutput(
                "upload".to_string(),
                "🔧 Using ampy for file upload".to_string(),
            ));
            Ok("ampy".to_string())
        } else {
            Err(anyhow::anyhow!("No MicroPython upload tool available"))
        }
    }

    async fn upload_with_mpremote(
        &self,
        project_dir: &Path,
        board_config: &BoardConfig,
        artifacts: &[BuildArtifact],
        port: Option<&str>,
        tx: mpsc::UnboundedSender<AppEvent>,
    ) -> Result<()> {
        let _ = tx.send(AppEvent::BuildOutput(
            board_config.name.clone(),
            "📤 Uploading files with mpremote...".to_string(),
        ));

        for artifact in artifacts {
            let file_name = artifact
                .file_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown.py");

            let _ = tx.send(AppEvent::BuildOutput(
                board_config.name.clone(),
                format!("📄 Uploading {}", file_name),
            ));

            let mut cmd = Command::new("mpremote");

            if let Some(port) = port {
                cmd.args(["connect", port]);
            }

            cmd.current_dir(project_dir)
                .args(["cp", &artifact.file_path.to_string_lossy(), ":"])
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped());

            let output = cmd.output().await.context("Failed to run mpremote")?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                let _ = tx.send(AppEvent::BuildOutput(
                    board_config.name.clone(),
                    format!("❌ Upload failed for {}: {}", file_name, stderr.trim()),
                ));
                return Err(anyhow::anyhow!("mpremote upload failed for {}", file_name));
            } else {
                let _ = tx.send(AppEvent::BuildOutput(
                    board_config.name.clone(),
                    format!("✅ Uploaded {}", file_name),
                ));
            }
        }

        let _ = tx.send(AppEvent::BuildOutput(
            board_config.name.clone(),
            "✅ MicroPython file upload completed successfully".to_string(),
        ));

        Ok(())
    }

    async fn upload_with_ampy(
        &self,
        project_dir: &Path,
        board_config: &BoardConfig,
        artifacts: &[BuildArtifact],
        port: Option<&str>,
        tx: mpsc::UnboundedSender<AppEvent>,
    ) -> Result<()> {
        let _ = tx.send(AppEvent::BuildOutput(
            board_config.name.clone(),
            "📤 Uploading files with ampy...".to_string(),
        ));

        let target_port = port.unwrap_or("/dev/ttyUSB0"); // Default port for ampy

        for artifact in artifacts {
            let file_name = artifact
                .file_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown.py");

            let _ = tx.send(AppEvent::BuildOutput(
                board_config.name.clone(),
                format!("📄 Uploading {}", file_name),
            ));

            let mut cmd = Command::new("ampy");
            cmd.current_dir(project_dir)
                .args([
                    "-p",
                    target_port,
                    "put",
                    &artifact.file_path.to_string_lossy(),
                ])
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped());

            let output = cmd.output().await.context("Failed to run ampy")?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                let _ = tx.send(AppEvent::BuildOutput(
                    board_config.name.clone(),
                    format!("❌ Upload failed for {}: {}", file_name, stderr.trim()),
                ));
                return Err(anyhow::anyhow!("ampy upload failed for {}", file_name));
            } else {
                let _ = tx.send(AppEvent::BuildOutput(
                    board_config.name.clone(),
                    format!("✅ Uploaded {}", file_name),
                ));
            }
        }

        let _ = tx.send(AppEvent::BuildOutput(
            board_config.name.clone(),
            "✅ MicroPython file upload completed successfully".to_string(),
        ));

        Ok(())
    }

    async fn monitor_with_mpremote(
        &self,
        _project_dir: &Path,
        board_config: &BoardConfig,
        port: Option<&str>,
        _baud_rate: u32,
        tx: mpsc::UnboundedSender<AppEvent>,
    ) -> Result<()> {
        let mut cmd = Command::new("mpremote");

        if let Some(port) = port {
            cmd.args(["connect", port]);
        }

        cmd.args(["repl"])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        let mut child = cmd.spawn().context("Failed to start mpremote repl")?;
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

        let status = child.wait().await.context("Failed to wait for mpremote")?;

        if status.success() {
            let _ = tx.send(AppEvent::BuildOutput(
                board_config.name.clone(),
                "✅ MicroPython REPL session completed".to_string(),
            ));
        }

        Ok(())
    }

    async fn monitor_with_serial(
        &self,
        _project_dir: &Path,
        board_config: &BoardConfig,
        port: Option<&str>,
        baud_rate: u32,
        tx: mpsc::UnboundedSender<AppEvent>,
    ) -> Result<()> {
        let _ = tx.send(AppEvent::BuildOutput(
            board_config.name.clone(),
            "💡 Using basic serial monitoring (install mpremote for full REPL features)"
                .to_string(),
        ));

        // Use miniterm.py or similar for basic serial monitoring
        let mut cmd = if self.is_tool_available("miniterm.py") {
            let mut c = Command::new("miniterm.py");
            if let Some(port) = port {
                c.arg(port);
            }
            c.arg(baud_rate.to_string());
            c
        } else if self.is_tool_available("screen") {
            let mut c = Command::new("screen");
            if let Some(port) = port {
                c.args([port, &baud_rate.to_string()]);
            }
            c
        } else {
            return Err(anyhow::anyhow!("No serial monitoring tool available"));
        };

        let mut child = cmd.spawn().context("Failed to start serial monitor")?;
        let status = child
            .wait()
            .await
            .context("Failed to wait for serial monitor")?;

        if status.success() {
            let _ = tx.send(AppEvent::BuildOutput(
                board_config.name.clone(),
                "✅ Serial monitoring session completed".to_string(),
            ));
        }

        Ok(())
    }
}
