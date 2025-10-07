use crate::AppEvent;
use crate::project::{ArtifactType, BoardConfig, BuildArtifact, ProjectHandler, ProjectType};
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
        // Check for code.py (CircuitPython's main file) or other CircuitPython indicators
        let has_code_py = project_dir.join("code.py").exists();
        let has_main_py = project_dir.join("main.py").exists();
        let has_boot_py = project_dir.join("boot.py").exists();

        // Also check for CircuitPython-specific libraries or imports
        let has_circuitpython_indicator = self.has_circuitpython_imports(project_dir);

        // CircuitPython often uses lib/ directory for libraries
        let has_lib_dir = project_dir.join("lib").exists();

        has_code_py || (has_circuitpython_indicator && (has_main_py || has_boot_py || has_lib_dir))
    }

    fn discover_boards(&self, project_dir: &Path) -> Result<Vec<BoardConfig>> {
        let mut boards = Vec::new();

        // CircuitPython doesn't have traditional build configurations
        // Create a single configuration representing the project
        let project_name = project_dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("circuitpython-project")
            .to_string();

        let target = self.detect_target_chip(project_dir)?;

        boards.push(BoardConfig {
            name: project_name,
            config_file: project_dir.join("code.py"), // Use code.py as config reference
            build_dir: project_dir.to_path_buf(),     // No separate build dir for CircuitPython
            target: Some(target),
            project_type: ProjectType::CircuitPython,
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
            "🐍 CircuitPython project - no compilation needed".to_string(),
        ));

        let _ = tx.send(AppEvent::BuildOutput(
            board_config.name.clone(),
            "📁 Scanning for Python source files and libraries...".to_string(),
        ));

        // Find all Python files and libraries in the project
        self.find_circuitpython_files(project_dir, board_config, tx.clone())
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
            "🔥 Starting CircuitPython file upload...".to_string(),
        ));

        // CircuitPython typically uses USB mass storage for file transfer
        // But also supports serial upload tools
        let upload_method = self.detect_upload_method(tx.clone()).await?;

        match upload_method.as_str() {
            "mass_storage" => {
                self.upload_via_mass_storage(project_dir, board_config, artifacts, tx)
                    .await
            }
            "circup" => {
                self.upload_with_circup(project_dir, board_config, artifacts, port, tx)
                    .await
            }
            "mpremote" => {
                self.upload_with_mpremote(project_dir, board_config, artifacts, port, tx)
                    .await
            }
            _ => Err(anyhow::anyhow!(
                "No suitable CircuitPython upload method found"
            )),
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
                "📺 Starting CircuitPython REPL monitor on {} at {} baud",
                port.unwrap_or("auto-detect"),
                baud_rate
            ),
        ));

        // Try different monitoring methods
        if self.is_tool_available("circup") {
            self.monitor_with_circup(project_dir, board_config, port, baud_rate, tx)
                .await
        } else if self.is_tool_available("mpremote") {
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
            "🧹 CircuitPython clean operation...".to_string(),
        ));

        // For CircuitPython, "clean" means providing info about resetting the device
        let _ = tx.send(AppEvent::BuildOutput(
            board_config.name.clone(),
            "⚠️  CircuitPython clean suggestions:".to_string(),
        ));

        let _ = tx.send(AppEvent::BuildOutput(
            board_config.name.clone(),
            "💡 1. Delete files from CIRCUITPY drive via file manager".to_string(),
        ));

        let _ = tx.send(AppEvent::BuildOutput(
            board_config.name.clone(),
            "💡 2. Reset device: import microcontroller; microcontroller.reset()".to_string(),
        ));

        let _ = tx.send(AppEvent::BuildOutput(
            board_config.name.clone(),
            "💡 3. Factory reset: Hold BOOT button while connecting USB".to_string(),
        ));

        Ok(())
    }

    fn get_build_command(&self, _project_dir: &Path, _board_config: &BoardConfig) -> String {
        "# CircuitPython projects don't require compilation".to_string()
    }

    fn get_flash_command(
        &self,
        project_dir: &Path,
        _board_config: &BoardConfig,
        _port: Option<&str>,
    ) -> String {
        if std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")) != *project_dir {
            format!(
                "cd {} && cp *.py /Volumes/CIRCUITPY/ # or use circup install",
                project_dir.display()
            )
        } else {
            "cp *.py /Volumes/CIRCUITPY/ # or use circup install".to_string()
        }
    }

    fn check_tools_available(&self) -> Result<(), String> {
        // CircuitPython can work with just file system access, but tools help
        // We'll be permissive and just warn about missing tools
        Ok(())
    }

    fn get_missing_tools_message(&self) -> String {
        "⚠️  CircuitPython development tools recommendation.\n".to_string()
            + "   For enhanced experience, consider installing:\n"
            + "   - circup (library manager): pip install circup\n"
            + "   - mpremote (REPL access): pip install mpremote\n"
            + "   - Basic file copy to CIRCUITPY drive also works\n"
            + "   Press Enter to continue anyway, or 'q' to quit."
    }
}

impl CircuitPythonHandler {
    fn is_tool_available(&self, tool: &str) -> bool {
        std::process::Command::new("which")
            .arg(tool)
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    fn has_circuitpython_imports(&self, project_dir: &Path) -> bool {
        // Check if any Python files contain CircuitPython-specific imports
        let python_files = self.find_python_files_sync(project_dir);

        for file_path in python_files {
            if let Ok(content) = fs::read_to_string(&file_path) {
                if content.contains("import board")
                    || content.contains("from board import")
                    || content.contains("import digitalio")
                    || content.contains("import analogio")
                    || content.contains("import busio")
                    || content.contains("import displayio")
                    || content.contains("import adafruit_")
                    || content.contains("import circuitpython")
                    || content.contains("import microcontroller")
                    || content.contains("from microcontroller import")
                {
                    return true;
                }
            }
        }

        false
    }

    fn find_python_files_sync(&self, project_dir: &Path) -> Vec<PathBuf> {
        let mut python_files = Vec::new();

        // Check root directory
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

        // Check lib directory (common in CircuitPython projects)
        let lib_dir = project_dir.join("lib");
        if lib_dir.exists() {
            if let Ok(entries) = lib_dir.read_dir() {
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
        }

        python_files.sort();
        python_files
    }

    async fn find_circuitpython_files(
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

            let relative_path = file_path
                .strip_prefix(project_dir)
                .unwrap_or(&file_path)
                .to_string_lossy()
                .to_string();

            let _ = tx.send(AppEvent::BuildOutput(
                board_config.name.clone(),
                format!("  📄 {}", relative_path),
            ));

            artifacts.push(BuildArtifact {
                name: file_name.clone(),
                file_path: file_path.clone(),
                artifact_type: ArtifactType::Binary, // Python source files
                offset: None,
            });
        }

        // Also look for requirements.txt or similar
        let requirements = project_dir.join("requirements.txt");
        if requirements.exists() {
            let _ = tx.send(AppEvent::BuildOutput(
                board_config.name.clone(),
                "  📋 requirements.txt".to_string(),
            ));

            artifacts.push(BuildArtifact {
                name: "requirements.txt".to_string(),
                file_path: requirements,
                artifact_type: ArtifactType::Binary,
                offset: None,
            });
        }

        if artifacts.is_empty() {
            return Err(anyhow::anyhow!(
                "No CircuitPython files found in project directory"
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
                } else if content.contains("ESP32") || content.contains("esp32") {
                    return Ok("ESP32".to_string());
                }

                // CircuitPython also runs on other microcontrollers
                if content.contains("Raspberry Pi Pico") || content.contains("rp2040") {
                    return Ok("RP2040".to_string());
                } else if content.contains("SAMD") {
                    return Ok("SAMD".to_string());
                }
            }
        }

        // Default to ESP32-S3 (common CircuitPython target) if we can't determine
        Ok("ESP32-S3".to_string())
    }

    async fn detect_upload_method(&self, tx: mpsc::UnboundedSender<AppEvent>) -> Result<String> {
        // Check if CIRCUITPY drive is mounted (mass storage method)
        if self.is_circuitpy_mounted() {
            let _ = tx.send(AppEvent::BuildOutput(
                "upload".to_string(),
                "🔧 Using mass storage (CIRCUITPY drive) for file upload".to_string(),
            ));
            Ok("mass_storage".to_string())
        } else if self.is_tool_available("circup") {
            let _ = tx.send(AppEvent::BuildOutput(
                "upload".to_string(),
                "🔧 Using circup for file upload".to_string(),
            ));
            Ok("circup".to_string())
        } else if self.is_tool_available("mpremote") {
            let _ = tx.send(AppEvent::BuildOutput(
                "upload".to_string(),
                "🔧 Using mpremote for file upload".to_string(),
            ));
            Ok("mpremote".to_string())
        } else {
            Ok("mass_storage".to_string()) // Fallback to mass storage
        }
    }

    fn is_circuitpy_mounted(&self) -> bool {
        // Check common CircuitPython mount points
        let mount_points = vec![
            "/Volumes/CIRCUITPY", // macOS
            "/media/CIRCUITPY",   // Linux
            "/mnt/CIRCUITPY",     // Linux alternative
        ];

        for mount_point in mount_points {
            if Path::new(mount_point).exists() {
                return true;
            }
        }

        false
    }

    async fn upload_via_mass_storage(
        &self,
        project_dir: &Path,
        board_config: &BoardConfig,
        artifacts: &[BuildArtifact],
        tx: mpsc::UnboundedSender<AppEvent>,
    ) -> Result<()> {
        let _ = tx.send(AppEvent::BuildOutput(
            board_config.name.clone(),
            "📤 Uploading files via mass storage (CIRCUITPY drive)...".to_string(),
        ));

        // Find CIRCUITPY mount point
        let mount_points = vec![
            "/Volumes/CIRCUITPY", // macOS
            "/media/CIRCUITPY",   // Linux
            "/mnt/CIRCUITPY",     // Linux alternative
        ];

        let mut circuitpy_path = None;
        for mount_point in mount_points {
            if Path::new(mount_point).exists() {
                circuitpy_path = Some(PathBuf::from(mount_point));
                break;
            }
        }

        let circuitpy_drive = circuitpy_path.ok_or_else(|| {
            anyhow::anyhow!(
                "CIRCUITPY drive not found. Please ensure the device is in mass storage mode."
            )
        })?;

        let _ = tx.send(AppEvent::BuildOutput(
            board_config.name.clone(),
            format!("📁 Using CIRCUITPY drive at: {}", circuitpy_drive.display()),
        ));

        for artifact in artifacts {
            let file_name = artifact
                .file_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown.py");

            let _ = tx.send(AppEvent::BuildOutput(
                board_config.name.clone(),
                format!("📄 Copying {}", file_name),
            ));

            // Determine destination path
            let dest_path = if artifact.file_path.starts_with(project_dir.join("lib")) {
                // Files in lib/ go to lib/ on the device
                let lib_dir = circuitpy_drive.join("lib");
                fs::create_dir_all(&lib_dir)?;
                lib_dir.join(file_name)
            } else {
                // Root files go to root of CIRCUITPY drive
                circuitpy_drive.join(file_name)
            };

            // Copy file
            fs::copy(&artifact.file_path, &dest_path)
                .context(format!("Failed to copy {} to CIRCUITPY drive", file_name))?;

            let _ = tx.send(AppEvent::BuildOutput(
                board_config.name.clone(),
                format!("✅ Copied {} to {}", file_name, dest_path.display()),
            ));
        }

        let _ = tx.send(AppEvent::BuildOutput(
            board_config.name.clone(),
            "✅ CircuitPython file upload completed successfully".to_string(),
        ));

        let _ = tx.send(AppEvent::BuildOutput(
            board_config.name.clone(),
            "💡 Device will automatically restart to run the new code".to_string(),
        ));

        Ok(())
    }

    async fn upload_with_circup(
        &self,
        project_dir: &Path,
        board_config: &BoardConfig,
        _artifacts: &[BuildArtifact],
        _port: Option<&str>,
        tx: mpsc::UnboundedSender<AppEvent>,
    ) -> Result<()> {
        let _ = tx.send(AppEvent::BuildOutput(
            board_config.name.clone(),
            "📤 Installing libraries with circup...".to_string(),
        ));

        // circup install libraries based on requirements.txt or direct file copy
        let mut cmd = Command::new("circup");
        cmd.current_dir(project_dir)
            .args(["install"])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        let output = cmd.output().await.context("Failed to run circup")?;

        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let _ = tx.send(AppEvent::BuildOutput(
                board_config.name.clone(),
                format!("✅ circup output: {}", stdout.trim()),
            ));
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let _ = tx.send(AppEvent::BuildOutput(
                board_config.name.clone(),
                format!("⚠️ circup warning: {}", stderr.trim()),
            ));
        }

        // Also copy main files manually
        self.upload_via_mass_storage(project_dir, board_config, &[], tx)
            .await
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
            "📤 Uploading files with mpremote (CircuitPython compatible mode)...".to_string(),
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
            "✅ CircuitPython file upload completed successfully".to_string(),
        ));

        Ok(())
    }

    async fn monitor_with_circup(
        &self,
        _project_dir: &Path,
        board_config: &BoardConfig,
        _port: Option<&str>,
        _baud_rate: u32,
        tx: mpsc::UnboundedSender<AppEvent>,
    ) -> Result<()> {
        let _ = tx.send(AppEvent::BuildOutput(
            board_config.name.clone(),
            "💡 circup doesn't provide monitoring. Using serial fallback...".to_string(),
        ));

        self.monitor_with_serial(_project_dir, board_config, _port, _baud_rate, tx)
            .await
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
                "✅ CircuitPython REPL session completed".to_string(),
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
