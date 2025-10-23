//! Boards command implementation - List connected USB boards

use anyhow::Result;
use log::{debug, info};

/// Execute the boards command to list connected USB serial ports
pub async fn execute_boards_command() -> Result<()> {
    info!("Scanning for connected USB serial ports...");

    // Use serialport to discover serial ports
    let ports = serialport::available_ports()?;

    if ports.is_empty() {
        println!("⚠️  No serial ports detected");
        return Ok(());
    }

    println!("🔍 Detected Serial Ports:");
    println!("========================\n");

    for port_info in &ports {
        println!("Port: {}", port_info.port_name);

        match &port_info.port_type {
            serialport::SerialPortType::UsbPort(usb_info) => {
                println!("  Type: USB Serial Port");
                println!("  VID:  0x{:04X}", usb_info.vid);
                println!("  PID:  0x{:04X}", usb_info.pid);

                if let Some(ref manufacturer) = usb_info.manufacturer {
                    println!("  Manufacturer: {}", manufacturer);
                }

                if let Some(ref product) = usb_info.product {
                    println!("  Product: {}", product);
                }

                if let Some(ref serial) = usb_info.serial_number {
                    println!("  Serial: {}", serial);
                }

                // Check if it looks like an ESP32 device
                let is_esp_vid = matches!(
                    usb_info.vid,
                    0x303A |  // Espressif Systems
                    0x1001 |  // Some ESP32 boards with alternate VID
                    0x10C4 |  // Silicon Labs CP210x
                    0x0403 |  // FTDI
                    0x1A86 |  // WCH CH340/CH341
                    0x067B // Prolific PL2303
                );

                if is_esp_vid {
                    println!("  ✅ Likely ESP32 device");
                } else {
                    debug!(
                        "Port {} does not match known ESP32 VID",
                        port_info.port_name
                    );
                }
            }
            serialport::SerialPortType::PciPort => {
                println!("  Type: PCI Serial Port");
            }
            serialport::SerialPortType::BluetoothPort => {
                println!("  Type: Bluetooth Serial Port");
            }
            serialport::SerialPortType::Unknown => {
                println!("  Type: Unknown");
            }
        }

        println!();
    }

    println!("Total ports detected: {}", ports.len());

    Ok(())
}
