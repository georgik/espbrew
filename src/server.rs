#!/usr/bin/env rust
//! ESPBrew Server - Remote ESP32 Flashing Server
//!
//! A network-based server that manages connected ESP32 boards and provides
//! remote flashing capabilities. This enables ESPBrew to work with remote
//! test farms, CI/CD environments, and distributed development setups.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use chrono::{DateTime, Local};
use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use warp::Filter;

/// Network protocol data structures
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectedBoard {
    /// Board identifier (assigned by server)
    pub id: String,
    /// Physical serial port
    pub port: String,
    /// Detected chip type (esp32s3, esp32p4, etc.)
    pub chip_type: String,
    /// Crystal frequency
    pub crystal_frequency: String,
    /// Flash size
    pub flash_size: String,
    /// Chip features (WiFi, BLE, etc.)
    pub features: String,
    /// MAC address (partially redacted for privacy)
    pub mac_address: String,
    /// USB device description
    pub device_description: String,
    /// Current status
    pub status: BoardStatus,
    /// Last activity timestamp
    pub last_updated: DateTime<Local>,
    /// User-assigned logical name (optional)
    pub logical_name: Option<String>,
    /// Unique identifier combining multiple chip characteristics
    pub unique_id: String,
    /// Chip revision (e.g., "v0.2")
    pub chip_revision: Option<String>,
    /// Chip ID from security info
    pub chip_id: Option<u32>,
    /// Flash manufacturer ID
    pub flash_manufacturer: Option<String>,
    /// Flash device ID
    pub flash_device_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BoardStatus {
    Available,
    Flashing,
    Monitoring,
    Error(String),
}

impl std::fmt::Display for BoardStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BoardStatus::Available => write!(f, "Available"),
            BoardStatus::Flashing => write!(f, "Flashing"),
            BoardStatus::Monitoring => write!(f, "Monitoring"),
            BoardStatus::Error(msg) => write!(f, "Error: {}", msg),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlashRequest {
    /// Board ID to flash
    pub board_id: String,
    /// Binary data to flash (base64 encoded)
    pub binary_data: Vec<u8>,
    /// Flash offset (usually 0x0 for merged binaries)
    pub offset: u32,
    /// Optional chip type override
    pub chip_type: Option<String>,
    /// Flash after completion
    pub verify: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlashResponse {
    pub success: bool,
    pub message: String,
    pub duration_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoardListResponse {
    pub boards: Vec<ConnectedBoard>,
    pub server_info: ServerInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerInfo {
    pub version: String,
    pub hostname: String,
    pub last_scan: DateTime<Local>,
    pub total_boards: usize,
}

/// Server state management
#[derive(Debug)]
pub struct ServerState {
    boards: HashMap<String, ConnectedBoard>,
    config: ServerConfig,
    last_scan: DateTime<Local>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    /// Server listening address
    pub bind_address: String,
    /// Server listening port
    pub port: u16,
    /// Board discovery interval in seconds
    pub scan_interval: u64,
    /// Board mappings (port -> logical_name)
    pub board_mappings: HashMap<String, String>,
    /// Maximum binary size for uploads (in MB)
    pub max_binary_size_mb: usize,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            bind_address: "0.0.0.0".to_string(),
            port: 8080,
            scan_interval: 30,
            board_mappings: HashMap::new(),
            max_binary_size_mb: 50,
        }
    }
}

/// Structure to hold enhanced unique identifiers
#[derive(Debug)]
struct EnhancedUniqueInfo {
    chip_id: Option<u32>,
    flash_manufacturer: Option<String>,
    flash_device_id: Option<String>,
}

// Template structures for web UI
/*
#[derive(Template)]
#[template(path = "dashboard_test.html")]
struct DashboardTemplate {
    page: String,
    boards: Vec<ConnectedBoard>,
    server_info: ServerInfo,
}

#[derive(Template)]
#[template(path = "flash_test.html")]
struct FlashTemplate {
    page: String,
    boards: Vec<ConnectedBoard>,
    selected_board: Option<String>,
}
*/

impl ServerState {
    pub fn new(config: ServerConfig) -> Self {
        Self {
            boards: HashMap::new(),
            config,
            last_scan: Local::now(),
        }
    }

    /// Discover and update connected boards
    pub async fn scan_boards(&mut self) -> Result<()> {
        self.scan_boards_with_cancellation(None).await
    }

    /// Discover and update connected boards with optional cancellation support
    pub async fn scan_boards_with_cancellation(
        &mut self,
        cancel_signal: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
    ) -> Result<()> {
        // Check if we should cancel early
        if let Some(ref cancel) = cancel_signal {
            if cancel.load(std::sync::atomic::Ordering::Relaxed) {
                println!("🛑 Board scan cancelled before starting");
                return Ok(());
            }
        }

        println!("🔍 Scanning for USB serial ports...");

        // Use serialport to discover serial ports
        let ports = serialport::available_ports()?;
        let mut discovered_boards = HashMap::new();

        // Filter for relevant USB ports on macOS and Linux
        let relevant_ports: Vec<_> = ports
            .into_iter()
            .filter(|port_info| {
                let port_name = &port_info.port_name;
                // On macOS, focus on USB modem and USB serial ports
                port_name.contains("/dev/cu.usbmodem")
                    || port_name.contains("/dev/cu.usbserial")
                    || port_name.contains("/dev/tty.usbmodem")
                    || port_name.contains("/dev/tty.usbserial")
                    // On Linux, ESP32 devices typically appear as ttyUSB* or ttyACM*
                    || port_name.contains("/dev/ttyUSB")
                    || port_name.contains("/dev/ttyACM")
            })
            .collect();

        println!("📡 Found {} USB serial ports", relevant_ports.len());
        for port_info in &relevant_ports {
            println!("  🔌 {}", port_info.port_name);
        }

        if relevant_ports.is_empty() {
            println!("⚠️  No USB serial ports found. Connect your development boards via USB.");
        }

        for (index, port_info) in relevant_ports.iter().enumerate() {
            // Check for cancellation before processing each port
            if let Some(ref cancel) = cancel_signal {
                if cancel.load(std::sync::atomic::Ordering::Relaxed) {
                    println!("🛑 Board scan cancelled during port enumeration");
                    return Ok(());
                }
            }

            println!(
                "🔍 [{}/{}] Adding port: {}",
                index + 1,
                relevant_ports.len(),
                port_info.port_name
            );

            // Create a lightweight board entry using only USB information
            let board = self.create_usb_board_info(&port_info);

            println!(
                "✅ Added USB device on {}: {}",
                port_info.port_name, board.device_description
            );
            let board_id = format!("board_{}", board.port.replace("/", "_").replace(".", "_"));

            // Apply logical name mapping if configured
            let logical_name = self.config.board_mappings.get(&board.port).cloned();

            let connected_board = ConnectedBoard {
                id: board_id.clone(),
                port: board.port.clone(),
                chip_type: board.chip_type.clone(),
                crystal_frequency: board.crystal_frequency.clone(),
                flash_size: board.flash_size.clone(),
                features: board.features.clone(),
                mac_address: board.mac_address.clone(),
                device_description: board.device_description.clone(),
                status: BoardStatus::Available,
                last_updated: Local::now(),
                logical_name,
                unique_id: board.unique_id.clone(),
                chip_revision: board.chip_revision.clone(),
                chip_id: board.chip_id,
                flash_manufacturer: board.flash_manufacturer.clone(),
                flash_device_id: board.flash_device_id.clone(),
            };

            discovered_boards.insert(board_id, connected_board);
        }

        self.boards = discovered_boards;
        self.last_scan = Local::now();

        println!("✅ Scan complete. Found {} USB devices", self.boards.len());

        for board in self.boards.values() {
            let logical_name = board.logical_name.as_deref().unwrap_or("(unmapped)");
            println!(
                "  📱 {} [{}] - {} @ {} ({})",
                board.id, logical_name, board.chip_type, board.port, board.device_description
            );
        }

        Ok(())
    }

    /// Create a lightweight board info using only USB port information
    fn create_usb_board_info(&self, port_info: &serialport::SerialPortInfo) -> BoardInfo {
        use serialport::SerialPortType;

        // Determine likely chip type based on port name patterns
        let (chip_type, features) = if port_info.port_name.contains("usbmodem") {
            ("ESP32-S3/C3/C6/H2", "USB-OTG, WiFi, Bluetooth")
        } else if port_info.port_name.contains("usbserial") {
            ("ESP32/ESP8266", "WiFi, Bluetooth")
        } else if port_info.port_name.contains("/dev/ttyUSB") {
            // Linux: Most ESP32 boards with CP210x/FTDI appear as ttyUSB*
            ("ESP32/ESP8266", "WiFi, Bluetooth")
        } else if port_info.port_name.contains("/dev/ttyACM") {
            // Linux: ESP32-S3/C3/C6/H2 with native USB often appear as ttyACM*
            ("ESP32-S3/C3/C6/H2", "USB-OTG, WiFi, Bluetooth")
        } else {
            ("Unknown MCU", "Unknown")
        };

        // Get USB device description
        let device_description = match &port_info.port_type {
            SerialPortType::UsbPort(usb) => {
                format!(
                    "{} - {}",
                    usb.manufacturer
                        .as_deref()
                        .unwrap_or("Unknown Manufacturer"),
                    usb.product.as_deref().unwrap_or("USB Serial Device")
                )
            }
            SerialPortType::PciPort => "PCI Serial Port".to_string(),
            SerialPortType::BluetoothPort => "Bluetooth Serial Port".to_string(),
            SerialPortType::Unknown => "Unknown Serial Port".to_string(),
        };

        // Create a unique ID based on port name
        let unique_id = format!(
            "usb_port_{}",
            port_info.port_name.replace('/', "_").replace('.', "_")
        );

        BoardInfo {
            port: port_info.port_name.clone(),
            chip_type: chip_type.to_string(),
            crystal_frequency: "Unknown".to_string(),
            flash_size: "Unknown".to_string(),
            features: features.to_string(),
            mac_address: "Unknown".to_string(),
            device_description,
            chip_revision: None,
            chip_id: None,
            flash_manufacturer: None,
            flash_device_id: None,
            unique_id,
        }
    }

    /// Identify a board on the given port using probe-rs for accurate MCU detection
    async fn identify_board(&self, port: &str) -> Result<Option<BoardInfo>> {
        self.identify_board_with_cancellation(port, None).await
    }

    /// Identify a board on the given port with cancellation support
    async fn identify_board_with_cancellation(
        &self,
        port: &str,
        cancel_signal: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
    ) -> Result<Option<BoardInfo>> {
        use std::time::Duration;

        // Check for cancellation before starting identification
        if let Some(ref cancel) = cancel_signal {
            if cancel.load(std::sync::atomic::Ordering::Relaxed) {
                println!("🛑 Board identification cancelled for {}", port);
                return Ok(None);
            }
        }

        // Use 5-second timeout as requested (note: individual operations have their own timeouts)
        let _overall_timeout_dur = Duration::from_secs(5);
        let port_str = port.to_string();

        // First, do a quick serial port accessibility check
        if !Self::is_port_accessible(&port_str).await {
            println!("⚠️ Port {} not accessible, skipping", port_str);
            return Ok(None);
        }

        // Check if the port name contains indicators of board type
        let possible_board_type = if port_str.contains("usbmodem") {
            // Modern ESP32 dev kits often use usbmodem interface
            // This includes ESP32-S3, ESP32-C3, ESP32-C6, ESP32-P4
            Some("esp32-usb")
        } else if port_str.contains("usbserial") {
            // Traditional ESP32 boards with CP210x/FTDI often use usbserial
            Some("esp32-serial")
        } else if port_str.contains("/dev/ttyACM") {
            // Linux: ESP32-S3/C3/C6/H2 with native USB often appear as ttyACM*
            Some("esp32-usb")
        } else if port_str.contains("/dev/ttyUSB") {
            // Linux: Most ESP32 boards with CP210x/FTDI appear as ttyUSB*
            Some("esp32-serial")
        } else {
            None
        };

        if let Some(board_type) = possible_board_type {
            println!("🔍 Detected possible {} board on {}", board_type, port_str);
        }

        // Try USB-based detection first for quick identification
        println!(
            "🔍 Attempting to identify board on {} with USB detection",
            port_str
        );
        let result = tokio::time::timeout(
            Duration::from_millis(500),
            Self::identify_with_probe_rs(&port_str),
        )
        .await;

        match result {
            Ok(Ok(Some(board_info))) => {
                println!(
                    "✅ Successfully identified board with USB detection: {}",
                    board_info.chip_type
                );
                Ok(Some(board_info))
            }
            _ => {
                // Check for cancellation before trying espflash
                if let Some(ref cancel) = cancel_signal {
                    if cancel.load(std::sync::atomic::Ordering::Relaxed) {
                        println!(
                            "🛑 Board identification cancelled during USB detection for {}",
                            port
                        );
                        return Ok(None);
                    }
                }

                println!("ℹ️ USB detection inconclusive, trying espflash");

                // Try espflash with good timeout
                let espflash_result = tokio::time::timeout(
                    Duration::from_millis(3000),
                    Self::identify_with_espflash_subprocess(&port_str),
                )
                .await;

                match espflash_result {
                    Ok(Ok(Some(board_info))) => {
                        println!(
                            "✅ Successfully identified board with espflash: {}",
                            board_info.chip_type
                        );
                        Ok(Some(board_info))
                    }
                    Ok(Ok(None)) => {
                        println!("ℹ️ espflash found no ESP32, trying IDF tools as last resort");
                        // Try IDF tools as final fallback
                        let idf_result = tokio::time::timeout(
                            Duration::from_millis(1500),
                            Self::identify_with_idf_tools(&port_str),
                        )
                        .await;

                        match idf_result {
                            Ok(inner_result) => inner_result,
                            Err(_) => {
                                println!("⏰ IDF tools timeout on {}", port);
                                Ok(None)
                            }
                        }
                    }
                    Ok(Err(e)) => {
                        println!("⚠️ espflash error on {}: {}", port_str, e);
                        Ok(None)
                    }
                    Err(_) => {
                        println!("⏰ espflash timeout on {}, trying IDF tools fallback", port);
                        // Quick IDF tools fallback
                        let idf_result = tokio::time::timeout(
                            Duration::from_millis(1000),
                            Self::identify_with_idf_tools(&port_str),
                        )
                        .await;

                        match idf_result {
                            Ok(inner_result) => inner_result,
                            Err(_) => {
                                println!(
                                    "⏰ Complete timeout identifying board on {} (undetectable after 5s)",
                                    port
                                );
                                Ok(None)
                            }
                        }
                    }
                }
            }
        }
    }

    /// Quick port accessibility check without blocking terminal
    async fn is_port_accessible(port: &str) -> bool {
        use std::time::Duration;

        // Try to open port briefly with a very short timeout
        let result = tokio::time::timeout(
            Duration::from_millis(200),
            tokio::task::spawn_blocking({
                let port_str = port.to_string();
                move || {
                    match serialport::new(&port_str, 115200)
                        .timeout(Duration::from_millis(50))
                        .open()
                    {
                        Ok(port) => {
                            // Quick test: try to close it properly
                            drop(port);
                            true
                        }
                        Err(e) => {
                            // Log the specific error for debugging
                            println!("🔍 Port {} accessibility check failed: {}", port_str, e);
                            false
                        }
                    }
                }
            }),
        )
        .await;

        match result {
            Ok(Ok(accessible)) => accessible,
            Ok(Err(_)) => false, // Task panicked
            Err(_) => {
                println!("⏰ Port {} accessibility check timed out", port);
                false // Timeout
            }
        }
    }

    /// Identification using USB characteristics and enhanced espflash
    async fn identify_with_probe_rs(port: &str) -> Result<Option<BoardInfo>> {
        println!("🔍 Using USB-based detection for board on {}", port);

        // First, try to get USB device information based on the port
        let usb_info = Self::get_usb_device_info(port).await;

        // Use USB VID/PID to make educated guesses about ESP32 type
        if let Some((vid, pid, manufacturer, product)) = usb_info {
            println!(
                "🔍 USB Device: VID:0x{:04x}, PID:0x{:04x}, Mfg:{}, Product:{}",
                vid, pid, manufacturer, product
            );

            // Match known ESP32 development board USB identifiers
            let chip_guess = match (vid, pid) {
                // Espressif USB-JTAG/serial debug unit (ESP32-S3, ESP32-C3, ESP32-C6, etc.)
                (0x303A, _) => {
                    if product.to_lowercase().contains("esp32-s3") {
                        Some(("esp32s3", "WiFi, Bluetooth, USB-OTG", "40 MHz"))
                    } else if product.to_lowercase().contains("esp32-c6") {
                        Some(("esp32c6", "WiFi 6, Bluetooth 5, ZigBee 3.0", "40 MHz"))
                    } else if product.to_lowercase().contains("esp32-c3") {
                        Some(("esp32c3", "WiFi, Bluetooth 5", "40 MHz"))
                    } else if product.to_lowercase().contains("esp32-p4") {
                        Some(("esp32p4", "WiFi 6, High Performance AI", "40 MHz"))
                    } else if product.to_lowercase().contains("esp32-h2") {
                        Some(("esp32h2", "Bluetooth 5, ZigBee 3.0, Thread", "32 MHz"))
                    } else {
                        // Default for Espressif VID
                        Some(("esp32s3", "WiFi, Bluetooth, USB-OTG", "40 MHz"))
                    }
                }
                // Silicon Labs CP210x (common on ESP32 dev boards)
                (0x10C4, 0xEA60) => Some(("esp32", "WiFi, Bluetooth Classic", "40 MHz")),
                // FTDI (also used on some ESP32 boards)
                (0x0403, _) => Some(("esp32", "WiFi, Bluetooth Classic", "40 MHz")),
                // WCH CH340 (cheap USB-serial, often used on ESP32 clones)
                (0x1A86, _) => Some(("esp32", "WiFi, Bluetooth Classic", "40 MHz")),
                _ => None,
            };

            if let Some((chip_type, features, crystal_freq)) = chip_guess {
                println!("✅ USB-based identification suggests: {}", chip_type);

                // For USB-based detection, we'll need to get unique IDs later via enhanced detection
                let placeholder_unique_id = format!(
                    "USB-{:04x}:{:04x}-{}",
                    vid,
                    pid,
                    port.replace("/", "-").replace(".", "_")
                );

                return Ok(Some(BoardInfo {
                    port: port.to_string(),
                    chip_type: chip_type.to_string(),
                    crystal_frequency: crystal_freq.to_string(),
                    flash_size: "Unknown".to_string(),
                    features: features.to_string(),
                    mac_address: "**:**:**:**:**:**".to_string(),
                    device_description: format!("{} - {}", manufacturer, product),
                    chip_revision: None,
                    chip_id: None,
                    flash_manufacturer: None,
                    flash_device_id: None,
                    unique_id: placeholder_unique_id,
                }));
            }
        }

        println!("ℹ️ USB-based detection inconclusive, will fall back to espflash");
        Ok(None)
    }

    /// Get USB device information for a given serial port
    async fn get_usb_device_info(port: &str) -> Option<(u16, u16, String, String)> {
        use serialport::SerialPortType;

        // Get all available ports and find the one matching our port
        match serialport::available_ports() {
            Ok(ports) => {
                for port_info in ports {
                    if port_info.port_name == port {
                        if let SerialPortType::UsbPort(usb) = &port_info.port_type {
                            return Some((
                                usb.vid,
                                usb.pid,
                                usb.manufacturer
                                    .clone()
                                    .unwrap_or_else(|| "Unknown".to_string()),
                                usb.product.clone().unwrap_or_else(|| "Unknown".to_string()),
                            ));
                        }
                    }
                }
            }
            Err(_) => return None,
        }
        None
    }

    /// Identification using ESP-IDF tools as final fallback
    async fn identify_with_idf_tools(port: &str) -> Result<Option<BoardInfo>> {
        use std::process::Stdio;
        use tokio::process::Command;

        println!("🔍 Using ESP-IDF tools to identify board on {}", port);

        // Try using idf.py to detect the chip
        let result = async {
            let cmd = Command::new("python")
                .args([
                    "-c",
                    &format!(
                        "import esptool; esptool.main(['--port', '{}', 'chip_id'])",
                        port
                    ),
                ])
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .kill_on_drop(true)
                .spawn()?;

            let output = cmd.wait_with_output().await?;

            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);

                println!("ℹ️ esptool stdout: {}", stdout.trim());
                if !stderr.trim().is_empty() {
                    println!("ℹ️ esptool stderr: {}", stderr.trim());
                }

                // Parse esptool output for chip identification
                let combined_output = format!("{} {}", stdout, stderr);

                let (chip_type, features, crystal_freq) = if combined_output.contains("ESP32-S3") {
                    ("esp32s3", "WiFi, Bluetooth, USB-OTG", "40 MHz")
                } else if combined_output.contains("ESP32-P4") {
                    ("esp32p4", "WiFi 6, High Performance AI", "40 MHz")
                } else if combined_output.contains("ESP32-C6") {
                    ("esp32c6", "WiFi 6, Bluetooth 5, ZigBee 3.0", "40 MHz")
                } else if combined_output.contains("ESP32-C3") {
                    ("esp32c3", "WiFi, Bluetooth 5", "40 MHz")
                } else if combined_output.contains("ESP32-H2") {
                    ("esp32h2", "Bluetooth 5, ZigBee 3.0, Thread", "32 MHz")
                } else if combined_output.contains("ESP32") {
                    ("esp32", "WiFi, Bluetooth Classic", "40 MHz")
                } else {
                    println!("ℹ️ Could not determine chip type from esptool output");
                    return Ok(None);
                };

                println!("✅ IDF tools identified: {}", chip_type);

                // Generate a basic unique ID for IDF tools detection
                let unique_id = format!(
                    "{}-idf-{}",
                    chip_type,
                    port.replace("/", "-").replace(".", "_")
                );

                Ok(Some(BoardInfo {
                    port: port.to_string(),
                    chip_type: chip_type.to_string(),
                    crystal_frequency: crystal_freq.to_string(),
                    flash_size: "Unknown".to_string(),
                    features: features.to_string(),
                    mac_address: "**:**:**:**:**:**".to_string(),
                    device_description: "ESP Development Board (IDF detected)".to_string(),
                    chip_revision: None,
                    chip_id: None,
                    flash_manufacturer: None,
                    flash_device_id: None,
                    unique_id,
                }))
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                println!("ℹ️ esptool failed for {}: {}", port, stderr.trim());
                Ok(None)
            }
        }
        .await;

        result
    }

    /// Get enhanced unique identifiers using esptool commands
    async fn get_enhanced_unique_identifiers(port: &str) -> Option<EnhancedUniqueInfo> {
        use std::process::Stdio;
        use tokio::process::Command;

        let mut enhanced_info = EnhancedUniqueInfo {
            chip_id: None,
            flash_manufacturer: None,
            flash_device_id: None,
        };

        // Try to get security info for chip ID
        if let Ok(result) = tokio::time::timeout(std::time::Duration::from_millis(2000), async {
            Command::new("python")
                .args(["-m", "esptool", "--port", port, "get_security_info"])
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .kill_on_drop(true)
                .output()
                .await
        })
        .await
        {
            if let Ok(output) = result {
                if output.status.success() {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    for line in stdout.lines() {
                        if line.contains("Chip ID:") {
                            if let Some(id_str) = line.split(':').nth(1) {
                                if let Ok(chip_id) = id_str.trim().parse::<u32>() {
                                    enhanced_info.chip_id = Some(chip_id);
                                }
                            }
                        }
                    }
                }
            }
        }

        // Try to get flash ID information
        if let Ok(result) = tokio::time::timeout(std::time::Duration::from_millis(2000), async {
            Command::new("python")
                .args(["-m", "esptool", "--port", port, "flash_id"])
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .kill_on_drop(true)
                .output()
                .await
        })
        .await
        {
            if let Ok(output) = result {
                if output.status.success() {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    for line in stdout.lines() {
                        if line.contains("Manufacturer:") {
                            if let Some(mfg) = line.split(':').nth(1) {
                                enhanced_info.flash_manufacturer = Some(mfg.trim().to_string());
                            }
                        } else if line.contains("Device:") {
                            if let Some(device) = line.split(':').nth(1) {
                                enhanced_info.flash_device_id = Some(device.trim().to_string());
                            }
                        }
                    }
                }
            }
        }

        // Return Some if we got at least one piece of unique info
        if enhanced_info.chip_id.is_some() || enhanced_info.flash_manufacturer.is_some() {
            Some(enhanced_info)
        } else {
            None
        }
    }

    /// Generate a comprehensive unique ID from all available information
    fn generate_comprehensive_unique_id(board_info: &BoardInfo) -> String {
        let mut id_parts = Vec::new();

        // Start with chip type and revision
        id_parts.push(board_info.chip_type.clone());
        if let Some(ref revision) = board_info.chip_revision {
            id_parts.push(format!("rev{}", revision));
        }

        // Add chip ID if available
        if let Some(chip_id) = board_info.chip_id {
            id_parts.push(format!("chipid{}", chip_id));
        }

        // Add flash manufacturer and device info if available
        if let Some(ref flash_mfg) = board_info.flash_manufacturer {
            id_parts.push(format!("flash{}", flash_mfg));
        }
        if let Some(ref flash_dev) = board_info.flash_device_id {
            id_parts.push(format!("dev{}", flash_dev));
        }

        // Use MAC address if it's not masked
        if !board_info.mac_address.contains("*") && board_info.mac_address.len() > 10 {
            id_parts.push(format!("mac{}", board_info.mac_address.replace(":", "")));
        } else {
            // Fallback to port-based identifier
            id_parts.push(format!(
                "port{}",
                board_info.port.replace("/", "-").replace(".", "_")
            ));
        }

        id_parts.join("-")
    }

    /// Identification using espflash as subprocess with proper timeout and cancellation
    async fn identify_with_espflash_subprocess(port: &str) -> Result<Option<BoardInfo>> {
        use std::process::Stdio;
        use std::time::Duration;
        use tokio::process::Command;

        // Run espflash as separate subprocess to avoid terminal blocking
        // Increased timeout specifically for ESP32-P4 which can take longer to identify
        let timeout_dur = Duration::from_millis(1000);

        println!("🔍 Running espflash board-info for {}", port);

        let result = tokio::time::timeout(timeout_dur, async {
            let cmd = Command::new("espflash")
                .args(["board-info", "--port", port])
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .kill_on_drop(true) // Important: kill child if parent is cancelled
                .spawn()?;

            // Wait for the command to complete
            let output = cmd.wait_with_output().await?;

            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);

                println!("ℹ️ espflash stdout: {}", stdout.trim());
                if !stderr.trim().is_empty() {
                    println!("ℹ️ espflash stderr: {}", stderr.trim());
                }

                // Parse espflash output for comprehensive information
                let mut chip_type = "esp32".to_string();
                let mut flash_size = "Unknown".to_string();
                let mut features = "WiFi".to_string();
                let mut crystal_freq = "40 MHz".to_string();
                let mut chip_revision = None;
                let mut mac_address = "**:**:**:**:**:**".to_string();

                for line in stdout.lines() {
                    let line = line.trim();
                    if line.contains("Chip type:") {
                        if let Some(chip_info) = line.split(':').nth(1) {
                            let chip_info = chip_info.trim();
                            // Extract both chip type and revision if present
                            if let Some((chip, rev)) = chip_info.split_once(" (revision ") {
                                chip_type = chip.trim().to_lowercase().replace("-", "");
                                if let Some(revision) = rev.strip_suffix(")") {
                                    chip_revision = Some(revision.to_string());
                                }
                            } else {
                                chip_type = chip_info.to_lowercase().replace("-", "");
                            }
                        }
                    } else if line.contains("Crystal frequency:") {
                        if let Some(freq) = line.split(':').nth(1) {
                            crystal_freq = freq.trim().to_string();
                        }
                    } else if line.contains("Flash size:") {
                        if let Some(size) = line.split(':').nth(1) {
                            flash_size = size.trim().to_string();
                        }
                    } else if line.contains("Features:") {
                        if let Some(feat) = line.split(':').nth(1) {
                            features = feat.trim().to_string();
                        }
                    } else if line.contains("MAC address:") {
                        if let Some(mac) = line.split(':').nth(1) {
                            mac_address = mac.trim().to_string();
                        }
                    }
                }

                // Handle specific chip type conversions
                let normalized_chip_type = match chip_type.as_str() {
                    "esp32s3" | "esp32-s3" => "esp32s3",
                    "esp32p4" | "esp32-p4" => "esp32p4",
                    "esp32c3" | "esp32-c3" => "esp32c3",
                    "esp32c6" | "esp32-c6" => "esp32c6",
                    "esp32h2" | "esp32-h2" => "esp32h2",
                    _ => "esp32",
                };

                // Generate a preliminary unique ID from available info
                // We'll enhance this with additional unique identifiers later
                let preliminary_unique_id =
                    if mac_address != "**:**:**:**:**:**" && !mac_address.contains("*") {
                        mac_address.clone()
                    } else {
                        format!(
                            "{}:{}-{}",
                            normalized_chip_type,
                            chip_revision.as_deref().unwrap_or("unknown"),
                            port.replace("/", "-").replace(".", "_")
                        )
                    };

                println!(
                    "✅ Successfully identified {} board: {} (rev: {})",
                    normalized_chip_type,
                    port,
                    chip_revision.as_deref().unwrap_or("unknown")
                );

                let mut board_info = BoardInfo {
                    port: port.to_string(),
                    chip_type: normalized_chip_type.to_string(),
                    crystal_frequency: crystal_freq,
                    flash_size,
                    features,
                    mac_address: mac_address.clone(),
                    device_description: "ESP Development Board (espflash detected)".to_string(),
                    chip_revision,
                    chip_id: None,
                    flash_manufacturer: None,
                    flash_device_id: None,
                    unique_id: preliminary_unique_id,
                };

                // Try to get additional unique identifiers via esptool commands
                if let Some(enhanced_info) = Self::get_enhanced_unique_identifiers(port).await {
                    board_info.chip_id = enhanced_info.chip_id;
                    board_info.flash_manufacturer = enhanced_info.flash_manufacturer;
                    board_info.flash_device_id = enhanced_info.flash_device_id;

                    // Create a more comprehensive unique ID
                    board_info.unique_id = Self::generate_comprehensive_unique_id(&board_info);
                }

                Ok(Some(board_info))
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                println!("ℹ️ espflash failed for {}: {}", port, stderr.trim());
                Ok(None)
            }
        })
        .await;

        match result {
            Ok(inner) => inner,
            Err(_) => {
                println!("⏰ espflash timed out for port {}", port);
                Ok(None) // Timeout
            }
        }
    }

    /// Flash a board with the provided binary
    pub async fn flash_board(&mut self, request: FlashRequest) -> Result<FlashResponse> {
        // Get board port before borrowing mutably
        let board_port = {
            let board = self
                .boards
                .get(&request.board_id)
                .ok_or_else(|| anyhow::anyhow!("Board not found: {}", request.board_id))?;
            board.port.clone()
        };

        // Update status to flashing
        if let Some(board) = self.boards.get_mut(&request.board_id) {
            board.status = BoardStatus::Flashing;
            board.last_updated = Local::now();
        }

        let start_time = std::time::Instant::now();

        // Perform the actual flashing using espflash
        let result = Self::perform_flash(&board_port, &request.binary_data, request.offset).await;

        let duration_ms = start_time.elapsed().as_millis() as u64;

        // Update board status based on result
        if let Some(board) = self.boards.get_mut(&request.board_id) {
            match result {
                Ok(_) => {
                    board.status = BoardStatus::Available;
                    board.last_updated = Local::now();
                    Ok(FlashResponse {
                        success: true,
                        message: format!(
                            "Successfully flashed {} ({} bytes)",
                            board.id,
                            request.binary_data.len()
                        ),
                        duration_ms: Some(duration_ms),
                    })
                }
                Err(e) => {
                    board.status = BoardStatus::Error(e.to_string());
                    board.last_updated = Local::now();
                    Ok(FlashResponse {
                        success: false,
                        message: format!("Flash failed: {}", e),
                        duration_ms: Some(duration_ms),
                    })
                }
            }
        } else {
            Err(anyhow::anyhow!(
                "Board not found during status update: {}",
                request.board_id
            ))
        }
    }

    /// Perform the actual flashing operation using subprocess (safer)
    async fn perform_flash(port: &str, binary_data: &[u8], offset: u32) -> Result<()> {
        use std::process::Stdio;
        use tokio::fs;
        use tokio::process::Command;

        // Create temporary file for binary data
        let temp_dir = std::env::temp_dir();
        let temp_file = temp_dir.join(format!(
            "espbrew_flash_{}.bin",
            uuid::Uuid::new_v4().simple()
        ));

        // Write binary data to temp file
        fs::write(&temp_file, binary_data).await?;

        // Use espflash as subprocess for safer operation
        let mut cmd = Command::new("espflash")
            .args([
                "write-bin",
                "--port",
                port,
                "--flash-addr",
                &format!("0x{:x}", offset),
                temp_file.to_str().unwrap(),
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        // Wait for completion with timeout
        let timeout_dur = std::time::Duration::from_secs(60); // 1 minute timeout for flashing
        let result = tokio::time::timeout(timeout_dur, cmd.wait()).await;

        // Clean up temp file
        let _ = fs::remove_file(&temp_file).await;

        match result {
            Ok(Ok(status)) if status.success() => Ok(()),
            Ok(Ok(_)) => Err(anyhow::anyhow!("Flash command failed")),
            Ok(Err(e)) => Err(anyhow::anyhow!("Failed to run flash command: {}", e)),
            Err(_) => Err(anyhow::anyhow!("Flash operation timed out")),
        }
    }
}

#[derive(Debug)]
struct BoardInfo {
    port: String,
    chip_type: String,
    crystal_frequency: String,
    flash_size: String,
    features: String,
    mac_address: String,
    device_description: String,
    chip_revision: Option<String>,
    chip_id: Option<u32>,
    flash_manufacturer: Option<String>,
    flash_device_id: Option<String>,
    unique_id: String, // Combined unique identifier
}

// API Handlers
pub async fn list_boards(
    state: Arc<RwLock<ServerState>>,
) -> Result<impl warp::Reply, warp::Rejection> {
    let state = state.read().await;
    let response = BoardListResponse {
        boards: state.boards.values().cloned().collect(),
        server_info: ServerInfo {
            version: env!("CARGO_PKG_VERSION").to_string(),
            hostname: hostname::get()
                .unwrap_or_else(|_| "unknown".into())
                .to_string_lossy()
                .to_string(),
            last_scan: state.last_scan,
            total_boards: state.boards.len(),
        },
    };
    Ok(warp::reply::json(&response))
}

pub async fn flash_board(
    request: FlashRequest,
    state: Arc<RwLock<ServerState>>,
) -> Result<impl warp::Reply, warp::Rejection> {
    let mut state = state.write().await;

    match state.flash_board(request).await {
        Ok(response) => Ok(warp::reply::json(&response)),
        Err(e) => {
            let error_response = FlashResponse {
                success: false,
                message: format!("Flash operation failed: {}", e),
                duration_ms: None,
            };
            Ok(warp::reply::json(&error_response))
        }
    }
}

pub async fn get_board_info(
    board_id: String,
    state: Arc<RwLock<ServerState>>,
) -> Result<impl warp::Reply, warp::Rejection> {
    let state = state.read().await;

    match state.boards.get(&board_id) {
        Some(board) => Ok(warp::reply::json(board)),
        None => {
            let error = serde_json::json!({
                "error": format!("Board not found: {}", board_id)
            });
            Ok(warp::reply::json(&error))
        }
    }
}

// Web Interface Handlers
/*
pub async fn dashboard(
    state: Arc<RwLock<ServerState>>,
) -> Result<impl warp::Reply, warp::Rejection> {
    let state = state.read().await;
    let template = DashboardTemplate {
        page: "dashboard".to_string(),
        boards: state.boards.values().cloned().collect(),
        server_info: ServerInfo {
            version: env!("CARGO_PKG_VERSION").to_string(),
            hostname: hostname::get()
                .unwrap_or_else(|_| "unknown".into())
                .to_string_lossy()
                .to_string(),
            last_scan: state.last_scan,
            total_boards: state.boards.len(),
        },
    };

    match template.render() {
        Ok(html) => Ok(warp::reply::html(html)),
        Err(e) => {
            eprintln!("Template rendering error: {}", e);
            Ok(warp::reply::html(format!("<h1>Error</h1><p>Failed to render template: {}</p>", e)))
        }
    }
}

pub async fn flash_page(
    query: HashMap<String, String>,
    state: Arc<RwLock<ServerState>>,
) -> Result<impl warp::Reply, warp::Rejection> {
    let state = state.read().await;
    let template = FlashTemplate {
        page: "flash".to_string(),
        boards: state.boards.values().cloned().collect(),
        selected_board: query.get("board").map(|s| s.clone()),
    };

    match template.render() {
        Ok(html) => Ok(warp::reply::html(html)),
        Err(e) => {
            eprintln!("Template rendering error: {}", e);
            Ok(warp::reply::html(format!("<h1>Error</h1><p>Failed to render template: {}</p>", e)))
        }
    }
}
*/

#[derive(Parser)]
#[command(name = "espbrew-server")]
#[command(about = "ESPBrew Remote Flashing Server")]
struct ServerCli {
    /// Server configuration file
    #[arg(short, long, default_value = "espbrew-server.toml")]
    config: PathBuf,

    /// Bind address
    #[arg(short, long, default_value = "0.0.0.0")]
    bind: String,

    /// Port to listen on
    #[arg(short, long, default_value = "8080")]
    port: u16,

    /// Board scan interval in seconds
    #[arg(long, default_value = "30")]
    scan_interval: u64,

    #[command(subcommand)]
    command: Option<ServerCommands>,
}

#[derive(Subcommand)]
enum ServerCommands {
    /// Start the server
    Start,
    /// Scan for boards and exit
    Scan,
    /// Generate default configuration
    Config,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = ServerCli::parse();

    // Load or create configuration
    let mut config = if cli.config.exists() {
        let config_str = tokio::fs::read_to_string(&cli.config).await?;
        toml::from_str(&config_str)?
    } else {
        ServerConfig::default()
    };

    // Override with CLI arguments
    config.bind_address = cli.bind.clone();
    config.port = cli.port;
    config.scan_interval = cli.scan_interval;

    match cli.command.unwrap_or(ServerCommands::Start) {
        ServerCommands::Config => {
            let config_str = toml::to_string_pretty(&config)?;
            tokio::fs::write(&cli.config, config_str).await?;
            println!("📝 Configuration written to {}", cli.config.display());
            return Ok(());
        }
        ServerCommands::Scan => {
            // Set up signal handling for graceful shutdown on CTRL+C
            let ctrl_c = async {
                tokio::signal::ctrl_c()
                    .await
                    .expect("Failed to install CTRL+C signal handler")
            };

            let mut state = ServerState::new(config);

            // Run scan with signal handling and timeout
            tokio::select! {
                result = state.scan_boards() => {
                    match result {
                        Ok(_) => println!("✅ Scan completed successfully"),
                        Err(e) => {
                            eprintln!("❌ Scan failed: {}", e);
                            return Err(e);
                        }
                    }
                }
                _ = ctrl_c => {
                    println!("\n🛑 Scan interrupted by user (CTRL+C)");
                    return Ok(());
                }
                _ = tokio::time::sleep(tokio::time::Duration::from_secs(60)) => {
                    println!("🕒 Scan timed out after 60 seconds");
                    return Ok(());
                }
            }

            return Ok(());
        }
        ServerCommands::Start => {
            // Continue to start the server
        }
    }

    println!("🍺 ESPBrew Server v{}", env!("CARGO_PKG_VERSION"));
    println!(
        "🌐 Starting server on {}:{}",
        config.bind_address, config.port
    );

    // Initialize server state
    let state = Arc::new(RwLock::new(ServerState::new(config.clone())));

    // Perform initial board scan
    {
        let mut state_lock = state.write().await;
        state_lock.scan_boards().await?;
    }

    // Setup shutdown notify for background tasks
    let shutdown_notify = Arc::new(tokio::sync::Notify::new());

    // Start periodic board scanning
    let scan_state = state.clone();
    let scan_interval = config.scan_interval;
    let scan_shutdown = shutdown_notify.clone();
    let scanner_handle = tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(scan_interval));
        let scan_cancel_signal = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    if let Ok(mut state) = scan_state.try_write() {
                        // Reset the cancel signal for this scan iteration
                        scan_cancel_signal.store(false, std::sync::atomic::Ordering::Relaxed);

                        if let Err(e) = state.scan_boards_with_cancellation(Some(scan_cancel_signal.clone())).await {
                            eprintln!("❌ Board scan failed: {}", e);
                        }
                    }
                }
                _ = scan_shutdown.notified() => {
                    println!("🛑 Stopping scanner task...");
                    // Signal any ongoing scan to cancel
                    scan_cancel_signal.store(true, std::sync::atomic::Ordering::Relaxed);
                    break;
                }
            }
        }
        println!("🛑 Scanner task stopped");
    });

    // Define routes
    let state_filter = warp::any().map(move || state.clone());

    // Web Interface routes (temporarily disabled)
    /*
    // GET / - Dashboard
    let web_dashboard = warp::path::end()
        .and(warp::get())
        .and(state_filter.clone())
        .and_then(dashboard);

    // GET /flash - Flash page
    let web_flash = warp::path("flash")
        .and(warp::path::end())
        .and(warp::get())
        .and(warp::query::<HashMap<String, String>>())
        .and(state_filter.clone())
        .and_then(flash_page);
    */

    let api = warp::path("api").and(warp::path("v1"));

    // GET /api/v1/boards - List all boards
    let boards = api
        .and(warp::path("boards"))
        .and(warp::path::end())
        .and(warp::get())
        .and(state_filter.clone())
        .and_then(list_boards);

    // GET /api/v1/boards/{id} - Get board info
    let board_info = api
        .and(warp::path("boards"))
        .and(warp::path::param())
        .and(warp::path::end())
        .and(warp::get())
        .and(state_filter.clone())
        .and_then(get_board_info);

    // POST /api/v1/flash - Flash a board
    let flash = api
        .and(warp::path("flash"))
        .and(warp::path::end())
        .and(warp::post())
        .and(warp::body::json())
        .and(state_filter.clone())
        .and_then(flash_board);

    // Health check endpoint
    let health = warp::path("health").and(warp::get()).map(|| {
        warp::reply::json(&serde_json::json!({
            "status": "healthy",
            "version": env!("CARGO_PKG_VERSION")
        }))
    });

    // Static file serving for web interface
    let static_files = warp::path("static").and(warp::fs::dir("static"));

    // Root redirect to dashboard
    let root_redirect = warp::path::end()
        .and(warp::get())
        .map(|| warp::redirect(warp::http::Uri::from_static("/static/index.html")));

    let routes = root_redirect
        .or(static_files)
        .or(boards)
        .or(board_info)
        .or(flash)
        .or(health)
        .with(warp::cors().allow_any_origin());
    // Removed warp::log middleware as it can cause shutdown delays

    // Start the server
    println!(
        "🚀 Server running at http://{}:{}",
        config.bind_address, config.port
    );
    println!("🌐 Web Interface:");
    println!("   GET  /                    - Dashboard (redirects to /static/index.html)");
    println!("   GET  /static/index.html   - ESP32 board dashboard");
    println!("   GET  /static/flash.html   - Flash firmware interface");
    println!("📡 API endpoints:");
    println!("   GET  /api/v1/boards       - List all connected boards");
    println!("   GET  /api/v1/boards/{{id}} - Get board information");
    println!("   POST /api/v1/flash        - Flash a board");
    println!("   GET  /health              - Health check");
    println!();
    println!("Press Ctrl+C to stop the server");

    // Simplified shutdown signal handling - just Ctrl+C
    async fn shutdown_signal() {
        let _ = tokio::signal::ctrl_c().await;
        println!("\n🛑 Shutdown signal received. Stopping HTTP server...");
    }

    let addr: std::net::IpAddr = config
        .bind_address
        .parse()
        .unwrap_or_else(|_| std::net::IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED));

    let (_server, server_fut) = {
        let shutdown_notify = shutdown_notify.clone();
        let (addr, server) =
            warp::serve(routes).bind_with_graceful_shutdown((addr, config.port), async move {
                shutdown_signal().await;
                shutdown_notify.notify_waiters();
            });
        (addr, server)
    };

    // Wait for server shutdown (this blocks until Ctrl+C is pressed)
    server_fut.await;

    // Now we're in shutdown phase - apply timeout here
    let shutdown_timeout = tokio::time::Duration::from_secs(2);
    let shutdown_result = tokio::time::timeout(shutdown_timeout, async {
        // Wait for scanner task to finish
        if let Err(e) = scanner_handle.await {
            eprintln!("⚠️ Scanner task join error: {}", e);
        }
    })
    .await;

    match shutdown_result {
        Ok(_) => println!("✅ Server stopped cleanly"),
        Err(_) => {
            println!("⚠️ Scanner task shutdown timed out after 2 seconds, forcing exit");
            std::process::exit(0);
        }
    }

    Ok(())
}
