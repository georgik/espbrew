use crate::models::{AppEvent, ArtifactType, BuildArtifact, ProjectBoardConfig, ProjectType};
use crate::projects::registry::ProjectHandler;

use anyhow::{Context, Result};
use async_trait::async_trait;
use std::fs;
use std::path::{Path, PathBuf};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;

/// Handler for CircuitPython projects
pub struct CircuitPythonHandler;

#[async_trait]
impl ProjectHandler for CircuitPythonHandler {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn project_type(&self) -> ProjectType {
        ProjectType::CircuitPython
    }

    fn can_handle(&self, project_dir: &Path) -> bool {
        // Look for code.py (CircuitPython main file), lib/ directory, or CircuitPython-specific imports
        project_dir.join("code.py").exists()
            || project_dir.join("lib").is_dir()
            || self.has_circuitpython_imports(project_dir)
    }

    fn discover_boards(&self, project_dir: &Path) -> Result<Vec<ProjectBoardConfig>> {
        let mut boards = Vec::new();

        // CircuitPython doesn't have strict board configuration files
        // We'll discover based on Python files and attempt to detect target
        let python_files = self.find_python_files(project_dir)?;

        if python_files.is_empty() {
            return Ok(Vec::new());
        }

        // Check for board-specific directories or configuration
        let detected_boards = self.detect_boards_from_files(project_dir, &python_files)?;

        if detected_boards.is_empty() {
            // Default to generic ESP32-S3 configuration (CircuitPython commonly runs on S3)
            boards.push(ProjectBoardConfig {
                name: "esp32s3".to_string(),
                config_file: project_dir.join("code.py"),
                build_dir: project_dir.to_path_buf(),
                target: Some("ESP32-S3".to_string()),
                project_type: ProjectType::CircuitPython,
            });
        } else {
            boards.extend(detected_boards);
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
            "🏗️  Preparing CircuitPython files...".to_string(),
        ));

        // CircuitPython doesn't have a traditional build step
        // We collect Python files as "artifacts"
        let python_files = self.find_python_files(project_dir)?;
        let mut artifacts = Vec::new();

        for py_file in python_files {
            let name = py_file
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown")
                .to_string();

            artifacts.push(BuildArtifact {
                name,
                file_path: py_file,
                artifact_type: ArtifactType::Python,
                offset: None,
            });
        }

        // Also include library files
        let lib_dir = project_dir.join("lib");
        if lib_dir.is_dir() {
            if let Ok(lib_files) = self.find_library_files(&lib_dir) {
                for lib_file in lib_files {
                    let name = format!(
                        "lib/{}",
                        lib_file
                            .strip_prefix(project_dir)
                            .unwrap_or(&lib_file)
                            .display()
                    );

                    artifacts.push(BuildArtifact {
                        name,
                        file_path: lib_file,
                        artifact_type: ArtifactType::Python,
                        offset: None,
                    });
                }
            }
        }

        let _ = tx.send(AppEvent::BuildOutput(
            board_config.name.clone(),
            format!("✅ Found {} Python files ready for upload", artifacts.len()),
        ));

        Ok(artifacts)
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
            "🔥 Starting CircuitPython upload...".to_string(),
        ));

        // CircuitPython supports multiple upload methods
        // 1. Mass storage (copy files directly if mounted)
        // 2. circup for library management
        // 3. mpremote/ampy as fallback

        // Try mass storage first
        if let Some(drive_path) = self.find_circuitpy_drive() {
            self.upload_via_mass_storage(project_dir, board_config, artifacts, &drive_path, tx)
                .await
        }
        // Try circup for libraries
        else if self.is_tool_available("circup") && port.is_some() {
            self.upload_with_circup(project_dir, board_config, artifacts, port.unwrap(), tx)
                .await
        }
        // Fall back to mpremote/ampy
        else {
            let port_str = port.unwrap_or("/dev/ttyUSB0");
            if self.is_tool_available("mpremote") {
                self.upload_with_mpremote(project_dir, board_config, artifacts, port_str, tx)
                    .await
            } else if self.is_tool_available("ampy") {
                self.upload_with_ampy(project_dir, board_config, artifacts, port_str, tx)
                    .await
            } else {
                let _ = tx.send(AppEvent::BuildOutput(
                    board_config.name.clone(),
                    "❌ No suitable upload method available".to_string(),
                ));
                Err(anyhow::anyhow!("No suitable upload tool available"))
            }
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
        let port_str = port.unwrap_or("/dev/ttyUSB0");

        let _ = tx.send(AppEvent::BuildOutput(
            board_config.name.clone(),
            format!(
                "📺 Starting CircuitPython REPL monitor on {} at {} baud",
                port_str, baud_rate
            ),
        ));

        // Try mpremote first, then fall back to serial tools
        if self.is_tool_available("mpremote") {
            self.monitor_with_mpremote(project_dir, board_config, port_str, tx)
                .await
        } else if self.is_tool_available("screen") {
            self.monitor_with_screen(project_dir, board_config, port_str, baud_rate, tx)
                .await
        } else {
            let _ = tx.send(AppEvent::BuildOutput(
                board_config.name.clone(),
                "❌ No suitable monitoring tool available (mpremote or screen)".to_string(),
            ));
            Err(anyhow::anyhow!("No suitable monitoring tool available"))
        }
    }

    async fn clean_board(
        &self,
        project_dir: &Path,
        board_config: &ProjectBoardConfig,
        tx: mpsc::UnboundedSender<AppEvent>,
    ) -> Result<()> {
        let _ = tx.send(AppEvent::BuildOutput(
            board_config.name.clone(),
            "🧹 Cleaning CircuitPython cache files...".to_string(),
        ));

        // Clean __pycache__ directories
        self.clean_pycache(project_dir).await?;

        let _ = tx.send(AppEvent::BuildOutput(
            board_config.name.clone(),
            "✅ Clean completed successfully".to_string(),
        ));

        Ok(())
    }

    fn get_build_command(&self, project_dir: &Path, _board_config: &ProjectBoardConfig) -> String {
        // CircuitPython doesn't have a build command
        format!(
            "# CircuitPython project - no build required\n# Python files in {} are ready for upload",
            project_dir.display()
        )
    }

    fn get_flash_command(
        &self,
        project_dir: &Path,
        _board_config: &ProjectBoardConfig,
        port: Option<&str>,
    ) -> String {
        let project_dir_str =
            if std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")) != *project_dir {
                format!("cd {} && ", project_dir.display())
            } else {
                String::new()
            };

        // Show the most appropriate command based on available tools
        if let Some(_drive) = self.find_circuitpy_drive() {
            format!("{}# Copy files to CIRCUITPY drive", project_dir_str)
        } else if self.is_tool_available("circup") {
            let _port_str = port.unwrap_or("/dev/ttyUSB0");
            format!("{}circup install -r requirements.txt", project_dir_str)
        } else if self.is_tool_available("mpremote") {
            let port_str = port.unwrap_or("/dev/ttyUSB0");
            format!("{}mpremote connect {} cp *.py :", project_dir_str, port_str)
        } else {
            let port_str = port.unwrap_or("/dev/ttyUSB0");
            format!("{}ampy --port {} put code.py", project_dir_str, port_str)
        }
    }

    fn check_tools_available(&self) -> Result<(), String> {
        // CircuitPython has multiple upload options, so it's more flexible
        if !self.is_tool_available("mpremote")
            && !self.is_tool_available("ampy")
            && !self.is_tool_available("circup")
            && self.find_circuitpy_drive().is_none()
        {
            return Err(
                "No suitable upload method found (mpremote, ampy, circup, or CIRCUITPY drive)"
                    .to_string(),
            );
        }
        Ok(())
    }

    fn get_missing_tools_message(&self) -> String {
        "⚠️  CircuitPython development environment is not properly set up.\n".to_string()
            + "   Please ensure one of the following is available:\n"
            + "   - CIRCUITPY drive mounted (easiest method)\n"
            + "   - circup: pip install circup (for library management)\n"
            + "   - mpremote: pip install mpremote\n"
            + "   - ampy: pip install adafruit-ampy\n"
            + "   - For monitoring: screen or other serial terminal\n"
            + "   Press Enter to continue anyway, or 'q' to quit."
    }
}

impl CircuitPythonHandler {
    fn has_circuitpython_imports(&self, project_dir: &Path) -> bool {
        let circuitpython_imports = [
            "import board",
            "from board import",
            "import digitalio",
            "import analogio",
            "import busio",
            "import displayio",
            "import adafruit_",
            "import circuitpython_",
            "import supervisor",
            "import microcontroller",
        ];

        if let Ok(entries) = project_dir.read_dir() {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().map_or(false, |ext| ext == "py") {
                    if let Ok(content) = fs::read_to_string(&path) {
                        for import in &circuitpython_imports {
                            if content.contains(import) {
                                return true;
                            }
                        }
                    }
                }
            }
        }

        false
    }

    fn find_python_files(&self, project_dir: &Path) -> Result<Vec<PathBuf>> {
        let mut python_files = Vec::new();

        if let Ok(entries) = project_dir.read_dir() {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() && path.extension().map_or(false, |ext| ext == "py") {
                    python_files.push(path);
                }
            }
        }

        Ok(python_files)
    }

    fn find_library_files(&self, lib_dir: &Path) -> Result<Vec<PathBuf>> {
        let mut lib_files = Vec::new();

        fn collect_files(dir: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
            if let Ok(entries) = dir.read_dir() {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_file()
                        && path
                            .extension()
                            .map_or(false, |ext| ext == "py" || ext == "mpy")
                    {
                        files.push(path);
                    } else if path.is_dir() {
                        collect_files(&path, files)?;
                    }
                }
            }
            Ok(())
        }

        collect_files(lib_dir, &mut lib_files)?;
        Ok(lib_files)
    }

    fn detect_boards_from_files(
        &self,
        project_dir: &Path,
        python_files: &[PathBuf],
    ) -> Result<Vec<ProjectBoardConfig>> {
        let mut boards = Vec::new();

        // Look for board-specific configurations in comments or filenames
        for py_file in python_files {
            if let Ok(content) = fs::read_to_string(py_file) {
                // Check for board hints in comments or imports
                let target = if content.contains("ESP32-S3")
                    || content.contains("esp32s3")
                    || content.contains("board.ESP32S3")
                {
                    "ESP32-S3"
                } else if content.contains("ESP32-C3")
                    || content.contains("esp32c3")
                    || content.contains("board.ESP32C3")
                {
                    "ESP32-C3"
                } else if content.contains("ESP32-C6")
                    || content.contains("esp32c6")
                    || content.contains("board.ESP32C6")
                {
                    "ESP32-C6"
                } else if content.contains("ESP32")
                    || content.contains("esp32")
                    || content.contains("board.ESP32")
                {
                    "ESP32"
                } else {
                    "ESP32-S3" // Default for CircuitPython
                };

                let board_name = target.to_lowercase().replace('-', "");

                // Avoid duplicates
                if !boards
                    .iter()
                    .any(|b: &ProjectBoardConfig| b.name == board_name)
                {
                    boards.push(ProjectBoardConfig {
                        name: board_name,
                        config_file: py_file.clone(),
                        build_dir: project_dir.to_path_buf(),
                        target: Some(target.to_string()),
                        project_type: ProjectType::CircuitPython,
                    });
                }
            }
        }

        Ok(boards)
    }

    fn find_circuitpy_drive(&self) -> Option<PathBuf> {
        // Common mount points for CIRCUITPY drive
        let possible_paths = [
            "/Volumes/CIRCUITPY", // macOS
            "/media/CIRCUITPY",   // Linux
            "/mnt/CIRCUITPY",     // Linux alternative
            "D:\\",               // Windows (would need adjustment for actual detection)
        ];

        for path in &possible_paths {
            let path_buf = PathBuf::from(path);
            if path_buf.is_dir() {
                return Some(path_buf);
            }
        }

        None
    }

    async fn upload_via_mass_storage(
        &self,
        project_dir: &Path,
        board_config: &ProjectBoardConfig,
        artifacts: &[BuildArtifact],
        drive_path: &Path,
        tx: mpsc::UnboundedSender<AppEvent>,
    ) -> Result<()> {
        let _ = tx.send(AppEvent::BuildOutput(
            board_config.name.clone(),
            format!(
                "📤 Uploading to CIRCUITPY drive at {}",
                drive_path.display()
            ),
        ));

        for artifact in artifacts {
            if artifact.artifact_type == ArtifactType::Python {
                let relative_path = artifact
                    .file_path
                    .strip_prefix(project_dir)
                    .unwrap_or(&artifact.file_path);
                let dest_path = drive_path.join(relative_path);

                // Create parent directories if needed
                if let Some(parent) = dest_path.parent() {
                    fs::create_dir_all(parent).context("Failed to create destination directory")?;
                }

                fs::copy(&artifact.file_path, &dest_path)
                    .context("Failed to copy file to CIRCUITPY drive")?;

                let _ = tx.send(AppEvent::BuildOutput(
                    board_config.name.clone(),
                    format!("✅ Copied {}", relative_path.display()),
                ));
            }
        }

        let _ = tx.send(AppEvent::BuildOutput(
            board_config.name.clone(),
            "✅ Upload completed via mass storage".to_string(),
        ));

        Ok(())
    }

    async fn upload_with_circup(
        &self,
        project_dir: &Path,
        board_config: &ProjectBoardConfig,
        _artifacts: &[BuildArtifact],
        port: &str,
        tx: mpsc::UnboundedSender<AppEvent>,
    ) -> Result<()> {
        let _ = tx.send(AppEvent::BuildOutput(
            board_config.name.clone(),
            "📤 Installing libraries with circup...".to_string(),
        ));

        // Check for requirements.txt or install individual libraries
        let requirements_file = project_dir.join("requirements.txt");

        let mut cmd = Command::new("circup");
        cmd.current_dir(project_dir);

        if requirements_file.exists() {
            cmd.args(["install", "-r", "requirements.txt"]);
        } else {
            cmd.args(["update"]);
        }

        if !port.is_empty() {
            cmd.args(["--port", port]);
        }

        let output = cmd.output().await.context("Failed to run circup")?;

        if output.status.success() {
            let _ = tx.send(AppEvent::BuildOutput(
                board_config.name.clone(),
                "✅ Libraries installed with circup".to_string(),
            ));
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let _ = tx.send(AppEvent::BuildOutput(
                board_config.name.clone(),
                format!("❌ circup failed: {}", stderr.trim()),
            ));
            return Err(anyhow::anyhow!("circup installation failed"));
        }

        Ok(())
    }

    async fn upload_with_mpremote(
        &self,
        project_dir: &Path,
        board_config: &ProjectBoardConfig,
        artifacts: &[BuildArtifact],
        port: &str,
        tx: mpsc::UnboundedSender<AppEvent>,
    ) -> Result<()> {
        for artifact in artifacts {
            if artifact.artifact_type == ArtifactType::Python {
                let _ = tx.send(AppEvent::BuildOutput(
                    board_config.name.clone(),
                    format!("📤 Uploading {}", artifact.file_path.display()),
                ));

                let mut cmd = Command::new("mpremote");
                cmd.current_dir(project_dir)
                    .args(["connect", port])
                    .args(["cp", &artifact.file_path.to_string_lossy(), ":"])
                    .stdout(std::process::Stdio::piped())
                    .stderr(std::process::Stdio::piped());

                let output = cmd.output().await.context("Failed to run mpremote")?;

                if output.status.success() {
                    let _ = tx.send(AppEvent::BuildOutput(
                        board_config.name.clone(),
                        format!("✅ Uploaded {}", artifact.name),
                    ));
                } else {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    let _ = tx.send(AppEvent::BuildOutput(
                        board_config.name.clone(),
                        format!("❌ Failed to upload {}: {}", artifact.name, stderr.trim()),
                    ));
                    return Err(anyhow::anyhow!("Upload failed"));
                }
            }
        }

        Ok(())
    }

    async fn upload_with_ampy(
        &self,
        project_dir: &Path,
        board_config: &ProjectBoardConfig,
        artifacts: &[BuildArtifact],
        port: &str,
        tx: mpsc::UnboundedSender<AppEvent>,
    ) -> Result<()> {
        for artifact in artifacts {
            if artifact.artifact_type == ArtifactType::Python {
                let _ = tx.send(AppEvent::BuildOutput(
                    board_config.name.clone(),
                    format!("📤 Uploading {} with ampy", artifact.file_path.display()),
                ));

                let mut cmd = Command::new("ampy");
                cmd.current_dir(project_dir)
                    .args(["--port", port])
                    .args(["put", &artifact.file_path.to_string_lossy()])
                    .stdout(std::process::Stdio::piped())
                    .stderr(std::process::Stdio::piped());

                let output = cmd.output().await.context("Failed to run ampy")?;

                if output.status.success() {
                    let _ = tx.send(AppEvent::BuildOutput(
                        board_config.name.clone(),
                        format!("✅ Uploaded {}", artifact.name),
                    ));
                } else {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    let _ = tx.send(AppEvent::BuildOutput(
                        board_config.name.clone(),
                        format!("❌ Failed to upload {}: {}", artifact.name, stderr.trim()),
                    ));
                    return Err(anyhow::anyhow!("Upload failed"));
                }
            }
        }

        Ok(())
    }

    async fn monitor_with_mpremote(
        &self,
        _project_dir: &Path,
        board_config: &ProjectBoardConfig,
        port: &str,
        tx: mpsc::UnboundedSender<AppEvent>,
    ) -> Result<()> {
        let mut cmd = Command::new("mpremote");
        cmd.args(["connect", port, "repl"])
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

        let _ = child
            .wait()
            .await
            .context("Failed to wait for mpremote repl")?;
        Ok(())
    }

    async fn monitor_with_screen(
        &self,
        _project_dir: &Path,
        board_config: &ProjectBoardConfig,
        port: &str,
        baud_rate: u32,
        tx: mpsc::UnboundedSender<AppEvent>,
    ) -> Result<()> {
        let _ = tx.send(AppEvent::BuildOutput(
            board_config.name.clone(),
            format!("📺 Starting screen session: screen {} {}", port, baud_rate),
        ));

        let mut cmd = Command::new("screen");
        cmd.args([port, &baud_rate.to_string()])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        let mut child = cmd.spawn().context("Failed to start screen")?;
        let _ = child.wait().await.context("Failed to wait for screen")?;

        Ok(())
    }

    async fn clean_pycache(&self, project_dir: &Path) -> Result<()> {
        fn clean_dir(dir: &Path) -> Result<()> {
            if let Ok(entries) = dir.read_dir() {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        if path.file_name().unwrap_or_default() == "__pycache__" {
                            fs::remove_dir_all(&path)
                                .context("Failed to remove __pycache__ directory")?;
                        } else {
                            clean_dir(&path)?;
                        }
                    }
                }
            }
            Ok(())
        }

        clean_dir(project_dir)
    }

    fn is_tool_available(&self, tool: &str) -> bool {
        std::process::Command::new("which")
            .arg(tool)
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }
}
