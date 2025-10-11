//! Discover command implementation

use crate::remote::discovery::discover_espbrew_servers;
use anyhow::Result;

pub async fn execute_discover_command(timeout: u64) -> Result<()> {
    log::info!(
        "Starting ESPBrew server discovery with timeout {}s",
        timeout
    );

    println!("🔍 ESPBrew Server Discovery");
    println!(
        "🔎 Scanning network for ESPBrew servers (timeout: {}s)...",
        timeout
    );

    log::debug!(
        "Calling discover_espbrew_servers with timeout: {}s",
        timeout
    );
    match discover_espbrew_servers(timeout).await {
        Ok(servers) => {
            log::debug!("Discovery completed, found {} servers", servers.len());
            if servers.is_empty() {
                println!("⚠️  No ESPBrew servers found on the network.");
                println!("📝 Make sure:");
                println!("   • ESPBrew server is running on the network");
                println!("   • mDNS/Bonjour is enabled on your system");
                println!("   • Firewall allows mDNS traffic (UDP port 5353)");
                return Ok(());
            }

            println!("✅ Found {} ESPBrew server(s):", servers.len());
            println!();

            for (i, server) in servers.iter().enumerate() {
                println!("{}. 🖥️  Server: {}", i + 1, server.name);
                println!("   🔗 Address: {}:{}", server.ip, server.port);
                println!("   🏷️  Version: {}", server.version);
                println!("   📋 Description: {}", server.description);

                if server.board_count > 0 {
                    println!("   📊 Boards: {} connected", server.board_count);
                    if !server.boards_list.is_empty() {
                        let boards: Vec<&str> = server.boards_list.split(',').collect();
                        for board in boards {
                            println!("     • {}", board.trim());
                        }
                    }
                } else {
                    println!("   📊 Boards: No boards connected");
                }

                // Use mDNS hostname directly (already includes .local suffix)
                let hostname_url = format!("http://{}:{}", server.name, server.port);
                let ip_url = match server.ip {
                    std::net::IpAddr::V6(_) => format!("http://[{}]:{}", server.ip, server.port),
                    std::net::IpAddr::V4(_) => format!("http://{}:{}", server.ip, server.port),
                };

                println!("   🌍 API URL: {} ({})", hostname_url, ip_url);

                // Test connectivity using hostname.local for better compatibility
                log::debug!("Testing connectivity to server: {}", hostname_url);
                print!("   🔌 Status: ");
                match test_server_connectivity(&hostname_url).await {
                    Ok(_) => {
                        log::debug!("Server {} is online and responsive", hostname_url);
                        println!("✅ Online and responsive");
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
                                println!("✅ Online via IP (hostname.local failed)");
                            }
                            Err(e2) => {
                                log::warn!(
                                    "Connection failed to server {} (hostname: {}, IP: {})",
                                    server.name,
                                    e,
                                    e2
                                );
                                println!("❌ Connection failed (both hostname and IP)");
                            }
                        }
                    }
                }

                if i < servers.len() - 1 {
                    println!();
                }
            }

            println!();
            println!("🎉 Discovery completed successfully!");

            // Show summary for multiple servers
            if servers.len() > 1 {
                println!();
                println!("📋 Summary:");
                for (i, server) in servers.iter().enumerate() {
                    // Use mDNS hostname directly (already includes .local suffix)
                    let url = format!("http://{}:{}", server.name, server.port);
                    println!(
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
                println!("💡 Next steps:");
                println!(
                    "   • Flash to remote board: espbrew remote-flash --server {}",
                    url
                );
                println!("   • List available boards: curl {}/api/v1/boards", url);
            } else if servers.len() > 1 {
                println!();
                println!("💡 Next steps:");
                println!("   • Flash to specific server: espbrew remote-flash --server <URL>");
                println!("   • Let auto-discovery pick: espbrew remote-flash");
            }
        }
        Err(e) => {
            log::error!("ESPBrew server discovery failed: {}", e);
            println!("❌ Discovery failed: {}", e);
            println!();
            println!("🔧 Troubleshooting:");
            println!("   • Check if mDNS/Bonjour service is running");
            println!("   • Verify network connectivity");
            println!("   • Try increasing timeout with: --timeout <seconds>");
            println!("   • Check firewall settings for UDP port 5353");
            return Err(e);
        }
    }

    Ok(())
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
