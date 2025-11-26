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

    // Implement real ESP32 monitoring using espflash library
    let result = run_real_monitoring(
        &serial_port,
        baud_rate,
        timeout_duration,
        success_regex,
        failure_regex,
        reset,
    )
    .await;

    // Handle the special success case
    if let Err(ref e) = result {
        if e.to_string() == "MONITOR_SUCCESS" {
            info!("Monitor completed successfully due to success pattern match");
            return Ok(());
        }
    }

    // Propagate any other errors
    result?;

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

/// Run real ESP32 monitoring using espflash library with timeout and pattern matching
async fn run_real_monitoring(
    port_info: &SerialPortInfo,
    baud_rate: u32,
    timeout_duration: Option<Duration>,
    success_regex: Option<Regex>,
    failure_regex: Option<Regex>,
    reset: bool,
) -> Result<()> {
    use espflash::connection::{Connection, ResetAfterOperation, ResetBeforeOperation};
    use serialport::SerialPort;
    use std::io::Read;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    let start_time = Instant::now();
    let should_exit = Arc::new(AtomicBool::new(false));

    info!(
        "Starting ESP32 serial monitoring on port: {}",
        port_info.port_name
    );

    // Setup Ctrl+C handler
    let should_exit_clone = should_exit.clone();
    let ctrl_c_handle = tokio::spawn(async move {
        #[cfg(unix)]
        {
            use tokio::signal::unix;
            let mut sigint = unix::signal(unix::SignalKind::interrupt()).unwrap();
            sigint.recv().await;
        }
        #[cfg(windows)]
        {
            use tokio::signal;
            signal::ctrl_c().await.unwrap();
        }
        should_exit_clone.store(true, Ordering::Relaxed);
    });

    // Prepare USB port info for espflash connection
    let usb_info = match &port_info.port_type {
        serialport::SerialPortType::UsbPort(usb_info) => usb_info.clone(),
        _ => {
            // For non-USB ports, create a dummy UsbPortInfo
            serialport::UsbPortInfo {
                vid: 0,
                pid: 0,
                serial_number: None,
                manufacturer: None,
                product: None,
            }
        }
    };

    // Open the serial port for reading
    let mut serial_port = serialport::new(&port_info.port_name, baud_rate)
        .timeout(Duration::from_millis(100))
        .open_native()
        .with_context(|| {
            format!(
                "Failed to open serial port for monitoring: {}",
                port_info.port_name
            )
        })?;

    // If reset was requested, perform it before establishing monitoring connection
    if reset {
        info!("Resetting ESP32 device to capture boot sequence");
        let port_name = port_info.port_name.clone();
        let usb_info_clone = usb_info.clone();
        let baud_clone = baud_rate;

        // Create a temporary connection just for reset
        let temp_serial_port = serialport::new(&port_name, baud_clone)
            .timeout(Duration::from_millis(1000))
            .open_native()
            .with_context(|| format!("Failed to open serial port for reset: {}", port_name))?;

        let mut temp_connection = Connection::new(
            *Box::new(temp_serial_port),
            usb_info_clone,
            ResetAfterOperation::HardReset,
            ResetBeforeOperation::DefaultReset,
            baud_clone,
        );

        tokio::task::spawn_blocking(move || {
            let _ = temp_connection.begin(); // This will reset the device
        })
        .await
        .context("Failed to reset ESP32 device")?;

        // Give the device time to start booting
        tokio::time::sleep(Duration::from_secs(2)).await;
    }

    // Set a reasonable timeout for reads
    serial_port.set_timeout(Duration::from_millis(100))?;

    let mut buffer = [0u8; 1024];
    let mut line_buffer = String::new();

    loop {
        // Check for exit conditions
        if should_exit.load(Ordering::Relaxed) {
            info!("Monitoring stopped by user");
            ctrl_c_handle.abort();
            return Ok(());
        }

        // Check timeout
        if let Some(timeout) = timeout_duration {
            if start_time.elapsed() >= timeout {
                info!(
                    "Monitor timeout reached after {} seconds",
                    timeout.as_secs()
                );
                ctrl_c_handle.abort();
                anyhow::bail!("Monitor timeout reached");
            }
        }

        // Try to read from serial port
        match serial_port.read(&mut buffer) {
            Ok(0) => {
                // No data available, continue
                tokio::time::sleep(Duration::from_millis(10)).await;
                continue;
            }
            Ok(bytes_read) => {
                // Convert bytes to UTF-8 string, handling partial UTF-8 sequences
                let chunk = String::from_utf8_lossy(&buffer[..bytes_read]);

                // Process the chunk line by line
                for ch in chunk.chars() {
                    if ch == '\n' || ch == '\r' {
                        if !line_buffer.is_empty() {
                            // We have a complete line
                            process_line(&line_buffer, &success_regex, &failure_regex)?;
                            line_buffer.clear();
                        }
                    } else if ch.is_control() {
                        // Skip other control characters
                        continue;
                    } else {
                        line_buffer.push(ch);
                    }
                }
            }
            Err(ref e) if (*e).kind() == std::io::ErrorKind::TimedOut => {
                // Timeout is expected, continue loop
                tokio::time::sleep(Duration::from_millis(10)).await;
                continue;
            }
            Err(e) => {
                error!("Serial port read error: {}", e);
                ctrl_c_handle.abort();
                anyhow::bail!("Serial port error: {}", e);
            }
        }

        // Small delay to prevent excessive CPU usage
        tokio::time::sleep(Duration::from_millis(1)).await;
    }
}

/// Process a single line of serial output for pattern matching
fn process_line(
    line: &str,
    success_regex: &Option<Regex>,
    failure_regex: &Option<Regex>,
) -> Result<()> {
    // Print the line (stdout is acceptable for monitor output)
    println!("{}", line.trim());

    // Check for success pattern
    if let Some(regex) = success_regex {
        if regex.is_match(line) {
            info!("Success pattern matched: {}", line);
            anyhow::bail!("MONITOR_SUCCESS"); // Special error to indicate success
        }
    }

    // Check for failure pattern
    if let Some(regex) = failure_regex {
        if regex.is_match(line) {
            error!("Failure pattern matched: {}", line);
            anyhow::bail!("Failure pattern matched: {}", line);
        }
    }

    Ok(())
}
