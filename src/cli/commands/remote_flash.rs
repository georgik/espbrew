use crate::cli::args::Cli;
use crate::models::board::RemoteBoard;
use crate::models::flash::{FlashBinaryInfo, FlashConfig, FlashResponse};
use crate::models::responses::RemoteBoardsResponse;
use crate::remote::discovery::discover_espbrew_servers_silent;
use anyhow::{Context, Result};
use std::path::PathBuf;

pub async fn execute_remote_flash_command(
    cli: &Cli,
    binary: Option<PathBuf>,
    _config: Option<PathBuf>,
    build_dir: Option<PathBuf>,
    mac: Option<String>,
    name: Option<String>,
    server: Option<String>,
) -> Result<()> {
    println!("📡 ESPBrew Remote Flash Command");

    // Get project directory
    let project_dir = cli
        .project_dir
        .as_ref()
        .map(|p| p.as_path())
        .unwrap_or_else(|| std::path::Path::new("."));

    println!("📁 Project directory: {}", project_dir.display());

    // Determine server URL - priority: CLI arg > discovery > default
    let server_url = if let Some(url) = server {
        println!("🔌 Using specified server: {}", url);
        url
    } else {
        println!("🔍 Discovering ESPBrew servers on network...");
        match discover_and_select_server().await {
            Ok(url) => {
                println!("✅ Server discovered: {}", url);
                url
            }
            Err(e) => {
                println!("⚠️ Server discovery failed: {}, using default", e);
                "http://localhost:8080".to_string()
            }
        }
    };

    // Fetch available boards from server
    println!("🔎 Fetching available boards from server...");
    let remote_boards = fetch_remote_boards(&server_url)
        .await
        .context("Failed to fetch boards from server")?;

    if remote_boards.is_empty() {
        return Err(anyhow::anyhow!("No boards found on server {}", server_url));
    }

    println!("🎯 Found {} board(s) on server:", remote_boards.len());
    for (i, board) in remote_boards.iter().enumerate() {
        let display_name = board.logical_name.as_deref().unwrap_or(&board.id);
        println!("  {}. {} ({})", i + 1, display_name, board.status);
        if !board.mac_address.is_empty() {
            println!("     MAC: {}", board.mac_address);
        }
    }

    // Select target board
    let target_board = select_target_board(&remote_boards, mac, name)?;
    let selected_name = target_board
        .logical_name
        .as_deref()
        .unwrap_or(&target_board.id);
    println!(
        "🎯 Selected board: {} ({})",
        selected_name, target_board.status
    );

    // Handle binary vs project flash
    if let Some(binary_path) = binary {
        // Flash specific binary
        flash_binary_file(&server_url, &target_board, &binary_path).await?
    } else {
        // Flash ESP-IDF project
        flash_esp_idf_project(&server_url, &target_board, project_dir, build_dir).await?
    }

    println!("🎉 Remote flash completed successfully!");
    Ok(())
}

/// Discover ESPBrew servers and select the best one
async fn discover_and_select_server() -> Result<String> {
    let servers = discover_espbrew_servers_silent(5)
        .await
        .context("Failed to discover ESPBrew servers")?;

    if servers.is_empty() {
        return Err(anyhow::anyhow!("No ESPBrew servers found on network"));
    }

    // Select preferred server (IPv4 over IPv6)
    let preferred_server = servers
        .iter()
        .find(|server| matches!(server.ip, std::net::IpAddr::V4(_)))
        .or_else(|| servers.first())
        .unwrap();

    // Format URL properly for IPv6/IPv4
    let ip_str = match preferred_server.ip {
        std::net::IpAddr::V6(_) => format!("[{}]", preferred_server.ip),
        std::net::IpAddr::V4(_) => preferred_server.ip.to_string(),
    };

    Ok(format!("http://{}:{}", ip_str, preferred_server.port))
}

/// Fetch available boards from remote server
async fn fetch_remote_boards(server_url: &str) -> Result<Vec<RemoteBoard>> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()?;

    let url = format!("{}/api/v1/boards", server_url.trim_end_matches('/'));
    println!("📡 Making request to: {}", url);

    let response = client
        .get(&url)
        .send()
        .await
        .context("Failed to connect to server")?;

    if !response.status().is_success() {
        return Err(anyhow::anyhow!(
            "Server returned error: {}",
            response.status()
        ));
    }

    let boards_response: RemoteBoardsResponse = response
        .json()
        .await
        .context("Failed to parse server response")?;

    Ok(boards_response.boards)
}

/// Select target board by MAC address, name, or interactive selection
fn select_target_board(
    boards: &[RemoteBoard],
    mac: Option<String>,
    name: Option<String>,
) -> Result<RemoteBoard> {
    // If MAC address specified, find by MAC
    if let Some(target_mac) = mac {
        if let Some(board) = boards.iter().find(|b| b.mac_address == target_mac) {
            return Ok(board.clone());
        } else {
            return Err(anyhow::anyhow!(
                "No board found with MAC address: {}",
                target_mac
            ));
        }
    }

    // If name specified, find by logical name or ID
    if let Some(target_name) = name {
        if let Some(board) = boards
            .iter()
            .find(|b| b.logical_name.as_deref() == Some(&target_name) || b.id == target_name)
        {
            return Ok(board.clone());
        } else {
            return Err(anyhow::anyhow!("No board found with name: {}", target_name));
        }
    }

    // Default to first available board
    if !boards.is_empty() {
        Ok(boards[0].clone())
    } else {
        Err(anyhow::anyhow!("No boards available"))
    }
}

/// Flash a specific binary file to remote board
async fn flash_binary_file(
    server_url: &str,
    board: &RemoteBoard,
    binary_path: &PathBuf,
) -> Result<()> {
    println!("📄 Flashing binary file: {}", binary_path.display());

    if !binary_path.exists() {
        return Err(anyhow::anyhow!(
            "Binary file not found: {}",
            binary_path.display()
        ));
    }

    let binary_data = std::fs::read(binary_path).context("Failed to read binary file")?;

    println!(
        "📆 Binary size: {:.1} KB",
        binary_data.len() as f64 / 1024.0
    );

    let client = reqwest::Client::new();
    let flash_url = format!("{}/api/v1/flash", server_url.trim_end_matches('/'));

    // Create multipart form for single binary
    let mut form = reqwest::multipart::Form::new();
    form = form.text("board_id", board.id.clone());
    form = form.text("binary_count", "1");

    // Add default flash configuration
    form = form.text("flash_mode", "dio");
    form = form.text("flash_freq", "80m");
    form = form.text("flash_size", "4MB");

    // Add binary data
    let filename = binary_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("firmware.bin")
        .to_string();

    form = form.part(
        "binary_0",
        reqwest::multipart::Part::bytes(binary_data)
            .file_name(filename.clone())
            .mime_str("application/octet-stream")?,
    );

    // Add binary metadata (default app offset)
    form = form.text("binary_0_name", "app");
    form = form.text("binary_0_offset", "0x10000");
    form = form.text("binary_0_filename", filename);

    println!("📡 Uploading to server...");

    let response = client
        .post(&flash_url)
        .multipart(form)
        .send()
        .await
        .context("Failed to upload binary")?;

    if response.status().is_success() {
        // Try to parse response for detailed info
        match response.json::<FlashResponse>().await {
            Ok(flash_response) => {
                if flash_response.success {
                    let duration_info = if let Some(duration) = flash_response.duration_ms {
                        format!(" ({}ms)", duration)
                    } else {
                        String::new()
                    };
                    println!(
                        "✅ Flash success{}: {}",
                        duration_info, flash_response.message
                    );
                } else {
                    return Err(anyhow::anyhow!("Flash failed: {}", flash_response.message));
                }
            }
            Err(_) => {
                println!("✅ Flash request completed (could not parse response details)");
            }
        }
    } else {
        let status = response.status();
        let error_text = response
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());
        return Err(anyhow::anyhow!(
            "Server rejected flash ({}): {}",
            status,
            error_text
        ));
    }

    Ok(())
}

/// Flash ESP-IDF project to remote board
async fn flash_esp_idf_project(
    server_url: &str,
    board: &RemoteBoard,
    project_dir: &std::path::Path,
    build_dir: Option<PathBuf>,
) -> Result<()> {
    println!(
        "📂 Flashing ESP-IDF project from: {}",
        project_dir.display()
    );

    // Discover build directories
    let build_dirs = if let Some(specific_dir) = build_dir {
        if !specific_dir.exists() {
            return Err(anyhow::anyhow!(
                "Specified build directory not found: {}",
                specific_dir.display()
            ));
        }
        vec![specific_dir]
    } else {
        discover_esp_build_directories(project_dir)?
    };

    if build_dirs.is_empty() {
        return Err(anyhow::anyhow!(
            "No ESP-IDF build directories found. Run 'idf.py build' first."
        ));
    }

    println!("🔍 Found {} build director(y/ies)", build_dirs.len());

    // Try each build directory until we find valid artifacts
    for build_dir in &build_dirs {
        let flash_args_path = build_dir.join("flash_args");

        if !flash_args_path.exists() {
            println!("⚠️ Skipping {}: no flash_args found", build_dir.display());
            continue;
        }

        println!("📝 Parsing flash_args: {}", flash_args_path.display());

        match parse_flash_args(&flash_args_path, build_dir) {
            Ok((flash_config, binaries)) => {
                println!("✅ Found {} binaries to flash", binaries.len());

                let total_size: u64 = binaries
                    .iter()
                    .map(|b| {
                        std::fs::metadata(&b.file_path)
                            .map(|m| m.len())
                            .unwrap_or(0)
                    })
                    .sum();

                println!("📆 Total size: {:.1} KB", total_size as f64 / 1024.0);

                for binary in &binaries {
                    let file_size = std::fs::metadata(&binary.file_path)
                        .map(|m| m.len())
                        .unwrap_or(0);
                    println!(
                        "  → {} @ 0x{:x} ({:.1} KB)",
                        binary.name,
                        binary.offset,
                        file_size as f64 / 1024.0
                    );
                }

                return upload_and_flash_esp_build(server_url, board, &flash_config, &binaries)
                    .await;
            }
            Err(e) => {
                println!("⚠️ Failed to parse {}: {}", flash_args_path.display(), e);
                continue;
            }
        }
    }

    Err(anyhow::anyhow!(
        "No valid ESP-IDF build artifacts found. Run 'idf.py build' first."
    ))
}

/// Discover ESP-IDF build directories in project
fn discover_esp_build_directories(project_dir: &std::path::Path) -> Result<Vec<PathBuf>> {
    use std::fs;

    let mut build_dirs = Vec::new();

    // Look for build directories
    if let Ok(entries) = fs::read_dir(project_dir) {
        for entry in entries.flatten() {
            if entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false) {
                let dir_name = entry.file_name();
                if let Some(name_str) = dir_name.to_str() {
                    if name_str.starts_with("build") {
                        build_dirs.push(entry.path());
                    }
                }
            }
        }
    }

    // Sort to get consistent ordering
    build_dirs.sort();
    Ok(build_dirs)
}

/// Parse ESP-IDF flash_args file
fn parse_flash_args(
    flash_args_path: &std::path::Path,
    build_dir: &std::path::Path,
) -> Result<(FlashConfig, Vec<FlashBinaryInfo>)> {
    let flash_args_content =
        std::fs::read_to_string(flash_args_path).context("Failed to read flash_args file")?;

    let mut flash_config = FlashConfig {
        flash_mode: "dio".to_string(),
        flash_freq: "80m".to_string(),
        flash_size: "4MB".to_string(),
    };

    let mut binaries = Vec::new();
    let mut args = flash_args_content.split_whitespace();

    while let Some(arg) = args.next() {
        match arg {
            "--flash_mode" => {
                if let Some(mode) = args.next() {
                    flash_config.flash_mode = mode.to_string();
                }
            }
            "--flash_freq" => {
                if let Some(freq) = args.next() {
                    flash_config.flash_freq = freq.to_string();
                }
            }
            "--flash_size" => {
                if let Some(size) = args.next() {
                    flash_config.flash_size = size.to_string();
                }
            }
            arg if arg.starts_with("0x") => {
                // Found flash offset, next should be binary path
                if let Some(binary_path_str) = args.next() {
                    let offset = u32::from_str_radix(&arg[2..], 16)
                        .context("Failed to parse flash offset")?;

                    let binary_path = if std::path::Path::new(binary_path_str).is_absolute() {
                        PathBuf::from(binary_path_str)
                    } else {
                        build_dir.join(binary_path_str)
                    };

                    if binary_path.exists() {
                        let file_name = binary_path
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("unknown.bin")
                            .to_string();

                        let name = match file_name.as_str() {
                            "bootloader.bin" => "bootloader",
                            "partition-table.bin" => "partition-table",
                            _ if file_name.ends_with(".bin") => "app",
                            _ => "unknown",
                        }
                        .to_string();

                        binaries.push(FlashBinaryInfo {
                            name,
                            offset,
                            file_name,
                            file_path: binary_path,
                        });
                    }
                }
            }
            _ => {}
        }
    }

    if binaries.is_empty() {
        return Err(anyhow::anyhow!("No binary files found in flash_args"));
    }

    Ok((flash_config, binaries))
}

/// Upload and flash ESP-IDF build to remote server
async fn upload_and_flash_esp_build(
    server_url: &str,
    board: &RemoteBoard,
    flash_config: &FlashConfig,
    binaries: &[FlashBinaryInfo],
) -> Result<()> {
    let client = reqwest::Client::new();
    let flash_url = format!("{}/api/v1/flash", server_url.trim_end_matches('/'));

    println!("📡 Uploading {} binaries to server...", binaries.len());

    // Create multipart form
    let mut form = reqwest::multipart::Form::new();
    form = form.text("board_id", board.id.clone());
    form = form.text("binary_count", binaries.len().to_string());
    form = form.text("flash_mode", flash_config.flash_mode.clone());
    form = form.text("flash_freq", flash_config.flash_freq.clone());
    form = form.text("flash_size", flash_config.flash_size.clone());

    // Add each binary
    for (i, binary_info) in binaries.iter().enumerate() {
        let binary_data = std::fs::read(&binary_info.file_path)
            .with_context(|| format!("Failed to read {}", binary_info.file_path.display()))?;

        println!(
            "  [{}/{}] {} → 0x{:x} ({:.1} KB)",
            i + 1,
            binaries.len(),
            binary_info.name,
            binary_info.offset,
            binary_data.len() as f64 / 1024.0
        );

        // Add binary data
        form = form.part(
            format!("binary_{}", i),
            reqwest::multipart::Part::bytes(binary_data)
                .file_name(binary_info.file_name.clone())
                .mime_str("application/octet-stream")?,
        );

        // Add binary metadata
        form = form.text(format!("binary_{}_name", i), binary_info.name.clone());
        form = form.text(
            format!("binary_{}_offset", i),
            format!("0x{:x}", binary_info.offset),
        );
        form = form.text(
            format!("binary_{}_filename", i),
            binary_info.file_name.clone(),
        );
    }

    println!("📤 Sending flash request to server...");

    let response = client
        .post(&flash_url)
        .multipart(form)
        .send()
        .await
        .context("Failed to send flash request")?;

    if response.status().is_success() {
        match response.json::<FlashResponse>().await {
            Ok(flash_response) => {
                if flash_response.success {
                    let duration_info = if let Some(duration) = flash_response.duration_ms {
                        format!(" ({}ms)", duration)
                    } else {
                        String::new()
                    };
                    println!(
                        "✅ Remote flash success{}: {}",
                        duration_info, flash_response.message
                    );
                } else {
                    return Err(anyhow::anyhow!("Flash failed: {}", flash_response.message));
                }
            }
            Err(_) => {
                println!("✅ Flash request completed (could not parse response details)");
            }
        }
    } else {
        let status = response.status();
        let error_text = response
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());
        return Err(anyhow::anyhow!(
            "Server rejected flash ({}): {}",
            status,
            error_text
        ));
    }

    Ok(())
}
