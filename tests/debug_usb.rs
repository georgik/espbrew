//! Debug USB device detection

use serialport::SerialPortType;

#[test]
fn debug_list_serial_ports() {
    let ports = serialport::available_ports().unwrap();

    println!("Found {} serial ports:", ports.len());

    for port in &ports {
        println!("\nPort: {}", port.port_name);
        match &port.port_type {
            SerialPortType::UsbPort(usb) => {
                println!("  Type: USB");
                println!("  VID: 0x{:04x}", usb.vid);
                println!("  PID: 0x{:04x}", usb.pid);
                println!("  Manufacturer: {:?}", usb.manufacturer);
                println!("  Product: {:?}", usb.product);
                println!("  Serial: {:?}", usb.serial_number);
            }
            SerialPortType::PciPort => println!("  Type: PCI"),
            SerialPortType::BluetoothPort => println!("  Type: Bluetooth"),
            SerialPortType::Unknown => println!("  Type: Unknown"),
        }
    }
}
