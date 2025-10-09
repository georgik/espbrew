//! List command implementation

use crate::cli::args::Cli;
use crate::projects::ProjectRegistry;
use anyhow::Result;

pub async fn execute_list_command(cli: &Cli) -> Result<()> {
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

    Ok(())
}
