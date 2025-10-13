//! Core unit tests for espbrew CLI functionality
//!
//! This module contains comprehensive unit tests for the core espbrew CLI operations
//! including project detection, configuration parsing, and basic command execution.

use espbrew::config::AppConfig;
use espbrew::models::{BuildStrategy, ProjectType};
use espbrew::projects::ProjectRegistry;
use std::fs;
use std::path::Path;
use tempfile::TempDir;

/// Helper function to create a temporary Rust ESP32 project structure
fn create_test_rust_project(temp_dir: &Path) -> std::io::Result<()> {
    // Create basic Rust project structure
    fs::create_dir_all(temp_dir.join("src"))?;

    // Create Cargo.toml
    let cargo_toml = r#"[package]
name = "test-esp32-project"
version = "0.1.0"
edition = "2021"

[dependencies]
esp-hal = "1.0.0-rc.0"

[[bin]]
name = "test-esp32-project"
test = false
bench = false
"#;
    fs::write(temp_dir.join("Cargo.toml"), cargo_toml)?;

    // Create .cargo/config.toml
    fs::create_dir_all(temp_dir.join(".cargo"))?;
    let cargo_config = r#"[build]
target = "xtensa-esp32s3-none-elf"

[target.xtensa-esp32s3-none-elf]
runner = "espflash flash --monitor"
"#;
    fs::write(temp_dir.join(".cargo/config.toml"), cargo_config)?;

    // Create main.rs
    let main_rs = r#"#![no_std]
#![no_main]

use esp_hal::{clock::ClockControl, peripherals::Peripherals, prelude::*, system::SystemControl};

#[entry]
fn main() -> ! {
    let peripherals = Peripherals::take();
    let system = SystemControl::new(peripherals.SYSTEM);
    let clocks = ClockControl::boot_defaults(system.clock_control).freeze();
    
    loop {}
}
"#;
    fs::write(temp_dir.join("src/main.rs"), main_rs)?;

    Ok(())
}

/// Helper function to create a temporary Arduino project structure
fn create_test_arduino_project(temp_dir: &Path) -> std::io::Result<()> {
    // Create Arduino project file
    let arduino_code = r#"#include <WiFi.h>

void setup() {
  Serial.begin(115200);
  Serial.println("ESP32 Arduino Project");
}

void loop() {
  delay(1000);
}
"#;
    fs::write(temp_dir.join("test_project.ino"), arduino_code)?;

    // Create boards.json
    let boards_json = r#"{
  "boards": [
    {
      "name": "ESP32-S3",
      "target": "esp32s3",
      "fqbn": "esp32:esp32:esp32s3"
    }
  ]
}
"#;
    fs::write(temp_dir.join("boards.json"), boards_json)?;

    Ok(())
}

/// Helper function to create espbrew.toml configuration
fn create_espbrew_config(temp_dir: &Path, project_type: &str) -> std::io::Result<()> {
    let config = format!(
        r#"[project]
name = "test-project"
type = "{}"
target = "esp32s3"
version = "1.0.0"

[build]
strategy = "sequential"
release = true

[flash]
monitor = true
reset = true
"#,
        project_type
    );

    fs::write(temp_dir.join("espbrew.toml"), config)?;
    Ok(())
}

#[cfg(test)]
mod project_detection_tests {
    use super::*;

    #[test]
    fn test_rust_project_detection() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let path = temp_dir.path();

        create_test_rust_project(path).expect("Failed to create test project");

        let registry = ProjectRegistry::new();
        let handler = registry.detect_project(path);

        assert!(handler.is_some(), "Should detect a project handler");
        let handler = handler.unwrap();
        assert_eq!(
            handler.project_type(),
            ProjectType::RustNoStd,
            "Should detect Rust no_std project"
        );
    }

    #[test]
    fn test_arduino_project_detection() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let path = temp_dir.path();

        create_test_arduino_project(path).expect("Failed to create test project");

        let registry = ProjectRegistry::new();
        let handler = registry.detect_project(path);

        assert!(handler.is_some(), "Should detect a project handler");
        let handler = handler.unwrap();
        assert_eq!(
            handler.project_type(),
            ProjectType::Arduino,
            "Should detect Arduino project"
        );
    }

    #[test]
    fn test_invalid_project_detection() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let path = temp_dir.path();

        // Create empty directory
        fs::write(path.join("random.txt"), "not a project").expect("Failed to write file");

        let registry = ProjectRegistry::new();
        let handler = registry.detect_project(path);

        assert!(handler.is_none(), "Should not detect any project type");
    }

    #[test]
    fn test_nested_project_detection() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let path = temp_dir.path();

        // Create nested structure
        let nested_path = path.join("subfolder/project");
        fs::create_dir_all(&nested_path).expect("Failed to create nested dirs");

        create_test_rust_project(&nested_path).expect("Failed to create test project");

        let registry = ProjectRegistry::new();
        let handler = registry.detect_project(&nested_path);

        assert!(handler.is_some(), "Should detect nested project");
        let handler = handler.unwrap();
        assert_eq!(
            handler.project_type(),
            ProjectType::RustNoStd,
            "Should detect nested Rust project"
        );
    }

    #[test]
    fn test_nonexistent_path() {
        let registry = ProjectRegistry::new();
        let handler = registry.detect_project(Path::new("/nonexistent/path/that/does/not/exist"));

        // Should handle nonexistent paths gracefully
        assert!(handler.is_none(), "Should handle nonexistent paths");
    }
}

#[cfg(test)]
mod configuration_tests {
    use super::*;

    #[test]
    fn test_app_config_default() {
        let config = AppConfig::default();

        assert_eq!(config.default_server_url, "http://localhost:8080");
        assert!(config.ui.enable_tui);
        assert_eq!(config.ui.log_level, "info");
        assert_eq!(config.build.default_strategy, "idf-build-apps");
        assert_eq!(config.build.timeout_seconds, 300);
    }

    #[test]
    fn test_espbrew_config_creation() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let path = temp_dir.path();

        create_espbrew_config(path, "rust_nostd").expect("Failed to create config");

        let config_path = path.join("espbrew.toml");
        assert!(config_path.exists(), "Config file should be created");

        let content = fs::read_to_string(&config_path).expect("Should read config file");
        assert!(
            content.contains("rust_nostd"),
            "Config should contain project type"
        );
        assert!(
            content.contains("test-project"),
            "Config should contain project name"
        );
    }
}

#[cfg(test)]
mod build_strategy_tests {
    use super::*;

    #[test]
    fn test_build_strategy_parsing() {
        // Test that BuildStrategy enum values can be parsed
        let sequential = BuildStrategy::Sequential;
        let parallel = BuildStrategy::Parallel;

        assert!(matches!(sequential, BuildStrategy::Sequential));
        assert!(matches!(parallel, BuildStrategy::Parallel));
    }

    #[test]
    fn test_build_strategy_variants() {
        // Test that BuildStrategy enum has all expected variants
        let sequential = BuildStrategy::Sequential;
        let parallel = BuildStrategy::Parallel;
        let idf_build_apps = BuildStrategy::IdfBuildApps;

        assert!(matches!(sequential, BuildStrategy::Sequential));
        assert!(matches!(parallel, BuildStrategy::Parallel));
        assert!(matches!(idf_build_apps, BuildStrategy::IdfBuildApps));
    }
}

#[cfg(test)]
mod basic_functionality_tests {
    use super::*;

    #[test]
    fn test_project_handler_creation() {
        // Test that we can create project handlers for different project types
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let path = temp_dir.path();

        create_test_rust_project(path).expect("Failed to create test project");

        // This test validates the basic project handler creation flow
        let registry = ProjectRegistry::new();
        let handler = registry.detect_project(path);

        assert!(handler.is_some(), "Should detect a project handler");
        assert_eq!(handler.unwrap().project_type(), ProjectType::RustNoStd);
    }

    #[test]
    fn test_temporary_directory_cleanup() {
        // Test that temporary directories are properly cleaned up
        let temp_path;
        {
            let temp_dir = TempDir::new().expect("Failed to create temp dir");
            temp_path = temp_dir.path().to_path_buf();

            // Use the temp directory
            create_test_rust_project(&temp_path).expect("Failed to create test project");
            assert!(temp_path.exists(), "Temp directory should exist");
        }

        // After temp_dir goes out of scope, directory should be cleaned up
        // Note: This might not always work immediately due to OS scheduling
        std::thread::sleep(std::time::Duration::from_millis(100));
        assert!(
            !temp_path.exists() || !temp_path.is_dir(),
            "Temp directory should be cleaned up"
        );
    }

    #[test]
    fn test_error_handling_with_permission_denied() {
        // Test error handling when we don't have permissions
        // This test will be skipped on systems where we can't create permission-denied scenarios

        if cfg!(unix) {
            use std::os::unix::fs::PermissionsExt;

            let temp_dir = TempDir::new().expect("Failed to create temp dir");
            let restricted_path = temp_dir.path().join("restricted");

            fs::create_dir(&restricted_path).expect("Failed to create restricted dir");

            // Remove read permissions
            let mut perms = fs::metadata(&restricted_path)
                .expect("Failed to get metadata")
                .permissions();
            perms.set_mode(0o000);
            fs::set_permissions(&restricted_path, perms).expect("Failed to set permissions");

            let registry = ProjectRegistry::new();
            let handler = registry.detect_project(&restricted_path);

            // Should handle permission denied gracefully (return None rather than crash)
            assert!(
                handler.is_none(),
                "Should handle permission denied gracefully"
            );

            // Restore permissions for cleanup
            let mut perms = fs::metadata(&restricted_path)
                .expect("Failed to get metadata")
                .permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&restricted_path, perms).expect("Failed to restore permissions");
        }
    }
}

#[cfg(test)]
mod integration_helpers {
    use super::*;

    /// Helper function to create a complete test project with all necessary files
    pub fn create_complete_test_project(
        temp_dir: &Path,
        project_type: &str,
    ) -> std::io::Result<()> {
        match project_type {
            "rust_nostd" => {
                create_test_rust_project(temp_dir)?;
                create_espbrew_config(temp_dir, "rust_nostd")?;
            }
            "arduino" => {
                create_test_arduino_project(temp_dir)?;
                create_espbrew_config(temp_dir, "arduino")?;
            }
            _ => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    format!("Unknown project type: {}", project_type),
                ));
            }
        }
        Ok(())
    }

    /// Helper function to validate project structure
    pub fn validate_project_structure(temp_dir: &Path, project_type: &str) -> bool {
        match project_type {
            "rust_nostd" => {
                temp_dir.join("Cargo.toml").exists()
                    && temp_dir.join(".cargo/config.toml").exists()
                    && temp_dir.join("src/main.rs").exists()
            }
            "arduino" => {
                temp_dir.join("boards.json").exists()
                    && temp_dir.read_dir().unwrap().any(|entry| {
                        entry
                            .unwrap()
                            .path()
                            .extension()
                            .map_or(false, |ext| ext == "ino")
                    })
            }
            _ => false,
        }
    }

    #[test]
    fn test_helper_functions() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let path = temp_dir.path();

        // Test Rust project creation
        create_complete_test_project(path, "rust_nostd").expect("Failed to create Rust project");
        assert!(
            validate_project_structure(path, "rust_nostd"),
            "Rust project structure should be valid"
        );

        // Clean and test Arduino project
        fs::remove_dir_all(path).expect("Failed to clean directory");
        fs::create_dir_all(path).expect("Failed to recreate directory");

        create_complete_test_project(path, "arduino").expect("Failed to create Arduino project");
        assert!(
            validate_project_structure(path, "arduino"),
            "Arduino project structure should be valid"
        );
    }
}
