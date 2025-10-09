//! Flash service for handling ESP32 board flashing operations

use anyhow::Result;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;

use crate::models::board::BoardStatus;
use crate::models::flash::{FlashBinary, FlashConfig, FlashRequest, FlashResponse};
use crate::server::app::ServerState;

/// Flash service for handling board flashing operations
#[derive(Clone)]
pub struct FlashService {
    state: Arc<RwLock<ServerState>>,
}

impl FlashService {
    pub fn new(state: Arc<RwLock<ServerState>>) -> Self {
        Self { state }
    }

    /// Flash a board with the provided binary
    pub async fn flash_board(&self, request: FlashRequest) -> Result<FlashResponse> {
        println!(
            "🔥 FlashService::flash_board called for board {}",
            request.board_id
        );

        // Get board port before borrowing mutably
        let board_port = {
            let state_lock = self.state.read().await;
            println!(
                "🔍 FlashService: Looking up board {} in state",
                request.board_id
            );
            let board = state_lock
                .boards
                .get(&request.board_id)
                .ok_or_else(|| anyhow::anyhow!("Board not found: {}", request.board_id))?;
            println!(
                "✅ FlashService: Found board {} on port {}",
                request.board_id, board.port
            );
            board.port.clone()
        };

        // Update status to flashing
        {
            println!("🔄 FlashService: Updating board status to flashing");
            let mut state_lock = self.state.write().await;
            if let Some(board) = state_lock.boards.get_mut(&request.board_id) {
                board.status = BoardStatus::Flashing;
                board.last_updated = chrono::Local::now();
                println!("✅ FlashService: Board status updated to flashing");
            } else {
                println!(
                    "❌ FlashService: Board {} not found when updating status",
                    request.board_id
                );
            }
        }

        let start_time = Instant::now();
        println!(
            "🔥 Starting flash operation for board {} on port {}",
            request.board_id, board_port
        );

        // Validate that we have valid binaries to flash
        println!("🚀 FlashService: Starting flash operation decision logic");
        let result = if let Some(flash_binaries) = &request.flash_binaries {
            // Validate multi-binary flash request
            if flash_binaries.is_empty() {
                let error_msg = "Multi-binary flash request contains no binaries";
                println!("❌ FlashService: {}", error_msg);
                return Ok(FlashResponse {
                    success: false,
                    message: error_msg.to_string(),
                    flash_id: None,
                    duration_ms: Some(0),
                    progress: None,
                });
            }

            // Check if all binaries have valid data
            let mut empty_binaries = Vec::new();
            for (i, binary) in flash_binaries.iter().enumerate() {
                if binary.data.is_empty() {
                    empty_binaries.push(format!("{}:{}", i, binary.name));
                }
            }

            if !empty_binaries.is_empty() {
                let error_msg = format!(
                    "Flash request contains {} empty binaries: [{}]",
                    empty_binaries.len(),
                    empty_binaries.join(", ")
                );
                println!("❌ FlashService: {}", error_msg);
                return Ok(FlashResponse {
                    success: false,
                    message: error_msg,
                    flash_id: None,
                    duration_ms: Some(0),
                    progress: None,
                });
            }

            // New multi-binary flash format - flash all binaries with proper offsets
            println!(
                "📦 FlashService: Multi-binary flash selected - {} binaries to flash",
                flash_binaries.len()
            );
            let total_size: usize = flash_binaries.iter().map(|b| b.data.len()).sum();
            println!(
                "📦 FlashService: ESP-IDF flash plan ({:.1} KB total):",
                total_size as f64 / 1024.0
            );
            for (i, binary) in flash_binaries.iter().enumerate() {
                println!(
                    "  [{}/{}] {} → 0x{:05x} | {:.1} KB | {}",
                    i + 1,
                    flash_binaries.len(),
                    binary.name,
                    binary.offset,
                    binary.data.len() as f64 / 1024.0,
                    binary.file_name
                );
            }
            println!("🔧 FlashService: Calling perform_multi_flash");
            Self::perform_multi_flash(&board_port, flash_binaries, &request.flash_config).await
        } else {
            // Legacy single binary flash - validate that we have data
            if request.binary_data.is_empty() {
                let error_msg = "Legacy flash request contains no binary data";
                println!("❌ FlashService: {}", error_msg);
                return Ok(FlashResponse {
                    success: false,
                    message: error_msg.to_string(),
                    flash_id: None,
                    duration_ms: Some(0),
                    progress: None,
                });
            }

            println!(
                "⚠️ FlashService: Using legacy single binary flash at offset 0x{:x}",
                request.offset
            );
            println!("🔧 FlashService: Calling perform_flash");
            Self::perform_flash(&board_port, &request.binary_data, request.offset).await
        };

        println!(
            "📊 FlashService: Flash operation completed with result: {:?}",
            result.is_ok()
        );

        let duration_ms = start_time.elapsed().as_millis() as u64;

        // Update board status based on result
        {
            let mut state_lock = self.state.write().await;
            if let Some(board) = state_lock.boards.get_mut(&request.board_id) {
                match &result {
                    Ok(_) => {
                        board.status = BoardStatus::Available;
                        board.last_updated = chrono::Local::now();
                    }
                    Err(e) => {
                        board.status = BoardStatus::Error(e.to_string());
                        board.last_updated = chrono::Local::now();
                    }
                }
            }
        }

        match result {
            Ok(_) => {
                let total_size: usize = if let Some(binaries) = &request.flash_binaries {
                    binaries.iter().map(|b| b.data.len()).sum()
                } else {
                    request.binary_data.len()
                };

                println!(
                    "✅ Flash operation completed successfully in {}ms",
                    duration_ms
                );
                Ok(FlashResponse {
                    success: true,
                    message: format!(
                        "Successfully flashed {} ({} bytes) in {}ms",
                        request.board_id, total_size, duration_ms
                    ),
                    flash_id: None,
                    duration_ms: Some(duration_ms),
                    progress: None,
                })
            }
            Err(e) => {
                println!("❌ Flash operation failed: {}", e);
                Ok(FlashResponse {
                    success: false,
                    message: format!("Flash failed: {}", e),
                    flash_id: None,
                    duration_ms: Some(duration_ms),
                    progress: None,
                })
            }
        }
    }

    /// Perform multi-binary flash operation (bootloader + partition table + application)
    async fn perform_multi_flash(
        port: &str,
        flash_binaries: &[FlashBinary],
        flash_config: &Option<FlashConfig>,
    ) -> Result<()> {
        let total_size: usize = flash_binaries.iter().map(|b| b.data.len()).sum();
        println!(
            "🔥 FlashService::perform_multi_flash STARTED - port: {}, {} binaries ({:.1} KB)",
            port,
            flash_binaries.len(),
            total_size as f64 / 1024.0
        );

        let flash_start = std::time::Instant::now();
        println!("🚀 FlashService: Starting native multi-binary flash operation...");
        let result = Self::perform_multi_flash_native(port, flash_binaries, flash_config).await;
        let flash_duration = flash_start.elapsed();

        match &result {
            Ok(_) => {
                println!(
                    "✅ FlashService::perform_multi_flash COMPLETED successfully in {:.2}s ({:.1} KB/s)",
                    flash_duration.as_secs_f64(),
                    total_size as f64 / 1024.0 / flash_duration.as_secs_f64().max(0.001)
                );
            }
            Err(e) => {
                println!(
                    "❌ FlashService::perform_multi_flash FAILED after {:.2}s: {}",
                    flash_duration.as_secs_f64(),
                    e
                );
            }
        }
        result
    }

    /// Native multi-binary flash using espflash library
    async fn perform_multi_flash_native(
        port: &str,
        flash_binaries: &[FlashBinary],
        _flash_config: &Option<FlashConfig>,
    ) -> Result<()> {
        println!(
            "✨ FlashService::perform_multi_flash_native ENTERED - port: {}",
            port
        );

        // Use our existing espflash utils for native flashing
        let mut flash_data_map = std::collections::HashMap::new();

        for (i, binary) in flash_binaries.iter().enumerate() {
            flash_data_map.insert(binary.offset, binary.data.clone());
            println!(
                "  🔥 [{}/{}] Preparing {} at 0x{:05x} ({:.1} KB)",
                i + 1,
                flash_binaries.len(),
                binary.name,
                binary.offset,
                binary.data.len() as f64 / 1024.0
            );
        }

        println!(
            "🚀 FlashService: Calling espflash_utils::flash_multi_binary with {} binaries",
            flash_data_map.len()
        );
        // Convert to the format expected by espflash_utils
        let result = crate::utils::espflash_utils::flash_multi_binary(port, flash_data_map)
            .await
            .map_err(|e| anyhow::anyhow!("Native espflash multi-binary flash failed: {}", e));
        println!(
            "📊 FlashService::perform_multi_flash_native result: {:?}",
            result.is_ok()
        );
        result
    }

    /// Perform single binary flash operation using native espflash (legacy support)
    async fn perform_flash(port: &str, binary_data: &[u8], offset: u32) -> Result<()> {
        println!(
            "🔥 Single binary native flash: {} bytes at 0x{:x} on port {}",
            binary_data.len(),
            offset,
            port
        );

        // Convert single binary to multi-binary format and use the same function
        let mut flash_data_map = std::collections::HashMap::new();
        flash_data_map.insert(offset, binary_data.to_vec());

        // Use the existing native multi-binary flash function
        crate::utils::espflash_utils::flash_multi_binary(port, flash_data_map)
            .await
            .map_err(|e| anyhow::anyhow!("Native espflash single binary flash failed: {}", e))
    }
}
