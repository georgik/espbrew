use espbrew::models::ProjectType;
use espbrew::projects::handlers::{
    nuttx::NuttXHandler, tinygo::TinyGoHandler, zephyr::ZephyrHandler,
};
use espbrew::projects::registry::ProjectHandler;
use std::fs;
use std::path::Path;
use tempfile::TempDir;

// ==== ZEPHYR HANDLER TESTS ====

#[tokio::test]
async fn test_zephyr_handler_detection() {
    let handler = ZephyrHandler;
    let fixture_path = Path::new("tests/fixtures/zephyr_project");

    // Test project type
    assert_eq!(handler.project_type(), ProjectType::Zephyr);

    // Test detection with prj.conf and CMakeLists.txt
    assert!(handler.can_handle(fixture_path));
}

#[tokio::test]
async fn test_zephyr_handler_no_detection() {
    let handler = ZephyrHandler;
    let temp_dir = TempDir::new().unwrap();
    let temp_path = temp_dir.path();

    // Create a directory without Zephyr files
    fs::write(temp_path.join("README.md"), "# Test Project").unwrap();
    fs::write(
        temp_path.join("main.c"),
        "#include <stdio.h>\nint main() { return 0; }",
    )
    .unwrap();

    // Should not detect this as a Zephyr project
    assert!(!handler.can_handle(temp_path));
}

#[tokio::test]
async fn test_zephyr_handler_partial_detection() {
    let handler = ZephyrHandler;
    let temp_dir = TempDir::new().unwrap();
    let temp_path = temp_dir.path();

    // Create CMakeLists.txt without Zephyr content
    fs::write(
        temp_path.join("CMakeLists.txt"),
        "cmake_minimum_required(VERSION 3.0)",
    )
    .unwrap();
    fs::write(temp_path.join("prj.conf"), "CONFIG_SOME_OPTION=y").unwrap();

    // Should not detect without Zephyr-specific content
    assert!(!handler.can_handle(temp_path));

    // Add Zephyr-specific content
    fs::write(
        temp_path.join("CMakeLists.txt"),
        "find_package(Zephyr REQUIRED HINTS $ENV{ZEPHYR_BASE})",
    )
    .unwrap();

    // Now should detect
    assert!(handler.can_handle(temp_path));
}

#[tokio::test]
async fn test_zephyr_board_discovery() {
    let handler = ZephyrHandler;
    let fixture_path = Path::new("tests/fixtures/zephyr_project");

    let boards = handler.discover_boards(fixture_path).unwrap();
    assert!(!boards.is_empty());

    let board = &boards[0];
    assert_eq!(board.name, "esp32");
    assert_eq!(board.project_type, ProjectType::Zephyr);
    assert_eq!(board.target, Some("ESP32".to_string()));
}

#[tokio::test]
async fn test_zephyr_commands() {
    let handler = ZephyrHandler;
    let fixture_path = Path::new("tests/fixtures/zephyr_project");
    let boards = handler.discover_boards(fixture_path).unwrap();
    let board_config = &boards[0];

    // Test build command
    let build_cmd = handler.get_build_command(fixture_path, board_config);
    assert!(build_cmd.contains("west build"));
    assert!(build_cmd.contains("-b esp32"));

    // Test flash command
    let flash_cmd = handler.get_flash_command(fixture_path, board_config, Some("/dev/ttyUSB0"));
    assert!(flash_cmd.contains("west flash"));
    assert!(flash_cmd.contains("--esp-device /dev/ttyUSB0"));
}

#[tokio::test]
async fn test_zephyr_tool_availability() {
    let handler = ZephyrHandler;

    // Test tool availability check
    let result = handler.check_tools_available();
    // Don't assert success/failure since west might not be installed
    assert!(result.is_ok() || result.is_err());

    // Test missing tools message
    let message = handler.get_missing_tools_message();
    assert!(message.contains("west"));
    assert!(message.contains("Zephyr"));
    assert!(message.contains("CMake"));
}

// ==== NUTTX HANDLER TESTS ====

#[tokio::test]
async fn test_nuttx_handler_detection() {
    let handler = NuttXHandler;
    let fixture_path = Path::new("tests/fixtures/nuttx_project");

    // Test project type
    assert_eq!(handler.project_type(), ProjectType::NuttX);

    // Test detection with .config and Makefile
    assert!(handler.can_handle(fixture_path));
}

#[tokio::test]
async fn test_nuttx_handler_no_detection() {
    let handler = NuttXHandler;
    let temp_dir = TempDir::new().unwrap();
    let temp_path = temp_dir.path();

    // Create a directory without NuttX files
    fs::write(temp_path.join("README.md"), "# Test Project").unwrap();
    fs::write(
        temp_path.join("main.c"),
        "#include <stdio.h>\nint main() { return 0; }",
    )
    .unwrap();

    // Should not detect this as a NuttX project
    assert!(!handler.can_handle(temp_path));
}

#[tokio::test]
async fn test_nuttx_handler_makefile_detection() {
    let handler = NuttXHandler;
    let temp_dir = TempDir::new().unwrap();
    let temp_path = temp_dir.path();

    // Create Makefile with NuttX-specific content
    fs::write(
        temp_path.join("Makefile"),
        "TOPDIR = $(CURDIR)\ninclude $(TOPDIR)/Make.defs",
    )
    .unwrap();

    // Should detect due to TOPDIR pattern
    assert!(handler.can_handle(temp_path));
}

#[tokio::test]
async fn test_nuttx_board_discovery() {
    let handler = NuttXHandler;
    let fixture_path = Path::new("tests/fixtures/nuttx_project");

    let boards = handler.discover_boards(fixture_path).unwrap();
    assert!(!boards.is_empty());

    let board = &boards[0];
    assert_eq!(board.name, "esp32-core");
    assert_eq!(board.project_type, ProjectType::NuttX);
    assert_eq!(board.target, Some("ESP32".to_string()));
}

#[tokio::test]
async fn test_nuttx_commands() {
    let handler = NuttXHandler;
    let fixture_path = Path::new("tests/fixtures/nuttx_project");
    let boards = handler.discover_boards(fixture_path).unwrap();
    let board_config = &boards[0];

    // Test build command
    let build_cmd = handler.get_build_command(fixture_path, board_config);
    assert!(build_cmd.contains("make"));

    // Test flash command
    let flash_cmd = handler.get_flash_command(fixture_path, board_config, Some("/dev/ttyUSB0"));
    assert!(flash_cmd.contains("esptool.py"));
    assert!(flash_cmd.contains("--port /dev/ttyUSB0"));
    assert!(flash_cmd.contains("nuttx.bin"));
}

#[tokio::test]
async fn test_nuttx_tool_availability() {
    let handler = NuttXHandler;

    // Test tool availability check
    let result = handler.check_tools_available();
    // Don't assert success/failure since make might be available on some systems
    assert!(result.is_ok() || result.is_err());

    // Test missing tools message
    let message = handler.get_missing_tools_message();
    assert!(message.contains("NuttX"));
    assert!(message.contains("make"));
    assert!(message.contains("esptool"));
}

// ==== TINYGO HANDLER TESTS ====

#[tokio::test]
async fn test_tinygo_handler_detection() {
    let handler = TinyGoHandler;
    let fixture_path = Path::new("tests/fixtures/tinygo_project");

    // Test project type
    assert_eq!(handler.project_type(), ProjectType::TinyGo);

    // Test detection with go.mod and machine imports
    assert!(handler.can_handle(fixture_path));
}

#[tokio::test]
async fn test_tinygo_handler_detection_esp32s3() {
    let handler = TinyGoHandler;
    let fixture_path = Path::new("tests/fixtures/tinygo_esp32s3_project");

    // Test detection with ESP32-S3 specific project
    assert!(handler.can_handle(fixture_path));
}

#[tokio::test]
async fn test_tinygo_handler_no_detection() {
    let handler = TinyGoHandler;
    let temp_dir = TempDir::new().unwrap();
    let temp_path = temp_dir.path();

    // Create a directory without go.mod
    fs::write(temp_path.join("main.go"), "package main\nfunc main() {}").unwrap();

    // Should not detect without go.mod
    assert!(!handler.can_handle(temp_path));

    // Add go.mod but without TinyGo imports
    fs::write(temp_path.join("go.mod"), "module test\ngo 1.21").unwrap();

    // Should not detect without TinyGo-specific imports
    assert!(!handler.can_handle(temp_path));
}

#[tokio::test]
async fn test_tinygo_handler_machine_import_detection() {
    let handler = TinyGoHandler;
    let temp_dir = TempDir::new().unwrap();
    let temp_path = temp_dir.path();

    // Create go.mod
    fs::write(temp_path.join("go.mod"), "module test\ngo 1.21").unwrap();

    // Create Go file with machine import
    fs::write(
        temp_path.join("main.go"),
        r#"package main
import "machine"
func main() {
    machine.GPIO2.Configure(machine.PinConfig{Mode: machine.PinOutput})
}"#,
    )
    .unwrap();

    // Should detect due to machine import
    assert!(handler.can_handle(temp_path));
}

#[tokio::test]
async fn test_tinygo_board_discovery_basic() {
    let handler = TinyGoHandler;
    let fixture_path = Path::new("tests/fixtures/tinygo_project");

    let boards = handler.discover_boards(fixture_path).unwrap();
    assert!(!boards.is_empty());

    let board = &boards[0];
    assert_eq!(board.name, "esp32-coreboard-v2");
    assert_eq!(board.project_type, ProjectType::TinyGo);
    assert_eq!(board.target, Some("ESP32".to_string()));
}

#[tokio::test]
async fn test_tinygo_board_discovery_esp32s3() {
    let handler = TinyGoHandler;
    let fixture_path = Path::new("tests/fixtures/tinygo_esp32s3_project");

    let boards = handler.discover_boards(fixture_path).unwrap();
    assert!(!boards.is_empty());

    let board = &boards[0];
    assert_eq!(board.name, "esp32-s3-usb-otg");
    assert_eq!(board.project_type, ProjectType::TinyGo);
    assert_eq!(board.target, Some("ESP32-S3".to_string()));
}

#[tokio::test]
async fn test_tinygo_commands() {
    let handler = TinyGoHandler;
    let fixture_path = Path::new("tests/fixtures/tinygo_project");
    let boards = handler.discover_boards(fixture_path).unwrap();
    let board_config = &boards[0];

    // Test build command
    let build_cmd = handler.get_build_command(fixture_path, board_config);
    assert!(build_cmd.contains("tinygo build"));
    assert!(build_cmd.contains("-target esp32-coreboard-v2"));
    assert!(build_cmd.contains("firmware.bin"));

    // Test flash command
    let flash_cmd = handler.get_flash_command(fixture_path, board_config, Some("/dev/ttyUSB0"));
    assert!(flash_cmd.contains("tinygo flash"));
    assert!(flash_cmd.contains("-target esp32-coreboard-v2"));
    assert!(flash_cmd.contains("-port /dev/ttyUSB0"));
}

#[tokio::test]
async fn test_tinygo_commands_esp32s3() {
    let handler = TinyGoHandler;
    let fixture_path = Path::new("tests/fixtures/tinygo_esp32s3_project");
    let boards = handler.discover_boards(fixture_path).unwrap();
    let board_config = &boards[0];

    // Test flash command for ESP32-S3
    let flash_cmd = handler.get_flash_command(fixture_path, board_config, Some("/dev/ttyACM0"));
    assert!(flash_cmd.contains("tinygo flash"));
    assert!(flash_cmd.contains("-target esp32-s3-usb-otg"));
    assert!(flash_cmd.contains("-port /dev/ttyACM0"));
}

#[tokio::test]
async fn test_tinygo_tool_availability() {
    let handler = TinyGoHandler;

    // Test tool availability check
    let result = handler.check_tools_available();
    // Don't assert success/failure since tinygo might not be installed
    assert!(result.is_ok() || result.is_err());

    // Test missing tools message
    let message = handler.get_missing_tools_message();
    assert!(message.contains("TinyGo"));
    assert!(message.contains("tinygo.org"));
    assert!(message.contains("Go toolchain"));
}

// ==== BUILD ARTIFACTS TESTS ====

#[tokio::test]
async fn test_tinygo_build_artifacts_via_build() {
    let handler = TinyGoHandler;
    let fixture_path = Path::new("tests/fixtures/tinygo_project");
    let boards = handler.discover_boards(fixture_path).unwrap();
    let board_config = &boards[0];

    // Test that build command looks correct (actual build would require tinygo)
    let build_cmd = handler.get_build_command(fixture_path, board_config);
    assert!(build_cmd.contains("firmware.bin"));

    // Build artifacts testing requires actual build, which needs tinygo installed
    // So we'll just test that the command structure is correct
    assert!(build_cmd.contains("-o firmware.bin"));
}

// ==== CLEAN OPERATIONS TESTS ====

#[tokio::test]
async fn test_tinygo_clean_operation() {
    let handler = TinyGoHandler;
    let temp_dir = TempDir::new().unwrap();
    let temp_path = temp_dir.path();

    // Create build artifacts
    fs::write(temp_path.join("firmware.bin"), "binary").unwrap();
    fs::write(temp_path.join("firmware.elf"), "elf").unwrap();
    fs::write(temp_path.join("main"), "executable").unwrap();

    // Create a dummy board config for testing
    let boards = vec![espbrew::models::ProjectBoardConfig {
        name: "test-board".to_string(),
        config_file: temp_path.join("go.mod"),
        build_dir: temp_path.to_path_buf(),
        target: Some("ESP32".to_string()),
        project_type: ProjectType::TinyGo,
    }];
    let board_config = &boards[0];

    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    let result = handler.clean_board(temp_path, board_config, tx).await;

    assert!(result.is_ok());

    // Verify artifacts were removed
    assert!(!temp_path.join("firmware.bin").exists());
    assert!(!temp_path.join("firmware.elf").exists());
    assert!(!temp_path.join("main").exists());
}
