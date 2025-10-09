//! Unit tests for TUI functionality

use super::main_app::App;
use crate::BuildStrategy;
use crate::models::server::DiscoveredServer;
use std::net::{IpAddr, Ipv4Addr};
use tokio::sync::mpsc;

#[tokio::test]
async fn test_server_discovery_initialization() {
    // Create a temporary directory for testing
    let temp_dir = std::env::temp_dir().join("espbrew_test");
    std::fs::create_dir_all(&temp_dir).unwrap();

    let app = App::new(
        temp_dir.clone(),
        BuildStrategy::Sequential,
        None,
        None,
        None,
    )
    .unwrap();

    // Check initial server discovery state
    assert!(!app.server_discovery_in_progress);
    assert_eq!(app.server_discovery_status, "Ready to discover servers...");
    assert!(app.discovered_servers.is_empty());

    // Clean up
    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[tokio::test]
async fn test_mdns_discovery_integration() {
    // This test verifies that the mDNS discovery function runs without error
    // In a real environment without ESPBrew servers, it should return an empty vector
    let result = crate::remote::discovery::discover_espbrew_servers_silent(1).await;

    // The function should succeed (no mDNS errors) even if no servers are found
    assert!(
        result.is_ok(),
        "mDNS discovery should not fail: {:?}",
        result
    );

    let servers = result.unwrap();
    // In a typical test environment, no ESPBrew servers should be found
    // This test validates that the discovery completes successfully
    println!("mDNS discovery found {} servers", servers.len());

    // Verify each discovered server has valid data structure
    for server in servers {
        assert!(!server.name.is_empty(), "Server name should not be empty");
        assert!(server.port > 0, "Server port should be valid");
    }
}

#[tokio::test]
async fn test_server_discovery_timeout() {
    // Test that discovery completes within a reasonable timeout
    let start = std::time::Instant::now();
    let result = crate::remote::discovery::discover_espbrew_servers_silent(2).await;
    let elapsed = start.elapsed();

    assert!(result.is_ok(), "Discovery should complete successfully");
    assert!(
        elapsed.as_secs() >= 2 && elapsed.as_secs() < 4,
        "Discovery should take approximately 2 seconds, took {:?}",
        elapsed
    );
}

#[tokio::test]
async fn test_server_discovery_state_transitions() {
    // Create a temporary directory for testing
    let temp_dir = std::env::temp_dir().join("espbrew_test_discovery");
    std::fs::create_dir_all(&temp_dir).unwrap();

    let mut app = App::new(
        temp_dir.clone(),
        BuildStrategy::Sequential,
        None,
        None,
        None,
    )
    .unwrap();

    // Create event channel
    let (tx, mut rx) = mpsc::unbounded_channel();

    // Start discovery
    app.start_server_discovery(tx.clone());

    // Check that discovery started
    assert!(app.server_discovery_in_progress);
    assert_eq!(app.server_discovery_status, "Discovering servers...");

    // Test successful discovery completion
    let test_servers = vec![DiscoveredServer {
        name: "test-server".to_string(),
        ip: IpAddr::V4(Ipv4Addr::new(192, 168, 1, 100)),
        port: 8080,
        hostname: "test-host".to_string(),
        version: "1.0.0".to_string(),
        description: "Test ESPBrew Server".to_string(),
        board_count: 2,
        boards_list: "esp32,esp32s3".to_string(),
    }];

    app.handle_server_discovery_completed(test_servers.clone());

    // Check post-completion state
    assert!(!app.server_discovery_in_progress);
    assert_eq!(app.discovered_servers.len(), 1);
    assert_eq!(
        app.server_discovery_status,
        "Found 1 server(s): test-server"
    );

    // Test that get_server_url returns discovered server
    let expected_url = "http://192.168.1.100:8080";
    assert_eq!(app.get_server_url(), expected_url);

    // Clean up
    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[tokio::test]
async fn test_server_discovery_failure() {
    // Create a temporary directory for testing
    let temp_dir = std::env::temp_dir().join("espbrew_test_failure");
    std::fs::create_dir_all(&temp_dir).unwrap();

    let mut app = App::new(
        temp_dir.clone(),
        BuildStrategy::Sequential,
        None,
        None,
        None,
    )
    .unwrap();

    // Create event channel
    let (tx, mut rx) = mpsc::unbounded_channel();

    // Start discovery
    app.start_server_discovery(tx.clone());

    // Simulate discovery failure
    let error_msg = "Network interface not available".to_string();
    app.handle_server_discovery_failed(error_msg.clone());

    // Check post-failure state
    assert!(!app.server_discovery_in_progress);
    assert!(app.discovered_servers.is_empty());
    assert_eq!(
        app.server_discovery_status,
        format!("Discovery failed: {}", error_msg)
    );

    // Test that get_server_url falls back to default
    assert_eq!(app.get_server_url(), "http://localhost:8080");

    // Clean up
    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[tokio::test]
async fn test_server_discovery_no_servers_found() {
    // Create a temporary directory for testing
    let temp_dir = std::env::temp_dir().join("espbrew_test_no_servers");
    std::fs::create_dir_all(&temp_dir).unwrap();

    let mut app = App::new(
        temp_dir.clone(),
        BuildStrategy::Sequential,
        None,
        None,
        None,
    )
    .unwrap();

    // Create event channel
    let (tx, mut rx) = mpsc::unbounded_channel();

    // Start discovery
    app.start_server_discovery(tx.clone());

    // Simulate no servers found (empty vector)
    app.handle_server_discovery_completed(vec![]);

    // Check post-completion state with no servers
    assert!(!app.server_discovery_in_progress);
    assert!(app.discovered_servers.is_empty());
    assert_eq!(app.server_discovery_status, "No servers found");

    // Clean up
    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[tokio::test]
async fn test_remote_board_fetching_with_discovered_server() {
    // Create a temporary directory for testing
    let temp_dir = std::env::temp_dir().join("espbrew_test_remote_fetch");
    std::fs::create_dir_all(&temp_dir).unwrap();

    let mut app = App::new(
        temp_dir.clone(),
        BuildStrategy::Sequential,
        None,
        None,
        None,
    )
    .unwrap();

    // Set up discovered server
    let test_server = DiscoveredServer {
        name: "test-server".to_string(),
        ip: IpAddr::V4(Ipv4Addr::new(192, 168, 1, 100)),
        port: 8080,
        hostname: "test-host".to_string(),
        version: "1.0.0".to_string(),
        description: "Test ESPBrew Server".to_string(),
        board_count: 1,
        boards_list: "esp32".to_string(),
    };

    app.handle_server_discovery_completed(vec![test_server]);

    // Test that remote board fetching uses discovered server URL
    let expected_server_url = "http://192.168.1.100:8080";
    assert_eq!(app.get_server_url(), expected_server_url);

    // Clean up
    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_app_initialization_without_project_handler() {
    // Create a temporary directory for testing
    let temp_dir = std::env::temp_dir().join("espbrew_test_init");
    std::fs::create_dir_all(&temp_dir).unwrap();

    let app = App::new(
        temp_dir.clone(),
        BuildStrategy::Sequential,
        Some("http://custom:9090".to_string()),
        Some("AA:BB:CC:DD:EE:FF".to_string()),
        None,
    )
    .unwrap();

    // Check that custom server URL and board MAC are preserved
    assert_eq!(app.server_url, Some("http://custom:9090".to_string()));
    assert_eq!(app.board_mac, Some("AA:BB:CC:DD:EE:FF".to_string()));

    // Check that get_server_url returns custom URL when no servers discovered
    assert_eq!(app.get_server_url(), "http://custom:9090");

    // Clean up
    let _ = std::fs::remove_dir_all(&temp_dir);
}
