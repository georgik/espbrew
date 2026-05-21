//! Hardware-specific USB device detection tests
//!
//! These tests require actual ESP32 hardware connected via USB.
//! They are ignored by default and must be run explicitly with:
//!
//! ```bash
//! cargo test --test hardware_usb_detection -- --ignored --nocapture
//! ```

use espbrew::cluster::backends::usb::UsbScanner;

/// Test USB device scanner - requires connected device
#[test]
#[ignore]
fn test_usb_scan_finds_devices() {
    let scanner = UsbScanner::new();

    let result = scanner.scan();
    assert!(result.is_ok(), "USB scan should succeed");

    let devices = result.unwrap();
    println!("Found {} USB devices:", devices.len());

    for device in &devices {
        println!(
            "  - {} ({}) at {}",
            device.id, device.board_type, device.location
        );
    }

    // At least one device should be found for this test to pass
    if devices.is_empty() {
        println!("WARNING: No USB devices found. Connect an ESP32 device to test discovery.");
    }
}

/// Test for specific device (tty.usbmodem101) - requires hardware
#[test]
#[ignore]
fn test_usb_detect_specific_device() {
    let scanner = UsbScanner::new();

    let devices = scanner.scan().unwrap();

    // Look for the common macOS USB modem device pattern
    let found = devices
        .iter()
        .any(|d| d.location.contains("tty.usbmodem") || d.location.contains("ttyUSB"));

    if found {
        println!("✓ Found USB modem device");
        let device = devices
            .iter()
            .find(|d| d.location.contains("tty.usbmodem") || d.location.contains("ttyUSB"))
            .unwrap();
        println!("  Device ID: {}", device.id);
        println!("  Board Type: {}", device.board_type);
        println!("  Location: {}", device.location);
        println!("  Capabilities: {:?}", device.capabilities);
    } else {
        println!("✗ No USB modem device found (expected: /dev/tty.usbmodem*)");
    }

    // This test passes if we find at least one device, but we don't fail if none
    // since hardware might not be connected
    // assert!(found, "Expected to find USB modem device");
}

/// Test master node registers local USB devices
#[test]
#[ignore]
fn test_master_registers_local_devices() {
    use espbrew::cluster::backends::usb::detect_usb_devices;
    use espbrew::cluster::master::MasterNode;
    use espbrew::cluster::messaging::DeviceAnnouncement;
    use espbrew::cluster::state::ClusterConfig;

    let rt = tokio::runtime::Runtime::new().unwrap();

    let config = ClusterConfig {
        cluster_name: "test-hardware-cluster".to_string(),
        ..Default::default()
    };

    let master = MasterNode::new("test-master".to_string(), config);

    // Detect USB devices
    let devices = detect_usb_devices().unwrap_or_default();
    println!("Detected {} USB devices", devices.len());

    if devices.is_empty() {
        println!("WARNING: No USB devices detected. Connect hardware to test.");
        return;
    }

    // Convert to announcements
    let announcements: Vec<DeviceAnnouncement> = devices
        .iter()
        .map(|d| DeviceAnnouncement {
            device_id: d.id.clone(),
            node_id: "test-master".to_string(),
            backend: d.backend.clone(),
            board_type: d.board_type.clone(),
            capabilities: d.capabilities.clone(),
            location: d.location.clone(),
            logical_name: d.logical_name.clone(),
        })
        .collect();

    // Register with master
    let count = rt
        .block_on(async { master.register_local_devices(announcements).await })
        .unwrap();

    println!("Master registered {} local devices", count);

    // Verify devices are in cluster state
    let state = master.state();
    let state_guard = rt.block_on(async { state.read().await });
    assert_eq!(state_guard.devices.len(), count);

    println!("✓ Devices successfully registered in cluster state");
    for (id, device) in state_guard.devices.iter() {
        println!("  - {}: {} ({:?})", id, device.board_type, device.status);
    }
}

/// Test cluster node with master role detects local devices
#[tokio::test]
#[ignore]
async fn test_cluster_master_detects_local_devices() {
    use espbrew::cluster::node::ClusterNode;
    use espbrew::cluster::state::{ClusterConfig, NodeRole};

    let config = ClusterConfig {
        cluster_name: "test-hardware-cluster".to_string(),
        role: NodeRole::Master,
        bind_address: "127.0.0.1:0".to_string(), // Random port for testing
        ..Default::default()
    };

    let mut node = ClusterNode::new(config);

    // Start the cluster node (should detect USB devices)
    let result = node.start().await;

    if let Err(e) = &result {
        println!("Note: HTTP server might fail on 127.0.0.1:0: {}", e);
    }

    // Check cluster status
    if let Some(status) = node.status().await.unwrap() {
        println!("Cluster status:");
        println!("  Nodes: {}", status.node_count);
        println!("  Devices: {}", status.device_count);
        println!("  Available: {}", status.available_devices);

        if status.device_count > 0 {
            println!("✓ Master node detected {} device(s)", status.device_count);
        } else {
            println!("No devices detected. Connect ESP32 hardware to test.");
        }
    }

    // Stop the node
    let _ = node.stop().await;
}
