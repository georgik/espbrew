//! Hardware Integration Tests
//!
//! Tests espbrew's hardware interaction capabilities using mock ESP32 devices
//! to validate flashing operations, device discovery, and error handling.

mod mock_hardware;

use mock_hardware::*;
use serde_json::json;

/// Test ESP32 device discovery functionality
#[tokio::test]
async fn test_device_discovery() {
    let env = MockHardwareEnvironment::new().expect("Failed to create test environment");

    // Simulate device discovery
    let discovered_devices = env.simulate_discover();

    assert_eq!(
        discovered_devices.len(),
        3,
        "Should discover 3 mock devices"
    );

    // Verify ESP32-S3 device info
    let esp32s3_info = discovered_devices
        .iter()
        .find(|d| d["chip_type"] == "ESP32-S3")
        .expect("Should find ESP32-S3 device");

    assert_eq!(esp32s3_info["flash_size"], 16 * 1024 * 1024);
    assert!(
        esp32s3_info["features"]
            .as_array()
            .unwrap()
            .contains(&json!("WiFi"))
    );
    assert!(
        esp32s3_info["features"]
            .as_array()
            .unwrap()
            .contains(&json!("USB-OTG"))
    );
}

/// Test serial port discovery
#[test]
fn test_serial_port_discovery() {
    let env = MockHardwareEnvironment::new().expect("Failed to create test environment");

    let ports = env.list_serial_ports();
    assert_eq!(ports.len(), 3, "Should find 3 serial ports");

    // Check that all expected ports are present
    assert!(ports.iter().any(|p| p.contains("esp32s3")));
    assert!(ports.iter().any(|p| p.contains("esp32c3")));
    assert!(ports.iter().any(|p| p.contains("esp32")));
}

/// Test basic device connection flow
#[tokio::test]
async fn test_device_connection() {
    let env = MockHardwareEnvironment::new().expect("Failed to create test environment");

    let device = env
        .get_device("esp32s3")
        .expect("Should get ESP32-S3 device");
    let mut device = device.lock().unwrap();

    // Test initial connection
    device.state = MockDeviceState::Disconnected;
    assert!(device.connect().is_ok(), "Device connection should succeed");
    assert_eq!(device.state, MockDeviceState::Connected);

    // Test bootloader entry
    assert!(
        device.enter_bootloader().is_ok(),
        "Bootloader entry should succeed"
    );
    assert_eq!(device.state, MockDeviceState::BootloaderMode);

    // Test device reset
    assert!(device.reset().is_ok(), "Device reset should succeed");
    assert_eq!(device.state, MockDeviceState::Running);
}

/// Test flash operation with mock device
#[tokio::test]
async fn test_flash_operation() {
    let env = MockHardwareEnvironment::new().expect("Failed to create test environment");

    // Create test binary
    let test_binary = env
        .create_test_binary("test_app", 1024)
        .expect("Should create test binary");
    let binary_data = std::fs::read(&test_binary).expect("Should read test binary");

    let device = env
        .get_device("esp32c3")
        .expect("Should get ESP32-C3 device");
    let mut device = device.lock().unwrap();

    // Prepare device for flashing
    assert!(device.connect().is_ok());
    assert!(device.enter_bootloader().is_ok());

    // Perform flash operation
    let flash_address = 0x10000; // Typical app partition address
    assert!(
        device.flash_data(flash_address, &binary_data).is_ok(),
        "Flash operation should succeed"
    );
    assert_eq!(device.state, MockDeviceState::Flashing);

    // Verify data was written correctly
    let flash_memory = device.flash_memory.lock().unwrap();
    let written_data =
        &flash_memory[flash_address as usize..(flash_address as usize + binary_data.len())];
    assert_eq!(
        written_data, binary_data,
        "Flashed data should match original"
    );
}

/// Test flash operation error scenarios
#[tokio::test]
async fn test_flash_error_scenarios() {
    let env = MockHardwareEnvironment::new().expect("Failed to create test environment");

    let device = env.get_device("esp32").expect("Should get ESP32 device");
    let mut device = device.lock().unwrap();

    // Test flashing without entering bootloader mode
    assert!(device.connect().is_ok());
    let result = device.flash_data(0x10000, &[0xAA; 100]);
    assert!(result.is_err(), "Flash should fail without bootloader mode");
    assert_eq!(result.unwrap_err(), MockHardwareError::InvalidState);

    // Test out-of-range flash address
    assert!(device.enter_bootloader().is_ok());
    let flash_size = device.flash_size;
    let result = device.flash_data(flash_size - 50, &[0xAA; 100]);
    assert!(
        result.is_err(),
        "Flash should fail with out-of-range address"
    );
    assert_eq!(result.unwrap_err(), MockHardwareError::AddressOutOfRange);
}

/// Test error injection for connection failures
#[tokio::test]
async fn test_connection_error_injection() {
    let env = MockHardwareEnvironment::new().expect("Failed to create test environment");

    let device = env
        .get_device("esp32s3")
        .expect("Should get ESP32-S3 device");
    let mut device = device.lock().unwrap();

    // Configure 100% connection failure rate
    device.configure_error_injection(MockErrorInjection {
        connection_failure_rate: 1.0,
        ..Default::default()
    });

    device.state = MockDeviceState::Disconnected;
    let result = device.connect();
    assert!(
        result.is_err(),
        "Connection should fail with error injection"
    );
    assert_eq!(result.unwrap_err(), MockHardwareError::ConnectionFailed);
    assert!(matches!(device.state, MockDeviceState::Error(_)));
}

/// Test error injection for flash failures
#[tokio::test]
async fn test_flash_error_injection() {
    let env = MockHardwareEnvironment::new().expect("Failed to create test environment");

    let device = env
        .get_device("esp32c3")
        .expect("Should get ESP32-C3 device");
    let mut device = device.lock().unwrap();

    // Configure 100% flash failure rate
    device.configure_error_injection(MockErrorInjection {
        flash_failure_rate: 1.0,
        ..Default::default()
    });

    assert!(device.connect().is_ok());
    assert!(device.enter_bootloader().is_ok());

    let result = device.flash_data(0x10000, &[0xAA; 100]);
    assert!(result.is_err(), "Flash should fail with error injection");
    assert_eq!(result.unwrap_err(), MockHardwareError::FlashFailed);
    assert!(matches!(device.state, MockDeviceState::Error(_)));
}

/// Test multiple device management
#[tokio::test]
async fn test_multiple_device_management() {
    let mut env = MockHardwareEnvironment::new().expect("Failed to create test environment");

    // Add additional custom device
    let custom_device = MockEsp32Device::new_esp32s3();
    env.add_device("custom_esp32s3", custom_device);

    // Verify we now have 4 devices
    assert_eq!(env.devices.len(), 4);
    assert_eq!(env.serial_ports.len(), 4);

    // Test accessing different devices
    let device1 = env
        .get_device("esp32s3")
        .expect("Should get first ESP32-S3");
    let device2 = env
        .get_device("custom_esp32s3")
        .expect("Should get custom ESP32-S3");

    // Verify they are different instances
    {
        let mut dev1 = device1.lock().unwrap();
        let mut dev2 = device2.lock().unwrap();

        dev1.state = MockDeviceState::BootloaderMode;
        dev2.state = MockDeviceState::Running;

        assert_ne!(
            dev1.state, dev2.state,
            "Devices should have independent states"
        );
    }
}

/// Test response timing simulation
#[tokio::test]
async fn test_response_timing() {
    let env = MockHardwareEnvironment::new().expect("Failed to create test environment");

    let device = env.get_device("esp32").expect("Should get ESP32 device");
    let mut device = device.lock().unwrap();

    // Configure very fast response times for testing
    device.configure_delays(MockResponseDelays {
        connection_delay: std::time::Duration::from_millis(1),
        bootloader_delay: std::time::Duration::from_millis(1),
        flash_delay_per_kb: std::time::Duration::from_millis(1),
        reset_delay: std::time::Duration::from_millis(1),
    });

    device.state = MockDeviceState::Disconnected;

    let start = std::time::Instant::now();
    assert!(device.connect().is_ok());
    assert!(device.enter_bootloader().is_ok());
    assert!(device.flash_data(0x10000, &[0xAA; 1024]).is_ok()); // 1KB
    assert!(device.reset().is_ok());
    let elapsed = start.elapsed();

    // Should complete very quickly with fast timings
    assert!(
        elapsed.as_millis() < 50,
        "Operations should complete quickly with fast timings"
    );
}

/// Test device information accuracy
#[tokio::test]
async fn test_device_info_accuracy() {
    let env = MockHardwareEnvironment::new().expect("Failed to create test environment");

    // Test ESP32-S3 info
    let esp32s3_device = env.get_device("esp32s3").unwrap();
    let esp32s3 = esp32s3_device.lock().unwrap();
    let info = esp32s3.get_device_info();
    assert_eq!(info["chip_type"], "ESP32-S3");
    assert_eq!(info["flash_size"], 16 * 1024 * 1024);
    assert!(
        info["features"]
            .as_array()
            .unwrap()
            .contains(&json!("Camera"))
    );

    // Test ESP32-C3 info
    let esp32c3_device = env.get_device("esp32c3").unwrap();
    let esp32c3 = esp32c3_device.lock().unwrap();
    let info = esp32c3.get_device_info();
    assert_eq!(info["chip_type"], "ESP32-C3");
    assert_eq!(info["flash_size"], 4 * 1024 * 1024);
    assert!(
        info["features"]
            .as_array()
            .unwrap()
            .contains(&json!("RISC-V"))
    );
    drop(esp32c3);

    // Test original ESP32 info
    let esp32_device = env.get_device("esp32").unwrap();
    let esp32 = esp32_device.lock().unwrap();
    let info = esp32.get_device_info();
    assert_eq!(info["chip_type"], "ESP32");
    assert_eq!(info["flash_size"], 4 * 1024 * 1024);
    let features = info["features"].as_array().unwrap();
    assert!(features.contains(&json!("WiFi")));
    assert!(features.contains(&json!("Bluetooth")));
    assert!(!features.contains(&json!("USB-OTG"))); // Original ESP32 doesn't have USB-OTG
}

/// Test binary file creation and management
#[tokio::test]
async fn test_binary_file_management() {
    let env = MockHardwareEnvironment::new().expect("Failed to create test environment");

    // Create binaries of different sizes
    let small_binary = env
        .create_test_binary("small_app", 512)
        .expect("Should create small binary");
    let large_binary = env
        .create_test_binary("large_app", 64 * 1024)
        .expect("Should create large binary");

    // Verify files exist and have correct sizes
    assert!(small_binary.exists());
    assert!(large_binary.exists());

    let small_data = std::fs::read(&small_binary).expect("Should read small binary");
    let large_data = std::fs::read(&large_binary).expect("Should read large binary");

    assert_eq!(small_data.len(), 512);
    assert_eq!(large_data.len(), 64 * 1024);
    assert_eq!(small_data, vec![0xAA; 512]); // Test pattern
    assert_eq!(large_data, vec![0xAA; 64 * 1024]); // Test pattern
}

/// Test global error injection configuration
#[tokio::test]
async fn test_global_error_injection() {
    let mut env = MockHardwareEnvironment::new().expect("Failed to create test environment");

    // Configure global error injection
    env.configure_global_error_injection(MockErrorInjection {
        connection_failure_rate: 1.0,
        flash_failure_rate: 1.0,
        ..Default::default()
    });

    // Test that all devices now have error injection
    for (id, device) in &env.devices {
        let mut device = device.lock().unwrap();
        device.state = MockDeviceState::Disconnected;

        let result = device.connect();
        assert!(result.is_err(), "Device {} should fail to connect", id);
    }
}

/// Test device state reset functionality
#[tokio::test]
async fn test_device_reset_functionality() {
    let mut env = MockHardwareEnvironment::new().expect("Failed to create test environment");

    // Set all devices to error state
    for device in env.devices.values() {
        let mut device = device.lock().unwrap();
        device.state = MockDeviceState::Error("Test error".to_string());
    }

    // Reset all devices
    env.reset_all_devices();

    // Verify all devices are back to connected state
    for device in env.devices.values() {
        let device = device.lock().unwrap();
        assert_eq!(device.state, MockDeviceState::Connected);
    }
}

/// Test mock serial port functionality
#[tokio::test]
async fn test_mock_serial_port() {
    let mut env = MockHardwareEnvironment::new().expect("Failed to create test environment");

    let port_name = "/dev/cu.usbmodemesp32s3".to_string();
    let serial_port = env
        .get_serial_port(&port_name)
        .expect("Should get serial port");

    // Test writing data to the port
    use std::io::Write;
    serial_port
        .write_all(b"Hello ESP32")
        .expect("Should write data");
    assert_eq!(serial_port.get_written_data(), b"Hello ESP32");

    // Test simulating device response
    serial_port.simulate_device_response(b"ESP32 Ready");

    // Test reading data from the port
    use std::io::Read;
    let mut buffer = [0u8; 32];
    let bytes_read = serial_port.read(&mut buffer).expect("Should read data");
    assert_eq!(bytes_read, 11); // "ESP32 Ready" length
    assert_eq!(&buffer[..bytes_read], b"ESP32 Ready");

    // Test buffer clearing
    serial_port.clear_buffers();
    assert_eq!(serial_port.get_written_data().len(), 0);
}

/// Benchmark flash operation performance
#[tokio::test]
async fn test_flash_performance_simulation() {
    let env = MockHardwareEnvironment::new().expect("Failed to create test environment");

    let device = env
        .get_device("esp32s3")
        .expect("Should get ESP32-S3 device");
    let mut device = device.lock().unwrap();

    // Prepare device
    assert!(device.connect().is_ok());
    assert!(device.enter_bootloader().is_ok());

    // Test flashing different sizes and measure timing
    let test_cases = vec![(1024, "1KB"), (16 * 1024, "16KB"), (64 * 1024, "64KB")];

    for (i, (size, description)) in test_cases.into_iter().enumerate() {
        let test_data = vec![0xBB; size];
        let start = std::time::Instant::now();
        let flash_address = 0x20000 + (i * 0x20000) as u32; // Use different addresses

        assert!(
            device.flash_data(flash_address, &test_data).is_ok(),
            "Should flash {} successfully",
            description
        );

        let elapsed = start.elapsed();
        println!("Flashing {} took {:?}", description, elapsed);

        // Reset device state for next operation
        device.state = MockDeviceState::BootloaderMode;

        // Verify realistic timing (should take at least some time based on our delays)
        let expected_min_time =
            device.response_delays.flash_delay_per_kb * ((size + 1023) / 1024) as u32;
        assert!(
            elapsed >= expected_min_time,
            "Flash timing should respect configured delays"
        );
    }
}
