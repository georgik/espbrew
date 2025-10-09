//! CLI command implementations

pub mod build;
pub mod discover;
pub mod flash;
pub mod list;
pub mod remote_flash;
pub mod remote_monitor;

use crate::cli::args::{Cli, Commands};
use anyhow::Result;

/// Execute a CLI command
pub async fn execute_command(command: Commands, cli: &Cli) -> Result<()> {
    match command {
        Commands::List => list::execute_list_command(cli).await,
        Commands::Build => build::execute_build_command(cli).await,
        Commands::Discover { timeout } => discover::execute_discover_command(timeout).await,
        Commands::Flash {
            binary,
            config,
            port,
        } => flash::execute_flash_command(cli, binary, config, port).await,
        Commands::RemoteFlash {
            binary,
            config,
            build_dir,
            mac,
            name,
            server,
        } => {
            remote_flash::execute_remote_flash_command(
                cli, binary, config, build_dir, mac, name, server,
            )
            .await
        }
        Commands::RemoteMonitor {
            mac,
            name,
            server,
            baud_rate,
            reset,
        } => {
            remote_monitor::execute_remote_monitor_command(cli, mac, name, server, baud_rate, reset)
                .await
        }
    }
}
