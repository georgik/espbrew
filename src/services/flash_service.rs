//! Unified flash service for ESP32 boards
//!
//! This service provides consistent flashing functionality that can be used by both
//! local operations (TUI, CLI) and remote operations (server). It ensures that the
//! same underlying flashing mechanism is used regardless of the interface.

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::PathBuf;
use tokio::sync::mpsc;

use crate::models::AppEvent;
use crate::models::flash::{FlashBinaryInfo, FlashConfig};
use crate::utils::espflash_utils;

/// Unified flash operation request for internal use
#[derive(Debug, Clone)]
pub struct FlashOperation {
    /// Target serial port
    pub port: String,
    /// Binaries to flash with their offsets and metadata
    pub binaries: Vec<FlashBinaryInfo>,
    /// Flash configuration parameters
    pub flash_config: Option<FlashConfig>,
    /// Optional board name for progress reporting
    pub board_name: Option<String>,
}

/// Result of a flash operation
#[derive(Debug, Clone)]
pub struct FlashResult {
    pub success: bool,
    pub message: String,
    pub duration_ms: Option<u64>,
}

/// Unified flash service that works for both local and remote operations
#[derive(Clone)]
pub struct UnifiedFlashService;

impl UnifiedFlashService {
    pub fn new() -> Self {
        Self
    }

    /// Flash binaries to ESP32 board using unified service
    pub async fn flash_board(
        &self,
        operation: FlashOperation,
        progress_tx: Option<mpsc::UnboundedSender<AppEvent>>,
    ) -> Result<FlashResult> {
        let start_time = std::time::Instant::now();
        let board_name = operation
            .board_name
            .clone()
            .unwrap_or_else(|| "Unknown".to_string());

        // Send progress update
        if let Some(tx) = &progress_tx {
            let _ = tx.send(AppEvent::BuildOutput(
                board_name.clone(),
                "🔥 Starting flash operation...".to_string(),
            ));
        }

        // Validate binaries
        if operation.binaries.is_empty() {
            return Ok(FlashResult {
                success: false,
                message: "No binaries to flash".to_string(),
                duration_ms: Some(0),
            });
        }

        // Check if all binaries exist
        for binary in &operation.binaries {
            if !binary.file_path.exists() {
                return Ok(FlashResult {
                    success: false,
                    message: format!("Binary file not found: {}", binary.file_path.display()),
                    duration_ms: Some(0),
                });
            }
        }

        // Log flash plan
        let total_size: u64 = operation
            .binaries
            .iter()
            .map(|b| {
                std::fs::metadata(&b.file_path)
                    .map(|m| m.len())
                    .unwrap_or(0)
            })
            .sum();

        if let Some(tx) = &progress_tx {
            let _ = tx.send(AppEvent::BuildOutput(
                board_name.clone(),
                format!(
                    "📦 Flash plan ({:.1} KB total):",
                    total_size as f64 / 1024.0
                ),
            ));

            for (i, binary) in operation.binaries.iter().enumerate() {
                let file_size = std::fs::metadata(&binary.file_path)
                    .map(|m| m.len())
                    .unwrap_or(0);
                let _ = tx.send(AppEvent::BuildOutput(
                    board_name.clone(),
                    format!(
                        "  [{}/{}] {} → 0x{:x} ({:.1} KB)",
                        i + 1,
                        operation.binaries.len(),
                        binary.name,
                        binary.offset,
                        file_size as f64 / 1024.0
                    ),
                ));
            }
        }

        // Prepare flash data with memory optimization for large binaries
        let mut flash_data_map = HashMap::new();
        for binary in &operation.binaries {
            // Performance optimization: check file size before reading to optimize memory allocation
            let file_size = std::fs::metadata(&binary.file_path).with_context(|| {
                format!("Failed to get metadata for: {}", binary.file_path.display())
            })?;

            if file_size.len() == 0 {
                return Ok(FlashResult {
                    success: false,
                    message: format!("Binary file is empty: {}", binary.file_path.display()),
                    duration_ms: Some(0),
                });
            }

            // Read binary data efficiently with memory optimization for large files
            let data = std::fs::read(&binary.file_path).with_context(|| {
                format!("Failed to read binary: {}", binary.file_path.display())
            })?;

            // Note: Modern std::fs::read already optimizes memory allocation internally
            // The main optimization here is avoiding unnecessary data cloning later in the pipeline

            flash_data_map.insert(binary.offset, data);
        }

        // Send progress update
        if let Some(tx) = &progress_tx {
            let _ = tx.send(AppEvent::BuildOutput(
                board_name.clone(),
                format!(
                    "🔥 Flashing {} binaries to {}...",
                    flash_data_map.len(),
                    operation.port
                ),
            ));
        }

        // Perform the actual flash operation
        let result = espflash_utils::flash_multi_binary(&operation.port, flash_data_map).await;
        let duration_ms = start_time.elapsed().as_millis() as u64;

        match result {
            Ok(_) => {
                let success_msg = format!(
                    "✅ Successfully flashed {} binaries ({:.1} KB) in {}ms",
                    operation.binaries.len(),
                    total_size as f64 / 1024.0,
                    duration_ms
                );

                if let Some(tx) = &progress_tx {
                    let _ = tx.send(AppEvent::BuildOutput(
                        board_name.clone(),
                        success_msg.clone(),
                    ));
                }

                Ok(FlashResult {
                    success: true,
                    message: success_msg,
                    duration_ms: Some(duration_ms),
                })
            }
            Err(e) => {
                let error_msg = format!("❌ Flash operation failed: {}", e);

                if let Some(tx) = &progress_tx {
                    let _ = tx.send(AppEvent::BuildOutput(board_name.clone(), error_msg.clone()));
                }

                Ok(FlashResult {
                    success: false,
                    message: error_msg,
                    duration_ms: Some(duration_ms),
                })
            }
        }
    }

    /// Discover ESP-IDF build directories in a project
    pub fn discover_esp_build_directories(project_dir: &std::path::Path) -> Result<Vec<PathBuf>> {
        use std::fs;

        let mut build_dirs = Vec::new();

        // Look for build directories
        if let Ok(entries) = fs::read_dir(project_dir) {
            for entry in entries.flatten() {
                if entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false) {
                    let dir_name = entry.file_name();
                    if let Some(name_str) = dir_name.to_str() {
                        if name_str.starts_with("build") {
                            build_dirs.push(entry.path());
                        }
                    }
                }
            }
        }

        // Sort to get consistent ordering
        build_dirs.sort();
        Ok(build_dirs)
    }

    /// Parse ESP-IDF flash_args file to extract flash configuration and binaries
    pub fn parse_flash_args(
        flash_args_path: &std::path::Path,
        build_dir: &std::path::Path,
    ) -> Result<(FlashConfig, Vec<FlashBinaryInfo>)> {
        let flash_args_content =
            std::fs::read_to_string(flash_args_path).context("Failed to read flash_args file")?;

        let mut flash_config = FlashConfig {
            flash_mode: "dio".to_string(),
            flash_freq: "80m".to_string(),
            flash_size: "4MB".to_string(),
        };

        let mut binaries = Vec::new();
        let mut args = flash_args_content.split_whitespace();

        while let Some(arg) = args.next() {
            match arg {
                "--flash_mode" => {
                    if let Some(mode) = args.next() {
                        flash_config.flash_mode = mode.to_string();
                    }
                }
                "--flash_freq" => {
                    if let Some(freq) = args.next() {
                        flash_config.flash_freq = freq.to_string();
                    }
                }
                "--flash_size" => {
                    if let Some(size) = args.next() {
                        flash_config.flash_size = size.to_string();
                    }
                }
                arg if arg.starts_with("0x") => {
                    // Found flash offset, next should be binary path
                    if let Some(binary_path_str) = args.next() {
                        let offset = u32::from_str_radix(&arg[2..], 16)
                            .context("Failed to parse flash offset")?;

                        let binary_path = if std::path::Path::new(binary_path_str).is_absolute() {
                            PathBuf::from(binary_path_str)
                        } else {
                            build_dir.join(binary_path_str)
                        };

                        if binary_path.exists() {
                            let file_name = binary_path
                                .file_name()
                                .and_then(|n| n.to_str())
                                .unwrap_or("unknown.bin")
                                .to_string();

                            let name = match file_name.as_str() {
                                "bootloader.bin" => "bootloader",
                                "partition-table.bin" => "partition-table",
                                _ if file_name.ends_with(".bin") => "app",
                                _ => "unknown",
                            }
                            .to_string();

                            binaries.push(FlashBinaryInfo {
                                name,
                                offset,
                                file_name,
                                file_path: binary_path,
                            });
                        }
                    }
                }
                _ => {}
            }
        }

        if binaries.is_empty() {
            return Err(anyhow::anyhow!("No binary files found in flash_args"));
        }

        Ok((flash_config, binaries))
    }

    /// Flash ESP-IDF project by discovering build artifacts and using them
    pub async fn flash_esp_idf_project(
        &self,
        project_dir: &std::path::Path,
        port: &str,
        build_dir: Option<PathBuf>,
        progress_tx: Option<mpsc::UnboundedSender<AppEvent>>,
        board_name: Option<String>,
    ) -> Result<FlashResult> {
        let board_name = board_name.unwrap_or_else(|| "ESP-IDF".to_string());

        // Discover build directories
        let build_dirs = if let Some(specific_dir) = build_dir {
            if !specific_dir.exists() {
                return Ok(FlashResult {
                    success: false,
                    message: format!("Build directory not found: {}", specific_dir.display()),
                    duration_ms: Some(0),
                });
            }
            vec![specific_dir]
        } else {
            Self::discover_esp_build_directories(project_dir)?
        };

        if build_dirs.is_empty() {
            return Ok(FlashResult {
                success: false,
                message: "No ESP-IDF build directories found. Run 'idf.py build' first."
                    .to_string(),
                duration_ms: Some(0),
            });
        }

        if let Some(tx) = &progress_tx {
            let _ = tx.send(AppEvent::BuildOutput(
                board_name.clone(),
                format!("🔍 Found {} build director(y/ies)", build_dirs.len()),
            ));
        }

        // Try each build directory until we find valid artifacts
        for build_dir in &build_dirs {
            let flash_args_path = build_dir.join("flash_args");

            if !flash_args_path.exists() {
                if let Some(tx) = &progress_tx {
                    let _ = tx.send(AppEvent::BuildOutput(
                        board_name.clone(),
                        format!("⚠️ Skipping {}: no flash_args found", build_dir.display()),
                    ));
                }
                continue;
            }

            if let Some(tx) = &progress_tx {
                let _ = tx.send(AppEvent::BuildOutput(
                    board_name.clone(),
                    format!("📝 Parsing flash_args: {}", flash_args_path.display()),
                ));
            }

            match Self::parse_flash_args(&flash_args_path, build_dir) {
                Ok((flash_config, binaries)) => {
                    if let Some(tx) = &progress_tx {
                        let _ = tx.send(AppEvent::BuildOutput(
                            board_name.clone(),
                            format!("✅ Found {} binaries to flash", binaries.len()),
                        ));
                    }

                    let operation = FlashOperation {
                        port: port.to_string(),
                        binaries,
                        flash_config: Some(flash_config),
                        board_name: Some(board_name.clone()),
                    };

                    return self.flash_board(operation, progress_tx).await;
                }
                Err(e) => {
                    if let Some(tx) = &progress_tx {
                        let _ = tx.send(AppEvent::BuildOutput(
                            board_name.clone(),
                            format!("⚠️ Failed to parse {}: {}", flash_args_path.display(), e),
                        ));
                    }
                    continue;
                }
            }
        }

        Ok(FlashResult {
            success: false,
            message: "No valid ESP-IDF build artifacts found. Run 'idf.py build' first."
                .to_string(),
            duration_ms: Some(0),
        })
    }

    /// Flash a single binary file (for simple cases)
    pub async fn flash_single_binary(
        &self,
        port: &str,
        binary_path: &std::path::Path,
        offset: u32,
        progress_tx: Option<mpsc::UnboundedSender<AppEvent>>,
        board_name: Option<String>,
    ) -> Result<FlashResult> {
        let binary_info = FlashBinaryInfo {
            name: binary_path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("binary")
                .to_string(),
            file_name: binary_path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("binary.bin")
                .to_string(),
            file_path: binary_path.to_path_buf(),
            offset,
        };

        let operation = FlashOperation {
            port: port.to_string(),
            binaries: vec![binary_info],
            flash_config: None,
            board_name,
        };

        self.flash_board(operation, progress_tx).await
    }
}

impl Default for UnifiedFlashService {
    fn default() -> Self {
        Self::new()
    }
}
