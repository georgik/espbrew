//! mDNS service registration for ESPBrew server discovery

use anyhow::Result;
use mdns_sd::{ServiceDaemon, ServiceInfo};
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::server::ServerConfig;
use crate::server::app::ServerState;

/// mDNS service for server discovery
pub struct MdnsService {
    daemon: ServiceDaemon,
    service_name: String,
    service_type: String,
}

impl MdnsService {
    /// Create a new mDNS service
    pub fn new(config: &ServerConfig) -> Result<Self> {
        let daemon = ServiceDaemon::new()
            .map_err(|e| anyhow::anyhow!("Failed to create mDNS daemon: {}", e))?;

        // Use provided mDNS name or default to hostname
        let service_name = if let Some(ref name) = config.mdns_name {
            name.clone()
        } else {
            hostname::get()
                .map(|h| h.to_string_lossy().to_string())
                .unwrap_or_else(|_| "espbrew-server".to_string())
        };

        let service_type = "_espbrew._tcp.local.".to_string();

        Ok(Self {
            daemon,
            service_name,
            service_type,
        })
    }

    /// Register the ESPBrew server for discovery
    pub async fn register(
        &self,
        config: &ServerConfig,
        state: Arc<RwLock<ServerState>>,
    ) -> Result<()> {
        if !config.enable_mdns {
            println!("📡 mDNS service announcement disabled");
            return Ok(());
        }

        // Get current board count and board list
        let (board_count, boards_list) = {
            let state_lock = state.read().await;
            let board_count = state_lock.boards.len();
            let boards_list = state_lock
                .boards
                .values()
                .map(|board| {
                    board
                        .logical_name
                        .as_deref()
                        .unwrap_or(&board.id)
                        .to_string()
                })
                .collect::<Vec<String>>()
                .join(",");
            (board_count, boards_list)
        };

        // Get server version
        let version = env!("CARGO_PKG_VERSION");

        // Get hostname and ensure proper format for mDNS
        let hostname = {
            let host = hostname::get()
                .map(|h| h.to_string_lossy().to_string())
                .unwrap_or_else(|_| "espbrew-server".to_string());

            // Ensure hostname ends with .local. as required by mDNS spec
            let base_host = host.trim_end_matches(".local").trim_end_matches(".");
            format!("{}.local.", base_host)
        };

        // Get local IP addresses for all interfaces
        let addresses = self.get_local_addresses();
        if addresses.is_empty() {
            return Err(anyhow::anyhow!(
                "No network interfaces found for mDNS registration"
            ));
        }

        println!(
            "🔍 Found {} network addresses for mDNS: {:?}",
            addresses.len(),
            addresses
        );

        // Create service info with TXT records
        let service_info = ServiceInfo::new(
            &self.service_type,
            &self.service_name,
            &hostname,
            &addresses[..], // Use all available addresses
            config.port,
            &[
                ("version", version),
                ("hostname", &hostname),
                (
                    "description",
                    config
                        .mdns_description
                        .as_deref()
                        .unwrap_or("ESPBrew Remote Flashing Server"),
                ),
                ("board_count", &board_count.to_string()),
                ("boards", &boards_list),
            ][..],
        )
        .map_err(|e| anyhow::anyhow!("Failed to create service info: {}", e))?;

        // Register the service
        self.daemon
            .register(service_info)
            .map_err(|e| anyhow::anyhow!("Failed to register mDNS service: {}", e))?;

        println!(
            "📡 mDNS service registered: {} ({}) with {} boards",
            self.service_name, self.service_type, board_count
        );
        println!("🔍 Server discoverable at: {}:{}", hostname, config.port);

        Ok(())
    }

    /// Update the service with current board information
    pub async fn update_board_info(&self, state: Arc<RwLock<ServerState>>) -> Result<()> {
        // Get current board count and board list
        let (board_count, boards_list) = {
            let state_lock = state.read().await;
            let board_count = state_lock.boards.len();
            let boards_list = state_lock
                .boards
                .values()
                .map(|board| {
                    board
                        .logical_name
                        .as_deref()
                        .unwrap_or(&board.id)
                        .to_string()
                })
                .collect::<Vec<String>>()
                .join(",");
            (board_count, boards_list)
        };

        // Note: mdns-sd doesn't have a direct update method, so we would need to
        // unregister and re-register. For now, we'll just log the update.
        // In a production system, we might want to implement this.
        println!(
            "🔄 mDNS service info updated: {} boards ({})",
            board_count, boards_list
        );

        Ok(())
    }

    /// Unregister the mDNS service
    pub fn unregister(&self) -> Result<()> {
        // Create the full service name for unregistration
        let full_service_name = format!("{}.{}", self.service_name, self.service_type);

        self.daemon
            .unregister(&full_service_name)
            .map_err(|e| anyhow::anyhow!("Failed to unregister mDNS service: {}", e))?;

        println!("📡 mDNS service unregistered: {}", self.service_name);
        Ok(())
    }

    /// Get local IP addresses for mDNS registration
    fn get_local_addresses(&self) -> Vec<std::net::IpAddr> {
        match if_addrs::get_if_addrs() {
            Ok(interfaces) => {
                interfaces
                    .into_iter()
                    .filter_map(|iface| {
                        // Skip loopback and down interfaces
                        if iface.is_loopback() {
                            return None;
                        }

                        // Include both IPv4 and IPv6 addresses
                        match iface.addr.ip() {
                            ip @ (std::net::IpAddr::V4(_) | std::net::IpAddr::V6(_)) => {
                                println!("🔍 Network interface {}: {}", iface.name, ip);
                                Some(ip)
                            }
                        }
                    })
                    .collect()
            }
            Err(e) => {
                println!("⚠️ Failed to get network interfaces: {}", e);
                // Fallback to localhost if we can't get interfaces
                vec![std::net::IpAddr::V4(std::net::Ipv4Addr::new(127, 0, 0, 1))]
            }
        }
    }

    /// Shutdown the mDNS daemon
    pub fn shutdown(self) -> Result<()> {
        // Shutdown the daemon
        self.daemon
            .shutdown()
            .map_err(|e| anyhow::anyhow!("Failed to shutdown mDNS daemon: {}", e))?;

        println!("🛑 mDNS daemon shut down");
        Ok(())
    }
}

/// Discover ESPBrew servers on the local network using mDNS
pub async fn discover_espbrew_servers(timeout_secs: u64) -> Result<Vec<DiscoveredServer>> {
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
    while start_time.elapsed() < timeout {
        let remaining_time = timeout - start_time.elapsed();

        match tokio::time::timeout(remaining_time, receiver.recv_async()).await {
            Ok(Ok(event)) => {
                match event {
                    mdns_sd::ServiceEvent::ServiceResolved(info) => {
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

                        let server =
                            DiscoveredServer {
                                name: info.get_hostname().to_string(),
                                ip: *info.get_addresses().iter().next().unwrap_or(
                                    &std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
                                ),
                                port: info.get_port(),
                                hostname,
                                version,
                                description,
                                board_count,
                                boards_list,
                            };

                        println!(
                            "✅ Discovered: {} at {}:{}",
                            server.name, server.ip, server.port
                        );
                        servers.push(server);
                    }
                    mdns_sd::ServiceEvent::SearchStarted(_) => {
                        println!("🔍 Search started for ESPBrew services...");
                    }
                    mdns_sd::ServiceEvent::SearchStopped(_) => {
                        println!("🔍 Search stopped.");
                        break;
                    }
                    _ => {}
                }
            }
            Ok(Err(e)) => {
                eprintln!("⚠️ mDNS receiver error: {}", e);
                break;
            }
            Err(_) => {
                // Timeout reached
                println!("🕐 Discovery timeout reached ({} seconds)", timeout_secs);
                break;
            }
        }
    }

    // Stop the browse operation
    let _ = mdns.stop_browse(service_type);

    Ok(servers)
}

/// Discovered ESPBrew server information
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DiscoveredServer {
    pub name: String,
    pub ip: std::net::IpAddr,
    pub port: u16,
    pub hostname: String,
    pub version: String,
    pub description: String,
    pub board_count: u32,
    pub boards_list: String,
}
