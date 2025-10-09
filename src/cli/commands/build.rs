//! Build command implementation

use crate::cli::args::Cli;
use crate::projects::ProjectRegistry;
use anyhow::Result;

pub async fn execute_build_command(cli: &Cli) -> Result<()> {
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
    if let Some(handler) = project_registry.detect_project(&project_dir) {
        println!(
            "🔨 Building {} project in {}",
            handler.project_type().name(),
            project_dir.display()
        );

        // Discover boards/targets
        match handler.discover_boards(&project_dir) {
            Ok(boards) => {
                if boards.is_empty() {
                    println!("⚠️  No boards/targets found to build.");
                    return Ok(());
                }

                for board in boards {
                    println!(
                        "🔨 Would build board: {} (target: {})",
                        board.name,
                        board.target.as_deref().unwrap_or("auto-detect")
                    );
                }
                println!("✅ Build command completed (stub implementation)");
            }
            Err(e) => {
                println!("❌ Error discovering boards: {}", e);
            }
        }
    } else {
        println!("⚠️  Unknown project type. Cannot build.");
    }

    Ok(())
}
