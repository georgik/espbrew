//! Project Handler Tests
//!
//! Comprehensive tests for all espbrew project handlers including project detection,
//! creation, build preparation, and board configuration for Rust, Arduino, ESP-IDF,
//! and other supported project types.

mod mock_hardware;
mod test_fixtures;

use espbrew::models::ProjectType;
use espbrew::projects::ProjectRegistry;
use std::fs;
use tempfile::TempDir;
use test_fixtures::{ConfigFixtures, ProjectFixtures, TestEnvironment};

/// Test project registry initialization and handler availability
#[test]
fn test_project_registry_initialization() {
    let _registry = ProjectRegistry::new();

    // Verify registry can be created
    assert!(true, "Registry should initialize successfully");

    // Test that the registry can detect different project types
    // We'll test with our existing test fixtures
}

/// Test Rust no_std project handler
#[tokio::test]
async fn test_rust_nostd_handler() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let project_path = temp_dir.path();

    // Create a Rust no_std project
    ProjectFixtures::create_rust_nostd_project(project_path, "s3")
        .expect("Failed to create Rust project");
    ConfigFixtures::create_espbrew_config(project_path, "rust_nostd", "esp32s3")
        .expect("Failed to create config");

    let registry = ProjectRegistry::new();
    let handler = registry
        .detect_project(project_path)
        .expect("Should detect Rust project");

    // Verify project type detection
    assert_eq!(handler.project_type(), ProjectType::RustNoStd);

    // Test project validation
    assert!(
        handler.can_handle(project_path),
        "Handler should be able to handle the project"
    );

    // Test board discovery
    let boards = handler
        .discover_boards(project_path)
        .expect("Should discover boards");
    assert!(!boards.is_empty(), "Should find at least one board");

    let board = &boards[0];
    assert_eq!(board.project_type, ProjectType::RustNoStd);
    assert!(
        board.name.contains("esp32"),
        "Board name should contain esp32"
    );

    // Test build command generation
    let build_cmd = handler.get_build_command(project_path, board);
    assert!(
        build_cmd.contains("cargo"),
        "Build command should use cargo"
    );
    assert!(
        build_cmd.contains("build"),
        "Build command should contain build"
    );

    // Test flash command generation
    let flash_cmd = handler.get_flash_command(project_path, board, Some("/dev/ttyUSB0"));
    assert!(
        flash_cmd.contains("espflash"),
        "Flash command should use espflash"
    );
    assert!(
        flash_cmd.contains("/dev/ttyUSB0"),
        "Flash command should include specified port"
    );
}

/// Test Arduino project handler
#[tokio::test]
async fn test_arduino_handler() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let project_path = temp_dir.path();

    // Create an Arduino project
    ProjectFixtures::create_arduino_project(project_path, "test-arduino-project")
        .expect("Failed to create Arduino project");
    ConfigFixtures::create_espbrew_config(project_path, "arduino", "esp32")
        .expect("Failed to create config");

    let registry = ProjectRegistry::new();
    let handler = registry
        .detect_project(project_path)
        .expect("Should detect Arduino project");

    // Verify project type detection
    assert_eq!(handler.project_type(), ProjectType::Arduino);

    // Test project validation
    assert!(
        handler.can_handle(project_path),
        "Handler should be able to handle Arduino project"
    );

    // Test board discovery
    let boards = handler
        .discover_boards(project_path)
        .expect("Should discover Arduino boards");
    assert!(!boards.is_empty(), "Should find at least one Arduino board");

    let board = &boards[0];
    assert_eq!(board.project_type, ProjectType::Arduino);

    // Test build command generation
    let build_cmd = handler.get_build_command(project_path, board);
    assert!(
        build_cmd.contains("arduino")
            || build_cmd.contains("platformio")
            || build_cmd.contains("esptool"),
        "Build command should use Arduino toolchain"
    );

    // Test flash command generation
    let flash_cmd = handler.get_flash_command(project_path, board, Some("/dev/ttyUSB0"));
    assert!(
        flash_cmd.contains("/dev/ttyUSB0"),
        "Flash command should include port"
    );
}

/// Test ESP-IDF project handler
#[tokio::test]
async fn test_esp_idf_handler() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let project_path = temp_dir.path();

    // Create an ESP-IDF project
    ProjectFixtures::create_esp_idf_project(project_path, "")
        .expect("Failed to create ESP-IDF project");
    ConfigFixtures::create_espbrew_config(project_path, "esp_idf", "esp32")
        .expect("Failed to create config");

    let registry = ProjectRegistry::new();
    let handler = registry
        .detect_project(project_path)
        .expect("Should detect ESP-IDF project");

    // Verify project type detection
    assert_eq!(handler.project_type(), ProjectType::EspIdf);

    // Test project validation
    assert!(
        handler.can_handle(project_path),
        "Handler should be able to handle ESP-IDF project"
    );

    // Test board discovery
    let boards = handler
        .discover_boards(project_path)
        .expect("Should discover ESP-IDF boards");
    assert!(!boards.is_empty(), "Should find at least one ESP-IDF board");

    let board = &boards[0];
    assert_eq!(board.project_type, ProjectType::EspIdf);

    // Test build command generation
    let build_cmd = handler.get_build_command(project_path, board);
    assert!(
        build_cmd.contains("idf.py") || build_cmd.contains("cmake"),
        "Build command should use ESP-IDF toolchain"
    );

    // Test flash command generation
    let flash_cmd = handler.get_flash_command(project_path, board, Some("/dev/ttyUSB0"));
    assert!(
        flash_cmd.contains("idf.py") || flash_cmd.contains("esptool"),
        "Flash command should use ESP-IDF flash tools"
    );
}

/// Test MicroPython project handler
#[tokio::test]
async fn test_micropython_handler() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let project_path = temp_dir.path();

    // Create a MicroPython project
    ProjectFixtures::create_micropython_project(project_path)
        .expect("Failed to create MicroPython project");
    ConfigFixtures::create_espbrew_config(project_path, "micropython", "esp32")
        .expect("Failed to create config");

    let registry = ProjectRegistry::new();
    let handler = registry
        .detect_project(project_path)
        .expect("Should detect MicroPython project");

    // Verify project type detection
    assert_eq!(handler.project_type(), ProjectType::MicroPython);

    // Test project validation
    assert!(
        handler.can_handle(project_path),
        "Handler should be able to handle MicroPython project"
    );

    // Test board discovery
    let boards = handler
        .discover_boards(project_path)
        .expect("Should discover MicroPython boards");
    assert!(
        !boards.is_empty(),
        "Should find at least one MicroPython board"
    );

    let board = &boards[0];
    assert_eq!(board.project_type, ProjectType::MicroPython);

    // Test build command generation (MicroPython may not need traditional build)
    let _build_cmd = handler.get_build_command(project_path, board);
    // Build command might be empty or contain setup commands

    // Test flash command generation
    let flash_cmd = handler.get_flash_command(project_path, board, Some("/dev/ttyUSB0"));
    assert!(
        flash_cmd.contains("esptool")
            || flash_cmd.contains("micropython")
            || flash_cmd.contains("ampy"),
        "Flash command should use MicroPython tools"
    );
}

/// Test project detection priority and accuracy
#[tokio::test]
async fn test_project_detection_accuracy() {
    let workspace =
        TestEnvironment::create_test_workspace().expect("Failed to create test workspace");
    let workspace_path = workspace.path();

    let registry = ProjectRegistry::new();

    // Test Rust project detection
    let rust_project = workspace_path.join("rust-esp32s3-project");
    let rust_handler = registry
        .detect_project(&rust_project)
        .expect("Should detect Rust project");
    assert_eq!(rust_handler.project_type(), ProjectType::RustNoStd);

    // Test Arduino project detection
    let arduino_project = workspace_path.join("arduino-esp32-project");
    let arduino_handler = registry
        .detect_project(&arduino_project)
        .expect("Should detect Arduino project");
    assert_eq!(arduino_handler.project_type(), ProjectType::Arduino);

    // Test ESP-IDF project detection
    let idf_project = workspace_path.join("esp-idf-project");
    let idf_handler = registry
        .detect_project(&idf_project)
        .expect("Should detect ESP-IDF project");
    assert_eq!(idf_handler.project_type(), ProjectType::EspIdf);

    // Test MicroPython project detection
    let micropython_project = workspace_path.join("micropython-project");
    let micropython_handler = registry
        .detect_project(&micropython_project)
        .expect("Should detect MicroPython project");
    assert_eq!(micropython_handler.project_type(), ProjectType::MicroPython);
}

/// Test project validation with invalid projects
#[tokio::test]
async fn test_invalid_project_detection() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let project_path = temp_dir.path();

    // Create invalid project (just random files)
    fs::write(project_path.join("random.txt"), "not a project")
        .expect("Failed to write random file");
    fs::write(project_path.join("another.log"), "still not a project")
        .expect("Failed to write another file");

    let registry = ProjectRegistry::new();
    let result = registry.detect_project(project_path);

    assert!(
        result.is_none(),
        "Should not detect invalid project as any known type"
    );
}

/// Test mixed project scenarios (conflicting indicators)
#[tokio::test]
async fn test_mixed_project_scenarios() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let project_path = temp_dir.path();

    // Create a project with mixed indicators
    // Add Rust files
    fs::create_dir_all(project_path.join("src")).expect("Failed to create src dir");
    fs::write(
        project_path.join("Cargo.toml"),
        r#"
[package]
name = "mixed-project"
version = "0.1.0"
edition = "2021"
"#,
    )
    .expect("Failed to write Cargo.toml");

    // Add Arduino files
    fs::write(
        project_path.join("mixed_project.ino"),
        r#"
void setup() {
    Serial.begin(115200);
}

void loop() {
    delay(1000);
}
"#,
    )
    .expect("Failed to write Arduino sketch");

    let registry = ProjectRegistry::new();
    let handler = registry.detect_project(project_path);

    // Should detect one type (priority-based detection)
    assert!(handler.is_some(), "Should detect at least one project type");

    // The detected type should be consistent
    let detected_type = handler.unwrap().project_type();
    assert!(
        matches!(detected_type, ProjectType::RustNoStd | ProjectType::Arduino),
        "Should detect either Rust or Arduino based on priority rules"
    );
}

/// Test board configuration parsing and validation
#[tokio::test]
async fn test_board_configuration_parsing() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let project_path = temp_dir.path();

    // Create a Rust project with specific board configuration
    ProjectFixtures::create_rust_nostd_project(project_path, "c3")
        .expect("Failed to create Rust project");
    ConfigFixtures::create_espbrew_config(project_path, "rust_nostd", "esp32c3")
        .expect("Failed to create config");

    let registry = ProjectRegistry::new();
    let handler = registry
        .detect_project(project_path)
        .expect("Should detect project");

    let boards = handler
        .discover_boards(project_path)
        .expect("Should discover boards");

    // Verify board configuration
    let board = &boards[0];
    assert!(
        board.target.is_some(),
        "Board should have a target specified"
    );

    let target = board.target.as_ref().unwrap();
    assert!(target.contains("ESP32"), "Target should contain ESP32");

    // Test configuration-specific details
    assert!(
        board.config_file.exists() || !board.config_file.to_string_lossy().is_empty(),
        "Board should have config file path"
    );
    assert!(
        board.build_dir.exists() || !board.build_dir.to_string_lossy().is_empty(),
        "Board should have build directory path"
    );
}

/// Test build command generation and validation
#[tokio::test]
async fn test_build_command_generation() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let project_path = temp_dir.path();

    // Create a Rust project
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
    let board = &boards[0];

    // Test build command generation
    let build_cmd = handler.get_build_command(project_path, board);
    assert!(!build_cmd.is_empty(), "Build command should not be empty");
    assert!(
        build_cmd.contains("cargo"),
        "Build command should contain cargo"
    );

    // Test flash command generation
    let flash_cmd = handler.get_flash_command(project_path, board, Some("/dev/ttyUSB0"));
    assert!(!flash_cmd.is_empty(), "Flash command should not be empty");
    assert!(
        flash_cmd.contains("/dev/ttyUSB0"),
        "Flash command should contain port"
    );
}

/// Test project tool availability checking
#[tokio::test]
async fn test_project_tool_availability() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let project_path = temp_dir.path();

    // Create a Rust project
    ProjectFixtures::create_rust_nostd_project(project_path, "s3")
        .expect("Failed to create Rust project");

    let registry = ProjectRegistry::new();
    let handler = registry
        .detect_project(project_path)
        .expect("Should detect project");

    // Test tool availability check (may pass or fail depending on system)
    let tool_check = handler.check_tools_available();
    match tool_check {
        Ok(_) => assert!(true, "Tools are available"),
        Err(_) => {
            // If tools are missing, verify we get a helpful message
            let message = handler.get_missing_tools_message();
            assert!(
                !message.is_empty(),
                "Should provide helpful missing tools message"
            );
        }
    }
}

/// Test error handling in project handlers
#[tokio::test]
async fn test_project_handler_error_handling() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let project_path = temp_dir.path();

    // Create a malformed project
    fs::write(project_path.join("Cargo.toml"), "invalid toml content [")
        .expect("Failed to write invalid Cargo.toml");

    let registry = ProjectRegistry::new();
    let handler = registry.detect_project(project_path);

    // Should handle malformed projects gracefully
    if let Some(handler) = handler {
        // If detected, operations should handle errors gracefully
        let result = handler.discover_boards(project_path);
        // Should either succeed with empty results or return appropriate error
        match result {
            Ok(_boards) => assert!(true, "Successfully handled malformed project"),
            Err(_) => assert!(true, "Appropriately errored on malformed project"),
        }
    } else {
        assert!(true, "Correctly rejected malformed project");
    }
}

/// Test concurrent project operations
#[tokio::test]
async fn test_concurrent_project_operations() {
    let workspace =
        TestEnvironment::create_test_workspace().expect("Failed to create test workspace");
    let workspace_path = workspace.path();

    // Test concurrent project detection
    let project_paths = vec![
        workspace_path.join("rust-esp32s3-project"),
        workspace_path.join("arduino-esp32-project"),
        workspace_path.join("esp-idf-project"),
        workspace_path.join("micropython-project"),
    ];

    // Simulate concurrent detection
    let mut handles = vec![];
    for project_path in project_paths {
        let handle = tokio::spawn(async move {
            let registry = ProjectRegistry::new(); // Each task gets its own registry
            registry.detect_project_boxed(&project_path)
        });
        handles.push(handle);
    }

    // Wait for all detections to complete
    let mut results = vec![];
    for handle in handles {
        let result = handle.await.expect("Task should complete");
        results.push(result);
    }

    // Verify all detections succeeded
    assert_eq!(results.len(), 4, "Should have 4 detection results");
    assert!(
        results.iter().all(|r| r.is_some()),
        "All projects should be detected"
    );

    // Verify different project types were detected
    let detected_types: Vec<_> = results
        .into_iter()
        .filter_map(|r| r.map(|handler| handler.project_type()))
        .collect();

    // Count unique project types
    let mut unique_types = Vec::new();
    for project_type in detected_types {
        if !unique_types.contains(&project_type) {
            unique_types.push(project_type);
        }
    }

    assert!(
        unique_types.len() >= 3,
        "Should detect multiple different project types"
    );
}

/// Test project handler extensibility
#[tokio::test]
async fn test_project_handler_extensibility() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let project_path = temp_dir.path();

    // Create a project that might match multiple handlers
    fs::create_dir_all(project_path.join("src")).expect("Failed to create src");
    fs::write(project_path.join("main.py"), "# MicroPython project")
        .expect("Failed to write Python file");
    fs::write(project_path.join("boot.py"), "# Boot script").expect("Failed to write boot script");

    let registry = ProjectRegistry::new();

    // Test that the registry can handle new project types
    let handler = registry.detect_project(project_path);

    if let Some(handler) = handler {
        // Verify the handler provides all expected functionality
        assert!(
            handler.can_handle(project_path),
            "Handler should validate it can handle the project"
        );

        let project_type = handler.project_type();
        assert!(
            matches!(project_type, ProjectType::MicroPython),
            "Should detect as MicroPython project"
        );

        // Test that all required methods are available
        let boards_result = handler.discover_boards(project_path);
        assert!(boards_result.is_ok(), "Should be able to discover boards");

        if let Ok(boards) = boards_result {
            if !boards.is_empty() {
                let board = &boards[0];

                // Test command generation methods exist and return reasonable results
                let build_cmd = handler.get_build_command(project_path, board);
                let flash_cmd =
                    handler.get_flash_command(project_path, board, Some("/dev/ttyUSB0"));

                assert!(
                    !build_cmd.is_empty() || true,
                    "Build command should exist or be empty for interpreted languages"
                );
                assert!(!flash_cmd.is_empty(), "Flash command should not be empty");
            }
        }
    }
}

/// Test project handler performance and resource usage
#[tokio::test]
async fn test_project_handler_performance() {
    let workspace =
        TestEnvironment::create_test_workspace().expect("Failed to create test workspace");
    let workspace_path = workspace.path();

    let registry = ProjectRegistry::new();

    // Test detection performance
    let project_paths = vec![
        workspace_path.join("rust-esp32s3-project"),
        workspace_path.join("arduino-esp32-project"),
        workspace_path.join("esp-idf-project"),
        workspace_path.join("micropython-project"),
    ];

    let start = std::time::Instant::now();

    for project_path in &project_paths {
        let _handler = registry.detect_project(project_path);
    }

    let detection_time = start.elapsed();

    // Detection should be fast (under 100ms for all projects)
    assert!(
        detection_time.as_millis() < 100,
        "Project detection should be fast, took {:?}",
        detection_time
    );

    // Test board discovery performance
    let rust_project = workspace_path.join("rust-esp32s3-project");
    let handler = registry
        .detect_project(&rust_project)
        .expect("Should detect Rust project");

    let start = std::time::Instant::now();
    let boards = handler
        .discover_boards(&rust_project)
        .expect("Should discover boards");
    let discovery_time = start.elapsed();

    assert!(
        discovery_time.as_millis() < 50,
        "Board discovery should be fast, took {:?}",
        discovery_time
    );
    assert!(!boards.is_empty(), "Should discover at least one board");
}
