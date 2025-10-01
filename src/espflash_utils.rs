use anyhow::{Context, Result};
use std::time::Duration;

/// Information about an ESP32 board discovered on a serial port
#[derive(Debug, Clone)]
pub struct EspBoardInfo {
    pub port: String,
    pub chip_type: String,
    pub crystal_frequency: String,
    pub flash_size: String,
    pub features: String,
    pub mac_address: String,
    pub device_description: String,
    pub chip_revision: Option<String>,
}

/// Find all available ESP32-compatible serial ports
pub fn find_esp_ports() -> Result<Vec<String>> {
    println!("🔍 Scanning for ESP32-compatible serial ports...");

    let ports = serialport::available_ports()?;

    // Filter for relevant USB ports on macOS and Linux
    let esp_ports: Vec<String> = ports
        .into_iter()
        .filter_map(|port_info| {
            let port_name = &port_info.port_name;
            // On macOS, focus on USB modem and USB serial ports
            if port_name.contains("/dev/cu.usbmodem")
                || port_name.contains("/dev/cu.usbserial")
                || port_name.contains("/dev/tty.usbmodem")
                || port_name.contains("/dev/tty.usbserial")
                // On Linux, ESP32 devices typically appear as ttyUSB* or ttyACM*
                || port_name.contains("/dev/ttyUSB")
                || port_name.contains("/dev/ttyACM")
            {
                Some(port_name.clone())
            } else {
                None
            }
        })
        .collect();

    println!("📡 Found {} ESP32-compatible serial ports", esp_ports.len());
    for port in &esp_ports {
        println!("  🔌 {}", port);
    }

    Ok(esp_ports)
}

/// Select the first available ESP32 port, or use environment variable if set
pub fn select_esp_port() -> Result<String> {
    // Check if user specified a port via environment variable
    if let Ok(port) = std::env::var("ESPFLASH_PORT") {
        println!(
            "🎯 Using port from ESPFLASH_PORT environment variable: {}",
            port
        );
        return Ok(port);
    }

    // Find available ports
    let ports = find_esp_ports()?;

    if ports.is_empty() {
        return Err(anyhow::anyhow!(
            "No ESP32-compatible serial ports found. Please connect your development board via USB."
        ));
    }

    if ports.len() == 1 {
        let port = ports[0].clone();
        println!("🎯 Auto-selected single available port: {}", port);
        return Ok(port);
    }

    // Multiple ports available - for now, select the first one
    // In the future, this could be enhanced with interactive selection
    let port = ports[0].clone();
    println!(
        "🎯 Multiple ports available, auto-selected first: {} (set ESPFLASH_PORT to override)",
        port
    );
    println!("   Available ports: {}", ports.join(", "));

    Ok(port)
}

/// Flash a binary file to an ESP32 using espflash library directly
pub async fn flash_binary_to_esp(binary_path: &std::path::Path, port: Option<&str>) -> Result<()> {
    let target_port = match port {
        Some(p) => p.to_string(),
        None => select_esp_port()?,
    };

    println!("🔥 Starting espflash operation on port: {}", target_port);
    println!("📁 Binary file: {}", binary_path.display());

    // Check if this is an ELF file (common for esp-hal Rust projects)
    let is_elf = binary_path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.is_empty()) // Rust binaries typically have no extension
        .unwrap_or(false)
        || binary_path
            .to_str()
            .map(|path| !path.contains(".bin") && !path.contains(".hex"))
            .unwrap_or(false);

    if is_elf {
        println!("📄 Detected ELF file, using espflash with ELF support");
        flash_elf_file(&target_port, binary_path).await
    } else {
        println!("📄 Detected binary file, using raw binary flash");
        // Read the binary file
        let binary_data = std::fs::read(binary_path)
            .with_context(|| format!("Failed to read binary file: {}", binary_path.display()))?;

        println!("💾 Binary size: {} bytes", binary_data.len());
        flash_binary_data(&target_port, &binary_data, 0x10000).await
    }
}

/// Flash binary data directly using espflash library
pub async fn flash_binary_data(port: &str, binary_data: &[u8], offset: u32) -> Result<()> {
    use espflash::connection::{Connection, ResetAfterOperation, ResetBeforeOperation};
    use espflash::flasher::Flasher;
    use serialport::SerialPortType;

    println!(
        "🔥 Starting native espflash: port={}, offset=0x{:x}, size={} bytes",
        port,
        offset,
        binary_data.len()
    );

    // Validate binary data
    if binary_data.is_empty() {
        return Err(anyhow::anyhow!("Binary data is empty"));
    }

    // Get port info for creating connection
    let ports = serialport::available_ports()
        .map_err(|e| anyhow::anyhow!("Failed to enumerate serial ports: {}", e))?;

    let port_info = ports
        .iter()
        .find(|p| p.port_name == port)
        .ok_or_else(|| anyhow::anyhow!("Port {} not found in available ports", port))?
        .clone();

    let usb_info = match &port_info.port_type {
        SerialPortType::UsbPort(info) => info.clone(),
        _ => {
            // For non-USB ports, create a dummy UsbPortInfo
            serialport::UsbPortInfo {
                vid: 0,
                pid: 0,
                serial_number: None,
                manufacturer: None,
                product: None,
                interface: None,
            }
        }
    };

    // Create serial port with appropriate timeout and baud rate for flashing
    let serial_port = serialport::new(port, 460800)
        .timeout(Duration::from_millis(2000))
        .open_native()
        .map_err(|e| anyhow::anyhow!("Failed to open serial port {}: {}", port, e))?;

    // Create connection with hard reset after flashing
    let connection = Connection::new(
        *Box::new(serial_port),
        usb_info,
        ResetAfterOperation::HardReset,
        ResetBeforeOperation::DefaultReset,
        460800,
    );

    println!("🔗 Establishing connection to ESP32 device...");

    // Create flasher and connect in blocking task (espflash is not async)
    let mut flasher = tokio::task::spawn_blocking(move || {
        Flasher::connect(connection, true, true, true, None, None)
    })
    .await
    .map_err(|e| anyhow::anyhow!("Task join error: {}", e))?
    .map_err(|e| anyhow::anyhow!("Failed to connect to ESP32 device on {}: {}", port, e))?;

    println!("🚀 Connected to ESP32 device, starting flash operation...");

    // Get device info for validation and logging
    let device_info_result = tokio::task::spawn_blocking(move || {
        let device_info = flasher.device_info()?;
        println!("📎 Detected chip: {} with features: {:?}", device_info.chip, device_info.features);
        // TODO: Implement proper espflash API for writing flash data
        // For now, we'll fall back to using espflash command-line tool
        anyhow::Result::<()>::Err(anyhow::anyhow!(
            "Native espflash library flashing not yet fully implemented. Falling back to command-line tool."
        ))
    })
    .await;

    match device_info_result {
        Ok(Ok(_)) => {
            println!("✨ Native espflash operation completed successfully");
            Ok(())
        }
        Ok(Err(_)) | Err(_) => {
            println!("⚠️ Falling back to espflash command-line tool");
            flash_binary_data_with_cli(port, binary_data, offset).await
        }
    }
}

/// Flash an ELF file using espflash library (specifically for esp-hal Rust projects)
async fn flash_elf_file(port: &str, elf_path: &std::path::Path) -> Result<()> {
    use espflash::connection::{Connection, ResetAfterOperation, ResetBeforeOperation};
    use espflash::flasher::Flasher;
    use serialport::SerialPortType;
    use std::fs;

    println!(
        "🔥 Flashing ELF file using espflash library: port={}, file={}",
        port,
        elf_path.display()
    );

    // Read the ELF file
    let elf_data = fs::read(elf_path)
        .with_context(|| format!("Failed to read ELF file: {}", elf_path.display()))?;

    println!("💾 ELF file size: {} bytes", elf_data.len());

    // Get port info for creating connection
    let ports = serialport::available_ports()
        .map_err(|e| anyhow::anyhow!("Failed to enumerate serial ports: {}", e))?;

    let port_info = ports
        .iter()
        .find(|p| p.port_name == port)
        .ok_or_else(|| anyhow::anyhow!("Port {} not found in available ports", port))?
        .clone();

    let usb_info = match &port_info.port_type {
        SerialPortType::UsbPort(info) => info.clone(),
        _ => {
            // For non-USB ports, create a dummy UsbPortInfo
            serialport::UsbPortInfo {
                vid: 0,
                pid: 0,
                serial_number: None,
                manufacturer: None,
                product: None,
                interface: None,
            }
        }
    };

    // Create serial port with appropriate timeout and baud rate for flashing
    let serial_port = serialport::new(port, 460800)
        .timeout(Duration::from_millis(2000))
        .open_native()
        .map_err(|e| anyhow::anyhow!("Failed to open serial port {}: {}", port, e))?;

    // Create connection with hard reset after flashing
    let connection = Connection::new(
        *Box::new(serial_port),
        usb_info,
        ResetAfterOperation::HardReset,
        ResetBeforeOperation::DefaultReset,
        460800,
    );

    println!("🔗 Establishing connection to ESP32 device...");

    // Create flasher and connect in blocking task
    let mut flasher = tokio::task::spawn_blocking(move || {
        Flasher::connect(connection, true, true, true, None, None)
    })
    .await
    .map_err(|e| anyhow::anyhow!("Task join error: {}", e))?
    .map_err(|e| anyhow::anyhow!("Failed to connect to ESP32 device on {}: {}", port, e))?;

    println!("🚀 Connected to ESP32 device, processing ELF for flash operation...");

    // For esp-hal projects, the best approach is to use cargo run which respects the runner configuration
    // Get the project directory from the ELF path
    let project_dir = elf_path
        .ancestors()
        .find(|path| path.join("Cargo.toml").exists())
        .ok_or_else(|| {
            anyhow::anyhow!(
                "Could not find project directory for: {}",
                elf_path.display()
            )
        })?;

    println!(
        "📁 Using project directory for cargo run: {}",
        project_dir.display()
    );

    // Try using cargo run first, which respects the runner configuration in .cargo/config.toml
    let flash_result = cargo_run_flash(port, project_dir).await;

    match flash_result {
        Ok(()) => {
            println!("✅ Cargo run ELF flash operation completed successfully");
            Ok(())
        }
        Err(e) => {
            println!("⚠️ Cargo run ELF flash failed: {}", e);
            // Fallback to CLI approach as a last resort
            flash_elf_file_with_cli(port, elf_path).await
        }
    }
}

/// Flash using cargo run (respects runner configuration in .cargo/config.toml)
async fn cargo_run_flash(port: &str, project_dir: &std::path::Path) -> Result<()> {
    use std::process::Stdio;
    use tokio::process::Command;

    println!(
        "🔥 Using cargo run for flashing (respects .cargo/config.toml runner): port={}, dir={}",
        port,
        project_dir.display()
    );

    // Use cargo run with proper environment variables to make espflash non-interactive
    let cmd = Command::new("cargo")
        .args(["run", "--release"])
        .current_dir(project_dir)
        .env("ESPFLASH_PORT", port) // Set the port via environment variable
        .env("RUST_LOG", "info") // Set logging level for better output
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()?;

    println!(
        "🚀 Running cargo run --release with ESPFLASH_PORT={}...",
        port
    );

    // Wait for completion with timeout
    let timeout_dur = Duration::from_secs(180); // 3 minute timeout for cargo run
    let result = tokio::time::timeout(timeout_dur, cmd.wait_with_output()).await;

    match result {
        Ok(Ok(output)) => {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);

                // Print output for debugging
                if !stdout.trim().is_empty() {
                    println!("📝 cargo run stdout: {}", stdout.trim());
                }
                if !stderr.trim().is_empty() {
                    println!("📝 cargo run stderr: {}", stderr.trim());
                }

                println!("✅ Cargo run flash operation completed successfully");
                Ok(())
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                let stdout = String::from_utf8_lossy(&output.stdout);
                Err(anyhow::anyhow!(
                    "Cargo run flash command failed (exit code: {}): stdout: {}, stderr: {}",
                    output.status.code().unwrap_or(-1),
                    stdout.trim(),
                    stderr.trim()
                ))
            }
        }
        Ok(Err(e)) => Err(anyhow::anyhow!("Failed to run cargo command: {}", e)),
        Err(_) => Err(anyhow::anyhow!(
            "Cargo run flash operation timed out after 3 minutes"
        )),
    }
}

/// Fallback ELF flash implementation using espflash command-line tool
async fn flash_elf_file_with_cli(port: &str, elf_path: &std::path::Path) -> Result<()> {
    use std::process::Stdio;
    use tokio::process::Command;

    println!(
        "🔥 Fallback ELF flash using espflash CLI: port={}, file={}",
        port,
        elf_path.display()
    );

    // Use espflash flash command with ELF file directly
    // For esp-hal projects, we need to ignore the app descriptor requirement
    let cmd = Command::new("espflash")
        .args([
            "flash",
            "--port",
            port,
            "--non-interactive", // This prevents the interactive port selection!
            "--ignore_app_descriptor", // Required for pure esp-hal projects without bootloader integration
            elf_path.to_str().unwrap(),
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()?;

    println!("🚀 Running espflash CLI ELF flash command...");

    // Wait for completion with timeout
    let timeout_dur = Duration::from_secs(180); // 3 minute timeout for ELF flashing
    let result = tokio::time::timeout(timeout_dur, cmd.wait_with_output()).await;

    match result {
        Ok(Ok(output)) => {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);

                // Print output for debugging
                if !stdout.trim().is_empty() {
                    println!("📝 espflash stdout: {}", stdout.trim());
                }
                if !stderr.trim().is_empty() {
                    println!("📝 espflash stderr: {}", stderr.trim());
                }

                println!("✅ ELF flash operation completed successfully");
                Ok(())
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                let stdout = String::from_utf8_lossy(&output.stdout);
                Err(anyhow::anyhow!(
                    "ELF flash command failed (exit code: {}): {} {}",
                    output.status.code().unwrap_or(-1),
                    stderr.trim(),
                    stdout.trim()
                ))
            }
        }
        Ok(Err(e)) => Err(anyhow::anyhow!("Failed to run ELF flash command: {}", e)),
        Err(_) => Err(anyhow::anyhow!(
            "ELF flash operation timed out after 3 minutes"
        )),
    }
}

/// Fallback flash implementation using espflash command-line tool
async fn flash_binary_data_with_cli(port: &str, binary_data: &[u8], offset: u32) -> Result<()> {
    use std::process::Stdio;
    use tokio::fs;
    use tokio::process::Command;

    println!(
        "🔥 Fallback flash using espflash CLI: port={}, offset=0x{:x}, size={} bytes",
        port,
        offset,
        binary_data.len()
    );

    // Create temporary file for binary data
    let temp_dir = std::env::temp_dir();
    let temp_file = temp_dir.join(format!(
        "espbrew_flash_{}.bin",
        uuid::Uuid::new_v4().simple()
    ));

    // Write binary data to temp file
    fs::write(&temp_file, binary_data).await?;
    println!(
        "💾 Wrote {} bytes to temp file: {}",
        binary_data.len(),
        temp_file.display()
    );

    // Use espflash for reliable flashing with non-interactive mode
    let cmd = Command::new("espflash")
        .args([
            "flash",
            "--port",
            port,
            "--non-interactive", // This prevents the interactive port selection!
            temp_file.to_str().unwrap(),
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()?;

    println!("🚀 Running espflash command...");

    // Wait for completion with timeout
    let timeout_dur = Duration::from_secs(120); // 2 minute timeout for flashing
    let result = tokio::time::timeout(timeout_dur, cmd.wait_with_output()).await;

    // Clean up temp file
    let _ = fs::remove_file(&temp_file).await;

    match result {
        Ok(Ok(output)) => {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);

                // Print output for debugging
                if !stdout.trim().is_empty() {
                    println!("📝 espflash stdout: {}", stdout.trim());
                }
                if !stderr.trim().is_empty() {
                    println!("📝 espflash stderr: {}", stderr.trim());
                }

                println!("✅ Flash operation completed successfully");
                Ok(())
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                let stdout = String::from_utf8_lossy(&output.stdout);
                Err(anyhow::anyhow!(
                    "Flash command failed (exit code: {}): {} {}",
                    output.status.code().unwrap_or(-1),
                    stderr.trim(),
                    stdout.trim()
                ))
            }
        }
        Ok(Err(e)) => Err(anyhow::anyhow!("Failed to run flash command: {}", e)),
        Err(_) => Err(anyhow::anyhow!("Flash operation timed out after 2 minutes")),
    }
}

/// Identify ESP32 board information on a specific port
pub async fn identify_esp_board(port: &str) -> Result<Option<EspBoardInfo>> {
    use espflash::connection::{Connection, ResetAfterOperation, ResetBeforeOperation};
    use espflash::flasher::Flasher;
    use serialport::SerialPortType;

    println!("🔍 Identifying ESP32 board on port: {}", port);

    // Get port info for creating connection
    let ports = match serialport::available_ports() {
        Ok(ports) => ports,
        Err(e) => {
            println!("⚠️ Failed to enumerate ports: {}", e);
            return Ok(None);
        }
    };

    let port_info = match ports.iter().find(|p| p.port_name == port) {
        Some(info) => info.clone(),
        None => {
            println!("⚠️ Port {} not found in available ports", port);
            return Ok(None);
        }
    };

    let usb_info = match &port_info.port_type {
        SerialPortType::UsbPort(info) => info.clone(),
        _ => {
            // For non-USB ports, create a dummy UsbPortInfo
            serialport::UsbPortInfo {
                vid: 0,
                pid: 0,
                serial_number: None,
                manufacturer: None,
                product: None,
                interface: None,
            }
        }
    };

    // Create serial port with timeout
    let serial_port = match serialport::new(port, 115200)
        .timeout(Duration::from_millis(1000))
        .open_native()
    {
        Ok(port) => port,
        Err(e) => {
            println!("⚠️ Failed to open serial port {}: {}", port, e);
            return Ok(None);
        }
    };

    // Create connection
    let connection = Connection::new(
        *Box::new(serial_port),
        usb_info,
        ResetAfterOperation::HardReset,
        ResetBeforeOperation::DefaultReset,
        115200,
    );

    // Create flasher and connect in blocking task
    let flasher_result = tokio::task::spawn_blocking(move || {
        Flasher::connect(connection, true, true, true, None, None)
    })
    .await;

    let mut flasher = match flasher_result {
        Ok(Ok(flasher)) => flasher,
        Ok(Err(e)) => {
            println!("⚠️ Failed to connect to flasher on {}: {}", port, e);
            return Ok(None);
        }
        Err(e) => {
            println!("⚠️ Task error connecting to flasher on {}: {}", port, e);
            return Ok(None);
        }
    };

    // Get device info which includes MAC address and other details
    let device_info_result = tokio::task::spawn_blocking(move || flasher.device_info()).await;

    let device_info = match device_info_result {
        Ok(Ok(info)) => info,
        Ok(Err(e)) => {
            println!("⚠️ Failed to get device info on {}: {}", port, e);
            return Ok(None);
        }
        Err(e) => {
            println!("⚠️ Task error getting device info on {}: {}", port, e);
            return Ok(None);
        }
    };

    // Map espflash chip type to our board info format
    let chip_type = device_info.chip.to_string();
    let features = device_info.features.join(", ");
    let flash_size = device_info.flash_size.to_string();
    let crystal_frequency = device_info.crystal_frequency.to_string();
    let mac_address = device_info
        .mac_address
        .map(|mac| mac.to_string().to_uppercase())
        .unwrap_or_else(|| "Unknown".to_string());

    // Extract revision info
    let chip_revision = device_info
        .revision
        .map(|(major, minor)| format!("{}.{}", major, minor));

    println!(
        "✅ Successfully identified {} board: {} (rev: {})",
        chip_type,
        port,
        chip_revision.as_deref().unwrap_or("unknown")
    );

    Ok(Some(EspBoardInfo {
        port: port.to_string(),
        chip_type,
        crystal_frequency,
        flash_size,
        features,
        mac_address,
        device_description: "ESP Development Board (espflash native detected)".to_string(),
        chip_revision,
    }))
}
