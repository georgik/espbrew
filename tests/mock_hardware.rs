//! Mock Hardware Simulation Framework
//!
//! This module provides comprehensive mocking capabilities for ESP32 hardware
//! interactions, enabling thorough testing without requiring real devices.

use serde_json::{Value, json};
use std::collections::HashMap;
use std::io::{self, Read, Write};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tempfile::TempDir;

/// Mock ESP32 device that simulates real hardware responses
#[derive(Debug, Clone)]
pub struct MockEsp32Device {
    /// Device identifier (e.g., "ESP32-S3", "ESP32-C3")
    pub chip_type: String,
    /// MAC address of the device
    pub mac_address: String,
    /// Current device state
    pub state: MockDeviceState,
    /// Flash size in bytes
    pub flash_size: u32,
    /// Simulated flash memory contents
    pub flash_memory: Arc<Mutex<Vec<u8>>>,
    /// Response delays to simulate real hardware timing
    pub response_delays: MockResponseDelays,
    /// Error injection for testing error scenarios
    pub error_injection: MockErrorInjection,
}

/// Possible states of the mock device
#[derive(Debug, Clone, PartialEq)]
pub enum MockDeviceState {
    /// Device is disconnected
    Disconnected,
    /// Device is connected and ready
    Connected,
    /// Device is in bootloader mode
    BootloaderMode,
    /// Device is currently being flashed
    Flashing,
    /// Device is running application
    Running,
    /// Device encountered an error
    Error(String),
}

/// Configurable response delays to simulate real hardware
#[derive(Debug, Clone)]
pub struct MockResponseDelays {
    /// Connection establishment delay
    pub connection_delay: Duration,
    /// Bootloader entry delay
    pub bootloader_delay: Duration,
    /// Flash operation delay per KB
    pub flash_delay_per_kb: Duration,
    /// Reset delay
    pub reset_delay: Duration,
}

/// Error injection configuration for testing error scenarios
#[derive(Debug, Clone)]
pub struct MockErrorInjection {
    /// Probability of connection failure (0.0-1.0)
    pub connection_failure_rate: f32,
    /// Probability of flash failure (0.0-1.0)
    pub flash_failure_rate: f32,
    /// Simulate timeout errors
    #[allow(dead_code)]
    pub timeout_errors: bool,
    /// Simulate checksum errors
    #[allow(dead_code)]
    pub checksum_errors: bool,
}

impl Default for MockResponseDelays {
    fn default() -> Self {
        Self {
            connection_delay: Duration::from_millis(100),
            bootloader_delay: Duration::from_millis(50),
            flash_delay_per_kb: Duration::from_millis(10),
            reset_delay: Duration::from_millis(200),
        }
    }
}

impl Default for MockErrorInjection {
    fn default() -> Self {
        Self {
            connection_failure_rate: 0.0,
            flash_failure_rate: 0.0,
            timeout_errors: false,
            checksum_errors: false,
        }
    }
}

impl MockEsp32Device {
    /// Create a new mock ESP32-S3 device
    pub fn new_esp32s3() -> Self {
        Self {
            chip_type: "ESP32-S3".to_string(),
            mac_address: "24:6F:28:12:34:56".to_string(),
            state: MockDeviceState::Connected,
            flash_size: 16 * 1024 * 1024, // 16MB
            flash_memory: Arc::new(Mutex::new(vec![0xFF; 16 * 1024 * 1024])),
            response_delays: MockResponseDelays::default(),
            error_injection: MockErrorInjection::default(),
        }
    }

    /// Create a new mock ESP32-C3 device
    pub fn new_esp32c3() -> Self {
        Self {
            chip_type: "ESP32-C3".to_string(),
            mac_address: "7C:DF:A1:12:34:56".to_string(),
            state: MockDeviceState::Connected,
            flash_size: 4 * 1024 * 1024, // 4MB
            flash_memory: Arc::new(Mutex::new(vec![0xFF; 4 * 1024 * 1024])),
            response_delays: MockResponseDelays::default(),
            error_injection: MockErrorInjection::default(),
        }
    }

    /// Create a new mock ESP32 (original) device
    pub fn new_esp32() -> Self {
        Self {
            chip_type: "ESP32".to_string(),
            mac_address: "30:AE:A4:12:34:56".to_string(),
            state: MockDeviceState::Connected,
            flash_size: 4 * 1024 * 1024, // 4MB
            flash_memory: Arc::new(Mutex::new(vec![0xFF; 4 * 1024 * 1024])),
            response_delays: MockResponseDelays::default(),
            error_injection: MockErrorInjection::default(),
        }
    }

    /// Simulate connecting to the device
    pub fn connect(&mut self) -> Result<(), MockHardwareError> {
        std::thread::sleep(self.response_delays.connection_delay);

        if self.should_inject_error("connection") {
            self.state = MockDeviceState::Error("Connection failed".to_string());
            return Err(MockHardwareError::ConnectionFailed);
        }

        self.state = MockDeviceState::Connected;
        Ok(())
    }

    /// Simulate entering bootloader mode
    pub fn enter_bootloader(&mut self) -> Result<(), MockHardwareError> {
        if self.state != MockDeviceState::Connected {
            return Err(MockHardwareError::InvalidState);
        }

        std::thread::sleep(self.response_delays.bootloader_delay);
        self.state = MockDeviceState::BootloaderMode;
        Ok(())
    }

    /// Simulate flashing data to the device
    pub fn flash_data(&mut self, address: u32, data: &[u8]) -> Result<(), MockHardwareError> {
        if self.state != MockDeviceState::BootloaderMode {
            return Err(MockHardwareError::InvalidState);
        }

        if self.should_inject_error("flash") {
            self.state = MockDeviceState::Error("Flash failed".to_string());
            return Err(MockHardwareError::FlashFailed);
        }

        // Simulate flash timing
        let kb_count = (data.len() + 1023) / 1024;
        let flash_duration = self.response_delays.flash_delay_per_kb * kb_count as u32;
        std::thread::sleep(flash_duration);

        // Write data to mock flash memory
        let end_address = address as usize + data.len();
        if end_address > self.flash_size as usize {
            return Err(MockHardwareError::AddressOutOfRange);
        }

        let mut flash_memory = self.flash_memory.lock().unwrap();
        flash_memory[address as usize..end_address].copy_from_slice(data);

        self.state = MockDeviceState::Flashing;
        Ok(())
    }

    /// Simulate device reset
    pub fn reset(&mut self) -> Result<(), MockHardwareError> {
        std::thread::sleep(self.response_delays.reset_delay);
        self.state = MockDeviceState::Running;
        Ok(())
    }

    /// Get device information as JSON (simulates espflash info command)
    pub fn get_device_info(&self) -> Value {
        json!({
            "chip_type": self.chip_type,
            "mac_address": self.mac_address,
            "flash_size": self.flash_size,
            "state": format!("{:?}", self.state),
            "features": self.get_chip_features()
        })
    }

    /// Get chip-specific features
    fn get_chip_features(&self) -> Vec<&str> {
        match self.chip_type.as_str() {
            "ESP32-S3" => vec!["WiFi", "Bluetooth", "USB-OTG", "Camera"],
            "ESP32-C3" => vec!["WiFi", "Bluetooth 5.0", "RISC-V"],
            "ESP32" => vec!["WiFi", "Bluetooth"],
            _ => vec![],
        }
    }

    /// Check if an error should be injected based on configuration
    fn should_inject_error(&self, operation: &str) -> bool {
        let rate = match operation {
            "connection" => self.error_injection.connection_failure_rate,
            "flash" => self.error_injection.flash_failure_rate,
            _ => 0.0,
        };

        if rate <= 0.0 {
            return false;
        }

        // Simple pseudo-random based on operation and current time
        let pseudo_random = (operation.len() as f32 * 0.1) % 1.0;
        pseudo_random < rate
    }

    /// Configure error injection for testing
    pub fn configure_error_injection(&mut self, config: MockErrorInjection) {
        self.error_injection = config;
    }

    /// Configure response delays for testing
    pub fn configure_delays(&mut self, delays: MockResponseDelays) {
        self.response_delays = delays;
    }
}

/// Errors that can occur during mock hardware operations
#[derive(Debug, PartialEq)]
pub enum MockHardwareError {
    ConnectionFailed,
    InvalidState,
    FlashFailed,
    AddressOutOfRange,
    #[allow(dead_code)]
    TimeoutError,
    #[allow(dead_code)]
    ChecksumError,
}

impl std::fmt::Display for MockHardwareError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MockHardwareError::ConnectionFailed => write!(f, "Failed to connect to device"),
            MockHardwareError::InvalidState => {
                write!(f, "Device is in invalid state for operation")
            }
            MockHardwareError::FlashFailed => write!(f, "Flash operation failed"),
            MockHardwareError::AddressOutOfRange => write!(f, "Flash address out of range"),
            MockHardwareError::TimeoutError => write!(f, "Operation timed out"),
            MockHardwareError::ChecksumError => write!(f, "Checksum verification failed"),
        }
    }
}

impl std::error::Error for MockHardwareError {}

/// Mock serial port that simulates serial communication with ESP32
pub struct MockSerialPort {
    /// The mock device this port is connected to
    #[allow(dead_code)]
    pub device: Arc<Mutex<MockEsp32Device>>,
    /// Buffer for incoming data
    read_buffer: Vec<u8>,
    /// Buffer for outgoing data
    write_buffer: Vec<u8>,
    /// Port name (e.g., "/dev/cu.usbmodem14101")
    #[allow(dead_code)]
    pub port_name: String,
}

impl MockSerialPort {
    /// Create a new mock serial port connected to a device
    pub fn new(device: Arc<Mutex<MockEsp32Device>>, port_name: String) -> Self {
        Self {
            device,
            read_buffer: Vec::new(),
            write_buffer: Vec::new(),
            port_name,
        }
    }

    /// Simulate receiving data from the device
    pub fn simulate_device_response(&mut self, data: &[u8]) {
        self.read_buffer.extend_from_slice(data);
    }

    /// Get data written to the device
    pub fn get_written_data(&self) -> &[u8] {
        &self.write_buffer
    }

    /// Clear buffers
    pub fn clear_buffers(&mut self) {
        self.read_buffer.clear();
        self.write_buffer.clear();
    }
}

impl Read for MockSerialPort {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let bytes_to_read = buf.len().min(self.read_buffer.len());
        if bytes_to_read == 0 {
            return Ok(0);
        }

        buf[..bytes_to_read].copy_from_slice(&self.read_buffer[..bytes_to_read]);
        self.read_buffer.drain(..bytes_to_read);
        Ok(bytes_to_read)
    }
}

impl Write for MockSerialPort {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.write_buffer.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Mock hardware test environment that manages multiple devices and ports
pub struct MockHardwareEnvironment {
    /// Available mock devices
    pub devices: HashMap<String, Arc<Mutex<MockEsp32Device>>>,
    /// Available mock serial ports
    pub serial_ports: HashMap<String, MockSerialPort>,
    /// Temporary directory for test artifacts
    pub temp_dir: TempDir,
}

impl MockHardwareEnvironment {
    /// Create a new test environment with common ESP32 variants
    pub fn new() -> io::Result<Self> {
        let temp_dir = TempDir::new()?;
        let mut environment = Self {
            devices: HashMap::new(),
            serial_ports: HashMap::new(),
            temp_dir,
        };

        // Add common ESP32 devices
        environment.add_device("esp32s3", MockEsp32Device::new_esp32s3());
        environment.add_device("esp32c3", MockEsp32Device::new_esp32c3());
        environment.add_device("esp32", MockEsp32Device::new_esp32());

        Ok(environment)
    }

    /// Add a mock device to the environment
    pub fn add_device(&mut self, id: &str, device: MockEsp32Device) {
        let device_arc = Arc::new(Mutex::new(device));
        self.devices.insert(id.to_string(), device_arc.clone());

        // Create corresponding serial port
        let port_name = format!("/dev/cu.usbmodem{}", id);
        let serial_port = MockSerialPort::new(device_arc, port_name.clone());
        self.serial_ports.insert(port_name, serial_port);
    }

    /// Get a device by ID
    pub fn get_device(&self, id: &str) -> Option<Arc<Mutex<MockEsp32Device>>> {
        self.devices.get(id).cloned()
    }

    /// Get a serial port by name
    pub fn get_serial_port(&mut self, port_name: &str) -> Option<&mut MockSerialPort> {
        self.serial_ports.get_mut(port_name)
    }

    /// List available serial ports (simulates port discovery)
    pub fn list_serial_ports(&self) -> Vec<String> {
        self.serial_ports.keys().cloned().collect()
    }

    /// Create a test binary file for flashing
    pub fn create_test_binary(&self, name: &str, size: usize) -> io::Result<std::path::PathBuf> {
        let binary_path = self.temp_dir.path().join(format!("{}.bin", name));
        let test_data = vec![0xAA; size]; // Simple test pattern
        std::fs::write(&binary_path, test_data)?;
        Ok(binary_path)
    }

    /// Simulate espflash discover command
    pub fn simulate_discover(&self) -> Vec<Value> {
        self.devices
            .values()
            .map(|device| {
                let device = device.lock().unwrap();
                device.get_device_info()
            })
            .collect()
    }

    /// Configure error injection for all devices
    pub fn configure_global_error_injection(&mut self, config: MockErrorInjection) {
        for device in self.devices.values() {
            device
                .lock()
                .unwrap()
                .configure_error_injection(config.clone());
        }
    }

    /// Reset all devices to connected state
    pub fn reset_all_devices(&mut self) {
        for device in self.devices.values() {
            let mut device = device.lock().unwrap();
            device.state = MockDeviceState::Connected;
        }
    }
}

impl Default for MockHardwareEnvironment {
    fn default() -> Self {
        Self::new().expect("Failed to create mock hardware environment")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_device_creation() {
        let device = MockEsp32Device::new_esp32s3();
        assert_eq!(device.chip_type, "ESP32-S3");
        assert_eq!(device.flash_size, 16 * 1024 * 1024);
        assert_eq!(device.state, MockDeviceState::Connected);
    }

    #[test]
    fn test_device_connection_flow() {
        let mut device = MockEsp32Device::new_esp32();
        device.state = MockDeviceState::Disconnected;

        // Test connection
        assert!(device.connect().is_ok());
        assert_eq!(device.state, MockDeviceState::Connected);

        // Test bootloader entry
        assert!(device.enter_bootloader().is_ok());
        assert_eq!(device.state, MockDeviceState::BootloaderMode);
    }

    #[test]
    fn test_flash_operation() {
        let mut device = MockEsp32Device::new_esp32c3();
        device.state = MockDeviceState::BootloaderMode;

        let test_data = vec![0x12, 0x34, 0x56, 0x78];
        assert!(device.flash_data(0x1000, &test_data).is_ok());

        // Verify data was written to flash memory
        let flash_memory = device.flash_memory.lock().unwrap();
        assert_eq!(&flash_memory[0x1000..0x1004], &test_data);
    }

    #[test]
    fn test_error_injection() {
        let mut device = MockEsp32Device::new_esp32();
        device.configure_error_injection(MockErrorInjection {
            connection_failure_rate: 1.0, // Always fail
            ..Default::default()
        });

        device.state = MockDeviceState::Disconnected;
        assert!(device.connect().is_err());
        assert_eq!(
            device.state,
            MockDeviceState::Error("Connection failed".to_string())
        );
    }

    #[test]
    fn test_mock_environment() {
        let env = MockHardwareEnvironment::new().unwrap();

        // Test device access
        assert!(env.get_device("esp32s3").is_some());
        assert!(env.get_device("esp32c3").is_some());
        assert!(env.get_device("nonexistent").is_none());

        // Test port listing
        let ports = env.list_serial_ports();
        assert!(!ports.is_empty());
        assert!(ports.iter().any(|p| p.contains("esp32s3")));
    }

    #[test]
    fn test_device_info_json() {
        let device = MockEsp32Device::new_esp32s3();
        let info = device.get_device_info();

        assert_eq!(info["chip_type"], "ESP32-S3");
        assert_eq!(info["flash_size"], 16 * 1024 * 1024);
        assert!(
            info["features"]
                .as_array()
                .unwrap()
                .contains(&json!("WiFi"))
        );
    }
}
