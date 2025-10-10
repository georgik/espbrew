use anyhow::{Context, Result};
use std::borrow::Cow;
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
    println!("🔍 Scanning for ESP32-compatible serial ports...");

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

    println!("📡 Found {} ESP32-compatible serial ports", esp_ports.len());
    for port in &esp_ports {
        println!("  🔌 {}", port);
    }

    Ok(esp_ports)
}

/// Select the first available ESP32 port, or use environment variable if set
pub fn select_esp_port() -> Result<String> {
    // Check if user specified a port via environment variable
    if let Ok(port) = std::env::var("ESPFLASH_PORT") {
        println!(
            "🎯 Using port from ESPFLASH_PORT environment variable: {}",
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
        println!("🎯 Auto-selected single available port: {}", port);
        return Ok(port);
    }

    // Multiple ports available - for now, select the first one
    // In the future, this could be enhanced with interactive selection
    let port = ports[0].clone();
    println!(
        "🎯 Multiple ports available, auto-selected first: {} (set ESPFLASH_PORT to override)",
        port
    );
    println!("   Available ports: {}", ports.join(", "));

    Ok(port)
}

/// Flash a binary file to an ESP32 using espflash library directly (native API only)
pub async fn flash_binary_to_esp(binary_path: &std::path::Path, port: Option<&str>) -> Result<()> {
    let target_port = match port {
        Some(p) => p.to_string(),
        None => select_esp_port()?,
    };

    println!(
        "🔥 Starting native espflash operation on port: {}",
        target_port
    );
    println!("📁 Binary file: {}", binary_path.display());

    // For now we support raw .bin images via native API. ELF handling without external tools
    // requires building an image; until implemented, return a clear error for ELF inputs.
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
        return Err(anyhow::anyhow!(
            "ELF flashing via native API is not implemented yet. Please provide partition binaries (bootloader, partition table, app) as raw .bin files with offsets."
        ));
    }

    // Read the binary file
    let binary_data = std::fs::read(binary_path)
        .with_context(|| format!("Failed to read binary file: {}", binary_path.display()))?;

    println!("💾 Binary size: {} bytes", binary_data.len());
    flash_binary_data(&target_port, &binary_data, 0x10000).await
}

/// Flash binary data using native espflash crate API only
pub async fn flash_binary_data(port: &str, binary_data: &[u8], offset: u32) -> Result<()> {
    println!(
        "🔥 Native espflash: port={}, offset=0x{:x}, size={} bytes",
        port,
        offset,
        binary_data.len()
    );

    if binary_data.is_empty() {
        return Err(anyhow::anyhow!("Binary data is empty"));
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
    println!(
        "🚀 Native multi-binary flash: {} binaries on port {}",
        flash_data.len(),
        port
    );

    // Sort by offset to ensure correct order
    let mut sorted_entries: Vec<_> = flash_data.into_iter().collect();
    sorted_entries.sort_by_key(|(offset, _)| *offset);

    let mut segments: Vec<(u32, Vec<u8>, String)> = Vec::new();
    for (i, (offset, data)) in sorted_entries.into_iter().enumerate() {
        println!("  📄 Binary at 0x{:x}: {} bytes", offset, data.len());
        segments.push((offset, data, format!("segment_{}", i + 1)));
    }

    write_segments_native(port, segments).await
}

/// Identify ESP32 board information on a specific port
pub async fn identify_esp_board(port: &str) -> Result<Option<EspBoardInfo>> {
    println!("🔍 Identifying ESP32 board on port: {}", port);

    // Get port info for creating connection
    let ports = match serialport::available_ports() {
        Ok(ports) => ports,
        Err(e) => {
            println!("⚠️ Failed to enumerate ports: {}", e);
            return Ok(None);
        }
    };

    let port_info = match ports.iter().find(|p| p.port_name == port) {
        Some(info) => info.clone(),
        None => {
            println!("⚠️ Port {} not found in available ports", port);
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
            println!("⚠️ Failed to open serial port {}: {}", port, e);
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
            println!("⚠️ Failed to connect to flasher on {}: {}", port, e);
            return Ok(None);
        }
        Err(e) => {
            println!("⚠️ Task error connecting to flasher on {}: {}", port, e);
            return Ok(None);
        }
    };

    // Get device info which includes MAC address and other details
    let device_info_result = tokio::task::spawn_blocking(move || flasher.device_info()).await;

    let device_info = match device_info_result {
        Ok(Ok(info)) => info,
        Ok(Err(e)) => {
            println!("⚠️ Failed to get device info on {}: {}", port, e);
            return Ok(None);
        }
        Err(e) => {
            println!("⚠️ Task error getting device info on {}: {}", port, e);
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

    println!(
        "✅ Successfully identified {} board: {} (rev: {})",
        chip_type,
        port,
        chip_revision.as_deref().unwrap_or("unknown")
    );

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
    println!(
        "🔥 Starting native flash on {} with {} segment(s)",
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

        let mut progress = NativeProgress::new(progress_info);

        // Build segments with owned data inside the blocking task
        let segments: Vec<Segment> = segments_in
            .iter()
            .map(|(addr, data, _)| Segment {
                addr: *addr,
                data: Cow::Owned(data.clone()),
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
            println!(
                "  🔥 Starting segment {}/{}: {} at 0x{:x} ({} bytes)",
                idx + 1,
                self.segments.len(),
                name,
                addr,
                size
            );
        } else {
            println!("  🔥 Starting segment at 0x{:x} ({} bytes)", addr, total);
        }
    }

    fn update(&mut self, current: usize) {
        let overall = self.total_written + current;
        let overall_pct = if self.total_size > 0 {
            (overall as f32 / self.total_size as f32 * 100.0) as u32
        } else {
            0
        };

        // Rate limit: only log at most once per second OR when progress changes by 5%
        let now = std::time::Instant::now();
        let should_log = now.duration_since(self.last_log_time).as_secs() >= 1
            || overall_pct.saturating_sub(self.last_logged_progress) >= 5;

        if should_log {
            self.last_log_time = now;
            self.last_logged_progress = overall_pct;

            if let Some((_, name, size)) = self.find_segment(self.current_addr) {
                let seg_pct = if size > 0 {
                    (current as f32 / size as f32 * 100.0) as u32
                } else {
                    100
                };
                println!(
                    "    📊 {}: {} / {} bytes ({}%) | Overall: {} / {} bytes ({}%)",
                    name, current, size, seg_pct, overall, self.total_size, overall_pct
                );
            } else {
                println!(
                    "    📊 Progress: {} bytes | Overall: {} / {} bytes ({}%)",
                    current, overall, self.total_size, overall_pct
                );
            }
        }
    }

    fn verifying(&mut self) {
        if let Some((idx, name, _)) = self.find_segment(self.current_addr) {
            println!("    🔍 Verifying {}: {}", idx + 1, name);
        } else {
            println!(
                "    🔍 Verifying flash contents at 0x{:x}...",
                self.current_addr
            );
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
            println!(
                "    ✅ Segment {}/{} COMPLETED: {} ({} bytes) | Overall progress: {}%",
                idx + 1,
                self.segments.len(),
                name,
                size,
                overall_pct
            );
        } else {
            println!(
                "    ✅ Segment flash completed at 0x{:x}",
                self.current_addr
            );
        }
    }
}
