//! Flash service for handling ESP32 board flashing operations

use anyhow::Result;
use log::{debug, error, info};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{RwLock, mpsc};

use crate::models::board::{BoardFlashProgress, BoardStatus};
use crate::models::flash::{FlashBinary, FlashConfig, FlashRequest, FlashResponse};
use crate::server::app::ServerState;
use crate::utils::espflash_utils::ProgressUpdate;

/// Progress message sent from flash task to progress updater
#[derive(Debug, Clone)]
struct FlashProgressMessage {
    board_id: String,
    progress: BoardFlashProgress,
}

/// Flash task completion message
#[derive(Debug, Clone)]
struct FlashCompletionMessage {
    board_id: String,
    result: Result<String, String>, // Ok(success_message) or Err(error_message)
    duration_ms: u64,
}

/// Flash service for handling board flashing operations
#[derive(Clone)]
pub struct FlashService {
    state: Arc<RwLock<ServerState>>,
    /// Channel for receiving progress updates from flash tasks
    progress_tx: mpsc::UnboundedSender<FlashProgressMessage>,
    /// Channel for receiving completion notifications from flash tasks
    completion_tx: mpsc::UnboundedSender<FlashCompletionMessage>,
}

impl FlashService {
    /// Convert ProgressUpdate to BoardFlashProgress
    fn progress_update_to_board_progress(update: ProgressUpdate) -> BoardFlashProgress {
        BoardFlashProgress {
            current_segment: update.current_segment,
            total_segments: update.total_segments,
            current_segment_name: update.current_segment_name,
            overall_progress: update.overall_progress,
            segment_progress: update.segment_progress,
            bytes_written: update.bytes_written,
            total_bytes: update.total_bytes,
            current_operation: update.current_operation,
            started_at: update.started_at,
            estimated_completion: None, // Will be calculated later
        }
    }

    pub fn new(state: Arc<RwLock<ServerState>>) -> Self {
        // Create channels for progress and completion communication
        let (progress_tx, progress_rx) = mpsc::unbounded_channel::<FlashProgressMessage>();
        let (completion_tx, completion_rx) = mpsc::unbounded_channel::<FlashCompletionMessage>();

        // Spawn background task to handle progress updates and completions
        let state_clone = Arc::clone(&state);
        tokio::spawn(Self::progress_updater_task(
            state_clone,
            progress_rx,
            completion_rx,
        ));

        Self {
            state,
            progress_tx,
            completion_tx,
        }
    }

    /// Background task that handles progress updates and completion notifications
    async fn progress_updater_task(
        state: Arc<RwLock<ServerState>>,
        mut progress_rx: mpsc::UnboundedReceiver<FlashProgressMessage>,
        mut completion_rx: mpsc::UnboundedReceiver<FlashCompletionMessage>,
    ) {
        // Also create a receiver for direct ProgressUpdate messages from espflash utils
        let (direct_progress_tx, mut direct_progress_rx) =
            mpsc::unbounded_channel::<ProgressUpdate>();
        // Store the sender for later use in flash operations
        // Note: This is a simplified approach. In a more complex system, you might want to
        // pass this through the flash service methods.
        info!("Flash progress updater task started");

        let mut last_log_times: std::collections::HashMap<String, Instant> =
            std::collections::HashMap::new();
        const LOG_INTERVAL: std::time::Duration = std::time::Duration::from_secs(5);

        loop {
            tokio::select! {
                // Handle progress updates
                progress_msg = progress_rx.recv() => {
                    match progress_msg {
                        Some(msg) => {
                            let now = Instant::now();
                            let should_log = last_log_times
                                .get(&msg.board_id)
                                .map_or(true, |last_time| now.duration_since(*last_time) >= LOG_INTERVAL);

                            // Update board state
                            {
                                let mut state_lock = state.write().await;
                                if let Some(board) = state_lock.boards.get_mut(&msg.board_id) {
                                    board.flash_progress = Some(msg.progress.clone());
                                    board.last_updated = chrono::Local::now();

                                    // Log progress every 5 seconds at INFO level
                                    if should_log {
                                        let speed_kbps = if msg.progress.bytes_written > 0 {
                                            let elapsed_secs = chrono::Local::now().signed_duration_since(msg.progress.started_at).num_seconds().max(1) as f64;
                                            (msg.progress.bytes_written as f64 / 1024.0) / elapsed_secs
                                        } else {
                                            0.0
                                        };

                                        info!(
                                            "🔥 Flash Progress [{}]: {}/{} ({:.1}%) | {} | {:.1} KB/s | {} | ETA: {}",
                                            msg.board_id,
                                            msg.progress.current_segment,
                                            msg.progress.total_segments,
                                            msg.progress.overall_progress,
                                            msg.progress.current_segment_name,
                                            speed_kbps,
                                            msg.progress.current_operation,
                                            msg.progress.estimated_completion
                                                .map(|eta| eta.format("%H:%M:%S").to_string())
                                                .unwrap_or_else(|| "calculating...".to_string())
                                        );
                                        last_log_times.insert(msg.board_id.clone(), now);
                                    }
                                }
                            }
                        }
                        None => {
                            debug!("Progress channel closed");
                            break;
                        }
                    }
                }

                // Handle completion notifications
                completion_msg = completion_rx.recv() => {
                    match completion_msg {
                        Some(msg) => {
                            info!("🏁 Flash operation completed for board {}", msg.board_id);

                            // Update board status based on result
                            {
                                let mut state_lock = state.write().await;
                                if let Some(board) = state_lock.boards.get_mut(&msg.board_id) {
                                    match msg.result {
                                        Ok(success_msg) => {
                                            board.status = BoardStatus::Available;
                                            board.flash_progress = None;
                                            board.last_updated = chrono::Local::now();
                                            info!("✅ Flash SUCCESS for {}: {} ({}ms)", msg.board_id, success_msg, msg.duration_ms);
                                        }
                                        Err(error_msg) => {
                                            board.status = BoardStatus::Error(error_msg.clone());
                                            board.flash_progress = None;
                                            board.last_updated = chrono::Local::now();
                                            error!("❌ Flash FAILED for {}: {} ({}ms)", msg.board_id, error_msg, msg.duration_ms);
                                        }
                                    }
                                }
                            }

                            // Remove from log tracking
                            last_log_times.remove(&msg.board_id);
                        }
                        None => {
                            debug!("Completion channel closed");
                            break;
                        }
                    }
                }
            }
        }

        info!("Flash progress updater task terminated");
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

        // Initialize flash progress and update status to flashing
        let total_segments = if let Some(flash_binaries) = &request.flash_binaries {
            flash_binaries.len() as u32
        } else {
            1
        };

        let total_bytes: u64 = if let Some(flash_binaries) = &request.flash_binaries {
            flash_binaries.iter().map(|b| b.data.len() as u64).sum()
        } else {
            request.binary_data.len() as u64
        };

        {
            println!("🔄 FlashService: Updating board status to flashing with progress tracking");
            let mut state_lock = self.state.write().await;
            if let Some(board) = state_lock.boards.get_mut(&request.board_id) {
                board.status = BoardStatus::Flashing;
                board.flash_progress = Some(crate::models::board::BoardFlashProgress {
                    current_segment: 0,
                    total_segments,
                    current_segment_name: "Initializing...".to_string(),
                    overall_progress: 0.0,
                    segment_progress: 0.0,
                    bytes_written: 0,
                    total_bytes,
                    current_operation: "Connecting".to_string(),
                    started_at: chrono::Local::now(),
                    estimated_completion: None,
                });
                board.last_updated = chrono::Local::now();
                println!(
                    "✅ FlashService: Board status updated to flashing with progress tracking"
                );
            } else {
                println!(
                    "❌ FlashService: Board {} not found when updating status",
                    request.board_id
                );
            }
        }

        // Spawn flash operation in background using fire-and-forget pattern
        let board_id_clone = request.board_id.clone();
        let port_clone = board_port.clone();
        let progress_tx_clone = self.progress_tx.clone();
        let completion_tx_clone = self.completion_tx.clone();
        let request_clone = request.clone();

        tokio::spawn(async move {
            Self::perform_flash_operation(
                board_id_clone,
                port_clone,
                request_clone,
                progress_tx_clone,
                completion_tx_clone,
            )
            .await
        });

        // Return immediate response - flash is running in background
        let total_size: usize = if let Some(flash_binaries) = &request.flash_binaries {
            flash_binaries.iter().map(|b| b.data.len()).sum()
        } else {
            request.binary_data.len()
        };

        println!(
            "✅ Flash operation started in background for board {}",
            request.board_id
        );
        Ok(FlashResponse {
            success: true,
            message: format!(
                "Flash operation started for {} ({} bytes)",
                request.board_id, total_size
            ),
            flash_id: Some(format!("flash_{}", request.board_id)),
            duration_ms: Some(0), // Will be reported on completion
            progress: None,
        })
    }

    /// Perform the actual flash operation in background
    async fn perform_flash_operation(
        board_id: String,
        board_port: String,
        request: FlashRequest,
        progress_tx: mpsc::UnboundedSender<FlashProgressMessage>,
        completion_tx: mpsc::UnboundedSender<FlashCompletionMessage>,
    ) {
        let start_time = Instant::now();
        println!(
            "🔥 Starting flash operation for board {} on port {}",
            board_id, board_port
        );

        // Validate that we have valid binaries to flash
        println!("🚀 FlashService: Starting flash operation decision logic");
        let result = if let Some(flash_binaries) = &request.flash_binaries {
            // Validate multi-binary flash request
            if flash_binaries.is_empty() {
                let error_msg = "Multi-binary flash request contains no binaries";
                println!("❌ FlashService: {}", error_msg);
                let _ = completion_tx.send(FlashCompletionMessage {
                    board_id: board_id.clone(),
                    result: Err(error_msg.to_string()),
                    duration_ms: start_time.elapsed().as_millis() as u64,
                });
                return;
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
                let _ = completion_tx.send(FlashCompletionMessage {
                    board_id: board_id.clone(),
                    result: Err(error_msg),
                    duration_ms: start_time.elapsed().as_millis() as u64,
                });
                return;
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
            println!("🔧 FlashService: Calling perform_multi_flash_with_progress");
            Self::perform_multi_flash_with_progress(
                &board_port,
                flash_binaries,
                &request.flash_config,
                &board_id,
                &progress_tx,
            )
            .await
        } else {
            // Legacy single binary flash - validate that we have data
            if request.binary_data.is_empty() {
                let error_msg = "Legacy flash request contains no binary data";
                println!("❌ FlashService: {}", error_msg);
                let _ = completion_tx.send(FlashCompletionMessage {
                    board_id: board_id.clone(),
                    result: Err(error_msg.to_string()),
                    duration_ms: start_time.elapsed().as_millis() as u64,
                });
                return;
            }

            println!(
                "⚠️ FlashService: Using legacy single binary flash at offset 0x{:x}",
                request.offset
            );
            println!("🔧 FlashService: Calling perform_flash_with_progress");
            Self::perform_flash_with_progress(
                &board_port,
                &request.binary_data,
                request.offset,
                &board_id,
                &progress_tx,
            )
            .await
        };

        println!(
            "📊 FlashService: Flash operation completed with result: {:?}",
            result.is_ok()
        );

        let duration_ms = start_time.elapsed().as_millis() as u64;

        // Send completion message
        let completion_message = match result {
            Ok(_) => {
                let total_size: usize = if let Some(binaries) = &request.flash_binaries {
                    binaries.iter().map(|b| b.data.len()).sum()
                } else {
                    request.binary_data.len()
                };

                FlashCompletionMessage {
                    board_id: board_id.clone(),
                    result: Ok(format!("Successfully flashed {} bytes", total_size)),
                    duration_ms,
                }
            }
            Err(e) => FlashCompletionMessage {
                board_id: board_id.clone(),
                result: Err(e.to_string()),
                duration_ms,
            },
        };

        let _ = completion_tx.send(completion_message);
        println!(
            "🏁 Flash background operation completed for board {}",
            board_id
        );
    }

    /// Perform multi-binary flash operation with progress reporting
    async fn perform_multi_flash_with_progress(
        port: &str,
        flash_binaries: &[FlashBinary],
        flash_config: &Option<FlashConfig>,
        board_id: &str,
        progress_tx: &mpsc::UnboundedSender<FlashProgressMessage>,
    ) -> Result<()> {
        let total_size: usize = flash_binaries.iter().map(|b| b.data.len()).sum();
        println!(
            "🔥 FlashService::perform_multi_flash_with_progress STARTED - port: {}, {} binaries ({:.1} KB)",
            port,
            flash_binaries.len(),
            total_size as f64 / 1024.0
        );

        // Convert to format expected by espflash utils and include progress reporting
        let mut flash_data_map = std::collections::HashMap::new();
        for binary in flash_binaries {
            flash_data_map.insert(binary.offset, binary.data.clone());
        }

        // Create a channel to receive progress updates from espflash utils
        let (util_progress_tx, mut util_progress_rx) =
            mpsc::unbounded_channel::<crate::utils::espflash_utils::ProgressUpdate>();

        // Spawn a task to convert ProgressUpdate to FlashProgressMessage
        let board_id_clone = board_id.to_string();
        let progress_tx_clone = progress_tx.clone();
        let progress_forwarder = tokio::spawn(async move {
            while let Some(progress_update) = util_progress_rx.recv().await {
                let board_progress = Self::progress_update_to_board_progress(progress_update);
                let flash_progress_msg = FlashProgressMessage {
                    board_id: board_id_clone.clone(),
                    progress: board_progress,
                };
                let _ = progress_tx_clone.send(flash_progress_msg);
            }
        });

        // Call espflash with progress reporting
        let result = crate::utils::espflash_utils::flash_multi_binary_with_progress(
            port,
            flash_data_map,
            Some(board_id.to_string()),
            Some(util_progress_tx),
        )
        .await;

        // Clean up progress forwarder
        progress_forwarder.abort();

        result
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

    /// Native multi-binary flash using espflash library with progress tracking
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

    /// Update flash progress for a board
    pub async fn update_flash_progress(
        &self,
        board_id: &str,
        progress: crate::models::board::BoardFlashProgress,
    ) {
        let mut state_lock = self.state.write().await;
        if let Some(board) = state_lock.boards.get_mut(board_id) {
            if matches!(board.status, crate::models::board::BoardStatus::Flashing) {
                board.flash_progress = Some(progress);
                board.last_updated = chrono::Local::now();
                log::debug!(
                    "Updated flash progress for {}: {}%",
                    board_id,
                    board.flash_progress.as_ref().unwrap().overall_progress
                );
            }
        }
    }

    /// Perform single binary flash operation with progress reporting
    async fn perform_flash_with_progress(
        port: &str,
        binary_data: &[u8],
        offset: u32,
        board_id: &str,
        progress_tx: &mpsc::UnboundedSender<FlashProgressMessage>,
    ) -> Result<()> {
        println!(
            "🔥 Single binary native flash with progress: {} bytes at 0x{:x} on port {}",
            binary_data.len(),
            offset,
            port
        );

        // Convert single binary to multi-binary format
        let mut flash_data_map = std::collections::HashMap::new();
        flash_data_map.insert(offset, binary_data.to_vec());

        // Create a channel to receive progress updates from espflash utils
        let (util_progress_tx, mut util_progress_rx) =
            mpsc::unbounded_channel::<crate::utils::espflash_utils::ProgressUpdate>();

        // Spawn a task to convert ProgressUpdate to FlashProgressMessage
        let board_id_clone = board_id.to_string();
        let progress_tx_clone = progress_tx.clone();
        let progress_forwarder = tokio::spawn(async move {
            while let Some(progress_update) = util_progress_rx.recv().await {
                let board_progress = Self::progress_update_to_board_progress(progress_update);
                let flash_progress_msg = FlashProgressMessage {
                    board_id: board_id_clone.clone(),
                    progress: board_progress,
                };
                let _ = progress_tx_clone.send(flash_progress_msg);
            }
        });

        // Use the progress-enabled multi-binary flash function
        let result = crate::utils::espflash_utils::flash_multi_binary_with_progress(
            port,
            flash_data_map,
            Some(board_id.to_string()),
            Some(util_progress_tx),
        )
        .await
        .map_err(|e| anyhow::anyhow!("Native espflash single binary flash failed: {}", e));

        // Clean up progress forwarder
        progress_forwarder.abort();

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
