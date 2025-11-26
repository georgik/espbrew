//! Flash endpoint routes

use anyhow;
use bytes::Buf;
use futures_util::TryStreamExt;
use log::{debug, error, info};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use warp::Filter;

use crate::models::flash::{FlashBinary, FlashConfig, FlashRequest, FlashResponse};
use crate::server::app::ServerState;
use crate::server::services::flash_service::FlashService;

/// Create all flash-related routes
pub fn create_flash_routes(
    state: Arc<RwLock<ServerState>>,
) -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
    let flash_json = flash_json_route(state.clone());
    let flash_form_multi = flash_form_multi_route(state.clone());
    let flash_form_legacy = flash_form_legacy_route(state.clone());

    warp::path("api")
        .and(warp::path("v1"))
        .and(warp::path("flash"))
        .and(warp::path::end())
        .and(
            // Try JSON first, then multi-binary form, then legacy form (match original order)
            flash_json.or(flash_form_multi).or(flash_form_legacy),
        )
}

/// POST /api/v1/flash - Flash a board (JSON API)
fn flash_json_route(
    state: Arc<RwLock<ServerState>>,
) -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
    warp::post()
        .and(warp::body::json())
        .and(with_server_state(state))
        .and_then(flash_json_handler)
}

/// POST /api/v1/flash - Flash a board (Multipart form for web interface - multi-binary)
fn flash_form_multi_route(
    state: Arc<RwLock<ServerState>>,
) -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
    warp::post()
        .and(warp::multipart::form().max_length(500 * 1024 * 1024)) // 500MB max for multi-binary
        .and(with_server_state(state))
        .and_then(flash_form_multi_handler)
}

/// POST /api/v1/flash - Flash a board (Multipart form for web interface - legacy single binary)
fn flash_form_legacy_route(
    state: Arc<RwLock<ServerState>>,
) -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
    warp::post()
        .and(warp::multipart::form().max_length(100 * 1024 * 1024)) // 100MB max for legacy
        .and(with_server_state(state))
        .and_then(flash_form_legacy_handler)
}

/// Helper function to pass server state to handlers
fn with_server_state(
    state: Arc<RwLock<ServerState>>,
) -> impl Filter<Extract = (Arc<RwLock<ServerState>>,), Error = std::convert::Infallible> + Clone {
    warp::any().map(move || Arc::clone(&state))
}

/// Handler for POST /api/v1/flash (JSON)
async fn flash_json_handler(
    request: FlashRequest,
    state: Arc<RwLock<ServerState>>,
) -> Result<impl warp::Reply, warp::Rejection> {
    info!(
        "📥 JSON Flash Handler: Processing request for board {}",
        request.board_id
    );
    let flash_service = FlashService::new(state);

    match flash_service.flash_board(request).await {
        Ok(response) => {
            info!("✅ JSON Flash Handler: Operation completed successfully");
            Ok(warp::reply::json(&response))
        }
        Err(e) => {
            error!("❌ JSON Flash Handler: Operation failed: {}", e);
            let error_response = FlashResponse {
                success: false,
                message: format!("Flash operation failed: {}", e),
                flash_id: None,
                duration_ms: None,
                progress: None,
            };
            Ok(warp::reply::json(&error_response))
        }
    }
}

/// Handler for POST /api/v1/flash (Multipart - multi-binary)
async fn flash_form_multi_handler(
    form: warp::multipart::FormData,
    state: Arc<RwLock<ServerState>>,
) -> Result<impl warp::Reply, warp::Rejection> {
    info!("📥 FLASH HANDLER CALLED - MULTIPART MULTI-BINARY");
    info!("📥 Multi-binary Flash Handler: Processing multipart form...");
    let request_start = std::time::Instant::now();

    debug!("📥 About to parse multipart form...");

    match parse_multipart_flash_form(form).await {
        Ok(flash_request) => {
            let parsing_duration = request_start.elapsed();

            // Log detailed request information
            let binary_count = flash_request
                .flash_binaries
                .as_ref()
                .map(|b| b.len())
                .unwrap_or(0);
            let total_size: usize = flash_request
                .flash_binaries
                .as_ref()
                .map(|binaries| binaries.iter().map(|b| b.data.len()).sum())
                .unwrap_or(0);

            info!(
                "📋 Parsed flash request in {:.2}ms:",
                parsing_duration.as_secs_f64() * 1000.0
            );
            info!("  📋 Board ID: {}", flash_request.board_id);
            info!("  📦 Binary count: {}", binary_count);
            info!("  📊 Total size: {:.1} KB", total_size as f64 / 1024.0);
            info!(
                "  ⚙️ Has flash config: {}",
                flash_request.flash_config.is_some()
            );

            if let Some(config) = &flash_request.flash_config {
                println!(
                    "  ⚙️ Flash config: mode={}, freq={}, size={}",
                    config.flash_mode, config.flash_freq, config.flash_size
                );
            }

            // Log each binary in detail
            if let Some(binaries) = &flash_request.flash_binaries {
                println!("  📄 Binaries received:");
                for (i, binary) in binaries.iter().enumerate() {
                    println!(
                        "    [{}/{}] {} → 0x{:05x} | {:.1} KB | {}",
                        i + 1,
                        binaries.len(),
                        binary.name,
                        binary.offset,
                        binary.data.len() as f64 / 1024.0,
                        binary.file_name
                    );
                }
            } else {
                println!("  ⚠️ No binaries in flash request!");
            }

            // Handle both multi-binary and single-binary cases in this handler
            // No need to fall back to another handler - just process the request here
            println!(
                "📦 Multi-binary Flash Handler: Processing flash request for board {}",
                flash_request.board_id
            );
            let flash_service = FlashService::new(state);

            match flash_service.flash_board(flash_request).await {
                Ok(response) => {
                    println!("✅ Multi-binary Flash Handler: Operation completed successfully");
                    Ok(warp::reply::json(&response))
                }
                Err(e) => {
                    println!("❌ Multi-binary Flash Handler: Operation failed: {}", e);
                    let error_response = FlashResponse {
                        success: false,
                        message: format!("Multi-binary flash operation failed: {}", e),
                        flash_id: None,
                        duration_ms: None,
                        progress: None,
                    };
                    Ok(warp::reply::json(&error_response))
                }
            }
        }
        Err(e) => {
            println!("❌ Failed to parse multipart form: {}", e);
            let error_response = FlashResponse {
                success: false,
                message: format!("Failed to parse multipart form: {}", e),
                flash_id: None,
                duration_ms: None,
                progress: None,
            };
            Ok(warp::reply::json(&error_response))
        }
    }
}

/// Handler for POST /api/v1/flash (Multipart - legacy single binary)
async fn flash_form_legacy_handler(
    form: warp::multipart::FormData,
    state: Arc<RwLock<ServerState>>,
) -> Result<impl warp::Reply, warp::Rejection> {
    println!("📥 Legacy Flash Handler: Processing multipart legacy flash request...");

    match parse_multipart_flash_form(form).await {
        Ok(flash_request) => {
            println!(
                "📦 Legacy Flash Handler: Processing legacy single binary flash request for board {}",
                flash_request.board_id
            );
            let flash_service = FlashService::new(state);

            match flash_service.flash_board(flash_request).await {
                Ok(response) => {
                    println!("✅ Legacy Flash Handler: Operation completed successfully");
                    Ok(warp::reply::json(&response))
                }
                Err(e) => {
                    println!("❌ Legacy Flash Handler: Operation failed: {}", e);
                    let error_response = FlashResponse {
                        success: false,
                        message: format!("Legacy flash operation failed: {}", e),
                        flash_id: None,
                        duration_ms: None,
                        progress: None,
                    };
                    Ok(warp::reply::json(&error_response))
                }
            }
        }
        Err(e) => {
            println!("❌ Failed to parse multipart form: {}", e);
            let error_response = FlashResponse {
                success: false,
                message: format!("Failed to parse multipart form: {}", e),
                flash_id: None,
                duration_ms: None,
                progress: None,
            };
            Ok(warp::reply::json(&error_response))
        }
    }
}

/// Parse multipart form data into FlashRequest
async fn parse_multipart_flash_form(
    mut form: warp::multipart::FormData,
) -> Result<FlashRequest, anyhow::Error> {
    println!("🔧 PARSING MULTIPART FORM STARTED");
    let mut board_id = String::new();
    let mut flash_mode = String::new();
    let mut flash_freq = String::new();
    let mut flash_size = String::new();
    let mut binary_count = 0usize;
    let mut binaries = Vec::new();
    let mut binary_offsets: HashMap<usize, u32> = HashMap::new();
    let mut binary_names: HashMap<usize, String> = HashMap::new();
    let mut binary_filenames: HashMap<usize, String> = HashMap::new();
    let mut single_binary_data: Option<Vec<u8>> = None;

    // Process all form parts
    while let Some(part) = form
        .try_next()
        .await
        .map_err(|e| anyhow::anyhow!("Error reading multipart: {}", e))?
    {
        let name = part.name();

        println!("🔧 Processing multipart field: '{}'", name);

        if name == "board_id" {
            let data = part
                .stream()
                .try_fold(Vec::new(), |mut acc, chunk| async move {
                    acc.extend_from_slice(chunk.chunk());
                    Ok(acc)
                })
                .await
                .map_err(|e| anyhow::anyhow!("Error reading board_id: {}", e))?;
            board_id = String::from_utf8(data)
                .map_err(|e| anyhow::anyhow!("Invalid UTF-8 in board_id: {}", e))?;
        } else if name == "flash_mode" {
            let data = part
                .stream()
                .try_fold(Vec::new(), |mut acc, chunk| async move {
                    acc.extend_from_slice(chunk.chunk());
                    Ok(acc)
                })
                .await
                .map_err(|e| anyhow::anyhow!("Error reading flash_mode: {}", e))?;
            flash_mode = String::from_utf8(data)
                .map_err(|e| anyhow::anyhow!("Invalid UTF-8 in flash_mode: {}", e))?;
        } else if name == "flash_freq" {
            let data = part
                .stream()
                .try_fold(Vec::new(), |mut acc, chunk| async move {
                    acc.extend_from_slice(chunk.chunk());
                    Ok(acc)
                })
                .await
                .map_err(|e| anyhow::anyhow!("Error reading flash_freq: {}", e))?;
            flash_freq = String::from_utf8(data)
                .map_err(|e| anyhow::anyhow!("Invalid UTF-8 in flash_freq: {}", e))?;
        } else if name == "flash_size" {
            let data = part
                .stream()
                .try_fold(Vec::new(), |mut acc, chunk| async move {
                    acc.extend_from_slice(chunk.chunk());
                    Ok(acc)
                })
                .await
                .map_err(|e| anyhow::anyhow!("Error reading flash_size: {}", e))?;
            flash_size = String::from_utf8(data)
                .map_err(|e| anyhow::anyhow!("Invalid UTF-8 in flash_size: {}", e))?;
        } else if name == "binary_count" {
            let data = part
                .stream()
                .try_fold(Vec::new(), |mut acc, chunk| async move {
                    acc.extend_from_slice(chunk.chunk());
                    Ok(acc)
                })
                .await
                .map_err(|e| anyhow::anyhow!("Error reading binary_count: {}", e))?;
            let count_str = String::from_utf8(data)
                .map_err(|e| anyhow::anyhow!("Invalid UTF-8 in binary_count: {}", e))?;
            binary_count = count_str
                .parse()
                .map_err(|e| anyhow::anyhow!("Invalid number in binary_count: {}", e))?;
        } else if name == "binary_file" {
            // Legacy single binary handling
            let binary_data = part
                .stream()
                .try_fold(Vec::new(), |mut acc, chunk| async move {
                    acc.extend_from_slice(chunk.chunk());
                    Ok(acc)
                })
                .await
                .map_err(|e| anyhow::anyhow!("Error reading binary file: {}", e))?;
            single_binary_data = Some(binary_data);
        } else if name.starts_with("binary_") {
            // Multi-binary handling
            if let Some(suffix) = name.strip_prefix("binary_") {
                if let Some(index_str) = suffix.split('_').next() {
                    if let Ok(index) = index_str.parse::<usize>() {
                        if suffix.contains("_offset") {
                            let data = part
                                .stream()
                                .try_fold(Vec::new(), |mut acc, chunk| async move {
                                    acc.extend_from_slice(chunk.chunk());
                                    Ok(acc)
                                })
                                .await
                                .map_err(|e| {
                                    anyhow::anyhow!("Error reading binary offset: {}", e)
                                })?;
                            let offset_str = String::from_utf8(data).map_err(|e| {
                                anyhow::anyhow!("Invalid UTF-8 in binary offset: {}", e)
                            })?;
                            let offset = if offset_str.starts_with("0x") {
                                u32::from_str_radix(&offset_str[2..], 16)
                            } else {
                                offset_str.parse()
                            }
                            .map_err(|e| anyhow::anyhow!("Invalid offset format: {}", e))?;
                            binary_offsets.insert(index, offset);
                        } else if suffix.contains("_name") {
                            let data = part
                                .stream()
                                .try_fold(Vec::new(), |mut acc, chunk| async move {
                                    acc.extend_from_slice(chunk.chunk());
                                    Ok(acc)
                                })
                                .await
                                .map_err(|e| anyhow::anyhow!("Error reading binary name: {}", e))?;
                            let name = String::from_utf8(data).map_err(|e| {
                                anyhow::anyhow!("Invalid UTF-8 in binary name: {}", e)
                            })?;
                            binary_names.insert(index, name);
                        } else if suffix.contains("_filename") {
                            let data = part
                                .stream()
                                .try_fold(Vec::new(), |mut acc, chunk| async move {
                                    acc.extend_from_slice(chunk.chunk());
                                    Ok(acc)
                                })
                                .await
                                .map_err(|e| {
                                    anyhow::anyhow!("Error reading binary filename: {}", e)
                                })?;
                            let filename = String::from_utf8(data).map_err(|e| {
                                anyhow::anyhow!("Invalid UTF-8 in binary filename: {}", e)
                            })?;
                            binary_filenames.insert(index, filename);
                        } else if !suffix.contains('_') {
                            // This is the actual binary data
                            let binary_data = part
                                .stream()
                                .try_fold(Vec::new(), |mut acc, chunk| async move {
                                    acc.extend_from_slice(chunk.chunk());
                                    Ok(acc)
                                })
                                .await
                                .map_err(|e| anyhow::anyhow!("Error reading binary data: {}", e))?;

                            // Ensure we have a slot for this binary
                            while binaries.len() <= index {
                                binaries.push(None);
                            }
                            binaries[index] = Some(binary_data);
                        }
                    }
                }
            }
        }
    }

    if board_id.is_empty() {
        return Err(anyhow::anyhow!("Missing board_id in form data"));
    }

    // Build the FlashRequest
    let flash_config = if !flash_mode.is_empty() || !flash_freq.is_empty() || !flash_size.is_empty()
    {
        Some(FlashConfig {
            flash_mode: if flash_mode.is_empty() {
                "dio".to_string()
            } else {
                flash_mode
            },
            flash_freq: if flash_freq.is_empty() {
                "40m".to_string()
            } else {
                flash_freq
            },
            flash_size: if flash_size.is_empty() {
                "detect".to_string()
            } else {
                flash_size
            },
        })
    } else {
        None
    };

    let flash_binaries = if let Some(single_data) = single_binary_data {
        // Legacy single binary format
        println!(
            "📦 Processing single binary: {:.1} KB (legacy mode)",
            single_data.len() as f64 / 1024.0
        );
        Some(vec![FlashBinary {
            offset: 0x10000, // Default app offset for single binary flash
            data: single_data,
            name: "application".to_string(),
            file_name: "firmware.bin".to_string(),
        }])
    } else if binary_count > 0 && binaries.len() >= binary_count {
        // Multi-binary format
        let mut flash_binaries = Vec::new();
        for i in 0..binary_count {
            if let Some(Some(data)) = binaries.get(i) {
                let offset = binary_offsets.get(&i).cloned().unwrap_or(0);
                let name = binary_names
                    .get(&i)
                    .cloned()
                    .unwrap_or_else(|| format!("binary_{}", i));
                let file_name = binary_filenames
                    .get(&i)
                    .cloned()
                    .unwrap_or_else(|| format!("binary_{}.bin", i));

                println!(
                    "📦 [{}/{}] {} at 0x{:x}: {:.1} KB ({})",
                    i + 1,
                    binary_count,
                    name,
                    offset,
                    data.len() as f64 / 1024.0,
                    file_name
                );
                flash_binaries.push(FlashBinary {
                    offset,
                    data: data.clone(),
                    name,
                    file_name,
                });
            }
        }
        let total_size: usize = flash_binaries.iter().map(|b| b.data.len()).sum();
        println!(
            "✅ Server received {} ESP-IDF binaries ({:.1} KB total) for board {}",
            flash_binaries.len(),
            total_size as f64 / 1024.0,
            board_id
        );
        Some(flash_binaries)
    } else {
        println!(
            "⚠️ Server: No valid binaries found in multipart form for board {}",
            board_id
        );
        println!(
            "⚠️ Debug info: binary_count={}, binaries.len()={}",
            binary_count,
            binaries.len()
        );
        None
    };

    println!("🔧 PARSING MULTIPART FORM COMPLETED");
    println!(
        "🔧 Final result: board_id='{}', has_binaries={}",
        board_id,
        flash_binaries.is_some()
    );

    Ok(FlashRequest {
        board_id,
        binary_data: Vec::new(), // Deprecated field
        offset: 0,               // Deprecated field
        chip_type: None,
        verify: false,
        flash_binaries,
        flash_config,
    })
}
