//! mDNS Server Discovery for ESPBrew
//!
//! This module provides functionality to discover ESPBrew servers on the local
//! network using mDNS (multicast DNS) service discovery.

use crate::models::server::DiscoveredServer;
use anyhow::Result;
use mdns_sd::{ServiceDaemon, ServiceEvent};
use std::net::{IpAddr, Ipv4Addr};

/// Discover ESPBrew servers on the local network using mDNS (silent version for TUI)
/// This version doesn't print to console, making it suitable for TUI applications
pub async fn discover_espbrew_servers_silent(timeout_secs: u64) -> Result<Vec<DiscoveredServer>> {
    log::debug!(
        "Starting silent ESPBrew server discovery with timeout: {}s",
        timeout_secs
    );

    let mdns =
        ServiceDaemon::new().map_err(|e| anyhow::anyhow!("Failed to create mDNS daemon: {}", e))?;

    // Browse for ESPBrew services
    let service_type = "_espbrew._tcp.local.";
    let receiver = mdns
        .browse(service_type)
        .map_err(|e| anyhow::anyhow!("Failed to start mDNS browse: {}", e))?;

    let mut servers = Vec::new();
    let timeout = tokio::time::Duration::from_secs(timeout_secs);
    let start_time = tokio::time::Instant::now();

    // Listen for mDNS events with timeout
    let receiver = receiver;
    while start_time.elapsed() < timeout {
        let remaining_time = timeout - start_time.elapsed();

        match tokio::time::timeout(remaining_time, receiver.recv_async()).await {
            Ok(Ok(event)) => {
                match event {
                    ServiceEvent::ServiceResolved(info) => {
                        log::trace!("Resolved service: {}", info.get_fullname());
                        // Parse TXT records
                        let mut version = "unknown".to_string();
                        let mut hostname = "unknown".to_string();
                        let mut description = "ESPBrew Server".to_string();
                        let mut board_count = 0u32;
                        let mut boards_list = String::new();

                        // Parse TXT record properties
                        let properties = info.get_properties();
                        for property in properties.iter() {
                            let property_string = format!("{}", property);
                            if let Some((key, value)) = property_string.split_once('=') {
                                match key {
                                    "version" => version = value.to_string(),
                                    "hostname" => hostname = value.to_string(),
                                    "description" => description = value.to_string(),
                                    "board_count" => {
                                        board_count = value.parse().unwrap_or(0);
                                    }
                                    "boards" => boards_list = value.to_string(),
                                    _ => {}
                                }
                            }
                        }

                        let server = DiscoveredServer {
                            name: info.get_hostname().to_string(),
                            ip: *info
                                .get_addresses()
                                .iter()
                                .next()
                                .unwrap_or(&IpAddr::V4(Ipv4Addr::LOCALHOST)),
                            port: info.get_port(),
                            hostname,
                            version,
                            description,
                            board_count,
                            boards_list,
                        };

                        log::debug!(
                            "Discovered server: {} at {}:{}",
                            server.name,
                            server.ip,
                            server.port
                        );
                        servers.push(server);
                    }
                    ServiceEvent::SearchStarted(_) => {
                        log::trace!("mDNS search started for ESPBrew services");
                    }
                    ServiceEvent::SearchStopped(_) => {
                        log::trace!("mDNS search stopped");
                        break;
                    }
                    _ => {}
                }
            }
            Ok(Err(e)) => {
                log::debug!("mDNS receiver error in silent discovery: {}", e);
                break;
            }
            Err(_) => {
                log::debug!(
                    "Discovery timeout reached in silent mode: {}s",
                    timeout_secs
                );
                break;
            }
        }
    }

    // Stop the browse operation
    let _ = mdns.stop_browse(service_type);

    Ok(servers)
}

/// Discover ESPBrew servers on the local network using mDNS (verbose version for CLI)
/// This version prints discovery progress, suitable for CLI applications
pub async fn discover_espbrew_servers(timeout_secs: u64) -> Result<Vec<DiscoveredServer>> {
    log::info!(
        "Starting verbose ESPBrew server discovery with timeout: {}s",
        timeout_secs
    );

    let mdns =
        ServiceDaemon::new().map_err(|e| anyhow::anyhow!("Failed to create mDNS daemon: {}", e))?;

    // Browse for ESPBrew services
    let service_type = "_espbrew._tcp.local.";
    let receiver = mdns
        .browse(service_type)
        .map_err(|e| anyhow::anyhow!("Failed to start mDNS browse: {}", e))?;

    println!("🔍 Browsing for {} services...", service_type);

    let mut servers = Vec::new();
    let timeout = tokio::time::Duration::from_secs(timeout_secs);
    let start_time = tokio::time::Instant::now();

    // Listen for mDNS events with timeout
    let receiver = receiver;
    while start_time.elapsed() < timeout {
        let remaining_time = timeout - start_time.elapsed();

        match tokio::time::timeout(remaining_time, receiver.recv_async()).await {
            Ok(Ok(event)) => {
                match event {
                    ServiceEvent::ServiceResolved(info) => {
                        log::debug!("Found mDNS service: {}", info.get_fullname());
                        println!("🔍 Found service: {}", info.get_fullname());

                        // Parse TXT records
                        let mut version = "unknown".to_string();
                        let mut hostname = "unknown".to_string();
                        let mut description = "ESPBrew Server".to_string();
                        let mut board_count = 0u32;
                        let mut boards_list = String::new();

                        // Parse TXT record properties
                        let properties = info.get_properties();
                        for property in properties.iter() {
                            let property_string = format!("{}", property);
                            if let Some((key, value)) = property_string.split_once('=') {
                                match key {
                                    "version" => version = value.to_string(),
                                    "hostname" => hostname = value.to_string(),
                                    "description" => description = value.to_string(),
                                    "board_count" => {
                                        board_count = value.parse().unwrap_or(0);
                                    }
                                    "boards" => boards_list = value.to_string(),
                                    _ => {}
                                }
                            }
                        }

                        let server = DiscoveredServer {
                            name: info.get_hostname().to_string(),
                            ip: *info
                                .get_addresses()
                                .iter()
                                .next()
                                .unwrap_or(&IpAddr::V4(Ipv4Addr::LOCALHOST)),
                            port: info.get_port(),
                            hostname,
                            version,
                            description,
                            board_count,
                            boards_list,
                        };

                        log::info!(
                            "Successfully resolved ESPBrew server: {} at {}:{}",
                            server.name,
                            server.ip,
                            server.port
                        );
                        println!(
                            "✅ Discovered: {} at {}:{}",
                            server.name, server.ip, server.port
                        );
                        servers.push(server);
                    }
                    ServiceEvent::SearchStarted(_) => {
                        log::debug!("mDNS search started for ESPBrew services");
                        println!("🔍 Search started for ESPBrew services...");
                    }
                    ServiceEvent::SearchStopped(_) => {
                        log::debug!("mDNS search stopped");
                        println!("🔍 Search stopped.");
                        break;
                    }
                    _ => {}
                }
            }
            Ok(Err(e)) => {
                log::error!("mDNS receiver error during discovery: {}", e);
                eprintln!("⚠️ mDNS receiver error: {}", e);
                break;
            }
            Err(_) => {
                // Timeout reached
                log::debug!("Discovery timeout reached: {}s", timeout_secs);
                println!("🕐 Discovery timeout reached ({} seconds)", timeout_secs);
                break;
            }
        }
    }

    // Stop the browse operation
    let _ = mdns.stop_browse(service_type);

    Ok(servers)
}
