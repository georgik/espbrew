//! Command line argument parsing

use crate::models::project::BuildStrategy;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
#[command(name = "espbrew")]
#[command(
    about = "🍺 Multi-Platform ESP32 Build Manager - Supports ESP-IDF, Rust no_std, and Arduino projects!"
)]
pub struct Cli {
    /// Path to project directory (ESP-IDF, Rust no_std, or Arduino - defaults to current directory)
    #[arg(global = true, value_name = "PROJECT_DIR")]
    pub project_dir: Option<PathBuf>,

    /// Run in CLI mode without TUI - for automation and scripting
    #[arg(long, help = "Run in CLI mode without interactive TUI")]
    pub cli: bool,

    /// Increase logging verbosity (-v for debug, -vv for trace)
    #[arg(short = 'v', long = "verbose", action = clap::ArgAction::Count)]
    pub verbose: u8,

    /// Decrease logging verbosity (only errors)
    #[arg(short = 'q', long = "quiet")]
    pub quiet: bool,

    /// Build strategy: 'idf-build-apps' (default, professional), 'sequential' (safe) or 'parallel' (may have conflicts)
    #[arg(
        long,
        default_value = "idf-build-apps",
        help = "Build strategy for multiple boards"
    )]
    pub build_strategy: BuildStrategy,

    /// Remote ESPBrew server URL for remote flashing
    #[arg(
        long,
        help = "ESPBrew server URL for remote flashing (default: http://localhost:8080)"
    )]
    pub server_url: Option<String>,

    /// Target board MAC address for remote flashing
    #[arg(long, help = "Target board MAC address for remote flashing")]
    pub board_mac: Option<String>,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand, Clone)]
pub enum Commands {
    /// List boards and components (default CLI behavior)
    List,
    /// Build all boards
    Build {
        /// Build only specific board (if not specified, builds all boards)
        #[arg(short, long, help = "Build only specific board configuration")]
        board: Option<String>,
    },
    /// Discover ESPBrew servers on the local network via mDNS
    Discover {
        /// Timeout for discovery in seconds
        #[arg(short, long, default_value = "5")]
        timeout: u64,
    },
    /// Flash firmware to board(s) using local tools (idf.py flash or esptool)
    Flash {
        /// Path to binary file to flash (if not specified, will look for built binary)
        #[arg(short, long)]
        binary: Option<PathBuf>,
        /// Board configuration file to use for flashing
        #[arg(short, long)]
        config: Option<PathBuf>,
        /// Serial port to flash to (e.g., /dev/ttyUSB0, COM3)
        #[arg(short, long)]
        port: Option<String>,
        /// Force rebuild even if artifacts exist
        #[arg(long)]
        force_rebuild: bool,
    },
    /// Flash firmware to remote board(s) via ESPBrew server API
    RemoteFlash {
        /// Path to binary file to flash (if not specified, will look for built binary)
        #[arg(short, long)]
        binary: Option<PathBuf>,
        /// Board configuration file to use for flashing
        #[arg(short, long)]
        config: Option<PathBuf>,
        /// Path to ESP-IDF build directory (e.g., build.esp32_p4_function_ev_board)
        #[arg(long)]
        build_dir: Option<PathBuf>,
        /// Target board MAC address (if not specified, will list available boards)
        #[arg(short, long)]
        mac: Option<String>,
        /// Target board logical name (alternative to MAC address)
        #[arg(short, long)]
        name: Option<String>,
        /// ESPBrew server URL (default: http://localhost:8080)
        #[arg(short, long)]
        server: Option<String>,
    },
    /// Monitor remote board(s) via ESPBrew server API
    RemoteMonitor {
        /// Target board MAC address (if not specified, will list available boards)
        #[arg(short, long)]
        mac: Option<String>,
        /// Target board logical name (alternative to MAC address)
        #[arg(short, long)]
        name: Option<String>,
        /// ESPBrew server URL (default: http://localhost:8080)
        #[arg(short, long)]
        server: Option<String>,
        /// Baud rate for serial monitoring (default: 115200)
        #[arg(short, long, default_value = "115200")]
        baud_rate: u32,
        /// Reset the board after establishing monitoring connection to capture boot logs
        #[arg(
            short,
            long,
            help = "Reset board after starting monitoring to capture complete boot sequence"
        )]
        reset: bool,
    },
}

impl Cli {
    pub fn parse_args() -> Self {
        Self::parse()
    }
}
