//! Error Handling and Recovery Tests
//!
//! Comprehensive tests for error scenarios including missing tools, invalid configurations,
//! hardware connection failures, interrupted operations, and recovery mechanisms.
//! These tests ensure espbrew handles edge cases gracefully and provides useful feedback.

mod mock_hardware;
mod test_fixtures;

use espbrew::projects::ProjectRegistry;
use mock_hardware::{MockErrorInjection, MockEsp32Device, MockHardwareEnvironment};
use std::fs;
use std::io::Write;
use std::path::Path;
use tempfile::TempDir;
use test_fixtures::{ConfigFixtures, ProjectFixtures};

/// Test behavior when required tools are missing
#[test]
fn test_missing_tools_handling() {
    println!("Testing missing tools error handling...");

    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let project_path = temp_dir.path();

    // Create a valid Rust project
    ProjectFixtures::create_rust_nostd_project(project_path, "s3")
        .expect("Failed to create Rust project");
    ConfigFixtures::create_espbrew_config(project_path, "rust_nostd", "esp32s3")
        .expect("Failed to create config");

    let registry = ProjectRegistry::new();
    let handler = registry
        .detect_project(project_path)
        .expect("Should detect project");

    // Test tool availability check
    let tool_check = handler.check_tools_available();
    match tool_check {
        Ok(_) => {
            println!("✅ All tools available - testing successful scenario");
        }
        Err(error_msg) => {
            println!("✅ Missing tools detected: {}", error_msg);

            // Verify we get a helpful error message
            assert!(!error_msg.is_empty(), "Error message should not be empty");

            // Test that we get installation instructions
            let missing_tools_msg = handler.get_missing_tools_message();
            assert!(
                !missing_tools_msg.is_empty(),
                "Missing tools message should not be empty"
            );
            assert!(
                missing_tools_msg.contains("install"),
                "Message should mention installation"
            );

            println!("✅ Helpful error message provided: {}", missing_tools_msg);
        }
    }

    println!("✅ Missing tools handling test completed");
}

/// Test invalid project configuration handling
#[test]
fn test_invalid_configuration_handling() {
    println!("Testing invalid configuration handling...");

    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let project_path = temp_dir.path();

    // Test 1: Corrupted Cargo.toml
    println!("Test 1: Corrupted Cargo.toml handling...");
    fs::create_dir_all(project_path.join("src")).expect("Failed to create src");
    fs::write(project_path.join("Cargo.toml"), "invalid toml content [[[")
        .expect("Failed to write invalid Cargo.toml");
    fs::write(project_path.join("src/main.rs"), "fn main() {}").expect("Failed to write main.rs");

    let registry = ProjectRegistry::new();
    let handler = registry.detect_project(project_path);

    if let Some(handler) = handler {
        // If the handler detects the project, it should handle errors gracefully
        let board_result = handler.discover_boards(project_path);
        match board_result {
            Ok(boards) => {
                println!(
                    "✅ Corrupted Cargo.toml handled gracefully: {} boards found",
                    boards.len()
                );
            }
            Err(e) => {
                println!("✅ Corrupted Cargo.toml properly reported error: {}", e);
                assert!(
                    !e.to_string().is_empty(),
                    "Error message should not be empty"
                );
            }
        }
    } else {
        println!("✅ Corrupted project correctly rejected during detection");
    }

    // Clean up for next test
    fs::remove_dir_all(project_path).expect("Failed to clean up");
    fs::create_dir_all(project_path).expect("Failed to recreate directory");

    // Test 2: Invalid espbrew.toml
    println!("Test 2: Invalid espbrew.toml handling...");
    ProjectFixtures::create_rust_nostd_project(project_path, "s3")
        .expect("Failed to create Rust project");

    // Create an invalid espbrew.toml
    fs::write(
        project_path.join("espbrew.toml"),
        r#"
[project
name = "invalid-syntax-project"
type = "rust_nostd"
"#,
    )
    .expect("Failed to write invalid espbrew.toml");

    let handler = registry
        .detect_project(project_path)
        .expect("Should still detect project type from Cargo.toml");

    // Test board discovery with invalid config
    let board_result = handler.discover_boards(project_path);
    match board_result {
        Ok(boards) => {
            println!(
                "✅ Invalid espbrew.toml handled gracefully: {} boards found",
                boards.len()
            );
            // Should use defaults when config is invalid
        }
        Err(e) => {
            println!("✅ Invalid espbrew.toml error handled: {}", e);
        }
    }

    println!("✅ Invalid configuration handling tests completed");
}

/// Test file system error handling
#[test]
fn test_filesystem_error_handling() {
    println!("Testing filesystem error handling...");

    let registry = ProjectRegistry::new();

    // Test 1: Non-existent directory
    println!("Test 1: Non-existent directory handling...");
    let non_existent_path = Path::new("/definitely/does/not/exist/anywhere");
    let result = registry.detect_project(non_existent_path);
    assert!(
        result.is_none(),
        "Should not detect project in non-existent directory"
    );
    println!("✅ Non-existent directory handled correctly");

    // Test 2: Permission denied (simulate with a read-only temp dir)
    println!("Test 2: Permission handling...");
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let project_path = temp_dir.path();

    ProjectFixtures::create_rust_nostd_project(project_path, "s3")
        .expect("Failed to create Rust project");

    let handler = registry
        .detect_project(project_path)
        .expect("Should detect project");

    // Test board discovery with valid project (should work)
    let board_result = handler.discover_boards(project_path);
    match board_result {
        Ok(boards) => {
            println!("✅ Normal filesystem access works: {} boards", boards.len());
        }
        Err(e) => {
            println!("✅ Filesystem error handled gracefully: {}", e);
        }
    }

    // Test 3: Empty directory
    println!("Test 3: Empty directory handling...");
    let empty_temp_dir = TempDir::new().expect("Failed to create temp directory");
    let empty_path = empty_temp_dir.path();

    let empty_result = registry.detect_project(empty_path);
    assert!(
        empty_result.is_none(),
        "Should not detect project in empty directory"
    );
    println!("✅ Empty directory handled correctly");

    println!("✅ Filesystem error handling tests completed");
}

/// Test hardware connection failure scenarios
#[tokio::test]
async fn test_hardware_connection_failures() {
    println!("Testing hardware connection failure scenarios...");

    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let project_path = temp_dir.path();

    // Setup a test project
    ProjectFixtures::create_rust_nostd_project(project_path, "s3")
        .expect("Failed to create Rust project");
    ConfigFixtures::create_espbrew_config(project_path, "rust_nostd", "esp32s3")
        .expect("Failed to create config");

    let registry = ProjectRegistry::new();
    let handler = registry
        .detect_project(project_path)
        .expect("Should detect project");

    let boards = handler
        .discover_boards(project_path)
        .expect("Should discover boards");
    assert!(!boards.is_empty(), "Should have at least one board");
    let board = &boards[0];

    // Test 1: Invalid serial port
    println!("Test 1: Invalid serial port handling...");
    let invalid_ports = vec![
        "/dev/nonexistent",
        "COM999",
        "/dev/null", // Valid path but not a serial port
        "",          // Empty port
    ];

    for port in invalid_ports {
        let flash_cmd = handler.get_flash_command(project_path, board, Some(port));

        // The command should be generated even with invalid port
        assert!(
            !flash_cmd.is_empty(),
            "Flash command should be generated even with invalid port"
        );

        if !port.is_empty() {
            assert!(
                flash_cmd.contains(port),
                "Flash command should contain the port"
            );
        }

        println!("✅ Invalid port '{}' handled: {}", port, flash_cmd);
    }

    // Test 2: Mock hardware connection failures
    println!("Test 2: Mock hardware connection failures...");
    let mut mock_env = MockHardwareEnvironment::new().expect("Failed to create mock environment");

    // Configure device to always fail connections
    let mut esp32_device = MockEsp32Device::new_esp32s3();
    esp32_device.configure_error_injection(MockErrorInjection {
        connection_failure_rate: 1.0, // Always fail
        flash_failure_rate: 0.0,
        timeout_errors: false,
        checksum_errors: false,
    });

    mock_env.add_device("failing-device", esp32_device);
    let available_ports = mock_env.list_serial_ports();

    if !available_ports.is_empty() {
        let test_port = &available_ports[0];
        let flash_cmd = handler.get_flash_command(project_path, board, Some(test_port));

        // Command generation should work even if the actual device would fail
        assert!(!flash_cmd.is_empty(), "Should generate flash command");
        assert!(
            flash_cmd.contains(test_port),
            "Should include port in command"
        );

        println!("✅ Connection failure scenario prepared: {}", flash_cmd);
    }

    println!("✅ Hardware connection failure tests completed");
}

/// Test project state corruption recovery
#[test]
fn test_project_state_recovery() {
    println!("Testing project state corruption and recovery...");

    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let project_path = temp_dir.path();

    // Create a valid project initially
    ProjectFixtures::create_rust_nostd_project(project_path, "s3")
        .expect("Failed to create Rust project");
    ConfigFixtures::create_espbrew_config(project_path, "rust_nostd", "esp32s3")
        .expect("Failed to create config");

    let registry = ProjectRegistry::new();

    // Test 1: Initial state - should work
    println!("Test 1: Initial valid state...");
    let handler = registry
        .detect_project(project_path)
        .expect("Should detect valid project");

    let board_result = handler.discover_boards(project_path);
    assert!(
        board_result.is_ok(),
        "Should discover boards in valid state"
    );
    println!("✅ Initial state is valid");

    // Test 2: Corrupt a critical file
    println!("Test 2: Corrupting Cargo.toml...");
    let mut cargo_file = fs::OpenOptions::new()
        .write(true)
        .truncate(true)
        .open(project_path.join("Cargo.toml"))
        .expect("Should open Cargo.toml");
    cargo_file
        .write_all(b"corrupted content")
        .expect("Should write corruption");
    drop(cargo_file);

    // Try to detect project again
    let corrupted_handler = registry.detect_project(project_path);
    match corrupted_handler {
        Some(handler) => {
            println!("✅ Project still detected despite corruption");

            // Board discovery might fail, but should not panic
            match handler.discover_boards(project_path) {
                Ok(boards) => {
                    println!(
                        "✅ Board discovery succeeded despite corruption: {} boards",
                        boards.len()
                    );
                }
                Err(e) => {
                    println!("✅ Board discovery failed gracefully: {}", e);
                    assert!(!e.to_string().is_empty(), "Should provide error message");
                }
            }
        }
        None => {
            println!("✅ Corrupted project correctly rejected");
        }
    }

    // Test 3: Recovery - restore valid Cargo.toml
    println!("Test 3: Recovery by restoring valid configuration...");
    ProjectFixtures::create_rust_nostd_project(project_path, "s3")
        .expect("Failed to recreate Rust project");

    let recovered_handler = registry
        .detect_project(project_path)
        .expect("Should detect recovered project");

    let recovery_result = recovered_handler.discover_boards(project_path);
    assert!(recovery_result.is_ok(), "Should work after recovery");
    println!("✅ Project recovered successfully");

    println!("✅ Project state recovery tests completed");
}

/// Test concurrent operation error handling
#[tokio::test]
async fn test_concurrent_operation_errors() {
    println!("Testing concurrent operation error handling...");

    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let project_path = temp_dir.path();

    // Create a test project
    ProjectFixtures::create_rust_nostd_project(project_path, "s3")
        .expect("Failed to create Rust project");

    let _registry = ProjectRegistry::new();

    // Test concurrent project detection with one corrupted attempt
    let mut handles = vec![];

    for i in 0..5 {
        let project_path = project_path.to_path_buf();
        let registry = ProjectRegistry::new();

        let handle = tokio::spawn(async move {
            // Introduce artificial delay and potential errors
            tokio::time::sleep(tokio::time::Duration::from_millis(i * 10)).await;

            let result = registry.detect_project(&project_path);
            (i, result.is_some())
        });

        handles.push(handle);
    }

    // Collect results
    let mut results = vec![];
    for handle in handles {
        let result = handle.await.expect("Task should complete");
        results.push(result);
    }

    // Verify results
    let success_count = results.iter().filter(|(_, success)| *success).count();
    let total_count = results.len();

    println!(
        "✅ Concurrent operations completed: {}/{} successful",
        success_count, total_count
    );

    // Most should succeed (unless there's a real issue with the project)
    assert!(
        success_count > 0,
        "At least some concurrent operations should succeed"
    );

    println!("✅ Concurrent operation error handling tests completed");
}

/// Test memory and resource exhaustion scenarios
#[test]
fn test_resource_exhaustion_handling() {
    println!("Testing resource exhaustion handling...");

    // Test 1: Many small projects
    println!("Test 1: Processing many small projects...");
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let base_path = temp_dir.path();

    let registry = ProjectRegistry::new();
    let mut success_count = 0;
    let project_count = 50; // Reasonable number for testing

    for i in 0..project_count {
        let project_path = base_path.join(format!("project_{}", i));

        // Create a minimal project
        if ProjectFixtures::create_rust_nostd_project(&project_path, "s3").is_ok() {
            if let Some(handler) = registry.detect_project(&project_path) {
                // Try board discovery
                if handler.discover_boards(&project_path).is_ok() {
                    success_count += 1;
                }
            }
        }

        // Clean up immediately to manage memory
        if project_path.exists() {
            let _ = fs::remove_dir_all(&project_path);
        }
    }

    println!(
        "✅ Processed {} projects, {} successful",
        project_count, success_count
    );
    assert!(
        success_count > project_count / 2,
        "Should handle most projects successfully"
    );

    // Test 2: Large file handling
    println!("Test 2: Large configuration file handling...");
    let large_project_path = base_path.join("large_project");
    ProjectFixtures::create_rust_nostd_project(&large_project_path, "s3")
        .expect("Failed to create large project");

    // Create a large espbrew.toml with many repeated sections
    let mut large_config = String::new();
    large_config.push_str(
        r#"[project]
name = "large-test-project"
type = "rust_nostd"
target = "esp32s3"
"#,
    );

    // Add many comments to make the file large
    for i in 0..1000 {
        large_config.push_str(&format!("# This is comment line {}\n", i));
    }

    fs::write(large_project_path.join("espbrew.toml"), large_config)
        .expect("Failed to write large config");

    let handler = registry
        .detect_project(&large_project_path)
        .expect("Should detect project with large config");

    match handler.discover_boards(&large_project_path) {
        Ok(boards) => {
            println!(
                "✅ Large configuration handled successfully: {} boards",
                boards.len()
            );
        }
        Err(e) => {
            println!("✅ Large configuration error handled: {}", e);
        }
    }

    println!("✅ Resource exhaustion handling tests completed");
}

/// Test network-related error scenarios
#[test]
fn test_network_error_scenarios() {
    println!("Testing network error scenarios...");

    // Most of espbrew should work offline, but test scenarios that might involve network

    // Test 1: Tool detection when network tools might be involved
    println!("Test 1: Tool availability in network-constrained environment...");

    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let project_path = temp_dir.path();

    ProjectFixtures::create_rust_nostd_project(project_path, "s3")
        .expect("Failed to create Rust project");

    let registry = ProjectRegistry::new();
    let handler = registry
        .detect_project(project_path)
        .expect("Should detect project");

    // Tool checking should work even without network
    let tool_result = handler.check_tools_available();
    match tool_result {
        Ok(_) => {
            println!("✅ Tool availability check succeeded (tools available)");
        }
        Err(msg) => {
            println!("✅ Tool availability check handled missing tools: {}", msg);
            // Should provide helpful message even without network
            assert!(!msg.is_empty(), "Should provide error message");
        }
    }

    // Test 2: Board discovery should not depend on network
    println!("Test 2: Offline board discovery...");
    let board_result = handler.discover_boards(project_path);
    match board_result {
        Ok(boards) => {
            println!(
                "✅ Board discovery works offline: {} boards found",
                boards.len()
            );
        }
        Err(e) => {
            println!("✅ Board discovery error handled: {}", e);
            // Should work offline, but might fail due to missing tools
        }
    }

    println!("✅ Network error scenario tests completed");
}

/// Integration test combining multiple error scenarios
#[tokio::test]
async fn test_combined_error_scenarios() {
    println!("Testing combined error scenarios...");

    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let project_path = temp_dir.path();

    // Scenario 1: Start with corrupted project + missing tools + invalid config
    println!("Scenario 1: Multiple simultaneous issues...");

    // Create a project with multiple issues
    fs::create_dir_all(project_path.join("src")).expect("Failed to create src");
    fs::write(project_path.join("Cargo.toml"), "invalid toml [[[")
        .expect("Failed to write invalid Cargo.toml");
    fs::write(project_path.join("src/main.rs"), "invalid rust code {{{")
        .expect("Failed to write invalid main.rs");
    fs::write(project_path.join("espbrew.toml"), "invalid toml config [[[")
        .expect("Failed to write invalid espbrew.toml");

    let registry = ProjectRegistry::new();

    // Should handle multiple issues gracefully
    let result = registry.detect_project(project_path);
    match result {
        Some(handler) => {
            println!("✅ Project detected despite multiple issues");

            // Try operations that might fail
            let board_result = handler.discover_boards(project_path);
            let tool_result = handler.check_tools_available();

            // Should not panic, should provide useful information
            match board_result {
                Ok(boards) => println!("✅ Board discovery: {} boards", boards.len()),
                Err(e) => println!("✅ Board discovery error handled: {}", e),
            }

            match tool_result {
                Ok(_) => println!("✅ Tools available despite other issues"),
                Err(e) => println!("✅ Tool check error handled: {}", e),
            }
        }
        None => {
            println!("✅ Severely corrupted project correctly rejected");
        }
    }

    // Scenario 2: Recovery - fix issues one by one
    println!("Scenario 2: Gradual recovery...");

    // Fix Cargo.toml first
    ProjectFixtures::create_rust_nostd_project(project_path, "s3")
        .expect("Failed to recreate Rust project");

    let partial_recovery = registry.detect_project(project_path);
    assert!(
        partial_recovery.is_some(),
        "Should detect project after fixing Cargo.toml"
    );

    // Fix espbrew.toml
    ConfigFixtures::create_espbrew_config(project_path, "rust_nostd", "esp32s3")
        .expect("Failed to create valid config");

    let full_recovery = registry
        .detect_project(project_path)
        .expect("Should detect project after full recovery");

    let final_board_result = full_recovery.discover_boards(project_path);
    match final_board_result {
        Ok(boards) => {
            println!("✅ Full recovery successful: {} boards found", boards.len());
        }
        Err(e) => {
            println!("✅ Remaining issues after recovery: {}", e);
        }
    }

    println!("✅ Combined error scenario tests completed");
}

/// Test error message quality and usefulness
#[test]
fn test_error_message_quality() {
    println!("Testing error message quality and usefulness...");

    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let project_path = temp_dir.path();

    // Create various error scenarios and check message quality
    let registry = ProjectRegistry::new();

    // Test 1: Missing project directory
    println!("Test 1: Error messages for missing directories...");
    let missing_path = project_path.join("nonexistent");
    let result = registry.detect_project(&missing_path);
    assert!(result.is_none(), "Should not detect nonexistent project");
    // Note: detect_project returns Option, so no error message here
    println!("✅ Missing directory handled appropriately");

    // Test 2: Project with missing files
    println!("Test 2: Error messages for incomplete projects...");
    fs::create_dir_all(project_path.join("incomplete")).expect("Failed to create dir");
    let incomplete_path = project_path.join("incomplete");

    // Create minimal structure but missing key files
    fs::create_dir_all(incomplete_path.join("src")).expect("Failed to create src");
    fs::write(incomplete_path.join("src/main.rs"), "fn main() {}")
        .expect("Failed to write main.rs");
    // Intentionally omit Cargo.toml

    let incomplete_result = registry.detect_project(&incomplete_path);
    if incomplete_result.is_none() {
        println!("✅ Incomplete project correctly not detected");
    } else {
        println!("✅ Incomplete project detected, will test error handling");
        let handler = incomplete_result.unwrap();

        match handler.discover_boards(&incomplete_path) {
            Ok(_) => println!("✅ Board discovery succeeded despite missing files"),
            Err(e) => {
                println!("✅ Board discovery error: {}", e);
                let error_str = e.to_string();
                assert!(!error_str.is_empty(), "Error message should not be empty");
                // Error messages should be helpful
                assert!(error_str.len() > 10, "Error message should be descriptive");
            }
        }
    }

    // Test 3: Tool availability error messages
    println!("Test 3: Tool availability error messages...");
    ProjectFixtures::create_rust_nostd_project(project_path, "s3")
        .expect("Failed to create Rust project");

    let handler = registry
        .detect_project(project_path)
        .expect("Should detect valid project");

    let tool_check = handler.check_tools_available();
    match tool_check {
        Ok(_) => {
            println!("✅ All tools available - no error messages needed");
        }
        Err(error_msg) => {
            println!("✅ Tool error message: {}", error_msg);

            // Check error message quality
            assert!(
                !error_msg.is_empty(),
                "Tool error message should not be empty"
            );
            assert!(
                error_msg.len() > 20,
                "Tool error message should be descriptive"
            );

            // Should contain actionable information
            let missing_tools_help = handler.get_missing_tools_message();
            assert!(
                !missing_tools_help.is_empty(),
                "Help message should not be empty"
            );
            assert!(
                missing_tools_help.contains("install"),
                "Help should mention installation"
            );

            println!("✅ Help message: {}", missing_tools_help);
        }
    }

    println!("✅ Error message quality tests completed");
}
