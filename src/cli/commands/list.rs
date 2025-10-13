//! List command implementation

use crate::cli::args::Cli;
use crate::projects::ProjectRegistry;
use anyhow::Result;

pub async fn execute_list_command(cli: &Cli) -> Result<()> {
    let project_dir = cli
        .project_dir
        .clone()
        .unwrap_or_else(|| std::env::current_dir().expect("Failed to get current directory"));

    log::info!(
        "Executing list command for project: {}",
        project_dir.display()
    );

    if !project_dir.exists() {
        return Err(anyhow::anyhow!(
            "Project directory does not exist: {:?}",
            project_dir
        ));
    }

    // Detect project type
    let project_registry = ProjectRegistry::new();
    let project_handler = project_registry.detect_project(&project_dir);

    if let Some(handler) = project_handler {
        log::debug!(
            "Detected project type: {:?} in {}",
            handler.project_type(),
            project_dir.display()
        );
        println!(
            "🔍 Detected {} project in {}",
            handler.project_type().name(),
            project_dir.display()
        );

        // Show project description
        println!("📖 {}", handler.project_type().description());

        // Discover boards/targets
        log::debug!("Discovering boards for project: {}", project_dir.display());
        match handler.discover_boards(&project_dir) {
            Ok(boards) => {
                log::debug!("Found {} boards in project", boards.len());
                if boards.is_empty() {
                    println!("⚠️  No boards/targets found in this project.");
                } else {
                    println!("🎯 Found {} board(s)/target(s):", boards.len());
                    for board in &boards {
                        log::trace!("Board: {} (target: {:?})", board.name, board.target);
                        println!(
                            "  - {} ({})",
                            board.name,
                            board.target.as_deref().unwrap_or("auto-detect")
                        );
                    }
                }
            }
            Err(e) => {
                log::error!(
                    "Board discovery failed for {}: {}",
                    project_dir.display(),
                    e
                );
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

    Ok(())
}
