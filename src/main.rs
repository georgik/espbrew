//! ESPBrew - Multi-Platform ESP32 Build Manager
//!
//! Main binary entry point with TUI support.

use anyhow::Result;
use clap::Parser;

use espbrew::cli::args::{Cli, Commands};
use espbrew::cli::commands::discover::execute_discover_command;
use espbrew::cli::commands::flash::execute_flash_command;
use espbrew::cli::commands::remote_flash::execute_remote_flash_command;
use espbrew::cli::tui::event_loop::run_tui_event_loop;
use espbrew::cli::tui::main_app::App;
use espbrew::projects::ProjectRegistry;

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    let project_dir = cli
        .project_dir
        .clone()
        .unwrap_or_else(|| std::env::current_dir().expect("Failed to get current directory"));

    if !project_dir.exists() {
        return Err(anyhow::anyhow!(
            "Project directory does not exist: {:?}",
            project_dir
        ));
    }

    // Detect project type
    let project_registry = ProjectRegistry::new();
    let project_handler = project_registry.detect_project(&project_dir);
    let boxed_project_handler = project_registry.detect_project_boxed(&project_dir);

    if let Some(ref handler) = project_handler {
        println!(
            "🔍 Detected {} project in {}",
            handler.project_type().name(),
            project_dir.display()
        );

        // Show project description
        println!("📖 {}", handler.project_type().description());

        // Discover boards/targets
        match handler.discover_boards(&project_dir) {
            Ok(boards) => {
                if boards.is_empty() {
                    println!("⚠️  No boards/targets found in this project.");
                } else {
                    println!("🎯 Found {} board(s)/target(s):", boards.len());
                    for board in &boards {
                        println!(
                            "  - {} ({})",
                            board.name,
                            board.target.as_deref().unwrap_or("auto-detect")
                        );
                    }
                }
            }
            Err(e) => {
                eprintln!("❌ Error discovering boards: {}", e);
            }
        }
        println!();
    } else {
        println!(
            "⚠️  Unknown project type in {}. Falling back to ESP-IDF mode.",
            project_dir.display()
        );
        println!("   Supported project types: ESP-IDF, Rust no_std, Arduino");
        println!();
    }

    let mut app = App::new(
        project_dir,
        cli.build_strategy.clone(),
        cli.server_url.clone(),
        cli.board_mac.clone(),
        boxed_project_handler,
    )?;

    // Generate support scripts
    println!("🍺 Generating build and flash scripts...");
    app.generate_support_scripts()?;
    println!("✅ Scripts generated in ./support/");
    println!("📦 Professional multi-board build: ./support/build-all-idf-build-apps.sh");

    if cli.cli_only || cli.command.is_some() {
        return run_cli_only(app, cli.command).await;
    }

    println!();
    println!("🍺 Starting ESPBrew TUI...");
    println!(
        "Found {} boards and {} components.",
        app.boards.len(),
        app.components.len()
    );
    println!("Press 'b' to build all boards, Tab to switch between panes.");
    println!("Press 'h' for help, 'q' to quit.");
    println!();

    // Run the full TUI event loop
    run_tui_event_loop(app).await?;

    Ok(())
}

// CLI-only mode with actual command implementations
async fn run_cli_only(app: App, command: Option<Commands>) -> Result<()> {
    let cli = Cli {
        project_dir: Some(app.project_dir.clone()),
        cli_only: true,
        build_strategy: app.build_strategy.clone(),
        server_url: app.server_url.clone(),
        board_mac: app.board_mac.clone(),
        command: command.clone(),
    };

    match command {
        Some(Commands::List) => {
            println!("📋 CLI List mode not yet implemented");
        }
        Some(Commands::Build) => {
            println!("🔨 CLI Build mode not yet implemented");
        }
        Some(Commands::Discover { timeout }) => {
            execute_discover_command(timeout).await?;
        }
        Some(Commands::Flash {
            binary,
            config,
            port,
        }) => {
            execute_flash_command(&cli, binary, config, port).await?;
        }
        Some(Commands::RemoteFlash {
            binary,
            config,
            build_dir,
            mac,
            name,
            server,
        }) => {
            execute_remote_flash_command(&cli, binary, config, build_dir, mac, name, server)
                .await?;
        }
        Some(Commands::RemoteMonitor { .. }) => {
            println!("📺 CLI Remote Monitor mode not yet implemented");
        }
        None => {
            println!("📋 Listing boards and components (default CLI behavior)");
        }
    }
    Ok(())
}
