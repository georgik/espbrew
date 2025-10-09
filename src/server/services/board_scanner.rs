//! Board scanner service for detecting and identifying ESP32 boards

use anyhow::Result;
use chrono::Local;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::models::board::{BoardStatus, ConnectedBoard, EnhancedBoardInfo};
use crate::server::app::{MonitoringSession, ServerState};
use crate::utils::espflash_utils;

/// Board information structure for internal identification
#[derive(Debug, Clone)]
pub struct BoardInfo {
    pub port: String,
    pub chip_type: String,
    pub crystal_frequency: String,
    pub flash_size: String,
    pub features: String,
    pub mac_address: String,
    pub device_description: String,
    pub chip_revision: Option<String>,
    pub chip_id: Option<u32>,
    pub flash_manufacturer: Option<String>,
    pub flash_device_id: Option<String>,
    pub unique_id: String,
}

/// Board scanner service
#[derive(Clone)]
pub struct BoardScanner {
    state: Arc<RwLock<ServerState>>,
}

impl BoardScanner {
    pub fn new(state: Arc<RwLock<ServerState>>) -> Self {
        Self { state }
    }

    /// Discover and update connected boards
    pub async fn scan_boards(&self) -> Result<usize> {
        self.scan_boards_with_cancellation(None).await
    }

    /// Discover and update connected boards with optional cancellation support
    pub async fn scan_boards_with_cancellation(
        &self,
        cancel_signal: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
    ) -> Result<usize> {
        // Check if we should cancel early
        if let Some(ref cancel) = cancel_signal {
            if cancel.load(std::sync::atomic::Ordering::Relaxed) {
                println!("🛑 Board scan cancelled before starting");
                return Ok(0);
            }
        }

        println!("🔍 Scanning for USB serial ports...");

        // Use serialport to discover serial ports
        let ports = serialport::available_ports()?;
        let mut discovered_boards = HashMap::new();

        // Filter for relevant USB ports on macOS, Linux, and Windows
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
                    // On Windows, ESP32 devices appear as COM ports
                    || port_name.starts_with("COM")
            })
            .collect();

        println!("📡 Found {} USB serial ports", relevant_ports.len());
        for port_info in &relevant_ports {
            println!("  🔌 {}", port_info.port_name);
        }

        if relevant_ports.is_empty() {
            println!("⚠️  No USB serial ports found. Connect your development boards via USB.");
            return Ok(0);
        }

        let state_lock = self.state.read().await;
        let board_mappings = &state_lock.config.board_mappings;

        for (index, port_info) in relevant_ports.iter().enumerate() {
            // Check for cancellation before processing each port
            if let Some(ref cancel) = cancel_signal {
                if cancel.load(std::sync::atomic::Ordering::Relaxed) {
                    println!("🛑 Board scan cancelled during port enumeration");
                    return Ok(discovered_boards.len());
                }
            }

            println!(
                "🔍 [{}/{}] Adding port: {}",
                index + 1,
                relevant_ports.len(),
                port_info.port_name
            );

            // Try enhanced board identification, fall back to USB info if needed
            println!(
                "🔍 Attempting enhanced identification for {}",
                port_info.port_name
            );

            let board = match self
                .identify_board_with_cancellation(&port_info.port_name, cancel_signal.clone())
                .await
            {
                Ok(Some(enhanced_board)) => {
                    println!(
                        "✅ Enhanced identification successful: {} ({})",
                        enhanced_board.chip_type, enhanced_board.unique_id
                    );
                    enhanced_board
                }
                Ok(None) => {
                    println!("⚠️ Enhanced identification failed, using USB fallback");
                    Self::create_usb_board_info(&port_info)
                }
                Err(e) => {
                    println!(
                        "⚠️ Enhanced identification error: {}, using USB fallback",
                        e
                    );
                    Self::create_usb_board_info(&port_info)
                }
            };

            println!(
                "✅ Added board on {}: {} ({})",
                port_info.port_name, board.device_description, board.unique_id
            );
            let board_id = format!("board_{}", board.port.replace("/", "_").replace(".", "_"));

            // Apply logical name mapping if configured
            let logical_name = board_mappings.get(&board.port).cloned();

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
                assigned_board_type_id: None,
                assigned_board_type: None,
            };

            discovered_boards.insert(board_id, connected_board);
        }

        // Update state with discovered boards
        drop(state_lock);
        let mut state_lock = self.state.write().await;
        state_lock.boards = discovered_boards;
        state_lock.last_scan = Local::now();

        let board_count = state_lock.boards.len();
        println!("✅ Scan complete. Found {} USB devices", board_count);

        for board in state_lock.boards.values() {
            let logical_name = board.logical_name.as_deref().unwrap_or("(unmapped)");
            println!(
                "  📱 {} [{}] - {} @ {} ({})",
                board.id, logical_name, board.chip_type, board.port, board.device_description
            );
        }

        Ok(board_count)
    }

    /// Create a lightweight board info using only USB port information
    fn create_usb_board_info(port_info: &serialport::SerialPortInfo) -> BoardInfo {
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
        } else if port_info.port_name.starts_with("COM") {
            // Windows: ESP32 boards appear as COM ports, determine type from USB info
            // We'll try to be more specific based on USB VID/PID in the USB detection logic
            ("ESP32/ESP32-S3", "WiFi, Bluetooth")
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

        let port_str = port.to_string();

        // Stage 1: Quick USB-based detection for basic chip identification
        let usb_result = tokio::time::timeout(
            Duration::from_millis(500),
            Self::identify_with_usb_info(&port_str),
        )
        .await;

        let mut base_board_info = match usb_result {
            Ok(Ok(Some(board_info))) => {
                println!(
                    "✅ Stage 1: USB detection identified: {}",
                    board_info.chip_type
                );
                Some(board_info)
            }
            _ => {
                println!("ℹ️ Stage 1: USB detection inconclusive");
                None
            }
        };

        // Check for cancellation before Stage 2
        if let Some(ref cancel) = cancel_signal {
            if cancel.load(std::sync::atomic::Ordering::Relaxed) {
                println!(
                    "🛑 Board identification cancelled before Stage 2 for {}",
                    port
                );
                return Ok(base_board_info);
            }
        }

        // Stage 2: Always try native espflash for accurate MAC address and detailed info
        println!(
            "🔍 Stage 2: Using native espflash for MAC address detection on {}",
            port_str
        );
        let espflash_result = tokio::time::timeout(
            Duration::from_millis(5000), // Extended for ESP32-P4
            Self::identify_with_espflash_native(&port_str),
        )
        .await;

        match espflash_result {
            Ok(Ok(Some(enhanced_board_info))) => {
                println!(
                    "✅ Stage 2: Enhanced identification successful: {} (MAC: {})",
                    enhanced_board_info.chip_type, enhanced_board_info.mac_address
                );
                // Use the enhanced info with real MAC address
                Ok(Some(enhanced_board_info))
            }
            Ok(Ok(None)) => {
                println!("ℹ️ Stage 2: Native espflash found no ESP32 board");
                // Fall back to USB detection if available
                Ok(base_board_info)
            }
            Ok(Err(e)) => {
                println!("⚠️ Stage 2: Native espflash error: {}", e);
                // Fall back to USB detection if available
                Ok(base_board_info)
            }
            Err(_) => {
                println!("⏰ Stage 2: Native espflash timeout");
                // Fall back to USB detection if available
                Ok(base_board_info)
            }
        }
    }

    /// Identification using USB characteristics
    async fn identify_with_usb_info(port: &str) -> Result<Option<BoardInfo>> {
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

    /// Identification using native espflash crate (gets real MAC address)
    async fn identify_with_espflash_native(port: &str) -> Result<Option<BoardInfo>> {
        println!(
            "🔍 Using native espflash for board identification on {}",
            port
        );

        // Use the native espflash identification from utils
        match espflash_utils::identify_esp_board(port).await {
            Ok(Some(esp_info)) => {
                println!(
                    "✅ Native espflash identified {} with MAC: {}",
                    esp_info.chip_type, esp_info.mac_address
                );

                // Generate unique ID based on MAC address if available
                let unique_id =
                    if esp_info.mac_address != "Unknown" && !esp_info.mac_address.contains("*") {
                        let mac_clean = esp_info.mac_address.replace(":", "");
                        format!("MAC{}", mac_clean)
                    } else {
                        format!(
                            "{}:{}-{}",
                            esp_info.chip_type,
                            esp_info.chip_revision.as_deref().unwrap_or("unknown"),
                            port.replace("/", "-").replace(".", "_")
                        )
                    };

                // Convert EspBoardInfo to BoardInfo
                let board_info = BoardInfo {
                    port: esp_info.port,
                    chip_type: esp_info.chip_type,
                    crystal_frequency: esp_info.crystal_frequency,
                    flash_size: esp_info.flash_size,
                    features: esp_info.features,
                    mac_address: esp_info.mac_address,
                    device_description: esp_info.device_description,
                    chip_revision: esp_info.chip_revision,
                    chip_id: None,
                    flash_manufacturer: None,
                    flash_device_id: None,
                    unique_id,
                };

                Ok(Some(board_info))
            }
            Ok(None) => {
                println!("ℹ️ Native espflash found no ESP32 board on {}", port);
                Ok(None)
            }
            Err(e) => {
                println!("⚠️ Native espflash error on {}: {}", port, e);
                Ok(None)
            }
        }
    }
}
