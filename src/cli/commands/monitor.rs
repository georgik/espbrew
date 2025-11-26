//! Local ESP32 monitoring command implementation

use anyhow::{Context, Result};
use log::{error, info};
use regex::Regex;
use serialport::SerialPortInfo;
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// Execute the local monitor command
pub async fn execute_monitor_command(
    port: Option<String>,
    baud_rate: u32,
    elf: Option<PathBuf>,
    log_format: &str,
    reset: bool,
    non_interactive: bool,
    no_addresses: bool,
    timeout: u64,
    success_pattern: Option<String>,
    failure_pattern: Option<String>,
) -> Result<()> {
    info!("Starting local monitor with baud rate: {}", baud_rate);

    // Validate log format
    if !["serial", "defmt"].contains(&log_format) {
        anyhow::bail!(
            "Unsupported log format: {}. Supported formats: serial, defmt",
            log_format
        );
    }

    // Validate ELF file if provided
    if let Some(ref elf_path) = elf {
        if !elf_path.exists() {
            anyhow::bail!("ELF file not found: {}", elf_path.display());
        }
        info!(
            "Using ELF file for symbol resolution: {}",
            elf_path.display()
        );
    }

    // Validate and compile regex patterns
    let success_regex = match success_pattern {
        Some(ref pattern) => {
            let regex = Regex::new(pattern)
                .with_context(|| format!("Invalid success pattern regex: {}", pattern))?;
            info!("Success pattern configured: {}", pattern);
            Some(regex)
        }
        None => None,
    };

    let failure_regex = match failure_pattern {
        Some(ref pattern) => {
            let regex = Regex::new(pattern)
                .with_context(|| format!("Invalid failure pattern regex: {}", pattern))?;
            info!("Failure pattern configured: {}", pattern);
            Some(regex)
        }
        None => None,
    };

    // Configure timeout
    let timeout_duration = if timeout > 0 {
        Some(Duration::from_secs(timeout))
    } else {
        None
    };

    if timeout_duration.is_some() {
        info!("Monitor timeout configured: {} seconds", timeout);
    }

    // Detect or validate serial port
    let serial_port = detect_or_validate_port(port)
        .await
        .context("Failed to detect or validate serial port")?;

    info!("Using serial port: {}", serial_port.port_name);

    // Print configuration
    print_monitor_configuration(
        &serial_port,
        baud_rate,
        elf.as_ref(),
        log_format,
        reset,
        non_interactive,
        no_addresses,
        timeout,
        &success_pattern,
        &failure_pattern,
    );

    // Implement basic mock monitoring for testing
    run_mock_monitoring(timeout_duration, success_regex, failure_regex).await?;

    Ok(())
}

/// Detect available serial ports or validate the specified port
async fn detect_or_validate_port(port_option: Option<String>) -> Result<SerialPortInfo> {
    match port_option {
        Some(port_name) => {
            // Validate the specified port
            info!("Validating specified port: {}", port_name);
            validate_port(&port_name)
        }
        None => {
            // Auto-detect available ports
            info!("Auto-detecting serial ports...");
            detect_serial_ports().await
        }
    }
}

/// Validate that a specific serial port exists
fn validate_port(port_name: &str) -> Result<SerialPortInfo> {
    let available_ports =
        serialport::available_ports().context("Failed to enumerate serial ports")?;

    for port_info in available_ports {
        if port_info.port_name == port_name {
            return Ok(port_info);
        }
    }

    anyhow::bail!("Serial port not found: {}", port_name);
}

/// Auto-detect ESP32 compatible serial ports
async fn detect_serial_ports() -> Result<SerialPortInfo> {
    let available_ports =
        serialport::available_ports().context("Failed to enumerate serial ports")?;

    if available_ports.is_empty() {
        anyhow::bail!(
            "No serial ports found. Please connect an ESP32 device or specify a port with --port"
        );
    }

    // For now, just return the first available port
    // TODO: Implement better port detection (USB VID/PID filtering, etc.)
    if available_ports.len() == 1 {
        let port = &available_ports[0];
        info!("Found single serial port: {}", port.port_name);
        return Ok(available_ports.into_iter().next().unwrap());
    }

    // If multiple ports, list them and let user know to specify
    error!("Multiple serial ports found. Please specify which port to use with --port:");
    for (i, port) in available_ports.iter().enumerate() {
        println!("  {}: {}", i + 1, port.port_name);
        match &port.port_type {
            serialport::SerialPortType::UsbPort(usb_info) => {
                if let Some(ref serial_number) = usb_info.serial_number {
                    println!("     Serial: {}", serial_number);
                }
                println!("     VID: {:04X}", usb_info.vid);
                println!("     PID: {:04X}", usb_info.pid);
                if let Some(ref manufacturer) = usb_info.manufacturer {
                    println!("     Manufacturer: {}", manufacturer);
                }
            }
            _ => {}
        }
    }

    anyhow::bail!("Please specify the port to use with --port <port_name>");
}

/// Print the current monitor configuration
fn print_monitor_configuration(
    port_info: &SerialPortInfo,
    baud_rate: u32,
    elf: Option<&PathBuf>,
    log_format: &str,
    reset: bool,
    non_interactive: bool,
    no_addresses: bool,
    timeout: u64,
    success_pattern: &Option<String>,
    failure_pattern: &Option<String>,
) {
    println!("\n=== Local Monitor Configuration ===");
    println!("Port: {}", port_info.port_name);
    println!("Baud Rate: {}", baud_rate);
    println!("Log Format: {}", log_format);
    println!("Reset on start: {}", reset);
    println!("Non-interactive: {}", non_interactive);
    println!("Address resolution disabled: {}", no_addresses);
    println!("Timeout: {} seconds (0 = infinite)", timeout);

    if let Some(pattern) = success_pattern {
        println!("Success Pattern: {}", pattern);
    } else {
        println!("Success Pattern: None");
    }

    if let Some(pattern) = failure_pattern {
        println!("Failure Pattern: {}", pattern);
    } else {
        println!("Failure Pattern: None");
    }

    if let Some(elf_path) = elf {
        println!("ELF file: {}", elf_path.display());
    } else {
        println!("ELF file: None (no symbol resolution)");
    }

    // Print additional port information if available
    match &port_info.port_type {
        serialport::SerialPortType::UsbPort(usb_info) => {
            if let Some(ref serial_number) = usb_info.serial_number {
                println!("Device Serial: {}", serial_number);
            }
            if let Some(ref manufacturer) = usb_info.manufacturer {
                println!("Device Manufacturer: {}", manufacturer);
            }
            if let Some(ref product_name) = usb_info.product {
                println!("Device Product: {}", product_name);
            }
        }
        _ => {}
    }

    println!("====================================\n");
}

/// Run mock monitoring for testing purposes with timeout and pattern matching
async fn run_mock_monitoring(
    timeout_duration: Option<Duration>,
    success_regex: Option<Regex>,
    failure_regex: Option<Regex>,
) -> Result<()> {
    let start_time = Instant::now();
    let mut lines_count = 0;

    println!("🔄 Starting mock monitoring (simulating serial output)...");
    println!("   Press Ctrl+C to stop at any time\n");

    // Simulate ESP32 boot output
    let mock_lines = vec![
        "ESP32 boot sequence initiated...",
        "CPU frequency: 240MHz",
        "Flash size: 4MB",
        "Loading app from partition...",
        "App partition found at offset 0x10000",
        "Initializing WiFi...",
        "WiFi initialized",
        "Starting application...",
        "Application started successfully",
        "System ready - awaiting connections...",
        "Error: Failed to connect to WiFi network",
        "Retrying WiFi connection...",
        "WiFi connection established",
        "All systems operational",
    ];

    loop {
        // Check timeout
        if let Some(timeout) = timeout_duration {
            if start_time.elapsed() >= timeout {
                info!(
                    "Monitor timeout reached after {} seconds",
                    timeout.as_secs()
                );
                println!("\n⏰ Monitor timeout reached");
                anyhow::bail!("Monitor timeout reached");
            }
        }

        // Simulate reading a line from serial port
        if lines_count < mock_lines.len() {
            let line = mock_lines[lines_count];
            println!("📡 {}", line);
            lines_count += 1;

            // Check for success pattern
            if let Some(ref regex) = success_regex {
                if regex.is_match(line) {
                    info!("Success pattern matched: {}", line);
                    println!("\n✅ Success pattern detected - exiting monitor");
                    return Ok(());
                }
            }

            // Check for failure pattern
            if let Some(ref regex) = failure_regex {
                if regex.is_match(line) {
                    error!("Failure pattern matched: {}", line);
                    println!("\n❌ Failure pattern detected - exiting monitor with error");
                    anyhow::bail!("Failure pattern matched: {}", line);
                }
            }
        } else {
            // Reset to beginning for continuous loop
            lines_count = 0;
            println!("🔄 Restarting mock output loop...");
        }

        // Simulate serial port read delay
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}
