//! ESPBrew Server module
//!
//! This module contains the server implementation for remote ESP32 board
//! management, including flashing, monitoring, and board discovery.

pub mod app;
pub mod middleware;
pub mod routes;
pub mod services;

pub use app::*;

use anyhow::Result;
use std::net::SocketAddr;

/// Server configuration
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ServerConfig {
    /// Server listening address
    pub bind_address: String,
    /// Server listening port
    pub port: u16,
    /// Board discovery interval in seconds
    pub scan_interval: u64,
    /// Board mappings (port -> logical_name)
    pub board_mappings: std::collections::HashMap<String, String>,
    /// Maximum binary size for uploads (in MB)
    pub max_binary_size_mb: usize,
    /// Enable mDNS service announcement
    pub enable_mdns: bool,
    /// mDNS service name (defaults to hostname)
    pub mdns_name: Option<String>,
    /// Server description for mDNS
    pub mdns_description: Option<String>,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            bind_address: "0.0.0.0".to_string(),
            port: 8080,
            scan_interval: 30,
            board_mappings: std::collections::HashMap::new(),
            max_binary_size_mb: 50,
            enable_mdns: true,
            mdns_name: None, // Will default to hostname
            mdns_description: Some("ESPBrew Remote Flashing Server".to_string()),
        }
    }
}

/// Start the ESPBrew server
pub async fn start_server(config: ServerConfig) -> Result<()> {
    let app = ServerApp::new(config.clone()).await?;
    app.run().await
}
