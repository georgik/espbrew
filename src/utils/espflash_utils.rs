use anyhow::{Context, Result};
use std::borrow::Cow;
use std::path::Path;
use std::time::Duration;

use espflash::connection::{Connection, ResetAfterOperation, ResetBeforeOperation};
use espflash::flasher::Flasher;
use espflash::image_format::Segment;
use espflash::target::ProgressCallbacks;
use serialport::SerialPortType;

/// Information about an ESP32 board discovered on a serial port
#[derive(Debug, Clone)]
pub struct EspBoardInfo {
    pub port: String,
    pub chip_type: String,
    pub crystal_frequency: String,
    pub flash_size: String,
    pub features: String,
    pub mac_address: String,
    pub device_description: String,
    pub chip_revision: Option<String>,
}

/// Find all available ESP32-compatible serial ports
pub fn find_esp_ports() -> Result<Vec<String>> {
    log::debug!("Scanning for ESP32-compatible serial ports");

    let ports = serialport::available_ports()?;

    // Filter for relevant USB ports on macOS and Linux
    let esp_ports: Vec<String> = ports
        .into_iter()
        .filter_map(|port_info| {
            let port_name = &port_info.port_name;
            // On macOS, focus on USB modem and USB serial ports
            if port_name.contains("/dev/cu.usbmodem")
                || port_name.contains("/dev/cu.usbserial")
                || port_name.contains("/dev/tty.usbmodem")
                || port_name.contains("/dev/tty.usbserial")
                // On Linux, ESP32 devices typically appear as ttyUSB* or ttyACM*
                || port_name.contains("/dev/ttyUSB")
                || port_name.contains("/dev/ttyACM")
            {
                Some(port_name.clone())
            } else {
                None
            }
        })
        .collect();

    log::debug!("Found {} ESP32-compatible serial ports", esp_ports.len());
    for port in &esp_ports {
        log::trace!("Available ESP32 port: {}", port);
    }

    Ok(esp_ports)
}

/// Select the first available ESP32 port, or use environment variable if set
pub fn select_esp_port() -> Result<String> {
    // Check if user specified a port via environment variable
    if let Ok(port) = std::env::var("ESPFLASH_PORT") {
        log::info!(
            "Using port from ESPFLASH_PORT environment variable: {}",
            port
        );
        return Ok(port);
    }

    // Find available ports
    let ports = find_esp_ports()?;

    if ports.is_empty() {
        return Err(anyhow::anyhow!(
            "No ESP32-compatible serial ports found. Please connect your development board via USB."
        ));
    }

    if ports.len() == 1 {
        let port = ports[0].clone();
        log::info!("Auto-selected single available port: {}", port);
        return Ok(port);
    }

    // Multiple ports available - for now, select the first one
    // In the future, this could be enhanced with interactive selection
    let port = ports[0].clone();
    log::info!(
        "Multiple ports available, auto-selected first: {} (set ESPFLASH_PORT to override)",
        port
    );
    log::debug!("Available ports: {}", ports.join(", "));

    Ok(port)
}

/// Flash a binary file to an ESP32 using espflash library directly (native API only)
pub async fn flash_binary_to_esp(binary_path: &std::path::Path, port: Option<&str>) -> Result<()> {
    let target_port = match port {
        Some(p) => p.to_string(),
        None => select_esp_port()?,
    };

    log::info!(
        "Starting native espflash operation on port: {}",
        target_port
    );
    log::debug!("Binary file: {}", binary_path.display());

    // Check if this is an ELF file
    let looks_like_elf = binary_path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.is_empty())
        .unwrap_or(false)
        || binary_path
            .to_str()
            .map(|path| !path.contains(".bin") && !path.contains(".hex"))
            .unwrap_or(false);

    if looks_like_elf {
        // Process ELF file and generate ESP-IDF compatible flash layout
        return flash_elf_to_esp(binary_path, Some(&target_port)).await;
    }

    // Read the binary file
    let binary_data = std::fs::read(binary_path)
        .with_context(|| format!("Failed to read binary file: {}", binary_path.display()))?;

    log::debug!(
        "Binary size: {} bytes ({:.1} KB)",
        binary_data.len(),
        binary_data.len() as f64 / 1024.0
    );
    flash_binary_data(&target_port, &binary_data, 0x10000).await
}

/// Flash binary data using native espflash crate API only
pub async fn flash_binary_data(port: &str, binary_data: &[u8], offset: u32) -> Result<()> {
    log::info!(
        "Native espflash: port={}, offset=0x{:x}, size={} bytes ({:.1} KB)",
        port,
        offset,
        binary_data.len(),
        binary_data.len() as f64 / 1024.0
    );

    if binary_data.is_empty() {
        return Err(anyhow::anyhow!("Binary data is empty - cannot flash"));
    }

    if binary_data.len() > 16 * 1024 * 1024 {
        return Err(anyhow::anyhow!(
            "Binary size ({} MB) exceeds maximum flash size (16 MB)",
            binary_data.len() / 1024 / 1024
        ));
    }

    let mut segments = Vec::new();
    segments.push((
        offset,
        binary_data.to_vec(),
        format!("segment_0x{:x}", offset),
    ));
    write_segments_native(port, segments).await
}

/// Flash multiple binaries to ESP32 using native espflash crate API only
pub async fn flash_multi_binary(
    port: &str,
    flash_data: std::collections::HashMap<u32, Vec<u8>>,
) -> Result<()> {
    flash_multi_binary_with_progress(port, flash_data, None, None).await
}

/// Flash multiple binaries with progress reporting
pub async fn flash_multi_binary_with_progress(
    port: &str,
    flash_data: std::collections::HashMap<u32, Vec<u8>>,
    board_id: Option<String>,
    progress_sender: Option<tokio::sync::mpsc::UnboundedSender<ProgressUpdate>>,
) -> Result<()> {
    log::info!(
        "Native multi-binary flash: {} binaries on port {}",
        flash_data.len(),
        port
    );

    // Sort by offset to ensure correct order
    let mut sorted_entries: Vec<_> = flash_data.into_iter().collect();
    sorted_entries.sort_by_key(|(offset, _)| *offset);

    let mut segments: Vec<(u32, Vec<u8>, String)> = Vec::new();
    for (i, (offset, data)) in sorted_entries.into_iter().enumerate() {
        log::debug!("Binary segment at 0x{:x}: {} bytes", offset, data.len());
        segments.push((offset, data, format!("segment_{}", i + 1)));
    }

    write_segments_native_with_progress(port, segments, board_id, progress_sender).await
}

/// Flash ELF file to ESP32 using native espflash API
///
/// This function handles ELF files by flashing them as application binaries at the standard
/// ESP32 application offset (0x10000). This works well for Rust embedded projects using esp-hal
/// and other no_std applications that don't require complex partition table setups.
///
/// For more complex ESP-IDF projects requiring custom partition tables and bootloaders,
/// the full ELF processing could be enhanced using our local espflash_local components.
pub async fn flash_elf_to_esp(elf_path: &Path, port: Option<&str>) -> Result<()> {
    let target_port = match port {
        Some(p) => p.to_string(),
        None => select_esp_port()?,
    };

    log::info!("Flashing ELF application to ESP32: {}", elf_path.display());
    log::debug!("Target port: {}", target_port);

    // Read ELF file
    let elf_data = std::fs::read(elf_path)
        .with_context(|| format!("Failed to read ELF file: {}", elf_path.display()))?;

    if elf_data.is_empty() {
        return Err(anyhow::anyhow!("ELF file is empty: {}", elf_path.display()));
    }

    log::debug!(
        "ELF file size: {} bytes ({:.1} KB)",
        elf_data.len(),
        elf_data.len() as f64 / 1024.0
    );
    log::debug!("Flashing as application binary at offset 0x10000");

    // Flash as application binary at standard ESP32 app offset
    flash_binary_data(&target_port, &elf_data, 0x10000)
        .await
        .with_context(|| "Failed to flash ELF application to ESP32")
}

// Note: ELF processing functionality has been simplified for POC.
// Full ELF processing with proper partition table and bootloader handling
// can be implemented later using our local espflash_local module.

/// Identify ESP32 board information on a specific port
pub async fn identify_esp_board(port: &str) -> Result<Option<EspBoardInfo>> {
    identify_esp_board_with_logging(port, None).await
}

/// Identify ESP32 board information on a specific port with optional logging channel
pub async fn identify_esp_board_with_logging(
    port: &str,
    logger: Option<tokio::sync::mpsc::UnboundedSender<crate::models::AppEvent>>,
) -> Result<Option<EspBoardInfo>> {
    let log_msg = format!("🔍 Identifying ESP32 board on port: {}", port);
    if let Some(ref tx) = logger {
        let _ = tx.send(crate::models::AppEvent::BuildOutput(
            "board-scan".to_string(),
            log_msg,
        ));
    } else {
        log::debug!("{}", log_msg);
    }

    // Get port info for creating connection
    let ports = match serialport::available_ports() {
        Ok(ports) => ports,
        Err(e) => {
            let error_msg = format!("⚠️ Failed to enumerate ports: {}", e);
            if let Some(ref tx) = logger {
                let _ = tx.send(crate::models::AppEvent::BuildOutput(
                    "board-scan".to_string(),
                    error_msg,
                ));
            } else {
                log::warn!("{}", error_msg);
            }
            return Ok(None);
        }
    };

    let port_info = match ports.iter().find(|p| p.port_name == port) {
        Some(info) => info.clone(),
        None => {
            let error_msg = format!("⚠️ Port {} not found in available ports", port);
            if let Some(ref tx) = logger {
                let _ = tx.send(crate::models::AppEvent::BuildOutput(
                    "board-scan".to_string(),
                    error_msg,
                ));
            } else {
                log::warn!("{}", error_msg);
            }
            return Ok(None);
        }
    };

    let usb_info = match &port_info.port_type {
        SerialPortType::UsbPort(info) => info.clone(),
        _ => {
            // For non-USB ports, create a dummy UsbPortInfo
            serialport::UsbPortInfo {
                vid: 0,
                pid: 0,
                serial_number: None,
                manufacturer: None,
                product: None,
            }
        }
    };

    // Create serial port with timeout
    let serial_port = match serialport::new(port, 115200)
        .timeout(Duration::from_millis(1000))
        .open_native()
    {
        Ok(port) => port,
        Err(e) => {
            let error_msg = format!("⚠️ Failed to open serial port {}: {}", port, e);
            if let Some(ref tx) = logger {
                let _ = tx.send(crate::models::AppEvent::BuildOutput(
                    "board-scan".to_string(),
                    error_msg,
                ));
            } else {
                log::error!("{}", error_msg);
            }
            return Ok(None);
        }
    };

    // Create connection
    let connection = Connection::new(
        *Box::new(serial_port),
        usb_info,
        ResetAfterOperation::HardReset,
        ResetBeforeOperation::DefaultReset,
        115200,
    );

    // Create flasher and connect in blocking task
    let flasher_result = tokio::task::spawn_blocking(move || {
        Flasher::connect(connection, true, true, true, None, None)
    })
    .await;

    let mut flasher = match flasher_result {
        Ok(Ok(flasher)) => flasher,
        Ok(Err(e)) => {
            let error_msg = format!("⚠️ Failed to connect to flasher on {}: {}", port, e);
            if let Some(ref tx) = logger {
                let _ = tx.send(crate::models::AppEvent::BuildOutput(
                    "board-scan".to_string(),
                    error_msg,
                ));
            } else {
                log::error!("{}", error_msg);
            }
            return Ok(None);
        }
        Err(e) => {
            let error_msg = format!("⚠️ Task error connecting to flasher on {}: {}", port, e);
            if let Some(ref tx) = logger {
                let _ = tx.send(crate::models::AppEvent::BuildOutput(
                    "board-scan".to_string(),
                    error_msg,
                ));
            } else {
                log::error!("{}", error_msg);
            }
            return Ok(None);
        }
    };

    // Get device info which includes MAC address and other details
    let device_info_result = tokio::task::spawn_blocking(move || flasher.device_info()).await;

    let device_info = match device_info_result {
        Ok(Ok(info)) => info,
        Ok(Err(e)) => {
            let error_msg = format!("⚠️ Failed to get device info on {}: {}", port, e);
            if let Some(ref tx) = logger {
                let _ = tx.send(crate::models::AppEvent::BuildOutput(
                    "board-scan".to_string(),
                    error_msg,
                ));
            } else {
                log::error!("{}", error_msg);
            }
            return Ok(None);
        }
        Err(e) => {
            let error_msg = format!("⚠️ Task error getting device info on {}: {}", port, e);
            if let Some(ref tx) = logger {
                let _ = tx.send(crate::models::AppEvent::BuildOutput(
                    "board-scan".to_string(),
                    error_msg,
                ));
            } else {
                log::error!("{}", error_msg);
            }
            return Ok(None);
        }
    };

    // Map espflash chip type to our board info format
    let chip_type = device_info.chip.to_string();
    let features = device_info.features.join(", ");
    let flash_size = device_info.flash_size.to_string();
    let crystal_frequency = device_info.crystal_frequency.to_string();
    let mac_address = device_info
        .mac_address
        .map(|mac| mac.to_string().to_uppercase())
        .unwrap_or_else(|| "Unknown".to_string());

    // Extract revision info
    let chip_revision = device_info
        .revision
        .map(|(major, minor)| format!("{}.{}", major, minor));

    let success_msg = format!(
        "✅ Successfully identified {} board: {} (rev: {})",
        chip_type,
        port,
        chip_revision.as_deref().unwrap_or("unknown")
    );
    if let Some(ref tx) = logger {
        let _ = tx.send(crate::models::AppEvent::BuildOutput(
            "board-scan".to_string(),
            success_msg,
        ));
    } else {
        log::info!("{}", success_msg);
    }

    Ok(Some(EspBoardInfo {
        port: port.to_string(),
        chip_type,
        crystal_frequency,
        flash_size,
        features,
        mac_address,
        device_description: "ESP Development Board (espflash native detected)".to_string(),
        chip_revision,
    }))
}

/// Internal helper: write one or more segments using native espflash API
async fn write_segments_native(port: &str, segments_in: Vec<(u32, Vec<u8>, String)>) -> Result<()> {
    write_segments_native_with_progress(port, segments_in, None, None).await
}

/// Write segments with optional progress reporting
async fn write_segments_native_with_progress(
    port: &str,
    segments_in: Vec<(u32, Vec<u8>, String)>,
    board_id: Option<String>,
    progress_sender: Option<tokio::sync::mpsc::UnboundedSender<ProgressUpdate>>,
) -> Result<()> {
    log::info!(
        "Starting native flash on {} with {} segment(s)",
        port,
        segments_in.len()
    );

    // Get port info for creating connection
    let ports = serialport::available_ports()
        .map_err(|e| anyhow::anyhow!("Failed to enumerate serial ports: {}", e))?;

    let port_info = ports
        .iter()
        .find(|p| p.port_name == port)
        .ok_or_else(|| anyhow::anyhow!("Port {} not found in available ports", port))?
        .clone();

    let usb_info = match &port_info.port_type {
        SerialPortType::UsbPort(info) => info.clone(),
        _ => serialport::UsbPortInfo {
            vid: 0,
            pid: 0,
            serial_number: None,
            manufacturer: None,
            product: None,
        },
    };

    // Open serial
    let serial_port = serialport::new(port, 460800)
        .timeout(Duration::from_millis(3000))
        .open_native()
        .map_err(|e| anyhow::anyhow!("Failed to open serial port {}: {}", port, e))?;

    let connection = Connection::new(
        *Box::new(serial_port),
        usb_info,
        ResetAfterOperation::HardReset,
        ResetBeforeOperation::DefaultReset,
        460800,
    );

    // Prepare progress tracker
    let progress_info = segments_in
        .iter()
        .map(|(addr, data, name)| (*addr, data.len(), name.clone()))
        .collect();

    // Connect and flash in a blocking task
    let result = tokio::task::spawn_blocking(move || -> Result<()> {
        let mut flasher = Flasher::connect(connection, true, true, true, None, None)
            .map_err(|e| anyhow::anyhow!("Failed to connect to ESP32: {}", e))?;

        // Create appropriate progress reporter based on whether we have progress reporting enabled
        let mut progress =
            if let (Some(board_id), Some(progress_sender)) = (board_id, progress_sender) {
                NativeProgress::with_progress_reporting(progress_info, board_id, progress_sender)
            } else {
                NativeProgress::new(progress_info)
            };

        // Build segments with memory-optimized data handling
        // For large binaries (>1MB), we avoid cloning by moving data ownership
        let segments: Vec<Segment> = segments_in
            .into_iter()
            .map(|(addr, data, _)| Segment {
                addr,
                data: Cow::Owned(data), // Move ownership instead of cloning
            })
            .collect();

        flasher
            .write_bins_to_flash(&segments, &mut progress)
            .map_err(|e| anyhow::anyhow!("Failed to write binaries to flash: {}", e))?;
        Ok(())
    })
    .await
    .map_err(|e| anyhow::anyhow!("Join error: {}", e))?;

    result
}

/// Progress reporter for native flashing
struct NativeProgress {
    total_size: usize,
    total_written: usize,
    segments: Vec<(u32, usize, String)>,
    current_addr: u32,
    last_log_time: std::time::Instant,
    last_logged_progress: u32,
    /// Optional channel to send progress updates for server state management
    progress_sender: Option<tokio::sync::mpsc::UnboundedSender<ProgressUpdate>>,
    /// Board ID for progress updates
    board_id: Option<String>,
    /// Flash start time for calculations
    flash_start_time: std::time::Instant,
}

/// Progress update message for communication with flash service
#[derive(Debug, Clone)]
pub struct ProgressUpdate {
    pub board_id: String,
    pub current_segment: u32,
    pub total_segments: u32,
    pub current_segment_name: String,
    pub overall_progress: f32,
    pub segment_progress: f32,
    pub bytes_written: u64,
    pub total_bytes: u64,
    pub current_operation: String,
    pub started_at: chrono::DateTime<chrono::Local>,
}

impl NativeProgress {
    fn new(segments_info: Vec<(u32, usize, String)>) -> Self {
        let total_size = segments_info.iter().map(|(_, size, _)| *size).sum();
        Self {
            total_size,
            total_written: 0,
            segments: segments_info,
            current_addr: 0,
            last_log_time: std::time::Instant::now(),
            last_logged_progress: 0,
            progress_sender: None,
            board_id: None,
            flash_start_time: std::time::Instant::now(),
        }
    }

    /// Create NativeProgress with progress reporting capability
    fn with_progress_reporting(
        segments_info: Vec<(u32, usize, String)>,
        board_id: String,
        progress_sender: tokio::sync::mpsc::UnboundedSender<ProgressUpdate>,
    ) -> Self {
        let total_size = segments_info.iter().map(|(_, size, _)| *size).sum();
        Self {
            total_size,
            total_written: 0,
            segments: segments_info,
            current_addr: 0,
            last_log_time: std::time::Instant::now(),
            last_logged_progress: 0,
            progress_sender: Some(progress_sender),
            board_id: Some(board_id),
            flash_start_time: std::time::Instant::now(),
        }
    }

    fn find_segment(&self, addr: u32) -> Option<(usize, String, usize)> {
        self.segments
            .iter()
            .enumerate()
            .find(|(_, (a, _, _))| *a == addr)
            .map(|(i, (_, size, name))| (i, name.clone(), *size))
    }
}

impl ProgressCallbacks for NativeProgress {
    fn init(&mut self, addr: u32, total: usize) {
        self.current_addr = addr;
        if let Some((idx, name, size)) = self.find_segment(addr) {
            log::info!(
                "Starting segment {}/{}: {} at 0x{:x} ({} bytes)",
                idx + 1,
                self.segments.len(),
                name,
                addr,
                size
            );

            // Send initial progress update for this segment
            if let (Some(sender), Some(board_id)) = (&self.progress_sender, &self.board_id) {
                let progress_update = ProgressUpdate {
                    board_id: board_id.clone(),
                    current_segment: (idx + 1) as u32,
                    total_segments: self.segments.len() as u32,
                    current_segment_name: name,
                    overall_progress: (self.total_written as f32 / self.total_size as f32 * 100.0),
                    segment_progress: 0.0,
                    bytes_written: self.total_written as u64,
                    total_bytes: self.total_size as u64,
                    current_operation: "Starting".to_string(),
                    started_at: chrono::DateTime::from_timestamp(
                        self.flash_start_time.elapsed().as_secs() as i64,
                        0,
                    )
                    .unwrap_or_else(chrono::Utc::now)
                    .with_timezone(&chrono::Local),
                };
                let _ = sender.send(progress_update);
            }
        } else {
            log::info!("Starting segment at 0x{:x} ({} bytes)", addr, total);
        }
    }

    fn update(&mut self, current: usize) {
        let overall = self.total_written + current;
        let overall_pct = if self.total_size > 0 {
            (overall as f32 / self.total_size as f32 * 100.0) as u32
        } else {
            0
        };

        // Send progress update through channel if available
        if let (Some(sender), Some(board_id)) = (&self.progress_sender, &self.board_id) {
            if let Some((idx, name, size)) = self.find_segment(self.current_addr) {
                let segment_progress = if size > 0 {
                    current as f32 / size as f32 * 100.0
                } else {
                    100.0
                };

                let progress_update = ProgressUpdate {
                    board_id: board_id.clone(),
                    current_segment: (idx + 1) as u32,
                    total_segments: self.segments.len() as u32,
                    current_segment_name: name,
                    overall_progress: overall_pct as f32,
                    segment_progress,
                    bytes_written: overall as u64,
                    total_bytes: self.total_size as u64,
                    current_operation: "Writing".to_string(),
                    started_at: chrono::DateTime::from_timestamp(
                        self.flash_start_time.elapsed().as_secs() as i64,
                        0,
                    )
                    .unwrap_or_else(chrono::Utc::now)
                    .with_timezone(&chrono::Local),
                };

                let _ = sender.send(progress_update); // Ignore send errors (receiver might be dropped)
            }
        }

        // Optimized rate limiting for large binaries:
        // - For files < 1MB: log every 10% change
        // - For files >= 1MB: log every 5% change but at most once per 500ms
        // - For files >= 10MB: log every 2% change but at most once per 250ms
        let now = std::time::Instant::now();
        let time_threshold = if self.total_size >= 10 * 1024 * 1024 {
            Duration::from_millis(250) // 10MB+: faster updates for large files
        } else if self.total_size >= 1024 * 1024 {
            Duration::from_millis(500) // 1-10MB: moderate updates
        } else {
            Duration::from_secs(1) // <1MB: standard updates
        };

        let progress_threshold = if self.total_size >= 10 * 1024 * 1024 {
            2 // 10MB+: 2% granularity for better feedback on large files
        } else if self.total_size >= 1024 * 1024 {
            5 // 1-10MB: 5% granularity
        } else {
            10 // <1MB: 10% granularity to reduce noise
        };

        let should_log = now.duration_since(self.last_log_time) >= time_threshold
            || overall_pct.saturating_sub(self.last_logged_progress) >= progress_threshold;

        if should_log {
            self.last_log_time = now;
            self.last_logged_progress = overall_pct;

            if let Some((_, name, size)) = self.find_segment(self.current_addr) {
                let seg_pct = if size > 0 {
                    (current as f32 / size as f32 * 100.0) as u32
                } else {
                    100
                };
                // Optimized formatting for large files - show MB for files > 1MB
                if self.total_size >= 1024 * 1024 {
                    log::debug!(
                        "{}: {:.1}MB / {:.1}MB ({}%) | Overall: {:.1}MB / {:.1}MB ({}%)",
                        name,
                        current as f64 / 1024.0 / 1024.0,
                        size as f64 / 1024.0 / 1024.0,
                        seg_pct,
                        overall as f64 / 1024.0 / 1024.0,
                        self.total_size as f64 / 1024.0 / 1024.0,
                        overall_pct
                    );
                } else {
                    log::debug!(
                        "{}: {:.1}KB / {:.1}KB ({}%) | Overall: {:.1}KB / {:.1}KB ({}%)",
                        name,
                        current as f64 / 1024.0,
                        size as f64 / 1024.0,
                        seg_pct,
                        overall as f64 / 1024.0,
                        self.total_size as f64 / 1024.0,
                        overall_pct
                    );
                }
            } else {
                // Optimized formatting for overall progress
                if self.total_size >= 1024 * 1024 {
                    log::debug!(
                        "Progress: {:.1}MB | Overall: {:.1}MB / {:.1}MB ({}%)",
                        current as f64 / 1024.0 / 1024.0,
                        overall as f64 / 1024.0 / 1024.0,
                        self.total_size as f64 / 1024.0 / 1024.0,
                        overall_pct
                    );
                } else {
                    log::debug!(
                        "Progress: {:.1}KB | Overall: {:.1}KB / {:.1}KB ({}%)",
                        current as f64 / 1024.0,
                        overall as f64 / 1024.0,
                        self.total_size as f64 / 1024.0,
                        overall_pct
                    );
                }
            }
        }
    }

    fn verifying(&mut self) {
        if let Some((idx, name, _)) = self.find_segment(self.current_addr) {
            log::debug!("Verifying {}: {}", idx + 1, name);
        } else {
            log::debug!("Verifying flash contents at 0x{:x}...", self.current_addr);
        }
    }

    fn finish(&mut self, _skipped: bool) {
        if let Some((idx, name, size)) = self.find_segment(self.current_addr) {
            self.total_written += size;
            let overall_pct = if self.total_size > 0 {
                (self.total_written as f32 / self.total_size as f32 * 100.0) as u32
            } else {
                0
            };
            log::info!(
                "Segment {}/{} COMPLETED: {} ({} bytes) | Overall progress: {}%",
                idx + 1,
                self.segments.len(),
                name,
                size,
                overall_pct
            );
        } else {
            log::info!("Segment flash completed at 0x{:x}", self.current_addr);
        }
    }
}
