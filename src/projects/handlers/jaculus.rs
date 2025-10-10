use crate::models::{AppEvent, ArtifactType, BuildArtifact, ProjectBoardConfig, ProjectType};
use crate::projects::registry::ProjectHandler;

use anyhow::{Context, Result};
use async_trait::async_trait;
use std::fs;
use std::path::{Path, PathBuf};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;

/// Handler for Jaculus projects (JavaScript runtime for ESP32)
pub struct JaculusHandler;

#[async_trait]
impl ProjectHandler for JaculusHandler {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn project_type(&self) -> ProjectType {
        ProjectType::Jaculus
    }

    fn can_handle(&self, project_dir: &Path) -> bool {
        // Look for Jaculus project indicators:
        // 1. JavaScript files (.js, .ts)
        // 2. jaculus.json configuration file
        // 3. package.json with Jaculus dependencies
        // 4. src/ directory with JavaScript files

        // Check for jaculus.json (main config file)
        if project_dir.join("jaculus.json").exists() {
            return true;
        }

        // Check for package.json with Jaculus-related content
        let package_json = project_dir.join("package.json");
        if package_json.exists() {
            if let Ok(content) = fs::read_to_string(&package_json) {
                if content.contains("jaculus") || content.contains("@jaculus/") {
                    return true;
                }
            }
        }

        // Check for JavaScript/TypeScript files in src/ directory
        let src_dir = project_dir.join("src");
        if src_dir.is_dir() {
            if self.has_js_files(&src_dir) {
                return true;
            }
        }

        // Check for JavaScript/TypeScript files in root directory with Jaculus patterns
        if self.has_js_files(project_dir) {
            // Look for Jaculus-specific patterns in JS files
            return self.has_jaculus_patterns(project_dir);
        }

        false
    }

    fn discover_boards(&self, project_dir: &Path) -> Result<Vec<ProjectBoardConfig>> {
        let mut boards = Vec::new();

        // Check for jaculus.json configuration
        let jaculus_config = project_dir.join("jaculus.json");
        if jaculus_config.exists() {
            if let Ok(config) = self.parse_jaculus_config(&jaculus_config) {
                boards.extend(config);
            }
        }

        // If no specific configuration, detect from JavaScript files
        if boards.is_empty() {
            let js_files = self.find_js_files(project_dir)?;
            if !js_files.is_empty() {
                // Determine target based on file content or default to ESP32
                let target = self.detect_esp32_target(project_dir, &js_files)?;

                boards.push(ProjectBoardConfig {
                    name: format!("jaculus-{}", target.to_lowercase().replace("-", "")),
                    config_file: js_files
                        .get(0)
                        .unwrap_or(&project_dir.join("index.js"))
                        .clone(),
                    build_dir: project_dir.to_path_buf(),
                    target: Some(target),
                    project_type: ProjectType::Jaculus,
                });
            }
        }

        // Default configuration if nothing found
        if boards.is_empty() {
            boards.push(ProjectBoardConfig {
                name: "jaculus-esp32".to_string(),
                config_file: project_dir.join("index.js"),
                build_dir: project_dir.to_path_buf(),
                target: Some("ESP32".to_string()),
                project_type: ProjectType::Jaculus,
            });
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
            "🏗️  Preparing Jaculus JavaScript files...".to_string(),
        ));

        // Jaculus doesn't have a traditional build step
        // We collect JavaScript/TypeScript files as "artifacts"
        let js_files = self.find_js_files(project_dir)?;
        let mut artifacts = Vec::new();

        for js_file in js_files {
            let name = js_file
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown")
                .to_string();

            artifacts.push(BuildArtifact {
                name,
                file_path: js_file,
                artifact_type: ArtifactType::Binary, // Use Binary for JS files
                offset: None,
            });
        }

        // Include configuration files
        let config_files = [
            project_dir.join("jaculus.json"),
            project_dir.join("package.json"),
            project_dir.join("tsconfig.json"),
        ];

        for config_file in config_files {
            if config_file.exists() {
                let name = config_file
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("config")
                    .to_string();

                artifacts.push(BuildArtifact {
                    name,
                    file_path: config_file,
                    artifact_type: ArtifactType::Binary,
                    offset: None,
                });
            }
        }

        let _ = tx.send(AppEvent::BuildOutput(
            board_config.name.clone(),
            format!(
                "✅ Found {} JavaScript files ready for upload",
                artifacts.len()
            ),
        ));

        Ok(artifacts)
    }

    async fn flash_board(
        &self,
        project_dir: &Path,
        board_config: &ProjectBoardConfig,
        _artifacts: &[BuildArtifact],
        port: Option<&str>,
        tx: mpsc::UnboundedSender<AppEvent>,
    ) -> Result<()> {
        let _ = tx.send(AppEvent::BuildOutput(
            board_config.name.clone(),
            "🔥 Starting Jaculus upload...".to_string(),
        ));

        // Use jaculus-tools for uploading
        if !self.is_tool_available("jaculus") {
            let _ = tx.send(AppEvent::BuildOutput(
                board_config.name.clone(),
                "❌ jaculus-tools not available".to_string(),
            ));
            return Err(anyhow::anyhow!("jaculus-tools not found in PATH"));
        }

        let mut cmd = Command::new("jaculus");
        cmd.current_dir(project_dir).args(["upload"]);

        // Add port if specified
        if let Some(port_str) = port {
            cmd.args(["--port", port_str]);
        }

        // Add target board if specified
        if let Some(target) = &board_config.target {
            if target.contains("ESP32-S3") {
                cmd.args(["--target", "esp32s3"]);
            } else {
                cmd.args(["--target", "esp32"]);
            }
        }

        cmd.stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        let upload_command_str = format!(
            "jaculus upload{}{}",
            port.map(|p| format!(" --port {}", p)).unwrap_or_default(),
            board_config
                .target
                .as_ref()
                .map(|t| if t.contains("ESP32-S3") {
                    " --target esp32s3"
                } else {
                    " --target esp32"
                })
                .unwrap_or("")
        );

        let _ = tx.send(AppEvent::BuildOutput(
            board_config.name.clone(),
            format!("🔨 Executing: {}", upload_command_str),
        ));

        let mut child = cmd.spawn().context("Failed to start jaculus upload")?;
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
            .context("Failed to wait for jaculus upload")?;

        if status.success() {
            let _ = tx.send(AppEvent::BuildOutput(
                board_config.name.clone(),
                "✅ Jaculus upload completed successfully".to_string(),
            ));
            Ok(())
        } else {
            let _ = tx.send(AppEvent::BuildOutput(
                board_config.name.clone(),
                "❌ Jaculus upload failed".to_string(),
            ));
            Err(anyhow::anyhow!("jaculus upload failed"))
        }
    }

    async fn monitor_board(
        &self,
        project_dir: &Path,
        board_config: &ProjectBoardConfig,
        port: Option<&str>,
        _baud_rate: u32,
        tx: mpsc::UnboundedSender<AppEvent>,
    ) -> Result<()> {
        let _ = tx.send(AppEvent::BuildOutput(
            board_config.name.clone(),
            "📺 Starting Jaculus monitor...".to_string(),
        ));

        if !self.is_tool_available("jaculus") {
            return Err(anyhow::anyhow!("jaculus-tools not found in PATH"));
        }

        let mut cmd = Command::new("jaculus");
        cmd.current_dir(project_dir).args(["monitor"]);

        if let Some(port_str) = port {
            cmd.args(["--port", port_str]);
        }

        cmd.stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        let mut child = cmd.spawn().context("Failed to start jaculus monitor")?;
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

        let _status = child
            .wait()
            .await
            .context("Failed to wait for jaculus monitor")?;
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
            "🧹 Cleaning Jaculus cache files...".to_string(),
        ));

        // Clean common JavaScript/Node.js cache directories
        let cache_dirs = ["node_modules", ".jaculus", "dist", "build"];

        for cache_dir in cache_dirs {
            let cache_path = project_dir.join(cache_dir);
            if cache_path.exists() && cache_path.is_dir() {
                tokio::fs::remove_dir_all(&cache_path)
                    .await
                    .with_context(|| format!("Failed to remove {}", cache_dir))?;

                let _ = tx.send(AppEvent::BuildOutput(
                    board_config.name.clone(),
                    format!("🗑️  Removed {}/", cache_dir),
                ));
            }
        }

        let _ = tx.send(AppEvent::BuildOutput(
            board_config.name.clone(),
            "✅ Clean completed successfully".to_string(),
        ));

        Ok(())
    }

    fn get_build_command(&self, project_dir: &Path, _board_config: &ProjectBoardConfig) -> String {
        // Jaculus doesn't have a build command, files are uploaded directly
        format!(
            "# Jaculus project - no build required\n# JavaScript files in {} are ready for upload",
            project_dir.display()
        )
    }

    fn get_flash_command(
        &self,
        project_dir: &Path,
        board_config: &ProjectBoardConfig,
        port: Option<&str>,
    ) -> String {
        let project_dir_str =
            if std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")) != *project_dir {
                format!("cd {} && ", project_dir.display())
            } else {
                String::new()
            };

        let port_arg = port.map(|p| format!(" --port {}", p)).unwrap_or_default();
        let target_arg = board_config
            .target
            .as_ref()
            .map(|t| {
                if t.contains("ESP32-S3") {
                    " --target esp32s3"
                } else {
                    " --target esp32"
                }
            })
            .unwrap_or("");

        format!(
            "{}jaculus upload{}{}",
            project_dir_str, port_arg, target_arg
        )
    }

    fn check_tools_available(&self) -> Result<(), String> {
        if !self.is_tool_available("jaculus") {
            return Err("jaculus-tools not found in PATH".to_string());
        }
        Ok(())
    }

    fn get_missing_tools_message(&self) -> String {
        "⚠️  Jaculus development environment is not properly set up.\n".to_string()
            + "   Please ensure jaculus-tools is installed:\n"
            + "   - Install with npm: npm install -g jaculus-tools\n"
            + "   - Or visit: https://github.com/RoboticsBrno/jaculus-tools\n"
            + "   - For more info: https://robotikabrno.cz/project/jaculus/\n"
            + "   Press Enter to continue anyway, or 'q' to quit."
    }
}

impl JaculusHandler {
    fn has_js_files(&self, dir: &Path) -> bool {
        if let Ok(entries) = dir.read_dir() {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() {
                    if let Some(extension) = path.extension() {
                        if extension == "js" || extension == "ts" || extension == "mjs" {
                            return true;
                        }
                    }
                }
            }
        }
        false
    }

    fn has_jaculus_patterns(&self, project_dir: &Path) -> bool {
        let jaculus_patterns = [
            "import",
            "require",
            "console.log",
            "setTimeout",
            "setInterval",
            "// Jaculus",
            "/* Jaculus",
            "ESP32",
            "digitalWrite",
            "analogRead",
        ];

        if let Ok(entries) = project_dir.read_dir() {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file()
                    && path
                        .extension()
                        .map_or(false, |ext| ext == "js" || ext == "ts")
                {
                    if let Ok(content) = fs::read_to_string(&path) {
                        for pattern in &jaculus_patterns {
                            if content.contains(pattern) {
                                return true;
                            }
                        }
                    }
                }
            }
        }
        false
    }

    fn find_js_files(&self, project_dir: &Path) -> Result<Vec<PathBuf>> {
        let mut js_files = Vec::new();

        // Check src/ directory first
        let src_dir = project_dir.join("src");
        if src_dir.is_dir() {
            self.collect_js_files(&src_dir, &mut js_files)?;
        }

        // Check root directory
        self.collect_js_files(project_dir, &mut js_files)?;

        Ok(js_files)
    }

    fn collect_js_files(&self, dir: &Path, js_files: &mut Vec<PathBuf>) -> Result<()> {
        if let Ok(entries) = dir.read_dir() {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() {
                    if let Some(extension) = path.extension() {
                        if extension == "js" || extension == "ts" || extension == "mjs" {
                            js_files.push(path);
                        }
                    }
                } else if path.is_dir()
                    && path.file_name() != Some(std::ffi::OsStr::new("node_modules"))
                {
                    // Recursively search subdirectories (except node_modules)
                    self.collect_js_files(&path, js_files)?;
                }
            }
        }
        Ok(())
    }

    fn detect_esp32_target(&self, _project_dir: &Path, js_files: &[PathBuf]) -> Result<String> {
        // Try to detect ESP32 variant from JavaScript file content
        for js_file in js_files {
            if let Ok(content) = fs::read_to_string(js_file) {
                if content.contains("ESP32-S3") || content.contains("esp32s3") {
                    return Ok("ESP32-S3".to_string());
                }
                if content.contains("ESP32-C3") || content.contains("esp32c3") {
                    return Ok("ESP32-C3".to_string());
                }
                if content.contains("ESP32-C6") || content.contains("esp32c6") {
                    return Ok("ESP32-C6".to_string());
                }
            }
        }
        // Default to ESP32 (Jaculus supports ESP32 and ESP32-S3)
        Ok("ESP32".to_string())
    }

    fn parse_jaculus_config(&self, config_path: &Path) -> Result<Vec<ProjectBoardConfig>> {
        let content = fs::read_to_string(config_path)?;
        // Try to parse JSON configuration
        // For now, return a basic configuration - could be enhanced with proper JSON parsing
        let project_dir = config_path.parent().unwrap_or(Path::new("."));

        let target = if content.contains("esp32s3") || content.contains("ESP32-S3") {
            "ESP32-S3"
        } else {
            "ESP32"
        };

        Ok(vec![ProjectBoardConfig {
            name: format!("jaculus-{}", target.to_lowercase().replace("-", "")),
            config_file: config_path.to_path_buf(),
            build_dir: project_dir.to_path_buf(),
            target: Some(target.to_string()),
            project_type: ProjectType::Jaculus,
        }])
    }

    fn is_tool_available(&self, tool: &str) -> bool {
        std::process::Command::new("which")
            .arg(tool)
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }
}
