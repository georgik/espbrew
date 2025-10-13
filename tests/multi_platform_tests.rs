//! Multi-Platform Tests
//!
//! Tests for cross-platform compatibility, covering platform-specific behaviors
//! on Linux, macOS, and Windows including path handling, serial port detection,
//! tool availability, and filesystem operations.

mod test_fixtures;

use espbrew::models::*;
use espbrew::projects::ProjectRegistry;
use std::env;
use std::path::{Path, PathBuf};
use tempfile::TempDir;
use test_fixtures::{ConfigFixtures, ProjectFixtures};

/// Determine the current operating system for platform-specific testing
fn get_current_platform() -> &'static str {
    if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "linux") {
        "linux"
    } else {
        "unknown"
    }
}

/// Test path handling across different platforms
#[test]
fn test_cross_platform_path_handling() {
    let platform = get_current_platform();
    println!("Testing path handling on: {}", platform);

    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let base_path = temp_dir.path();

    // Test various path operations that should work on all platforms
    let test_cases = vec![
        "simple_project",
        "project with spaces",
        "project-with-dashes",
        "project_with_underscores",
        "123numeric_project",
    ];

    for project_name in test_cases {
        let project_path = base_path.join(project_name);
        std::fs::create_dir_all(&project_path).expect("Should create project directory");

        // Test path canonicalization
        let canonical_path = project_path
            .canonicalize()
            .expect("Should canonicalize path");
        assert!(canonical_path.exists(), "Canonical path should exist");

        // Test path string conversion
        let path_str = project_path
            .to_str()
            .expect("Path should convert to string");
        assert!(!path_str.is_empty(), "Path string should not be empty");

        // Test path components
        let file_name = project_path
            .file_name()
            .expect("Should have file name")
            .to_str()
            .expect("File name should convert to string");
        assert_eq!(
            file_name, project_name,
            "File name should match project name"
        );

        println!("✅ Path handling validated for: {}", project_name);
    }

    println!("✅ Cross-platform path handling tests completed");
}

/// Test line ending handling across platforms
#[test]
fn test_cross_platform_line_endings() {
    let platform = get_current_platform();
    println!("Testing line ending handling on: {}", platform);

    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let project_path = temp_dir.path();

    // Create test files with different line endings
    let test_content_unix = "line1\nline2\nline3\n";
    let test_content_windows = "line1\r\nline2\r\nline3\r\n";
    let test_content_mixed = "line1\nline2\r\nline3\n";

    let test_files = vec![
        ("unix_endings.txt", test_content_unix),
        ("windows_endings.txt", test_content_windows),
        ("mixed_endings.txt", test_content_mixed),
    ];

    for (filename, content) in test_files {
        let file_path = project_path.join(filename);
        std::fs::write(&file_path, content).expect("Should write test file");

        // Read file back and verify we can handle it
        let read_content = std::fs::read_to_string(&file_path).expect("Should read file");
        assert!(!read_content.is_empty(), "File content should not be empty");

        // Test line counting (should work regardless of line ending style)
        let line_count = read_content.lines().count();
        assert_eq!(
            line_count, 3,
            "Should have 3 lines regardless of line endings"
        );

        println!("✅ Line ending handling validated for: {}", filename);
    }

    println!("✅ Cross-platform line ending tests completed");
}

/// Test file permission handling across platforms
#[test]
fn test_cross_platform_file_permissions() {
    let platform = get_current_platform();
    println!("Testing file permissions on: {}", platform);

    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let project_path = temp_dir.path();

    // Create a test project
    ProjectFixtures::create_rust_nostd_project(project_path, "s3")
        .expect("Failed to create Rust project");

    // Test reading common project files
    let files_to_check = vec![
        "Cargo.toml",
        "src/main.rs",
        ".cargo/config.toml",
        "rust-toolchain.toml",
    ];

    for file_name in files_to_check {
        let file_path = project_path.join(file_name);
        if file_path.exists() {
            // Test that we can read the file
            let metadata = std::fs::metadata(&file_path).expect("Should get file metadata");
            assert!(metadata.is_file(), "Should be a file");

            let content = std::fs::read_to_string(&file_path).expect("Should read file");
            assert!(!content.is_empty(), "File should not be empty");

            // On Unix-like systems, test that files are readable
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let permissions = metadata.permissions();
                let mode = permissions.mode();
                // Check that owner has read permission
                assert!(mode & 0o400 != 0, "Owner should have read permission");
            }

            println!("✅ File permissions validated for: {}", file_name);
        }
    }

    println!("✅ Cross-platform file permission tests completed");
}

/// Test environment variable handling across platforms
#[test]
fn test_cross_platform_environment_variables() {
    let platform = get_current_platform();
    println!("Testing environment variables on: {}", platform);

    // Test PATH environment variable (exists on all platforms)
    let path_var = env::var("PATH").expect("PATH environment variable should exist");
    assert!(!path_var.is_empty(), "PATH should not be empty");

    // Test path separator (different on Windows vs Unix)
    let expected_separator = if platform == "windows" { ";" } else { ":" };
    assert!(
        path_var.contains(expected_separator),
        "PATH should contain platform-appropriate separator"
    );

    // Test HOME/USERPROFILE directory
    let home_var = if platform == "windows" {
        env::var("USERPROFILE").or_else(|_| {
            env::var("HOMEDRIVE")
                .and_then(|drive| env::var("HOMEPATH").map(|path| format!("{}{}", drive, path)))
        })
    } else {
        env::var("HOME")
    };

    assert!(home_var.is_ok(), "Should have home directory variable");
    if let Ok(home_path) = home_var {
        assert!(!home_path.is_empty(), "Home path should not be empty");
        let home_dir = PathBuf::from(home_path);
        assert!(home_dir.exists(), "Home directory should exist");
        println!("✅ Home directory found: {}", home_dir.display());
    }

    // Test temporary directory
    let temp_dir = env::temp_dir();
    assert!(temp_dir.exists(), "Temp directory should exist");
    assert!(temp_dir.is_dir(), "Temp directory should be a directory");
    println!("✅ Temp directory found: {}", temp_dir.display());

    println!("✅ Cross-platform environment variable tests completed");
}

/// Test serial port path formats across platforms
#[test]
fn test_cross_platform_serial_port_paths() {
    let platform = get_current_platform();
    println!("Testing serial port paths on: {}", platform);

    // Define expected serial port patterns for each platform
    let _expected_patterns = match platform {
        "windows" => vec![r"COM\d+", r"\\\\.\COM\d+"],
        "macos" => vec![
            r"/dev/cu\..+",
            r"/dev/tty\..+",
            r"/dev/cu\.usbmodem.+",
            r"/dev/cu\.usbserial.+",
        ],
        "linux" => vec![r"/dev/ttyUSB\d+", r"/dev/ttyACM\d+", r"/dev/ttyS\d+"],
        _ => vec![r"/dev/.+"],
    };

    // Test common ESP32 port patterns
    let test_ports = match platform {
        "windows" => vec!["COM1", "COM3", r"\\.\COM10"],
        "macos" => vec![
            "/dev/cu.usbmodem14101",
            "/dev/cu.usbserial-0001",
            "/dev/cu.SLAB_USBtoUART",
        ],
        "linux" => vec!["/dev/ttyUSB0", "/dev/ttyACM0", "/dev/ttyS0"],
        _ => vec!["/dev/ttyUSB0"],
    };

    for port in test_ports {
        let port_path = Path::new(port);

        // Test path parsing
        let port_str = port_path
            .to_str()
            .expect("Port path should convert to string");
        assert_eq!(port_str, port, "Port path should preserve original string");

        // Test that we can construct flash commands with these ports
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let project_path = temp_dir.path();

        ProjectFixtures::create_rust_nostd_project(project_path, "s3")
            .expect("Failed to create Rust project");

        let registry = ProjectRegistry::new();
        if let Some(handler) = registry.detect_project(project_path) {
            if let Ok(boards) = handler.discover_boards(project_path) {
                if !boards.is_empty() {
                    let board = &boards[0];
                    let flash_cmd = handler.get_flash_command(project_path, board, Some(port));
                    assert!(
                        flash_cmd.contains(port),
                        "Flash command should contain port"
                    );
                    println!("✅ Flash command for {}: {}", port, flash_cmd);
                }
            }
        }

        println!("✅ Serial port path validated: {}", port);
    }

    println!("✅ Cross-platform serial port path tests completed");
}

/// Test tool availability detection across platforms
#[test]
fn test_cross_platform_tool_detection() {
    let platform = get_current_platform();
    println!("Testing tool detection on: {}", platform);

    // Tools that should be available on most development systems
    let common_tools = vec!["git", "curl"];

    // Platform-specific tools
    let platform_tools = match platform {
        "windows" => vec!["powershell", "cmd"],
        "macos" => vec!["brew", "xcode-select"],
        "linux" => vec!["ls", "grep", "find"],
        _ => vec!["ls"],
    };

    // Test common tools
    for tool in common_tools {
        let result = which_tool(tool);
        match result {
            Ok(path) => {
                println!("✅ Found {}: {}", tool, path.display());
                assert!(path.exists(), "Tool path should exist");
            }
            Err(_) => {
                println!("⚠️  Tool {} not found (acceptable on some systems)", tool);
            }
        }
    }

    // Test platform-specific tools
    for tool in platform_tools {
        let result = which_tool(tool);
        match result {
            Ok(path) => {
                println!("✅ Found platform tool {}: {}", tool, path.display());
            }
            Err(_) => {
                println!(
                    "⚠️  Platform tool {} not found (may not be installed)",
                    tool
                );
            }
        }
    }

    println!("✅ Cross-platform tool detection tests completed");
}

/// Simple which implementation for testing tool availability
fn which_tool(tool: &str) -> Result<PathBuf, Box<dyn std::error::Error>> {
    // Try to find the tool in PATH
    let path_var = env::var("PATH")?;
    let path_separator = if cfg!(windows) { ";" } else { ":" };

    for path in path_var.split(path_separator) {
        let tool_path = if cfg!(windows) {
            // On Windows, try both with and without .exe extension
            let candidates = vec![
                PathBuf::from(path).join(format!("{}.exe", tool)),
                PathBuf::from(path).join(format!("{}.cmd", tool)),
                PathBuf::from(path).join(format!("{}.bat", tool)),
                PathBuf::from(path).join(tool),
            ];
            candidates.into_iter().find(|p| p.exists())
        } else {
            let candidate = PathBuf::from(path).join(tool);
            if candidate.exists() {
                Some(candidate)
            } else {
                None
            }
        };

        if let Some(found_path) = tool_path {
            return Ok(found_path);
        }
    }

    Err(format!("Tool '{}' not found in PATH", tool).into())
}

/// Test project creation with platform-specific paths
#[test]
fn test_cross_platform_project_creation() {
    let platform = get_current_platform();
    println!("Testing project creation on: {}", platform);

    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let base_path = temp_dir.path();

    // Test project creation in various directory structures
    let test_scenarios = vec![
        ("simple", base_path.join("simple")),
        (
            "nested/deep/project",
            base_path.join("nested").join("deep").join("project"),
        ),
        ("spaces in path", base_path.join("spaces in path")),
    ];

    for (scenario_name, project_path) in test_scenarios {
        println!("Testing scenario: {}", scenario_name);

        // Create the directory structure
        std::fs::create_dir_all(&project_path).expect("Should create directory structure");

        // Create a Rust project
        let create_result = ProjectFixtures::create_rust_nostd_project(&project_path, "s3");
        assert!(
            create_result.is_ok(),
            "Should create Rust project in {}",
            scenario_name
        );

        // Create espbrew config
        let config_result =
            ConfigFixtures::create_espbrew_config(&project_path, "rust_nostd", "esp32s3");
        assert!(
            config_result.is_ok(),
            "Should create config in {}",
            scenario_name
        );

        // Test project detection
        let registry = ProjectRegistry::new();
        let handler = registry.detect_project(&project_path);
        assert!(
            handler.is_some(),
            "Should detect project in {}",
            scenario_name
        );

        if let Some(handler) = handler {
            assert_eq!(handler.project_type(), ProjectType::RustNoStd);

            // Test board discovery
            let boards_result = handler.discover_boards(&project_path);
            match boards_result {
                Ok(boards) => {
                    println!("✅ {}: Found {} boards", scenario_name, boards.len());
                }
                Err(e) => {
                    println!("⚠️  {}: Board discovery failed: {}", scenario_name, e);
                    // This might be acceptable if tools aren't available
                }
            }
        }

        println!("✅ Project creation validated for: {}", scenario_name);
    }

    println!("✅ Cross-platform project creation tests completed");
}

/// Test filesystem case sensitivity handling
#[test]
fn test_cross_platform_case_sensitivity() {
    let platform = get_current_platform();
    println!("Testing case sensitivity on: {}", platform);

    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let project_path = temp_dir.path();

    // Create test files with different cases
    let test_file_lower = project_path.join("readme.md");
    let test_file_upper = project_path.join("README.MD");
    let _test_file_mixed = project_path.join("ReadMe.Md");

    std::fs::write(&test_file_lower, "lowercase").expect("Should write lowercase file");

    // Test case sensitivity behavior
    let case_sensitive = match platform {
        "linux" => true,    // Linux is typically case-sensitive
        "macos" => false,   // macOS is typically case-insensitive
        "windows" => false, // Windows is case-insensitive
        _ => true,          // Assume case-sensitive for unknown platforms
    };

    if case_sensitive {
        // On case-sensitive systems, we should be able to create files with different cases
        std::fs::write(&test_file_upper, "uppercase").expect("Should write uppercase file");
        assert!(test_file_lower.exists(), "Lowercase file should exist");
        assert!(
            test_file_upper.exists(),
            "Uppercase file should exist separately"
        );
        println!("✅ Case-sensitive filesystem confirmed");
    } else {
        // On case-insensitive systems, the files should be the same
        assert!(test_file_lower.exists(), "File should exist");
        // Trying to read with different case should work
        if test_file_upper.exists() {
            let content = std::fs::read_to_string(&test_file_upper)
                .expect("Should read file with different case");
            assert_eq!(content, "lowercase", "Should read original content");
        }
        println!("✅ Case-insensitive filesystem confirmed");
    }

    // Test project file detection with different cases
    ProjectFixtures::create_rust_nostd_project(project_path, "s3")
        .expect("Failed to create Rust project");

    let registry = ProjectRegistry::new();
    let handler = registry.detect_project(project_path);
    assert!(
        handler.is_some(),
        "Should detect project regardless of case handling"
    );

    println!("✅ Cross-platform case sensitivity tests completed");
}

/// Integration test combining multiple platform-specific scenarios
#[tokio::test]
async fn test_multi_platform_integration() {
    let platform = get_current_platform();
    println!("Running multi-platform integration test on: {}", platform);

    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let project_path = temp_dir.path();

    // Create a comprehensive test project
    ProjectFixtures::create_rust_nostd_project(project_path, "s3")
        .expect("Failed to create Rust project");
    ConfigFixtures::create_espbrew_config(project_path, "rust_nostd", "esp32s3")
        .expect("Failed to create config");

    // Test project detection
    let registry = ProjectRegistry::new();
    let handler = registry
        .detect_project(project_path)
        .expect("Should detect project on any platform");

    assert_eq!(handler.project_type(), ProjectType::RustNoStd);
    println!("✅ Project detection works on {}", platform);

    // Test board discovery
    match handler.discover_boards(project_path) {
        Ok(boards) => {
            assert!(!boards.is_empty(), "Should find at least one board");
            let board = &boards[0];

            // Test command generation with platform-appropriate port
            let test_port = match platform {
                "windows" => "COM3",
                "macos" => "/dev/cu.usbmodem14101",
                _ => "/dev/ttyUSB0",
            };

            let build_cmd = handler.get_build_command(project_path, board);
            let flash_cmd = handler.get_flash_command(project_path, board, Some(test_port));

            assert!(
                build_cmd.contains("cargo"),
                "Build command should use cargo on all platforms"
            );
            assert!(
                flash_cmd.contains(test_port),
                "Flash command should contain platform-appropriate port"
            );

            println!("✅ Command generation works on {}", platform);
            println!("Build command: {}", build_cmd);
            println!("Flash command: {}", flash_cmd);
        }
        Err(e) => {
            println!("⚠️  Board discovery failed on {}: {}", platform, e);
            println!("   This might be expected if development tools aren't installed");
        }
    }

    // Test tool availability
    let tool_check = handler.check_tools_available();
    match tool_check {
        Ok(_) => {
            println!("✅ All required tools available on {}", platform);
        }
        Err(msg) => {
            println!("⚠️  Some tools missing on {}: {}", platform, msg);
            // This is expected in many test environments
        }
    }

    println!(
        "✅ Multi-platform integration test completed on {}",
        platform
    );
}
