//! USB backend for ESP32 device discovery
//!
//! Handles USB serial port discovery and ESP32 board detection

use crate::cluster::backends::BackendType;
use crate::cluster::messaging::DeviceStatus;
use crate::cluster::worker::LocalDevice;
use anyhow::Result;
use log::debug;
use serialport::SerialPortInfo;
use std::time::Duration;

/// USB device scanner
pub struct UsbScanner {
    /// Filter for ESP32 devices only
    esp32_only: bool,
}

impl Default for UsbScanner {
    fn default() -> Self {
        Self::new()
    }
}

impl UsbScanner {
    pub fn new() -> Self {
        Self {
            esp32_only: true,
        }
    }

    /// Create scanner that includes all USB devices
    pub fn all_devices() -> Self {
        Self {
            esp32_only: false,
        }
    }

    /// Scan for USB devices
    pub fn scan(&self) -> Result<Vec<LocalDevice>> {
        let ports = serialport::available_ports()
            .map_err(|e| anyhow::anyhow!("Failed to list serial ports: {}", e))?;

        let mut devices = Vec::new();

        for port in ports {
            if let Some(device) = self.inspect_port(&port)? {
                devices.push(device);
            }
        }

        debug!("USB scan found {} devices", devices.len());
        Ok(devices)
    }

    /// Inspect a serial port and create device info
    fn inspect_port(&self, port: &SerialPortInfo) -> Result<Option<LocalDevice>> {
        // Get port name
        let port_name = match &port.port_type {
            serialport::SerialPortType::UsbPort(info) => {
                // Check if this might be an ESP32 device
                if self.esp32_only {
                    // ESP32 devices typically have these VID/PID combinations
                    // FTDI: 0x0403
                    // CP210x: 0x10C4
                    // CH341: 0x1A86
                    // ESP32-S3 built-in USB: 0x303A (Espressif)
                    let vid = info.vid;

                    match vid {
                        0x0403 | 0x10C4 | 0x1A86 | 0x303A => {
                            // Known ESP32 USB converter VID
                            &port.port_name
                        }
                        _ => {
                            debug!("Skipping non-ESP32 USB device: {:?} (VID: {:04x})", port.port_name, vid);
                            return Ok(None);
                        }
                    }
                } else {
                    &port.port_name
                }
            }
            serialport::SerialPortType::PciPort => {
                // PCI ports are typically not ESP32
                if self.esp32_only {
                    return Ok(None);
                }
                &port.port_name
            }
            serialport::SerialPortType::Unknown => {
                // Unknown port type, include if not filtering
                if self.esp32_only {
                    return Ok(None);
                }
                &port.port_name
            }
            _ => {
                // Handle any other port types (BluetoothPort, etc.)
                if self.esp32_only {
                    return Ok(None);
                }
                &port.port_name
            }
        };

        // Try to detect board type by opening port and querying
        let board_type = self.detect_board_type(port_name)?;

        let device = LocalDevice {
            id: format!("usb-{}", port_name),
            backend: BackendType::USB {
                port: port_name.clone(),
            },
            board_type,
            location: port_name.clone(),
            capabilities: vec!["flash".to_string(), "monitor".to_string(), "reset".to_string()],
            logical_name: None,
            status: DeviceStatus::Available,
        };

        Ok(Some(device))
    }

    /// Detect board type by querying the device
    fn detect_board_type(&self, port: &str) -> Result<String> {
        // Try to open the port and detect the board
        // For now, we use a simple heuristic based on port name
        // A real implementation would try to communicate with the board

        // Common ESP32 board naming patterns
        if port.contains("USB") || port.contains("ACM") {
            // Try to open and send a basic detection command
            if let Ok(_) = self.try_detection(port) {
                return Ok("esp32s3".to_string());
            }
        }

        // Default to generic esp32
        Ok("esp32".to_string())
    }

    /// Try to detect ESP32 by sending a simple command
    fn try_detection(&self, port: &str) -> Result<()> {
        // Try to open port at common ESP32 baud rate
        let baud_rate = 115200;

        // Try with a short timeout
        let _port = serialport::new(port, baud_rate)
            .timeout(Duration::from_millis(100))
            .open()
            .map_err(|_| anyhow::anyhow!("Failed to open port"))?;

        // If we successfully opened, it's likely an ESP32
        // A real implementation would try to get chip info
        Ok(())
    }
}

/// Detect USB devices with ESP32 filter
pub fn detect_usb_devices() -> Result<Vec<LocalDevice>> {
    UsbScanner::new().scan()
}

/// Detect all USB serial devices (no filtering)
pub fn detect_all_usb_devices() -> Result<Vec<LocalDevice>> {
    UsbScanner::all_devices().scan()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scanner_creation() {
        let scanner = UsbScanner::new();
        assert!(scanner.esp32_only);

        let scanner = UsbScanner::all_devices();
        assert!(!scanner.esp32_only);
    }

    #[test]
    fn test_default_scanner() {
        let scanner = UsbScanner::default();
        assert!(scanner.esp32_only);
    }

    #[test]
    fn test_backend_type_usb() {
        let backend = BackendType::USB {
            port: "/dev/ttyUSB0".to_string(),
        };

        assert_eq!(backend.as_str(), "usb");
        assert_eq!(backend.location(), "/dev/ttyUSB0");
    }

    #[test]
    fn test_backend_type_from_str() {
        let usb = BackendType::from_str("usb:/dev/ttyUSB0");
        assert!(usb.is_some());
        assert_eq!(usb.unwrap(), BackendType::USB {
            port: "/dev/ttyUSB0".to_string()
        });

        let qemu = BackendType::from_str("qemu:esp32-instance-1");
        assert!(qemu.is_some());

        let wokwi = BackendType::from_str("wokwi:project-123");
        assert!(wokwi.is_some());

        let invalid = BackendType::from_str("invalid:value");
        assert!(invalid.is_none());
    }

    #[test]
    fn test_backend_type_qemu() {
        let backend = BackendType::QEMU {
            instance: "esp32-test".to_string(),
        };

        assert_eq!(backend.as_str(), "qemu");
        assert_eq!(backend.location(), "esp32-test");
    }

    #[test]
    fn test_backend_type_wokwi() {
        let backend = BackendType::Wokwi {
            project_id: "abc123".to_string(),
        };

        assert_eq!(backend.as_str(), "wokwi");
        assert_eq!(backend.location(), "abc123");
    }
}
