//! Discover command implementation

use crate::remote::discovery::discover_espbrew_servers;
use anyhow::Result;
use log::{error, info, warn};

pub async fn execute_discover_command(timeout: u64) -> Result<()> {
    log::info!(
        "Starting ESPBrew server discovery with timeout {}s",
        timeout
    );

    info!("🔍 ESPBrew Server Discovery");
    info!(
        "🔎 Scanning network for ESPBrew servers (timeout: {}s)...",
        timeout
    );
    match discover_espbrew_servers(timeout).await {
        Ok(servers) => {
            log::debug!("Discovery completed, found {} servers", servers.len());
            if servers.is_empty() {
                warn!("No ESPBrew servers found on the network.");
                info!("Make sure:");
                info!("   • ESPBrew server is running on the network");
                info!("   • mDNS/Bonjour is enabled on your system");
                info!("   • Firewall allows mDNS traffic (UDP port 5353)");
                return Ok(());
            }

            info!("Found {} ESPBrew server(s)", servers.len());

            for (i, server) in servers.iter().enumerate() {
                info!("{}. Server: {}", i + 1, server.name);
                info!("   Address: {}:{}", server.ip, server.port);
                info!("   Version: {}", server.version);
                info!("   Description: {}", server.description);

                if server.board_count > 0 {
                    info!("   Boards: {} connected", server.board_count);
                    if !server.boards_list.is_empty() {
                        let boards: Vec<&str> = server.boards_list.split(',').collect();
                        for board in boards {
                            info!("     • {}", board.trim());
                        }
                    }
                } else {
                    info!("   Boards: No boards connected");
                }

                // Use mDNS hostname directly (already includes .local suffix)
                let hostname_url = format!("http://{}:{}", server.name, server.port);
                let ip_url = match server.ip {
                    std::net::IpAddr::V6(_) => format!("http://[{}]:{}", server.ip, server.port),
                    std::net::IpAddr::V4(_) => format!("http://{}:{}", server.ip, server.port),
                };

                info!("   🌍 API URL: {} ({})", hostname_url, ip_url);

                // Test connectivity using hostname.local for better compatibility
                log::debug!("Testing connectivity to server: {}", hostname_url);
                let status = match test_server_connectivity(&hostname_url).await {
                    Ok(_) => {
                        log::debug!("Server {} is online and responsive", hostname_url);
                        "✅ Online and responsive"
                    }
                    Err(e) => {
                        log::debug!(
                            "Hostname connectivity failed for {}: {}, trying IP fallback",
                            hostname_url,
                            e
                        );
                        // If hostname.local fails, try IP address as fallback
                        match test_server_connectivity(&ip_url).await {
                            Ok(_) => {
                                log::debug!("Server {} is online via IP address", ip_url);
                                "✅ Online via IP (hostname.local failed)"
                            }
                            Err(e2) => {
                                log::warn!(
                                    "Connection failed to server {} (hostname: {}, IP: {})",
                                    server.name,
                                    e,
                                    e2
                                );
                                "❌ Connection failed (both hostname and IP)"
                            }
                        }
                    }
                };
                info!("   🔌 Status: {}", status);

                if i < servers.len() - 1 {
                    println!();
                }
            }

            println!();
            info!("🎉 Discovery completed successfully!");

            // Show summary for multiple servers
            if servers.len() > 1 {
                println!();
                info!("📋 Summary:");
                for (i, server) in servers.iter().enumerate() {
                    // Use mDNS hostname directly (already includes .local suffix)
                    let url = format!("http://{}:{}", server.name, server.port);
                    info!(
                        "  {}. {} - {} ({} boards)",
                        i + 1,
                        server.name,
                        url,
                        server.board_count
                    );
                }
            }

            // Provide helpful next steps
            if servers.len() == 1 {
                let server = &servers[0];
                // Use mDNS hostname directly (already includes .local suffix)
                let url = format!("http://{}:{}", server.name, server.port);
                println!();
                info!("💡 Next steps:");
                info!(
                    "   • Flash to remote board: espbrew remote-flash --server {}",
                    url
                );
                info!("   • List available boards: curl {}/api/v1/boards", url);
            } else if servers.len() > 1 {
                println!();
                info!("💡 Next steps:");
                info!("   • Flash to specific server: espbrew remote-flash --server <URL>");
                info!("   • Let auto-discovery pick: espbrew remote-flash");
            }
        }
        Err(e) => {
            log::error!("ESPBrew server discovery failed: {}", e);
            error!("❌ Discovery failed: {}", e);
            println!();
            error!("🔧 Troubleshooting:");
            error!("   • Check if mDNS/Bonjour service is running");
            error!("   • Verify network connectivity");
            error!("   • Try increasing timeout with: --timeout <seconds>");
            error!("   • Check firewall settings for UDP port 5353");
            return Err(e);
        }
    }

    Ok(())
}

/// Execute discover command with specific server URL (used by URL handler)
pub async fn execute_discover_command_with_server(server_url: &str) -> Result<()> {
    log::info!("Testing connectivity to specific server: {}", server_url);

    info!("🔍 ESPBrew Server Connectivity Test");
    info!("🔗 Testing server: {}", server_url);
    println!();

    match test_server_connectivity(server_url).await {
        Ok(_) => {
            info!("✅ Server is online and responsive!");

            // Try to get board information
            match get_server_boards(server_url).await {
                Ok(board_count) => {
                    info!("📊 Server has {} board(s) connected", board_count);
                    println!();
                    info!("💡 Next steps:");
                    info!(
                        "   • Flash to remote board: espbrew remote-flash --server {}",
                        server_url
                    );
                    info!(
                        "   • List available boards: curl {}/api/v1/boards",
                        server_url
                    );
                }
                Err(e) => {
                    log::warn!("Failed to get board information: {}", e);
                    warn!("⚠️  Could not retrieve board information: {}", e);
                }
            }
        }
        Err(e) => {
            error!("❌ Connection failed: {}", e);
            println!();
            error!("🔧 Troubleshooting:");
            error!("   • Check if ESPBrew server is running at {}", server_url);
            error!("   • Verify network connectivity");
            error!("   • Check firewall settings");
            return Err(e);
        }
    }

    Ok(())
}

/// Get board count from server
async fn get_server_boards(server_url: &str) -> Result<usize> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()?;

    let api_url = format!("{}/api/v1/boards", server_url.trim_end_matches('/'));
    let response = client.get(&api_url).send().await?.error_for_status()?;

    let body: serde_json::Value = response.json().await?;

    if let Some(boards) = body.get("boards").and_then(|b| b.as_array()) {
        Ok(boards.len())
    } else {
        Ok(0)
    }
}

/// Test connectivity to a discovered server
async fn test_server_connectivity(url: &str) -> Result<()> {
    log::trace!("Testing server connectivity to: {}", url);

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(3))
        .build()?;

    let api_url = format!("{}/api/v1/boards", url.trim_end_matches('/'));
    log::trace!("Making connectivity test request to: {}", api_url);

    let response = client.get(&api_url).send().await?.error_for_status()?;

    // Just check if we get a valid response, don't need to parse it
    let _ = response.bytes().await?;
    log::trace!("Connectivity test successful for: {}", url);
    Ok(())
}
