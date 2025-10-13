use espbrew::models::{ArtifactType, ProjectType};
use espbrew::projects::handlers::jaculus::JaculusHandler;
use espbrew::projects::registry::ProjectHandler;
use std::fs;
use std::path::Path;
use tempfile::TempDir;

#[tokio::test]
async fn test_jaculus_handler_detection_with_jaculus_json() {
    let handler = JaculusHandler;
    let fixture_path = Path::new("tests/fixtures/jaculus_project");

    // Test project type
    assert_eq!(handler.project_type(), ProjectType::Jaculus);

    // Test detection with jaculus.json
    assert!(handler.can_handle(fixture_path));
}

#[tokio::test]
async fn test_jaculus_handler_detection_with_package_json() {
    let handler = JaculusHandler;
    let fixture_path = Path::new("tests/fixtures/jaculus_package_project");

    // Test detection with package.json containing Jaculus dependencies
    assert!(handler.can_handle(fixture_path));
}

#[tokio::test]
async fn test_jaculus_handler_detection_esp32s3() {
    let handler = JaculusHandler;
    let fixture_path = Path::new("tests/fixtures/jaculus_esp32s3_project");

    // Test detection with ESP32-S3 specific project
    assert!(handler.can_handle(fixture_path));
}

#[tokio::test]
async fn test_jaculus_handler_no_detection() {
    let handler = JaculusHandler;
    let temp_dir = TempDir::new().unwrap();
    let temp_path = temp_dir.path();

    // Create a directory without JavaScript files or Jaculus configuration
    fs::write(temp_path.join("README.md"), "# Test Project").unwrap();
    fs::write(temp_path.join("Makefile"), "all:\n\techo hello").unwrap();

    // Should not detect this as a Jaculus project
    assert!(!handler.can_handle(temp_path));
}

#[tokio::test]
async fn test_jaculus_handler_js_files_only() {
    let handler = JaculusHandler;
    let temp_dir = TempDir::new().unwrap();
    let temp_path = temp_dir.path();

    // Create JavaScript files with Jaculus patterns
    fs::write(
        temp_path.join("index.js"),
        r#"
        const GPIO = require('gpio');
        console.log('ESP32 ready');
        setTimeout(() => {
            console.log('Hello from ESP32');
        }, 1000);
        "#,
    )
    .unwrap();

    // Should detect this as a Jaculus project due to JS files with ESP32 patterns
    assert!(handler.can_handle(temp_path));
}

#[tokio::test]
async fn test_jaculus_board_discovery_basic() {
    let handler = JaculusHandler;
    let fixture_path = Path::new("tests/fixtures/jaculus_project");

    let boards = handler.discover_boards(fixture_path).unwrap();
    assert!(!boards.is_empty());

    let board = &boards[0];
    assert_eq!(board.name, "jaculus-esp32");
    assert_eq!(board.project_type, ProjectType::Jaculus);
    assert_eq!(board.target, Some("ESP32".to_string()));
}

#[tokio::test]
async fn test_jaculus_board_discovery_esp32s3() {
    let handler = JaculusHandler;
    let fixture_path = Path::new("tests/fixtures/jaculus_esp32s3_project");

    let boards = handler.discover_boards(fixture_path).unwrap();
    assert!(!boards.is_empty());

    let board = &boards[0];
    assert_eq!(board.name, "jaculus-esp32s3");
    assert_eq!(board.project_type, ProjectType::Jaculus);
    assert_eq!(board.target, Some("ESP32-S3".to_string()));
}

#[tokio::test]
async fn test_jaculus_build_artifacts() {
    let handler = JaculusHandler;
    let fixture_path = Path::new("tests/fixtures/jaculus_project");
    let boards = handler.discover_boards(fixture_path).unwrap();
    let board_config = &boards[0];

    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    let artifacts = handler
        .build_board(fixture_path, board_config, tx)
        .await
        .unwrap();

    // Should find JavaScript files and configuration files
    assert!(!artifacts.is_empty());

    // Check for index.js
    let js_artifacts: Vec<_> = artifacts
        .iter()
        .filter(|a| {
            a.name == "index"
                || a.file_path
                    .file_name()
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .ends_with(".js")
        })
        .collect();
    assert!(!js_artifacts.is_empty());

    // Check for configuration files
    let config_artifacts: Vec<_> = artifacts
        .iter()
        .filter(|a| {
            a.name == "jaculus.json"
                || a.file_path.file_name().unwrap().to_str().unwrap() == "jaculus.json"
        })
        .collect();
    assert!(!config_artifacts.is_empty());

    // All artifacts should be Binary type for JavaScript projects
    for artifact in &artifacts {
        assert_eq!(artifact.artifact_type, ArtifactType::Binary);
    }
}

#[tokio::test]
async fn test_jaculus_build_artifacts_with_src_directory() {
    let handler = JaculusHandler;
    let fixture_path = Path::new("tests/fixtures/jaculus_project");
    let boards = handler.discover_boards(fixture_path).unwrap();
    let board_config = &boards[0];

    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    let artifacts = handler
        .build_board(fixture_path, board_config, tx)
        .await
        .unwrap();

    // Should find files from both root and src/ directory
    let src_artifacts: Vec<_> = artifacts
        .iter()
        .filter(|a| a.file_path.to_str().unwrap().contains("src/sensors.js"))
        .collect();
    assert!(
        !src_artifacts.is_empty(),
        "Should find sensors.js from src/ directory"
    );
}

#[tokio::test]
async fn test_jaculus_commands() {
    let handler = JaculusHandler;
    let fixture_path = Path::new("tests/fixtures/jaculus_project");
    let boards = handler.discover_boards(fixture_path).unwrap();
    let board_config = &boards[0];

    // Test build command (should indicate no build needed)
    let build_cmd = handler.get_build_command(fixture_path, board_config);
    assert!(build_cmd.contains("no build required"));
    assert!(build_cmd.contains("JavaScript files"));

    // Test flash command
    let flash_cmd = handler.get_flash_command(fixture_path, board_config, Some("/dev/ttyUSB0"));
    assert!(flash_cmd.contains("jaculus upload"));
    assert!(flash_cmd.contains("--port /dev/ttyUSB0"));
    assert!(flash_cmd.contains("--target esp32"));
}

#[tokio::test]
async fn test_jaculus_commands_esp32s3() {
    let handler = JaculusHandler;
    let fixture_path = Path::new("tests/fixtures/jaculus_esp32s3_project");
    let boards = handler.discover_boards(fixture_path).unwrap();
    let board_config = &boards[0];

    // Test flash command for ESP32-S3
    let flash_cmd = handler.get_flash_command(fixture_path, board_config, Some("/dev/ttyACM0"));
    assert!(flash_cmd.contains("jaculus upload"));
    assert!(flash_cmd.contains("--port /dev/ttyACM0"));
    assert!(flash_cmd.contains("--target esp32s3"));
}

#[tokio::test]
async fn test_jaculus_tool_availability() {
    let handler = JaculusHandler;

    // Test tool availability check (will likely fail unless jaculus-tools is installed)
    let result = handler.check_tools_available();
    // Don't assert success/failure since jaculus-tools might not be installed
    // Just verify it returns a Result
    assert!(result.is_ok() || result.is_err());

    // Test missing tools message
    let message = handler.get_missing_tools_message();
    assert!(message.contains("jaculus-tools"));
    assert!(message.contains("npm install"));
    assert!(message.contains("github.com"));
}

#[tokio::test]
async fn test_jaculus_clean_operation() {
    let handler = JaculusHandler;
    let temp_dir = TempDir::new().unwrap();
    let temp_path = temp_dir.path();

    // Create cache directories that should be cleaned
    fs::create_dir(temp_path.join("node_modules")).unwrap();
    fs::create_dir(temp_path.join(".jaculus")).unwrap();
    fs::create_dir(temp_path.join("dist")).unwrap();
    fs::write(temp_path.join("node_modules/test"), "cache").unwrap();
    fs::write(temp_path.join(".jaculus/cache"), "cache").unwrap();
    fs::write(temp_path.join("dist/output.js"), "built").unwrap();

    // Create a dummy board config for testing
    let boards = vec![espbrew::models::ProjectBoardConfig {
        name: "test-board".to_string(),
        config_file: temp_path.join("jaculus.json"),
        build_dir: temp_path.to_path_buf(),
        target: Some("ESP32".to_string()),
        project_type: ProjectType::Jaculus,
    }];
    let board_config = &boards[0];

    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    let result = handler.clean_board(temp_path, board_config, tx).await;

    assert!(result.is_ok());

    // Verify cache directories were removed
    assert!(!temp_path.join("node_modules").exists());
    assert!(!temp_path.join(".jaculus").exists());
    assert!(!temp_path.join("dist").exists());
}

#[tokio::test]
async fn test_jaculus_target_detection() {
    let handler = JaculusHandler;
    let temp_dir = TempDir::new().unwrap();
    let temp_path = temp_dir.path();

    // Test ESP32-S3 detection
    fs::write(
        temp_path.join("main.js"),
        r#"
        // ESP32-S3 specific code
        const board = "ESP32-S3";
        console.log("Running on", board);
        "#,
    )
    .unwrap();

    assert!(handler.can_handle(temp_path));
    let boards = handler.discover_boards(temp_path).unwrap();
    assert_eq!(boards[0].target, Some("ESP32-S3".to_string()));

    // Clean up and test ESP32-C3 detection
    fs::remove_file(temp_path.join("main.js")).unwrap();
    fs::write(
        temp_path.join("app.js"),
        r#"
        // ESP32-C3 project
        const GPIO = require('gpio');
        const target = "esp32c3";
        "#,
    )
    .unwrap();

    let boards = handler.discover_boards(temp_path).unwrap();
    assert_eq!(boards[0].target, Some("ESP32-C3".to_string()));
}
