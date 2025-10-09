//! ESPBrew Server - Remote ESP32 Flashing Server
//!
//! Binary entry point for the server application.

use anyhow::Result;
use clap::{Parser, Subcommand};
use espbrew::server::{ServerConfig, start_server};
use std::path::PathBuf;
use tokio::fs;

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
            scan_boards_only().await
        }
        Some(ServerCommands::Config) => {
            println!("⚙️  Generating default configuration...");
            generate_config(&cli.config).await
        }
    }
}

/// Generate a default server configuration file
async fn generate_config(config_path: &PathBuf) -> Result<()> {
    let default_config = ServerConfig::default();

    // Serialize to TOML
    let toml_content = toml::to_string_pretty(&default_config)
        .map_err(|e| anyhow::anyhow!("Failed to serialize config to TOML: {}", e))?;

    // Write to file
    fs::write(config_path, toml_content).await.map_err(|e| {
        anyhow::anyhow!(
            "Failed to write config file '{}': {}",
            config_path.display(),
            e
        )
    })?;

    println!(
        "✅ Generated default configuration file: {}",
        config_path.display()
    );
    println!("ℹ️  You can edit this file to customize server settings.");
    println!(
        "ℹ️  Use --config {} to load this configuration.",
        config_path.display()
    );

    Ok(())
}

/// Scan for boards and display results without starting the server
async fn scan_boards_only() -> Result<()> {
    use espbrew::server::app::ServerState;
    use espbrew::server::services::board_scanner::BoardScanner;
    use std::sync::Arc;
    use tokio::sync::RwLock;

    println!("🔍 ESPBrew Board Scanner - Scan Only Mode");
    println!("📡 Scanning for connected ESP32/ESP8266 development boards...\n");

    // Create a minimal server state for the scanner
    let server_state = Arc::new(RwLock::new(ServerState::new(ServerConfig::default())));

    // Create and run the board scanner
    let scanner = BoardScanner::new(server_state.clone());

    match scanner.scan_boards().await {
        Ok(board_count) => {
            println!();
            if board_count > 0 {
                println!("✅ Scan completed successfully!");
                println!("📊 Summary: Found {} board(s)", board_count);
                println!();
                println!("💡 To interact with these boards:");
                println!("   • Start the full server: espbrew-server");
                println!("   • Use the TUI: espbrew");
                println!("   • Flash remotely: espbrew remote-flash");
            } else {
                println!("📋 Scan completed - No boards found");
                println!();
                println!("💡 Troubleshooting:");
                println!("   • Ensure ESP32/ESP8266 boards are connected via USB");
                println!("   • Check that USB drivers are installed");
                println!("   • Verify boards are not in use by other applications");
                println!("   • Try different USB ports or cables");
            }
            Ok(())
        }
        Err(e) => {
            println!("❌ Board scan failed: {}", e);
            println!();
            println!("💡 This might be due to:");
            println!("   • Permission issues accessing serial ports");
            println!("   • Missing USB drivers");
            println!("   • Hardware connectivity problems");
            println!();
            println!("🔧 Try running with elevated permissions or check system logs");
            Err(e)
        }
    }
}
