//! Board scanner service for detecting and identifying ESP32 boards

use anyhow::Result;
use chrono::{DateTime, Local, Utc};
use log::{debug, info, trace};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::models::board::{BoardStatus, ConnectedBoard};
use crate::server::app::ServerState;
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

/// Cached board information to avoid repeated espflash calls
#[derive(Debug, Clone)]
struct CachedBoardInfo {
    pub board_info: BoardInfo,
    pub cache_timestamp: DateTime<Utc>,
}

/// Board scanner service
#[derive(Clone)]
pub struct BoardScanner {
    state: Arc<RwLock<ServerState>>,
    /// Cache of board information to avoid repeated espflash calls
    board_cache: Arc<RwLock<HashMap<String, CachedBoardInfo>>>, // Key: port name
}

impl BoardScanner {
    pub fn new(state: Arc<RwLock<ServerState>>) -> Self {
        Self {
            state,
            board_cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Check if cached board information is valid and recent
    async fn get_cached_board_info(&self, port: &str) -> Option<BoardInfo> {
        let cache = self.board_cache.read().await;
        if let Some(cached) = cache.get(port) {
            let now = Utc::now();
            let cache_age = now.signed_duration_since(cached.cache_timestamp);

            // Cache is valid for 5 minutes unless device is disconnected
            if cache_age.num_minutes() < 5 {
                trace!(
                    "Using cached board info for {} (cached {} seconds ago)",
                    port,
                    cache_age.num_seconds()
                );
                return Some(cached.board_info.clone());
            } else {
                debug!(
                    "Cache expired for {} (age: {} seconds)",
                    port,
                    cache_age.num_seconds()
                );
            }
        } else {
            trace!("No cached info available for {}", port);
        }
        None
    }

    /// Cache board information after successful identification
    async fn cache_board_info(&self, port: &str, board_info: BoardInfo) {
        let now = Utc::now();
        let cached_info = CachedBoardInfo {
            board_info: board_info.clone(),
            cache_timestamp: now,
        };

        let mut cache = self.board_cache.write().await;
        cache.insert(port.to_string(), cached_info);

        trace!("Cached board info for {} ({})", port, board_info.unique_id);
    }

    /// Clean up cache entries for disconnected devices
    async fn cleanup_disconnected_devices(&self, current_ports: &[String]) {
        let current_ports_set: std::collections::HashSet<_> = current_ports.iter().collect();

        let mut cache = self.board_cache.write().await;

        let cached_ports: Vec<String> = cache.keys().cloned().collect();
        let mut removed_count = 0;

        for cached_port in cached_ports {
            if !current_ports_set.contains(&cached_port) {
                cache.remove(&cached_port);
                removed_count += 1;
                debug!(
                    "Removed cached info for disconnected device: {}",
                    cached_port
                );
            }
        }

        if removed_count > 0 {
            debug!(
                "Cleaned up {} disconnected device(s) from cache",
                removed_count
            );
        }
    }

    /// Force refresh all board information, bypassing cache
    pub async fn refresh_all_boards(&self) -> Result<usize> {
        info!("Manual refresh requested - clearing all cached board information");

        // Clear all cache entries
        {
            let mut cache = self.board_cache.write().await;
            let cache_count = cache.len();
            cache.clear();
            debug!("Cleared {} cached board entries", cache_count);
        }

        // Perform fresh scan
        self.scan_boards().await
    }

    /// Force refresh specific board information by port, bypassing cache
    pub async fn refresh_board(&self, port: &str) -> Result<Option<BoardInfo>> {
        debug!("Manual refresh requested for port: {}", port);

        // Remove cached entry for this port
        {
            let mut cache = self.board_cache.write().await;
            cache.remove(port);
            debug!("Cleared cached entry for {}", port);
        }

        // Perform fresh identification
        self.identify_board_with_cancellation(port, None).await
    }

    /// Generate an informative default device name with MCU type and discovery timestamp
    fn generate_default_device_name(
        chip_type: &str,
        _original_description: &str,
        discovery_time: chrono::DateTime<chrono::Local>,
    ) -> String {
        // Format chip type for better readability
        let formatted_chip_type = match chip_type.to_lowercase().as_str() {
            "esp32" => "ESP32",
            "esp32s2" => "ESP32-S2",
            "esp32s3" => "ESP32-S3",
            "esp32c2" => "ESP32-C2",
            "esp32c3" => "ESP32-C3",
            "esp32c6" => "ESP32-C6",
            "esp32h2" => "ESP32-H2",
            "esp32p4" => "ESP32-P4",
            "esp8266" => "ESP8266",
            _ => chip_type, // Use as-is for unknown types
        };

        // Format timestamp as HH:MM:SS for readability
        let time_str = discovery_time.format("%H:%M:%S").to_string();

        format!("{} - {}", formatted_chip_type, time_str)
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
                debug!("Board scan cancelled before starting");
                return Ok(0);
            }
        }

        debug!("Scanning for USB serial ports...");

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

        info!("Found {} USB serial ports", relevant_ports.len());
        for port_info in &relevant_ports {
            debug!("  Port: {}", port_info.port_name);
        }

        if relevant_ports.is_empty() {
            debug!("No USB serial ports found. Connect your development boards via USB.");
            return Ok(0);
        }

        // Step 1: Get configuration data and release the lock
        let (persistent_config, board_mappings) = {
            let state_lock = self.state.read().await;
            (
                state_lock.persistent_config.clone(),
                state_lock.config.board_mappings.clone(),
            )
        };

        // Clean up cache for disconnected devices
        let current_port_names: Vec<String> =
            relevant_ports.iter().map(|p| p.port_name.clone()).collect();
        self.cleanup_disconnected_devices(&current_port_names).await;

        // Step 2: Identify all boards (both cu and tty) using cache when possible
        let mut all_board_info = Vec::new();

        for (index, port_info) in relevant_ports.iter().enumerate() {
            // Check for cancellation before processing each port
            if let Some(ref cancel) = cancel_signal {
                if cancel.load(std::sync::atomic::Ordering::Relaxed) {
                    debug!("Board scan cancelled during port enumeration");
                    return Ok(discovered_boards.len());
                }
            }

            debug!(
                "[{}/{}] Processing port: {}",
                index + 1,
                relevant_ports.len(),
                port_info.port_name
            );

            // First try to get cached information
            let board = if let Some(cached_board) =
                self.get_cached_board_info(&port_info.port_name).await
            {
                cached_board
            } else {
                // No cache or expired cache - perform full identification
                trace!(
                    "Performing fresh identification for {}",
                    port_info.port_name
                );

                match self
                    .identify_board_with_cancellation(&port_info.port_name, cancel_signal.clone())
                    .await
                {
                    Ok(Some(enhanced_board)) => {
                        debug!(
                            "Fresh identification successful: {} ({})",
                            enhanced_board.chip_type, enhanced_board.unique_id
                        );

                        // Cache the successful identification
                        self.cache_board_info(&port_info.port_name, enhanced_board.clone())
                            .await;
                        enhanced_board
                    }
                    Ok(None) => {
                        debug!("Enhanced identification failed, using USB fallback");
                        let usb_board = Self::create_usb_board_info(&port_info);

                        // Cache USB fallback information too (but with shorter TTL)
                        self.cache_board_info(&port_info.port_name, usb_board.clone())
                            .await;
                        usb_board
                    }
                    Err(e) => {
                        debug!("Enhanced identification error: {}, using USB fallback", e);
                        let usb_board = Self::create_usb_board_info(&port_info);

                        // Cache USB fallback information too
                        self.cache_board_info(&port_info.port_name, usb_board.clone())
                            .await;
                        usb_board
                    }
                }
            };

            all_board_info.push(board);
        }

        // Step 3: Deduplicate boards by unique_id, preferring cu over tty
        let deduplicated_boards = Self::deduplicate_boards_by_mac(all_board_info);

        // Step 4: Apply assignments and convert to ConnectedBoard
        for board in deduplicated_boards {
            info!(
                "Added board on {}: {} ({})",
                board.port, board.device_description, board.unique_id
            );

            // Generate MAC-based persistent board ID
            let board_id = Self::generate_persistent_board_id(&board.unique_id);

            // Apply logical name mapping if configured
            let logical_name = board_mappings.get(&board.port).cloned();

            // Look up board assignment based on unique_id
            let (assigned_board_type_id, assigned_board_type) = if let Some(assignment) =
                persistent_config
                    .board_assignments
                    .iter()
                    .find(|a| a.board_unique_id == board.unique_id)
            {
                // Find the complete board type from the ID
                let board_type = persistent_config
                    .board_types
                    .iter()
                    .find(|bt| bt.id == assignment.board_type_id)
                    .cloned();

                debug!(
                    "Applying assignment: {} -> {} ({})",
                    board.unique_id,
                    assignment.board_type_id,
                    board_type
                        .as_ref()
                        .map(|bt| bt.name.as_str())
                        .unwrap_or("Unknown")
                );

                (Some(assignment.board_type_id.clone()), board_type)
            } else {
                (None, None)
            };

            // Create informative default device description with MCU name and timestamp
            let current_time = Local::now();
            let device_description = Self::generate_default_device_name(
                &board.chip_type,
                &board.device_description,
                current_time,
            );

            let connected_board = ConnectedBoard {
                id: board_id.clone(),
                port: board.port.clone(),
                chip_type: board.chip_type.clone(),
                crystal_frequency: board.crystal_frequency.clone(),
                flash_size: board.flash_size.clone(),
                features: board.features.clone(),
                mac_address: board.mac_address.clone(),
                device_description,
                status: BoardStatus::Available,
                last_updated: current_time,
                logical_name,
                unique_id: board.unique_id.clone(),
                chip_revision: board.chip_revision.clone(),
                chip_id: board.chip_id,
                flash_manufacturer: board.flash_manufacturer.clone(),
                flash_device_id: board.flash_device_id.clone(),
                assigned_board_type_id,
                assigned_board_type,
            };

            discovered_boards.insert(board_id, connected_board);
        }

        // Update state with discovered boards
        let mut state_lock = self.state.write().await;
        state_lock.boards = discovered_boards;
        state_lock.last_scan = Local::now();

        let board_count = state_lock.boards.len();
        info!("Scan complete. Found {} USB devices", board_count);

        for board in state_lock.boards.values() {
            let logical_name = board.logical_name.as_deref().unwrap_or("(unmapped)");
            debug!(
                "  Board {} [{}] - {} @ {} ({})",
                board.id, logical_name, board.chip_type, board.port, board.device_description
            );
        }

        Ok(board_count)
    }

    /// Deduplicate boards by MAC address, preferring cu over tty devices
    fn deduplicate_boards_by_mac(all_boards: Vec<BoardInfo>) -> Vec<BoardInfo> {
        use std::collections::HashMap;

        let mut board_groups: HashMap<String, Vec<BoardInfo>> = HashMap::new();

        // Group boards by unique_id (which includes MAC when available)
        for board in all_boards {
            board_groups
                .entry(board.unique_id.clone())
                .or_default()
                .push(board);
        }

        let mut deduplicated = Vec::new();
        let mut ignored_ports = Vec::new();

        for (unique_id, mut boards) in board_groups {
            if boards.len() == 1 {
                // Single board, no deduplication needed
                deduplicated.push(boards.into_iter().next().unwrap());
            } else {
                // Multiple boards with same unique_id - prefer cu over tty
                debug!(
                    "Found {} duplicate entries for board {} - applying cu/tty preference",
                    boards.len(),
                    unique_id
                );

                // Sort by preference: cu devices first, then tty
                boards.sort_by(|a, b| {
                    let a_is_cu = a.port.contains("/dev/cu.");
                    let b_is_cu = b.port.contains("/dev/cu.");

                    match (a_is_cu, b_is_cu) {
                        (true, false) => std::cmp::Ordering::Less, // cu comes first
                        (false, true) => std::cmp::Ordering::Greater, // tty comes after cu
                        _ => a.port.cmp(&b.port),                  // same type, sort by port name
                    }
                });

                // Take the first (most preferred) board
                let preferred_board = boards.remove(0);
                debug!(
                    "Selected preferred port: {} ({})",
                    preferred_board.port, preferred_board.device_description
                );

                // Log ignored ports
                for ignored_board in &boards {
                    trace!(
                        "Ignoring duplicate port: {} (same MAC: {})",
                        ignored_board.port, unique_id
                    );
                    ignored_ports.push(ignored_board.port.clone());
                }

                deduplicated.push(preferred_board);
            }
        }

        if !ignored_ports.is_empty() {
            debug!(
                "Deduplication complete: {} boards selected, {} ports ignored",
                deduplicated.len(),
                ignored_ports.len()
            );
            trace!("Ignored ports: {:?}", ignored_ports);
        }

        deduplicated
    }

    /// Generate a persistent board ID based on unique_id (MAC-based when available)
    fn generate_persistent_board_id(unique_id: &str) -> String {
        // If unique_id starts with "MAC", it's MAC-based - use as-is
        if unique_id.starts_with("MAC") {
            format!("board_{}", unique_id)
        }
        // If unique_id contains MAC-like pattern, extract and format
        else if unique_id.len() >= 12 && unique_id.chars().all(|c| c.is_ascii_hexdigit()) {
            format!("board_MAC{}", unique_id)
        }
        // For USB-based or other IDs, create a stable hash-based ID
        else {
            use std::collections::hash_map::DefaultHasher;
            use std::hash::{Hash, Hasher};

            let mut hasher = DefaultHasher::new();
            unique_id.hash(&mut hasher);
            let hash = hasher.finish();

            format!("board_ID{:016x}", hash)
        }
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
                debug!("Board identification cancelled for {}", port);
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

        let base_board_info = match usb_result {
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
