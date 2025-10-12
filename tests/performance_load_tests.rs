//! Performance and Load Testing Suite
//!
//! Comprehensive performance tests for espbrew including:
//! - Large project handling and scalability
//! - Concurrent operations and thread safety
//! - Memory usage and resource management
//! - Response time benchmarks
//! - Load testing with multiple devices and projects

mod mock_hardware;
mod test_fixtures;

use espbrew::projects::ProjectRegistry;
use mock_hardware::{MockEsp32Device, MockHardwareEnvironment};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tempfile::TempDir;
use test_fixtures::{ConfigFixtures, ProjectFixtures, TestEnvironment};
use tokio::sync::Semaphore;

/// Benchmark thresholds for performance validation
const MAX_PROJECT_DETECTION_TIME_MS: u128 = 100;
const MAX_BOARD_DISCOVERY_TIME_MS: u128 = 500;
const MAX_COMMAND_GENERATION_TIME_MS: u128 = 50;
const MAX_CONCURRENT_OPERATIONS: usize = 20;
const MAX_MEMORY_GROWTH_MB: f64 = 110.0;

/// Test project detection performance with various project sizes
#[test]
fn test_project_detection_performance() {
    println!("Testing project detection performance...");

    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let base_path = temp_dir.path();

    let registry = ProjectRegistry::new();
    let mut detection_times = Vec::new();

    // Test with different project types and complexities
    let test_scenarios = vec![
        ("simple_rust", "rust_nostd", "s3"),
        ("complex_rust", "rust_nostd", "s3"),
        ("arduino", "arduino", ""),
        ("esp_idf", "esp_idf", ""),
        ("micropython", "micropython", ""),
    ];

    for (project_name, project_type, chip) in test_scenarios {
        let project_path = base_path.join(project_name);

        // Create project based on type
        let creation_result = match project_type {
            "rust_nostd" => ProjectFixtures::create_rust_nostd_project(&project_path, chip),
            "arduino" => ProjectFixtures::create_arduino_project(&project_path, project_name),
            "esp_idf" => ProjectFixtures::create_esp_idf_project(&project_path, chip),
            "micropython" => ProjectFixtures::create_micropython_project(&project_path),
            _ => continue,
        };

        if creation_result.is_ok() {
            // Add espbrew config for consistency
            let target = if chip.is_empty() {
                "esp32".to_string()
            } else {
                format!("esp32{}", chip)
            };
            let _ = ConfigFixtures::create_espbrew_config(&project_path, project_type, &target);

            // Measure detection time
            let start = Instant::now();
            let detection_result = registry.detect_project(&project_path);
            let detection_time = start.elapsed();

            detection_times.push((project_name, detection_time));

            if detection_result.is_some() {
                println!("✅ {} detected in {:?}", project_name, detection_time);
            } else {
                println!("⚠️  {} not detected", project_name);
            }

            // Validate performance threshold
            assert!(
                detection_time.as_millis() < MAX_PROJECT_DETECTION_TIME_MS,
                "Project detection for {} took too long: {:?}",
                project_name,
                detection_time
            );
        }
    }

    // Calculate statistics
    let total_time: Duration = detection_times.iter().map(|(_, time)| *time).sum();
    let avg_time = total_time / detection_times.len() as u32;
    let max_time = detection_times
        .iter()
        .map(|(_, time)| *time)
        .max()
        .unwrap_or_default();

    println!("📊 Project Detection Performance Summary:");
    println!("   Total projects: {}", detection_times.len());
    println!("   Average detection time: {:?}", avg_time);
    println!("   Maximum detection time: {:?}", max_time);
    println!(
        "   Performance threshold: {}ms",
        MAX_PROJECT_DETECTION_TIME_MS
    );

    assert!(
        avg_time.as_millis() < MAX_PROJECT_DETECTION_TIME_MS,
        "Average detection time exceeds threshold"
    );

    println!("✅ Project detection performance test completed");
}

/// Test board discovery performance across different project types
#[tokio::test]
async fn test_board_discovery_performance() {
    println!("Testing board discovery performance...");

    let workspace =
        TestEnvironment::create_test_workspace().expect("Failed to create test workspace");
    let workspace_path = workspace.path();

    let registry = ProjectRegistry::new();
    let mut discovery_times = Vec::new();

    // Test board discovery for each project type
    let project_paths = vec![
        workspace_path.join("rust-esp32s3-project"),
        workspace_path.join("arduino-esp32-project"),
        workspace_path.join("esp-idf-project"),
        workspace_path.join("micropython-project"),
    ];

    for project_path in project_paths {
        if let Some(handler) = registry.detect_project(&project_path) {
            let project_type = format!("{:?}", handler.project_type());

            // Measure board discovery time
            let start = Instant::now();
            let boards_result = handler.discover_boards(&project_path);
            let discovery_time = start.elapsed();

            discovery_times.push((project_type.clone(), discovery_time));

            match boards_result {
                Ok(boards) => {
                    println!(
                        "✅ {} board discovery: {} boards in {:?}",
                        project_type,
                        boards.len(),
                        discovery_time
                    );
                }
                Err(e) => {
                    println!(
                        "⚠️  {} board discovery failed: {} (time: {:?})",
                        project_type, e, discovery_time
                    );
                    // Still validate timing even on failure
                }
            }

            // Validate performance threshold
            assert!(
                discovery_time.as_millis() < MAX_BOARD_DISCOVERY_TIME_MS,
                "Board discovery for {} took too long: {:?}",
                project_type,
                discovery_time
            );
        }
    }

    // Performance summary
    if !discovery_times.is_empty() {
        let total_time: Duration = discovery_times.iter().map(|(_, time)| *time).sum();
        let avg_time = total_time / discovery_times.len() as u32;
        let max_time = discovery_times
            .iter()
            .map(|(_, time)| *time)
            .max()
            .unwrap_or_default();

        println!("📊 Board Discovery Performance Summary:");
        println!("   Projects tested: {}", discovery_times.len());
        println!("   Average discovery time: {:?}", avg_time);
        println!("   Maximum discovery time: {:?}", max_time);
        println!(
            "   Performance threshold: {}ms",
            MAX_BOARD_DISCOVERY_TIME_MS
        );

        assert!(
            avg_time.as_millis() < MAX_BOARD_DISCOVERY_TIME_MS,
            "Average board discovery time exceeds threshold"
        );
    }

    println!("✅ Board discovery performance test completed");
}

/// Test command generation performance
#[test]
fn test_command_generation_performance() {
    println!("Testing command generation performance...");

    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let project_path = temp_dir.path();

    // Create a test project
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
    let mut command_times = Vec::new();

    // Test multiple iterations to get consistent timing
    let iterations = 100;
    for i in 0..iterations {
        // Test build command generation
        let start = Instant::now();
        let build_cmd = handler.get_build_command(project_path, board);
        let build_time = start.elapsed();

        // Test flash command generation
        let start = Instant::now();
        let flash_cmd = handler.get_flash_command(project_path, board, Some("/dev/ttyUSB0"));
        let flash_time = start.elapsed();

        command_times.push((build_time, flash_time));

        // Validate commands are generated correctly
        assert!(!build_cmd.is_empty(), "Build command should not be empty");
        assert!(!flash_cmd.is_empty(), "Flash command should not be empty");

        if i == 0 {
            println!("✅ Sample commands generated:");
            println!("   Build: {}", build_cmd);
            println!("   Flash: {}", flash_cmd);
        }
    }

    // Calculate performance statistics
    let avg_build_time: Duration = command_times
        .iter()
        .map(|(build_time, _)| *build_time)
        .sum::<Duration>()
        / iterations as u32;

    let avg_flash_time: Duration = command_times
        .iter()
        .map(|(_, flash_time)| *flash_time)
        .sum::<Duration>()
        / iterations as u32;

    let max_build_time = command_times
        .iter()
        .map(|(build_time, _)| *build_time)
        .max()
        .unwrap_or_default();

    let max_flash_time = command_times
        .iter()
        .map(|(_, flash_time)| *flash_time)
        .max()
        .unwrap_or_default();

    println!("📊 Command Generation Performance Summary:");
    println!("   Iterations: {}", iterations);
    println!("   Average build command time: {:?}", avg_build_time);
    println!("   Average flash command time: {:?}", avg_flash_time);
    println!("   Maximum build command time: {:?}", max_build_time);
    println!("   Maximum flash command time: {:?}", max_flash_time);
    println!(
        "   Performance threshold: {}ms",
        MAX_COMMAND_GENERATION_TIME_MS
    );

    // Validate performance thresholds
    assert!(
        avg_build_time.as_millis() < MAX_COMMAND_GENERATION_TIME_MS,
        "Average build command generation time exceeds threshold"
    );
    assert!(
        avg_flash_time.as_millis() < MAX_COMMAND_GENERATION_TIME_MS,
        "Average flash command generation time exceeds threshold"
    );

    println!("✅ Command generation performance test completed");
}

/// Test concurrent operations performance and thread safety
#[tokio::test]
async fn test_concurrent_operations_performance() {
    println!("Testing concurrent operations performance...");

    let workspace =
        TestEnvironment::create_test_workspace().expect("Failed to create test workspace");
    let workspace_path = workspace.path();

    let registry = Arc::new(ProjectRegistry::new());
    let semaphore = Arc::new(Semaphore::new(MAX_CONCURRENT_OPERATIONS));

    let project_paths = vec![
        workspace_path.join("rust-esp32s3-project"),
        workspace_path.join("arduino-esp32-project"),
        workspace_path.join("esp-idf-project"),
        workspace_path.join("micropython-project"),
    ];

    let mut handles = Vec::new();
    let start_time = Instant::now();

    // Launch concurrent operations
    for i in 0..MAX_CONCURRENT_OPERATIONS {
        let project_path = project_paths[i % project_paths.len()].clone();
        let registry = registry.clone();
        let semaphore = semaphore.clone();

        let handle = tokio::spawn(async move {
            let _permit = semaphore.acquire().await.unwrap();
            let operation_start = Instant::now();

            // Perform multiple operations concurrently
            let detection_result = registry.detect_project_boxed(&project_path);
            let has_detection = detection_result.is_some();

            if let Some(handler) = detection_result {
                let boards_result = handler.discover_boards(&project_path);

                match boards_result {
                    Ok(boards) => {
                        if !boards.is_empty() {
                            let board = &boards[0];
                            let _build_cmd = handler.get_build_command(&project_path, board);
                            let _flash_cmd = handler.get_flash_command(
                                &project_path,
                                board,
                                Some("/dev/ttyUSB0"),
                            );
                        }
                    }
                    Err(_) => {
                        // Some operations may fail due to missing tools, which is acceptable
                    }
                }
            }

            let operation_time = operation_start.elapsed();
            (i, operation_time, has_detection)
        });

        handles.push(handle);
    }

    // Wait for all operations to complete
    let mut results = Vec::new();
    for handle in handles {
        let result = handle.await.expect("Task should complete");
        results.push(result);
    }

    let total_time = start_time.elapsed();

    // Analyze results
    let successful_operations = results.iter().filter(|(_, _, success)| *success).count();
    let operation_times: Vec<Duration> = results.iter().map(|(_, time, _)| *time).collect();

    let avg_operation_time =
        operation_times.iter().sum::<Duration>() / operation_times.len() as u32;
    let max_operation_time = operation_times.iter().max().copied().unwrap_or_default();

    println!("📊 Concurrent Operations Performance Summary:");
    println!("   Total operations: {}", MAX_CONCURRENT_OPERATIONS);
    println!("   Successful operations: {}", successful_operations);
    println!("   Total execution time: {:?}", total_time);
    println!("   Average operation time: {:?}", avg_operation_time);
    println!("   Maximum operation time: {:?}", max_operation_time);
    println!(
        "   Operations per second: {:.2}",
        MAX_CONCURRENT_OPERATIONS as f64 / total_time.as_secs_f64()
    );

    // Validate performance
    assert!(
        successful_operations > 0,
        "At least some concurrent operations should succeed"
    );

    let success_rate = successful_operations as f64 / MAX_CONCURRENT_OPERATIONS as f64;
    assert!(
        success_rate > 0.5,
        "Success rate should be reasonable: {:.2}%",
        success_rate * 100.0
    );

    println!("✅ Concurrent operations performance test completed");
}

/// Test memory usage patterns and resource management
#[test]
fn test_memory_usage_patterns() {
    println!("Testing memory usage patterns...");

    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let base_path = temp_dir.path();

    // Get initial memory usage (approximate)
    let initial_memory = get_approximate_memory_usage();

    let registry = ProjectRegistry::new();
    let mut created_projects = Vec::new();

    // Create and process many projects to test memory growth
    let project_count = 50;
    for i in 0..project_count {
        let project_path = base_path.join(format!("project_{}", i));

        if ProjectFixtures::create_rust_nostd_project(&project_path, "s3").is_ok() {
            if let Some(handler) = registry.detect_project(&project_path) {
                // Perform operations that might consume memory
                let _ = handler.discover_boards(&project_path);

                if let Ok(boards) = handler.discover_boards(&project_path) {
                    if !boards.is_empty() {
                        let board = &boards[0];
                        let _build_cmd = handler.get_build_command(&project_path, board);
                        let _flash_cmd =
                            handler.get_flash_command(&project_path, board, Some("/dev/ttyUSB0"));
                    }
                }
            }

            created_projects.push(project_path);
        }

        // Clean up periodically to test memory management
        if i % 10 == 9 {
            for project_path in &created_projects {
                let _ = std::fs::remove_dir_all(project_path);
            }
            created_projects.clear();

            // Force garbage collection if possible (platform-specific)
            #[cfg(not(target_os = "windows"))]
            {
                // On Unix-like systems, we can suggest GC
                std::hint::black_box(&registry);
            }
        }
    }

    let final_memory = get_approximate_memory_usage();
    let memory_growth = final_memory - initial_memory;

    println!("📊 Memory Usage Analysis:");
    println!("   Projects processed: {}", project_count);
    println!("   Initial memory: {:.2} MB", initial_memory);
    println!("   Final memory: {:.2} MB", final_memory);
    println!("   Memory growth: {:.2} MB", memory_growth);
    println!("   Memory growth threshold: {:.2} MB", MAX_MEMORY_GROWTH_MB);

    // Clean up remaining projects
    for project_path in &created_projects {
        let _ = std::fs::remove_dir_all(project_path);
    }

    // Validate memory usage is reasonable
    assert!(
        memory_growth < MAX_MEMORY_GROWTH_MB,
        "Memory growth exceeds threshold: {:.2} MB",
        memory_growth
    );

    println!("✅ Memory usage patterns test completed");
}

/// Test large project handling performance
#[test]
fn test_large_project_handling() {
    println!("Testing large project handling performance...");

    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let project_path = temp_dir.path();

    // Create a large Rust project
    ProjectFixtures::create_rust_nostd_project(project_path, "s3")
        .expect("Failed to create Rust project");

    // Add many source files to simulate a large project
    let src_dir = project_path.join("src");
    for i in 0..100 {
        let module_content = format!(
            r#"// Module {}
pub fn function_{}() {{
    println!("This is function {} in module {}");
}}

#[cfg(test)]
mod tests {{
    use super::*;

    #[test]
    fn test_function_{}() {{
        function_{}();
    }}
}}
"#,
            i, i, i, i, i, i
        );

        std::fs::write(src_dir.join(format!("module_{}.rs", i)), module_content)
            .expect("Failed to write module file");
    }

    // Update main.rs to include all modules
    let mut main_content = String::from("#![no_std]\n#![no_main]\n\nuse esp_backtrace as _;\n");
    for i in 0..100 {
        main_content.push_str(&format!("mod module_{};\n", i));
    }
    main_content.push_str("\n#[no_mangle]\npub extern \"C\" fn main() {\n    loop {}\n}\n");

    std::fs::write(src_dir.join("main.rs"), main_content).expect("Failed to update main.rs");

    // Create a large espbrew.toml
    let mut large_config = String::new();
    large_config.push_str(
        r#"[project]
name = "large-test-project"
type = "rust_nostd"
target = "esp32s3"
version = "1.0.0"
description = "Large test project for performance testing"
"#,
    );

    // Add many configuration sections
    for i in 0..50 {
        large_config.push_str(&format!(
            r#"
[profile.{profile}]
opt-level = "s"
debug = false
debug-assertions = false

"#,
            profile = format!("profile_{}", i)
        ));
    }

    std::fs::write(project_path.join("espbrew.toml"), large_config)
        .expect("Failed to write large config");

    // Test performance with large project
    let registry = ProjectRegistry::new();

    // Measure detection time
    let start = Instant::now();
    let handler = registry.detect_project(project_path);
    let detection_time = start.elapsed();

    assert!(handler.is_some(), "Should detect large project");
    let handler = handler.unwrap();

    // Measure board discovery time
    let start = Instant::now();
    let boards_result = handler.discover_boards(project_path);
    let discovery_time = start.elapsed();

    match boards_result {
        Ok(boards) => {
            println!("✅ Large project handled successfully:");
            println!("   Detection time: {:?}", detection_time);
            println!("   Board discovery time: {:?}", discovery_time);
            println!("   Boards found: {}", boards.len());

            if !boards.is_empty() {
                let board = &boards[0];

                // Test command generation with large project
                let start = Instant::now();
                let build_cmd = handler.get_build_command(project_path, board);
                let command_time = start.elapsed();

                println!("   Command generation time: {:?}", command_time);
                assert!(!build_cmd.is_empty(), "Build command should not be empty");
            }
        }
        Err(e) => {
            println!("⚠️  Large project board discovery failed: {}", e);
            println!("   Detection time: {:?}", detection_time);
            println!("   Board discovery time: {:?}", discovery_time);
        }
    }

    // Validate reasonable performance even with large projects
    assert!(
        detection_time.as_millis() < MAX_PROJECT_DETECTION_TIME_MS * 5, // Allow 5x threshold for large projects
        "Large project detection took too long: {:?}",
        detection_time
    );

    println!("✅ Large project handling performance test completed");
}

/// Test hardware simulation performance with multiple devices
#[test]
fn test_hardware_simulation_performance() {
    println!("Testing hardware simulation performance...");

    let start_time = Instant::now();

    // Create mock hardware environment with many devices
    let mut mock_env = MockHardwareEnvironment::new().expect("Failed to create mock environment");

    // MockHardwareEnvironment starts with 3 default devices, get the initial count
    let initial_device_count = mock_env.list_serial_ports().len();

    // Add additional ESP32 variants
    let additional_devices = 20;
    for i in 0..additional_devices {
        let device = match i % 3 {
            0 => MockEsp32Device::new_esp32(),
            1 => MockEsp32Device::new_esp32s3(),
            _ => MockEsp32Device::new_esp32c3(),
        };

        mock_env.add_device(&format!("esp32_device_{}", i), device);
    }

    let total_device_count = initial_device_count + additional_devices;

    let setup_time = start_time.elapsed();

    // Test port listing performance
    let start = Instant::now();
    let available_ports = mock_env.list_serial_ports();
    let port_listing_time = start.elapsed();

    // Test device simulation performance
    let start = Instant::now();
    let device_info = mock_env.simulate_discover();
    let discovery_time = start.elapsed();

    println!("📊 Hardware Simulation Performance Summary:");
    println!("   Initial devices: {}", initial_device_count);
    println!("   Additional devices: {}", additional_devices);
    println!("   Total devices: {}", total_device_count);
    println!("   Environment setup time: {:?}", setup_time);
    println!("   Port listing time: {:?}", port_listing_time);
    println!("   Device discovery time: {:?}", discovery_time);
    println!("   Available ports: {}", available_ports.len());
    println!("   Discovered devices: {}", device_info.len());

    // Validate performance
    assert_eq!(
        available_ports.len(),
        total_device_count,
        "Should have port for each device"
    );
    assert_eq!(
        device_info.len(),
        total_device_count,
        "Should discover all devices"
    );

    assert!(
        port_listing_time.as_millis() < 100,
        "Port listing should be fast: {:?}",
        port_listing_time
    );

    assert!(
        discovery_time.as_millis() < 200,
        "Device discovery should be fast: {:?}",
        discovery_time
    );

    println!("✅ Hardware simulation performance test completed");
}

/// Test load handling with burst operations
#[tokio::test]
async fn test_burst_load_performance() {
    println!("Testing burst load performance...");

    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let project_path = temp_dir.path();

    // Create a test project
    ProjectFixtures::create_rust_nostd_project(project_path, "s3")
        .expect("Failed to create Rust project");
    ConfigFixtures::create_espbrew_config(project_path, "rust_nostd", "esp32s3")
        .expect("Failed to create config");

    let registry = Arc::new(ProjectRegistry::new());

    // Test burst operations
    let burst_size = 50;
    let mut handles = Vec::new();
    let start_time = Instant::now();

    for i in 0..burst_size {
        let project_path = project_path.to_path_buf();
        let registry = registry.clone();

        let handle = tokio::spawn(async move {
            let operation_start = Instant::now();

            // Rapid-fire operations
            for _ in 0..5 {
                if let Some(handler) = registry.detect_project(&project_path) {
                    let _ = handler.discover_boards(&project_path);

                    if let Ok(boards) = handler.discover_boards(&project_path) {
                        if !boards.is_empty() {
                            let board = &boards[0];
                            let _ = handler.get_build_command(&project_path, board);
                            let _ = handler.get_flash_command(
                                &project_path,
                                board,
                                Some("/dev/ttyUSB0"),
                            );
                        }
                    }
                }
            }

            (i, operation_start.elapsed())
        });

        handles.push(handle);
    }

    // Collect results
    let mut results = Vec::new();
    for handle in handles {
        let result = handle.await.expect("Task should complete");
        results.push(result);
    }

    let total_time = start_time.elapsed();
    let operation_times: Vec<Duration> = results.iter().map(|(_, time)| *time).collect();

    let avg_time = operation_times.iter().sum::<Duration>() / operation_times.len() as u32;
    let max_time = operation_times.iter().max().copied().unwrap_or_default();
    let total_operations = burst_size * 5; // 5 operations per burst

    println!("📊 Burst Load Performance Summary:");
    println!("   Burst size: {}", burst_size);
    println!("   Total operations: {}", total_operations);
    println!("   Total time: {:?}", total_time);
    println!("   Average burst time: {:?}", avg_time);
    println!("   Maximum burst time: {:?}", max_time);
    println!(
        "   Operations per second: {:.2}",
        total_operations as f64 / total_time.as_secs_f64()
    );

    // Validate performance under load
    assert!(
        avg_time.as_millis() < 1000, // 1 second for 5 operations should be reasonable
        "Average burst time is too slow: {:?}",
        avg_time
    );

    println!("✅ Burst load performance test completed");
}

/// Approximate memory usage estimation (platform-specific)
fn get_approximate_memory_usage() -> f64 {
    // This is a rough estimation - in real scenarios you might use more sophisticated methods
    #[cfg(target_os = "macos")]
    {
        // On macOS, we can use system calls or parse /proc-like info
        // For testing purposes, we'll use a simple heuristic
        use std::process::Command;

        let output = Command::new("ps")
            .args(["-o", "rss=", "-p", &std::process::id().to_string()])
            .output();

        if let Ok(output) = output {
            let rss_str = String::from_utf8_lossy(&output.stdout);
            if let Ok(rss_kb) = rss_str.trim().parse::<f64>() {
                return rss_kb / 1024.0; // Convert KB to MB
            }
        }
    }

    #[cfg(target_os = "linux")]
    {
        // On Linux, read from /proc/self/status
        if let Ok(status) = std::fs::read_to_string("/proc/self/status") {
            for line in status.lines() {
                if line.starts_with("VmRSS:") {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() >= 2 {
                        if let Ok(kb) = parts[1].parse::<f64>() {
                            return kb / 1024.0; // Convert KB to MB
                        }
                    }
                }
            }
        }
    }

    // Fallback: return a reasonable estimate
    50.0 // 50MB baseline estimate
}

/// Integration test combining all performance aspects
#[tokio::test]
async fn test_comprehensive_performance_integration() {
    println!("Running comprehensive performance integration test...");

    let start_time = Instant::now();

    // Create test environment
    let workspace =
        TestEnvironment::create_test_workspace().expect("Failed to create test workspace");
    let workspace_path = workspace.path();

    // Test 1: Registry creation performance
    let registry_start = Instant::now();
    let registry = Arc::new(ProjectRegistry::new());
    let registry_creation_time = registry_start.elapsed();

    // Test 2: Multiple project types detection
    let detection_start = Instant::now();
    let project_paths = vec![
        workspace_path.join("rust-esp32s3-project"),
        workspace_path.join("arduino-esp32-project"),
        workspace_path.join("esp-idf-project"),
        workspace_path.join("micropython-project"),
    ];

    let mut detection_results = Vec::new();
    for project_path in &project_paths {
        let start = Instant::now();
        let result = registry.detect_project(project_path);
        let time = start.elapsed();
        detection_results.push((
            project_path
                .file_name()
                .unwrap()
                .to_string_lossy()
                .to_string(),
            time,
            result.is_some(),
        ));
    }
    let total_detection_time = detection_start.elapsed();

    // Test 3: Concurrent board discovery
    let discovery_start = Instant::now();
    let mut discovery_handles = Vec::new();

    for project_path in &project_paths {
        let project_path = project_path.clone();
        let registry = registry.clone();

        let handle = tokio::spawn(async move {
            if let Some(handler) = registry.detect_project(&project_path) {
                let start = Instant::now();
                let result = handler.discover_boards(&project_path);
                let time = start.elapsed();
                (
                    project_path
                        .file_name()
                        .unwrap()
                        .to_string_lossy()
                        .to_string(),
                    time,
                    result.is_ok(),
                )
            } else {
                (
                    project_path
                        .file_name()
                        .unwrap()
                        .to_string_lossy()
                        .to_string(),
                    Duration::from_millis(0),
                    false,
                )
            }
        });

        discovery_handles.push(handle);
    }

    let mut discovery_results = Vec::new();
    for handle in discovery_handles {
        let result = handle.await.expect("Task should complete");
        discovery_results.push(result);
    }
    let total_discovery_time = discovery_start.elapsed();

    // Test 4: Mock hardware performance
    let hardware_start = Instant::now();
    let mut mock_env = MockHardwareEnvironment::new().expect("Failed to create mock environment");
    for i in 0..10 {
        let device = MockEsp32Device::new_esp32s3();
        mock_env.add_device(&format!("perf_test_device_{}", i), device);
    }
    let ports = mock_env.list_serial_ports();
    let hardware_time = hardware_start.elapsed();

    let total_time = start_time.elapsed();

    // Performance analysis and reporting
    println!("📊 Comprehensive Performance Integration Results:");
    println!("=================================================");

    println!("🏗️  Registry Creation:");
    println!("   Time: {:?}", registry_creation_time);

    println!("🔍 Project Detection:");
    println!("   Total time: {:?}", total_detection_time);
    for (project, time, success) in &detection_results {
        println!(
            "   {}: {:?} ({})",
            project,
            time,
            if *success { "✅" } else { "❌" }
        );
    }

    println!("📋 Board Discovery:");
    println!("   Total time: {:?}", total_discovery_time);
    for (project, time, success) in &discovery_results {
        println!(
            "   {}: {:?} ({})",
            project,
            time,
            if *success { "✅" } else { "❌" }
        );
    }

    println!("🔌 Hardware Simulation:");
    println!("   Setup time: {:?}", hardware_time);
    println!("   Mock ports created: {}", ports.len());

    println!("⏱️  Overall Performance:");
    println!("   Total integration time: {:?}", total_time);
    println!(
        "   Average detection time: {:?}",
        detection_results
            .iter()
            .map(|(_, time, _)| *time)
            .sum::<Duration>()
            / detection_results.len() as u32
    );

    // Validate overall performance
    assert!(
        total_time.as_secs() < 10,
        "Integration test should complete within 10 seconds"
    );

    let successful_detections = detection_results
        .iter()
        .filter(|(_, _, success)| *success)
        .count();
    assert!(
        successful_detections > 0,
        "At least some projects should be detected"
    );

    println!("✅ Comprehensive performance integration test completed");
}
