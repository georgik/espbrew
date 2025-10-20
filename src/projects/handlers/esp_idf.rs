use crate::models::flash::{FlashBinaryInfo, FlashConfig};
use crate::models::{AppEvent, ArtifactType, BuildArtifact, ProjectBoardConfig, ProjectType};
use crate::projects::registry::ProjectHandler;
use crate::utils::idf_native::{IdfNativeConfig, IdfNativeHandler};

use anyhow::{Context, Result};
use async_trait::async_trait;
use glob::glob;
use std::{
    fs,
    path::{Path, PathBuf},
};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;

/// Handler for ESP-IDF projects with CMake build system
pub struct EspIdfHandler;

#[async_trait]
impl ProjectHandler for EspIdfHandler {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn project_type(&self) -> ProjectType {
        ProjectType::EspIdf
    }

    fn can_handle(&self, project_dir: &Path) -> bool {
        let cmake_file = project_dir.join("CMakeLists.txt");
        let sdkconfig_exists = project_dir.join("sdkconfig").exists()
            || project_dir
                .read_dir()
                .map(|mut entries| {
                    entries.any(|entry| {
                        entry
                            .map(|e| {
                                e.file_name()
                                    .to_string_lossy()
                                    .starts_with("sdkconfig.defaults")
                            })
                            .unwrap_or(false)
                    })
                })
                .unwrap_or(false);

        cmake_file.exists() && sdkconfig_exists
    }

    fn discover_boards(&self, project_dir: &Path) -> Result<Vec<ProjectBoardConfig>> {
        let pattern = project_dir.join("sdkconfig.defaults.*");
        let mut boards = Vec::new();

        // Check for multi-board configurations (sdkconfig.defaults.*)
        for entry in glob(&pattern.to_string_lossy())? {
            let config_file = entry?;
            if let Some(file_name) = config_file.file_name() {
                if let Some(name) = file_name.to_str() {
                    if let Some(board_name) = name.strip_prefix("sdkconfig.defaults.") {
                        let build_dir = project_dir.join(format!("build.{}", board_name));
                        let target = self.determine_target(&config_file).ok();

                        boards.push(ProjectBoardConfig {
                            name: board_name.to_string(),
                            config_file: config_file.clone(),
                            build_dir,
                            target,
                            project_type: ProjectType::EspIdf,
                        });
                    }
                }
            }
        }

        // If no multi-board configurations found, check for single board project (plain sdkconfig.defaults)
        if boards.is_empty() {
            let default_config = project_dir.join("sdkconfig.defaults");
            if default_config.exists() {
                let build_dir = project_dir.join("build");
                let target = self.determine_target(&default_config).ok();

                boards.push(ProjectBoardConfig {
                    name: "default".to_string(),
                    config_file: default_config,
                    build_dir,
                    target,
                    project_type: ProjectType::EspIdf,
                });
            }
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
            "🏗️  Starting ESP-IDF build...".to_string(),
        ));

        // Try native operations first, fallback to traditional if needed
        if IdfNativeHandler::is_available() {
            let _ = tx.send(AppEvent::BuildOutput(
                board_config.name.clone(),
                "⚡ Using native ESP-IDF operations (idf-rs)".to_string(),
            ));

            return self.build_board_native(project_dir, board_config, tx).await;
        } else {
            let _ = tx.send(AppEvent::BuildOutput(
                board_config.name.clone(),
                "🔄 Falling back to traditional ESP-IDF operations".to_string(),
            ));

            return self
                .build_board_traditional(project_dir, board_config, tx)
                .await;
        }
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
            "🔥 Starting ESP-IDF flash using unified service...".to_string(),
        ));

        // Use the unified flash service instead of calling idf.py flash
        use crate::services::UnifiedFlashService;
        let flash_service = UnifiedFlashService::new();

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

        // Use the unified flash service to flash ESP-IDF project
        let result = flash_service
            .flash_esp_idf_project(
                project_dir,
                &flash_port,
                Some(board_config.build_dir.clone()),
                Some(tx.clone()),
                Some(board_config.name.clone()),
            )
            .await?;

        if result.success {
            let _ = tx.send(AppEvent::BuildOutput(
                board_config.name.clone(),
                result.message,
            ));
            Ok(())
        } else {
            let _ = tx.send(AppEvent::BuildOutput(
                board_config.name.clone(),
                result.message.clone(),
            ));
            Err(anyhow::anyhow!("ESP-IDF flash failed: {}", result.message))
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
                "📺 Starting ESP-IDF monitor on {} at {} baud",
                port.unwrap_or("auto-detect"),
                baud_rate
            ),
        ));

        // Try native operations first, fallback to traditional if needed
        if IdfNativeHandler::is_available() {
            let _ = tx.send(AppEvent::BuildOutput(
                board_config.name.clone(),
                "⚡ Using native ESP-IDF monitor operations".to_string(),
            ));

            return self
                .monitor_board_native(project_dir, board_config, port, baud_rate, tx)
                .await;
        } else {
            let _ = tx.send(AppEvent::BuildOutput(
                board_config.name.clone(),
                "🔄 Falling back to traditional ESP-IDF monitor".to_string(),
            ));

            return self
                .monitor_board_traditional(project_dir, board_config, port, baud_rate, tx)
                .await;
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
            "🧹 Cleaning ESP-IDF build artifacts...".to_string(),
        ));

        // Try native operations first, fallback to traditional if needed
        if IdfNativeHandler::is_available() {
            let _ = tx.send(AppEvent::BuildOutput(
                board_config.name.clone(),
                "⚡ Using native ESP-IDF clean operations".to_string(),
            ));

            return self.clean_board_native(project_dir, board_config, tx).await;
        } else {
            let _ = tx.send(AppEvent::BuildOutput(
                board_config.name.clone(),
                "🔄 Falling back to traditional ESP-IDF clean".to_string(),
            ));

            return self
                .clean_board_traditional(project_dir, board_config, tx)
                .await;
        }
    }

    fn get_build_command(&self, project_dir: &Path, board_config: &ProjectBoardConfig) -> String {
        let config_path = board_config.config_file.display();
        let build_dir = board_config.build_dir.display();
        let sdkconfig_file = board_config.build_dir.join("sdkconfig");
        let sdkconfig_path = sdkconfig_file.display();

        if std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")) != *project_dir {
            format!(
                "cd {} && SDKCONFIG_DEFAULTS='{}' idf.py -D SDKCONFIG='{}' -B '{}' build",
                project_dir.display(),
                config_path,
                sdkconfig_path,
                build_dir
            )
        } else {
            format!(
                "SDKCONFIG_DEFAULTS='{}' idf.py -D SDKCONFIG='{}' -B '{}' build",
                config_path, sdkconfig_path, build_dir
            )
        }
    }

    fn get_flash_command(
        &self,
        project_dir: &Path,
        board_config: &ProjectBoardConfig,
        port: Option<&str>,
    ) -> String {
        let config_path = board_config.config_file.display();
        let build_dir = board_config.build_dir.display();
        let sdkconfig_file = board_config.build_dir.join("sdkconfig");
        let sdkconfig_path = sdkconfig_file.display();

        let port_arg = port.map(|p| format!(" -p {}", p)).unwrap_or_default();

        if std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")) != *project_dir {
            format!(
                "cd {} && SDKCONFIG_DEFAULTS='{}' idf.py -D SDKCONFIG='{}' -B '{}' flash{}",
                project_dir.display(),
                config_path,
                sdkconfig_path,
                build_dir,
                port_arg
            )
        } else {
            format!(
                "SDKCONFIG_DEFAULTS='{}' idf.py -D SDKCONFIG='{}' -B '{}' flash{}",
                config_path, sdkconfig_path, build_dir, port_arg
            )
        }
    }

    fn check_tools_available(&self) -> Result<(), String> {
        // Check for ESP-IDF availability using cross-platform detection
        if !crate::utils::esp_idf_utils::is_esp_idf_available() {
            return Err(
                "⚠️  ESP-IDF not found - ESP-IDF building unavailable, but flashing still works!"
                    .to_string(),
            );
        }

        Ok(())
    }

    fn get_missing_tools_message(&self) -> String {
        let detection_result = crate::utils::esp_idf_utils::detect_esp_idf_installations();
        let mut message =
            "⚠️  ESP-IDF development environment is not set up for building.\n".to_string();
        message += "   📍 Important: FLASHING STILL WORKS without ESP-IDF!\n";
        message += "   \n";

        if !detection_result.installations.is_empty() {
            message += "   Found ESP-IDF installations but they may be invalid:\n";
            for installation in &detection_result.installations {
                message += &format!("   - {}\n", installation.get_description());
            }
            message += "   \n";
        }

        if !detection_result.warnings.is_empty() {
            message += "   Issues detected:\n";
            for warning in &detection_result.warnings {
                message += &format!("   - {}\n", warning);
            }
            message += "   \n";
        }

        message += "   To enable ESP-IDF building, please ensure:\n";
        message += "   - ESP-IDF is installed: https://docs.espressif.com/projects/esp-idf/en/latest/get-started/\n";

        if cfg!(windows) {
            message += "   - On Windows: Use ESP-IDF Installation Manager or manual installation\n";
            message += "   - EIM config location: C:\\Espressif\\tools\\eim_idf.json\n";
        } else {
            message += "   - ESP-IDF environment is activated: source ~/esp/esp-idf/export.sh\n";
        }

        message += "   - idf.py (or idf.py.exe on Windows) is available in PATH\n";
        message += "   \n";
        message += "   You can still flash pre-built binaries and use other project types!\n";
        message += "   Press Enter to continue, or 'q' to quit.";
        message
    }
}

impl EspIdfHandler {
    /// Build using native idf-rs operations
    async fn build_board_native(
        &self,
        project_dir: &Path,
        board_config: &ProjectBoardConfig,
        tx: mpsc::UnboundedSender<AppEvent>,
    ) -> Result<Vec<BuildArtifact>> {
        // Determine target
        let target = self.determine_target(&board_config.config_file)?;

        // Create native configuration
        let config = IdfNativeConfig::new(
            project_dir,
            &board_config.build_dir,
            &target,
            Some(&board_config.config_file),
        );

        // Add ESP-IDF environment variables
        let esp_idf_env =
            crate::utils::esp_idf_utils::get_esp_idf_environment().unwrap_or_default();
        let mut config = config;
        for (key, value) in esp_idf_env {
            config = config.with_env_var(key, value);
        }
        config = config.with_verbose(true);

        let native_handler = IdfNativeHandler::new();

        // Set target first
        native_handler
            .set_target(&config, Some(tx.clone()), Some(&board_config.name))
            .await?;

        // Build the project
        native_handler
            .build(&config, Some(tx.clone()), Some(&board_config.name))
            .await?;

        // Find build artifacts
        self.find_build_artifacts(project_dir, board_config)
    }

    /// Build using traditional ESP-IDF operations (fallback)
    async fn build_board_traditional(
        &self,
        project_dir: &Path,
        board_config: &ProjectBoardConfig,
        tx: mpsc::UnboundedSender<AppEvent>,
    ) -> Result<Vec<BuildArtifact>> {
        let build_command = self.get_build_command(project_dir, board_config);
        let _ = tx.send(AppEvent::BuildOutput(
            board_config.name.clone(),
            format!("🔨 Executing: {}", build_command),
        ));

        // First determine target
        let target = self.determine_target(&board_config.config_file)?;
        let config_path = board_config.config_file.to_string_lossy();

        // Use board-specific sdkconfig file to avoid conflicts
        let sdkconfig_path = board_config.build_dir.join("sdkconfig");

        // Get cross-platform ESP-IDF command
        let idf_command = crate::utils::esp_idf_utils::get_esp_idf_command()
            .map_err(|e| anyhow::anyhow!("ESP-IDF not available: {}", e))?;

        // Set up environment for ESP-IDF
        let esp_idf_env =
            crate::utils::esp_idf_utils::get_esp_idf_environment().unwrap_or_default();

        // Set target command
        let mut cmd = Command::new(&idf_command);
        cmd.current_dir(project_dir)
            .env("SDKCONFIG_DEFAULTS", &*config_path)
            .envs(&esp_idf_env)
            .args([
                "-D",
                &format!("SDKCONFIG={}", sdkconfig_path.display()),
                "-B",
                &board_config.build_dir.to_string_lossy(),
                "set-target",
                &target,
            ])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        let output = cmd
            .output()
            .await
            .context("Failed to run idf.py set-target")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let _ = tx.send(AppEvent::BuildOutput(
                board_config.name.clone(),
                format!("❌ Set target failed: {}", stderr.trim()),
            ));
            return Err(anyhow::anyhow!("Failed to set target"));
        }

        // Build command with unbuffered output for real-time streaming
        let mut cmd = Command::new(&idf_command);
        cmd.current_dir(project_dir)
            .env("SDKCONFIG_DEFAULTS", &*config_path)
            .env("PYTHONUNBUFFERED", "1") // Force Python to not buffer output
            .envs(&esp_idf_env)
            .args([
                "-D",
                &format!("SDKCONFIG={}", sdkconfig_path.display()),
                "-B",
                &board_config.build_dir.to_string_lossy(),
                "build",
            ])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        let mut child = cmd.spawn().context("Failed to start idf.py build")?;
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
            .context("Failed to wait for idf.py build")?;

        if status.success() {
            let _ = tx.send(AppEvent::BuildOutput(
                board_config.name.clone(),
                "✅ ESP-IDF build completed successfully".to_string(),
            ));

            // Find build artifacts
            self.find_build_artifacts(project_dir, board_config)
        } else {
            let _ = tx.send(AppEvent::BuildOutput(
                board_config.name.clone(),
                "❌ ESP-IDF build failed".to_string(),
            ));
            Err(anyhow::anyhow!("ESP-IDF build failed"))
        }
    }

    /// Monitor using native idf-rs operations
    async fn monitor_board_native(
        &self,
        project_dir: &Path,
        board_config: &ProjectBoardConfig,
        port: Option<&str>,
        baud_rate: u32,
        tx: mpsc::UnboundedSender<AppEvent>,
    ) -> Result<()> {
        // Determine target
        let target = self.determine_target(&board_config.config_file)?;

        // Create native configuration
        let config = IdfNativeConfig::new(
            project_dir,
            &board_config.build_dir,
            &target,
            Some(&board_config.config_file),
        );

        // Add ESP-IDF environment variables
        let esp_idf_env =
            crate::utils::esp_idf_utils::get_esp_idf_environment().unwrap_or_default();
        let mut config = config;
        for (key, value) in esp_idf_env {
            config = config.with_env_var(key, value);
        }
        config = config.with_verbose(true);

        let native_handler = IdfNativeHandler::new();

        // Start monitoring
        native_handler
            .monitor(&config, port, baud_rate, Some(tx), Some(&board_config.name))
            .await
    }

    /// Monitor using traditional ESP-IDF operations (fallback)
    async fn monitor_board_traditional(
        &self,
        project_dir: &Path,
        board_config: &ProjectBoardConfig,
        port: Option<&str>,
        _baud_rate: u32,
        tx: mpsc::UnboundedSender<AppEvent>,
    ) -> Result<()> {
        let config_path = board_config.config_file.to_string_lossy();
        let sdkconfig_path = board_config.build_dir.join("sdkconfig");

        // Get cross-platform ESP-IDF command
        let idf_command = crate::utils::esp_idf_utils::get_esp_idf_command()
            .map_err(|e| anyhow::anyhow!("ESP-IDF not available: {}", e))?;

        // Set up environment for ESP-IDF
        let esp_idf_env =
            crate::utils::esp_idf_utils::get_esp_idf_environment().unwrap_or_default();

        let mut cmd = Command::new(&idf_command);
        cmd.current_dir(project_dir)
            .env("SDKCONFIG_DEFAULTS", &*config_path)
            .envs(&esp_idf_env)
            .args([
                "-D",
                &format!("SDKCONFIG={}", sdkconfig_path.display()),
                "-B",
                &board_config.build_dir.to_string_lossy(),
                "monitor",
            ]);

        if let Some(port) = port {
            cmd.args(["-p", port]);
        }

        cmd.stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        let mut child = cmd.spawn().context("Failed to start idf.py monitor")?;
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
            .context("Failed to wait for idf.py monitor")?;

        if status.success() {
            let _ = tx.send(AppEvent::BuildOutput(
                board_config.name.clone(),
                "✅ ESP-IDF monitoring session completed".to_string(),
            ));
        } else {
            let _ = tx.send(AppEvent::BuildOutput(
                board_config.name.clone(),
                "❌ ESP-IDF monitoring failed".to_string(),
            ));
        }

        Ok(())
    }

    /// Clean using native idf-rs operations
    async fn clean_board_native(
        &self,
        project_dir: &Path,
        board_config: &ProjectBoardConfig,
        tx: mpsc::UnboundedSender<AppEvent>,
    ) -> Result<()> {
        // Determine target
        let target = self.determine_target(&board_config.config_file)?;

        // Create native configuration
        let config = IdfNativeConfig::new(
            project_dir,
            &board_config.build_dir,
            &target,
            Some(&board_config.config_file),
        );

        // Add ESP-IDF environment variables
        let esp_idf_env =
            crate::utils::esp_idf_utils::get_esp_idf_environment().unwrap_or_default();
        let mut config = config;
        for (key, value) in esp_idf_env {
            config = config.with_env_var(key, value);
        }
        config = config.with_verbose(true);

        let native_handler = IdfNativeHandler::new();

        // Clean the project
        native_handler
            .clean(&config, Some(tx), Some(&board_config.name))
            .await
    }

    /// Clean using traditional ESP-IDF operations (fallback)
    async fn clean_board_traditional(
        &self,
        project_dir: &Path,
        board_config: &ProjectBoardConfig,
        tx: mpsc::UnboundedSender<AppEvent>,
    ) -> Result<()> {
        let config_path = board_config.config_file.to_string_lossy();
        let sdkconfig_path = board_config.build_dir.join("sdkconfig");

        // Get cross-platform ESP-IDF command
        let idf_command = crate::utils::esp_idf_utils::get_esp_idf_command()
            .map_err(|e| anyhow::anyhow!("ESP-IDF not available: {}", e))?;

        // Set up environment for ESP-IDF
        let esp_idf_env =
            crate::utils::esp_idf_utils::get_esp_idf_environment().unwrap_or_default();

        let mut cmd = Command::new(&idf_command);
        cmd.current_dir(project_dir)
            .env("SDKCONFIG_DEFAULTS", &*config_path)
            .envs(&esp_idf_env)
            .args([
                "-D",
                &format!("SDKCONFIG={}", sdkconfig_path.display()),
                "-B",
                &board_config.build_dir.to_string_lossy(),
                "clean",
            ]);

        let output = cmd.output().await.context("Failed to run idf.py clean")?;

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
            Err(anyhow::anyhow!("ESP-IDF clean failed"))
        }
    }

    fn determine_target(&self, config_file: &Path) -> Result<String> {
        let content = fs::read_to_string(config_file)?;

        if content.contains("esp32p4") || content.contains("CONFIG_IDF_TARGET=\"esp32p4\"") {
            Ok("esp32p4".to_string())
        } else if content.contains("esp32c6") || content.contains("CONFIG_IDF_TARGET=\"esp32c6\"") {
            Ok("esp32c6".to_string())
        } else if content.contains("esp32c3") || content.contains("CONFIG_IDF_TARGET=\"esp32c3\"") {
            Ok("esp32c3".to_string())
        } else {
            Ok("esp32s3".to_string()) // default
        }
    }

    pub fn find_build_artifacts(
        &self,
        _project_dir: &Path,
        board_config: &ProjectBoardConfig,
    ) -> Result<Vec<BuildArtifact>> {
        let mut artifacts = Vec::new();

        // Parse flash_args file to get all binaries and their offsets
        let flash_args_path = board_config.build_dir.join("flash_args");
        if flash_args_path.exists() {
            let (_, binaries) = self.parse_flash_args(&flash_args_path, &board_config.build_dir)?;
            for binary_info in binaries {
                let artifact_type = match binary_info.offset {
                    0x0 => ArtifactType::Bootloader,
                    0x8000 => ArtifactType::PartitionTable,
                    0x10000 => ArtifactType::Application,
                    _ => ArtifactType::Binary,
                };

                artifacts.push(BuildArtifact {
                    name: binary_info.name,
                    file_path: binary_info.file_path,
                    artifact_type,
                    offset: Some(binary_info.offset),
                });
            }
        } else {
            return Err(anyhow::anyhow!(
                "No flash_args file found in {}. Build the project first.",
                board_config.build_dir.display()
            ));
        }

        Ok(artifacts)
    }

    fn parse_flash_args(
        &self,
        flash_args_path: &Path,
        build_dir: &Path,
    ) -> Result<(FlashConfig, Vec<FlashBinaryInfo>)> {
        // Use the unified service's parse_flash_args method
        use crate::services::UnifiedFlashService;
        UnifiedFlashService::parse_flash_args(flash_args_path, build_dir)
    }
}
