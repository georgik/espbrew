//! ESPBrew Server - Remote ESP32 Flashing Server
//!
//! Binary entry point for the server application.

use anyhow::Result;
use clap::{Parser, Subcommand};
use espbrew::server::{ServerConfig, start_server};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "espbrew-server")]
#[command(about = "ESPBrew Remote Flashing Server")]
struct ServerCli {
    /// Server configuration file
    #[arg(short, long, default_value = "espbrew-server.toml")]
    config: PathBuf,

    /// Bind address
    #[arg(short, long, default_value = "0.0.0.0")]
    bind: String,

    /// Port to listen on
    #[arg(short, long, default_value = "8080")]
    port: u16,

    /// Board scan interval in seconds
    #[arg(long, default_value = "30")]
    scan_interval: u64,

    /// Disable mDNS service announcement
    #[arg(long)]
    no_mdns: bool,

    /// mDNS service name (defaults to hostname)
    #[arg(long)]
    mdns_name: Option<String>,

    #[command(subcommand)]
    command: Option<ServerCommands>,
}

#[derive(Subcommand)]
enum ServerCommands {
    /// Start the server
    Start,
    /// Scan for boards and exit
    Scan,
    /// Generate default configuration
    Config,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let cli = ServerCli::parse();

    let config = ServerConfig {
        bind_address: cli.bind,
        port: cli.port,
        scan_interval: cli.scan_interval,
        board_mappings: std::collections::HashMap::new(),
        max_binary_size_mb: 50,
        enable_mdns: !cli.no_mdns,
        mdns_name: cli.mdns_name,
        mdns_description: Some("ESPBrew Remote Flashing Server".to_string()),
    };

    match cli.command {
        Some(ServerCommands::Start) | None => {
            println!("🍺 Starting ESPBrew Server...");
            start_server(config).await
        }
        Some(ServerCommands::Scan) => {
            println!("🔍 Scanning for boards...");
            // TODO: Implement board scan only mode
            todo!("Board scan mode not yet implemented")
        }
        Some(ServerCommands::Config) => {
            println!("⚙️  Generating default configuration...");
            // TODO: Implement config generation
            todo!("Config generation not yet implemented")
        }
    }
}
