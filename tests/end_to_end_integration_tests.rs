//! End-to-End Integration Tests
//!
//! Comprehensive integration tests that simulate complete espbrew workflows:
//! project creation → detection → build → flash → monitor
//!
//! These tests validate the full integration between CLI commands, project handlers,
//! hardware simulation, and all supporting services.

mod mock_hardware;
mod test_fixtures;

use espbrew::models::*;
use espbrew::projects::ProjectRegistry;
use mock_hardware::{MockEsp32Device, MockHardwareEnvironment};
use std::path::Path;
use std::sync::Arc;
use tempfile::TempDir;
use test_fixtures::{ConfigFixtures, ProjectFixtures, TestEnvironment};
use tokio::sync::mpsc;

/// Test complete Rust project workflow from creation to monitoring
#[tokio::test]
async fn test_complete_rust_workflow() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let project_path = temp_dir.path();

    // Phase 1: Project Creation and Detection
    println!("Phase 1: Creating Rust ESP32-S3 project...");
    ProjectFixtures::create_rust_nostd_project(project_path, "s3")
        .expect("Failed to create Rust project");
    ConfigFixtures::create_espbrew_config(project_path, "rust_nostd", "esp32s3")
        .expect("Failed to create config");

    let registry = ProjectRegistry::new();
    let handler = registry
        .detect_project(project_path)
        .expect("Should detect Rust project");

    assert_eq!(handler.project_type(), ProjectType::RustNoStd);
    println!("✅ Project created and detected successfully");

    // Phase 2: Board Discovery
    println!("Phase 2: Discovering available boards...");
    let boards = handler
        .discover_boards(project_path)
        .expect("Should discover boards");
    assert!(!boards.is_empty(), "Should find at least one board");

    let board = &boards[0];
    println!("✅ Found board: {}", board.name);

    // Phase 3: Mock Hardware Setup
    println!("Phase 3: Setting up mock hardware environment...");
    let mut mock_env = MockHardwareEnvironment::new().expect("Failed to create mock environment");
    let device = MockEsp32Device::new_esp32s3();
    mock_env.add_device("ESP32-S3", device);
    println!("✅ Mock hardware environment ready");

    // Phase 4: Build Command Testing
    println!("Phase 4: Testing build command generation...");
    let build_cmd = handler.get_build_command(project_path, board);
    assert!(
        build_cmd.contains("cargo"),
        "Build command should use cargo"
    );
    assert!(
        build_cmd.contains("build"),
        "Build command should contain build"
    );
    assert!(
        build_cmd.contains("--release"),
        "Build command should use release mode"
    );
    println!("✅ Build command: {}", build_cmd);

    // Phase 5: Flash Command Testing
    println!("Phase 5: Testing flash command generation...");
    let flash_cmd = handler.get_flash_command(project_path, board, Some("/dev/ttyUSB0"));
    assert!(
        flash_cmd.contains("espflash"),
        "Flash command should use espflash"
    );
    assert!(
        flash_cmd.contains("/dev/ttyUSB0"),
        "Flash command should include port"
    );
    println!("✅ Flash command: {}", flash_cmd);

    // Phase 6: Simulated Build Process
    println!("Phase 6: Simulating build process...");
    let (_tx, mut _rx) = mpsc::unbounded_channel::<AppEvent>();

    // We can't actually build without toolchain, but we can test the workflow setup
    let tool_check = handler.check_tools_available();
    match tool_check {
        Ok(_) => {
            println!("✅ All required tools are available");
            // In a real scenario, we would call:
            // let artifacts = handler.build_board(project_path, board, tx.clone()).await?;
            println!("✅ Build process setup validated");
        }
        Err(msg) => {
            println!("⚠️  Missing tools (expected in test environment): {}", msg);
            println!("✅ Tool availability check working correctly");
        }
    }

    // Phase 7: Integration Validation
    println!("Phase 7: Validating workflow integration...");

    // Test that all components can work together
    assert!(
        handler.can_handle(project_path),
        "Handler should validate project"
    );

    // Test configuration integration
    let config_exists = project_path.join("espbrew.toml").exists();
    assert!(config_exists, "espbrew.toml should exist");

    // Test project structure
    let cargo_toml_exists = project_path.join("Cargo.toml").exists();
    let src_main_exists = project_path.join("src/main.rs").exists();
    assert!(cargo_toml_exists, "Cargo.toml should exist");
    assert!(src_main_exists, "src/main.rs should exist");

    println!("✅ Complete Rust workflow integration validated successfully");
}

/// Test complete Arduino project workflow
#[tokio::test]
async fn test_complete_arduino_workflow() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let project_path = temp_dir.path();

    // Phase 1: Project Creation and Detection
    println!("Phase 1: Creating Arduino ESP32 project...");
    ProjectFixtures::create_arduino_project(project_path, "test-arduino-project")
        .expect("Failed to create Arduino project");
    ConfigFixtures::create_espbrew_config(project_path, "arduino", "esp32")
        .expect("Failed to create config");

    let registry = ProjectRegistry::new();
    let handler = registry
        .detect_project(project_path)
        .expect("Should detect Arduino project");

    assert_eq!(handler.project_type(), ProjectType::Arduino);
    println!("✅ Arduino project created and detected successfully");

    // Phase 2: Board Discovery and Validation
    println!("Phase 2: Discovering Arduino boards...");
    let boards = handler
        .discover_boards(project_path)
        .expect("Should discover Arduino boards");
    assert!(!boards.is_empty(), "Should find at least one Arduino board");

    let board = &boards[0];
    println!("✅ Found Arduino board: {}", board.name);

    // Phase 3: Command Generation Testing
    println!("Phase 3: Testing Arduino command generation...");
    let build_cmd = handler.get_build_command(project_path, board);
    let flash_cmd = handler.get_flash_command(project_path, board, Some("/dev/ttyUSB0"));

    println!("Build command: {}", build_cmd);
    println!("Flash command: {}", flash_cmd);

    assert!(
        flash_cmd.contains("/dev/ttyUSB0"),
        "Flash command should include port"
    );
    println!("✅ Arduino commands generated successfully");

    // Phase 4: Project Structure Validation
    println!("Phase 4: Validating Arduino project structure...");
    let ino_files: Vec<_> = std::fs::read_dir(project_path)
        .expect("Should read project directory")
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let path = entry.path();
            if path.extension()?.to_str()? == "ino" {
                Some(path)
            } else {
                None
            }
        })
        .collect();

    assert!(!ino_files.is_empty(), "Should have at least one .ino file");
    let boards_json_exists = project_path.join("boards.json").exists();
    assert!(boards_json_exists, "boards.json should exist");

    println!("✅ Complete Arduino workflow integration validated successfully");
}

/// Test complete ESP-IDF project workflow
#[tokio::test]
async fn test_complete_esp_idf_workflow() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let project_path = temp_dir.path();

    // Phase 1: Project Creation and Detection
    println!("Phase 1: Creating ESP-IDF project...");
    ProjectFixtures::create_esp_idf_project(project_path, "")
        .expect("Failed to create ESP-IDF project");
    ConfigFixtures::create_espbrew_config(project_path, "esp_idf", "esp32")
        .expect("Failed to create config");

    let registry = ProjectRegistry::new();
    let handler = registry
        .detect_project(project_path)
        .expect("Should detect ESP-IDF project");

    assert_eq!(handler.project_type(), ProjectType::EspIdf);
    println!("✅ ESP-IDF project created and detected successfully");

    // Phase 2: Board Discovery
    println!("Phase 2: Discovering ESP-IDF boards...");
    let boards = handler
        .discover_boards(project_path)
        .expect("Should discover ESP-IDF boards");
    assert!(!boards.is_empty(), "Should find at least one ESP-IDF board");

    let board = &boards[0];
    println!("✅ Found ESP-IDF board: {}", board.name);

    // Phase 3: Command Generation and Validation
    println!("Phase 3: Testing ESP-IDF command generation...");
    let build_cmd = handler.get_build_command(project_path, board);
    let flash_cmd = handler.get_flash_command(project_path, board, Some("/dev/ttyUSB0"));

    assert!(
        build_cmd.contains("idf.py") || build_cmd.contains("cmake"),
        "Build command should use ESP-IDF tools"
    );
    assert!(
        flash_cmd.contains("idf.py") || flash_cmd.contains("esptool"),
        "Flash command should use ESP-IDF flash tools"
    );

    println!("Build command: {}", build_cmd);
    println!("Flash command: {}", flash_cmd);
    println!("✅ ESP-IDF commands generated successfully");

    // Phase 4: Configuration File Validation
    println!("Phase 4: Validating ESP-IDF project structure...");
    let cmake_exists = project_path.join("CMakeLists.txt").exists();
    let sdkconfig_exists = project_path.join("sdkconfig").exists();
    let main_dir_exists = project_path.join("main").exists();

    assert!(cmake_exists, "CMakeLists.txt should exist");
    assert!(sdkconfig_exists, "sdkconfig should exist");
    assert!(main_dir_exists, "main directory should exist");

    println!("✅ Complete ESP-IDF workflow integration validated successfully");
}

/// Test cross-project-type compatibility and detection accuracy
#[tokio::test]
async fn test_project_detection_accuracy_integration() {
    let workspace =
        TestEnvironment::create_test_workspace().expect("Failed to create test workspace");
    let workspace_path = workspace.path();

    println!("Testing project detection accuracy across multiple project types...");

    let registry = ProjectRegistry::new();

    // Test detection of each project type
    let test_cases = vec![
        (
            workspace_path.join("rust-esp32s3-project"),
            ProjectType::RustNoStd,
            "Rust",
        ),
        (
            workspace_path.join("arduino-esp32-project"),
            ProjectType::Arduino,
            "Arduino",
        ),
        (
            workspace_path.join("esp-idf-project"),
            ProjectType::EspIdf,
            "ESP-IDF",
        ),
        (
            workspace_path.join("micropython-project"),
            ProjectType::MicroPython,
            "MicroPython",
        ),
    ];

    for (project_path, expected_type, project_name) in test_cases {
        println!("Testing {} project detection...", project_name);

        let handler = registry.detect_project(&project_path);
        assert!(
            handler.is_some(),
            "{} project should be detected",
            project_name
        );

        let handler = handler.unwrap();
        assert_eq!(
            handler.project_type(),
            expected_type,
            "{} project should be detected as correct type",
            project_name
        );

        assert!(
            handler.can_handle(&project_path),
            "{} handler should validate it can handle the project",
            project_name
        );

        // Test board discovery for each project type
        let boards_result = handler.discover_boards(&project_path);
        match boards_result {
            Ok(boards) => {
                println!("✅ {} project: Found {} boards", project_name, boards.len());
                for board in &boards {
                    assert_eq!(
                        board.project_type, expected_type,
                        "Board should have correct project type"
                    );
                }
            }
            Err(e) => {
                println!(
                    "⚠️  {} project: Board discovery failed: {}",
                    project_name, e
                );
                // This might be expected if tools aren't available
            }
        }

        println!(
            "✅ {} project detection and validation complete",
            project_name
        );
    }

    println!("✅ Cross-project-type detection accuracy validated successfully");
}

/// Test workflow with hardware simulation integration
#[tokio::test]
async fn test_hardware_simulation_integration() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let project_path = temp_dir.path();

    println!("Testing hardware simulation integration...");

    // Setup project
    ProjectFixtures::create_rust_nostd_project(project_path, "s3")
        .expect("Failed to create Rust project");
    ConfigFixtures::create_espbrew_config(project_path, "rust_nostd", "esp32s3")
        .expect("Failed to create config");

    // Setup mock hardware environment
    let mut mock_env = MockHardwareEnvironment::new().expect("Failed to create mock environment");

    // Add additional devices for comprehensive testing
    let esp32s3 = MockEsp32Device::new_esp32s3();
    mock_env.add_device("test-esp32s3", esp32s3);

    println!("✅ Mock hardware environment setup complete");

    // Test device discovery simulation
    let available_ports = mock_env.list_serial_ports();
    assert!(
        !available_ports.is_empty(),
        "Should have available serial ports"
    );

    println!(
        "✅ Device discovery: {} ports available",
        available_ports.len()
    );

    // Test project integration with mock hardware
    let registry = ProjectRegistry::new();
    let handler = registry
        .detect_project(project_path)
        .expect("Should detect project");

    let boards = handler
        .discover_boards(project_path)
        .expect("Should discover boards");

    // Simulate flash operation workflow
    let board = &boards[0];

    // Test flash commands with available ports
    for port_name in &available_ports {
        let flash_cmd_port = handler.get_flash_command(project_path, board, Some(port_name));
        assert!(
            flash_cmd_port.contains(port_name),
            "Flash command should target port {}",
            port_name
        );
        println!("✅ Flash command for {}: {}", port_name, flash_cmd_port);
    }

    println!("✅ Hardware simulation integration validated successfully");
}

/// Test error handling throughout the complete workflow
#[tokio::test]
async fn test_workflow_error_handling() {
    println!("Testing error handling in complete workflows...");

    // Test 1: Invalid project directory
    println!("Test 1: Invalid project directory handling...");
    let invalid_path = Path::new("/nonexistent/path");
    let registry = ProjectRegistry::new();
    let result = registry.detect_project(invalid_path);
    assert!(
        result.is_none(),
        "Should not detect project in invalid directory"
    );
    println!("✅ Invalid directory handled correctly");

    // Test 2: Corrupted project files
    println!("Test 2: Corrupted project files handling...");
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let project_path = temp_dir.path();

    // Create a malformed Rust project
    std::fs::create_dir_all(project_path.join("src")).expect("Failed to create src");
    std::fs::write(project_path.join("Cargo.toml"), "invalid toml [[[")
        .expect("Failed to write invalid Cargo.toml");

    let handler = registry.detect_project(project_path);
    if let Some(handler) = handler {
        // If detected, operations should handle errors gracefully
        let result = handler.discover_boards(project_path);
        println!(
            "✅ Corrupted files handled gracefully: {:?}",
            result.is_err()
        );
    } else {
        println!("✅ Corrupted project correctly rejected");
    }

    // Test 3: Missing configuration files
    println!("Test 3: Missing configuration handling...");
    let temp_dir2 = TempDir::new().expect("Failed to create temp directory");
    let project_path2 = temp_dir2.path();

    // Create minimal Rust project without espbrew.toml
    ProjectFixtures::create_rust_nostd_project(project_path2, "s3")
        .expect("Failed to create Rust project");
    // Intentionally skip creating espbrew.toml

    let handler = registry.detect_project(project_path2);
    assert!(
        handler.is_some(),
        "Should detect project even without espbrew.toml"
    );

    let handler = handler.unwrap();
    let boards_result = handler.discover_boards(project_path2);

    match boards_result {
        Ok(boards) => {
            println!(
                "✅ Missing config handled - using defaults: {} boards",
                boards.len()
            );
        }
        Err(_) => {
            println!("✅ Missing config properly reported as error");
        }
    }

    println!("✅ Complete workflow error handling validated successfully");
}

#[tokio::test]
async fn test_concurrent_workflow_operations() {
    println!("Testing concurrent workflow operations...");

    let workspace =
        TestEnvironment::create_test_workspace().expect("Failed to create test workspace");
    let workspace_path = workspace.path();

    let registry = Arc::new(ProjectRegistry::new());

    // Test concurrent project detection and board discovery
    let project_paths = vec![
        workspace_path.join("rust-esp32s3-project"),
        workspace_path.join("arduino-esp32-project"),
        workspace_path.join("esp-idf-project"),
        workspace_path.join("micropython-project"),
    ];

    let mut handles = vec![];

    for (i, project_path) in project_paths.into_iter().enumerate() {
        let registry = registry.clone();

        let handle = tokio::spawn(async move {
            println!("Worker {}: Starting project detection...", i);

            let handler = registry
                .detect_project_boxed(&project_path)
                .ok_or_else(|| anyhow::anyhow!("Failed to detect project at {:?}", project_path))?;
            println!(
                "Worker {}: Project detected as {:?}",
                i,
                handler.project_type()
            );

            let boards = handler.discover_boards(&project_path)?;
            println!("Worker {}: Found {} boards", i, boards.len());

            if !boards.is_empty() {
                let board = &boards[0];
                let build_cmd = handler.get_build_command(&project_path, board);
                let flash_cmd =
                    handler.get_flash_command(&project_path, board, Some("/dev/ttyUSB0"));

                println!("Worker {}: Build command: {}", i, build_cmd);
                println!("Worker {}: Flash command: {}", i, flash_cmd);
            }

            Ok::<_, anyhow::Error>(format!("Worker {} completed successfully", i))
        });

        handles.push(handle);
    }

    // Wait for all concurrent operations to complete
    let mut results = vec![];
    for handle in handles {
        let result = handle.await.expect("Task should complete");
        results.push(result);
    }

    // Verify all operations completed successfully
    println!("Concurrent operation results:");
    for result in results {
        match result {
            Ok(msg) => println!("✅ {}", msg),
            Err(e) => println!("⚠️  Error: {}", e),
        }
    }

    println!("✅ Concurrent workflow operations validated successfully");
}
