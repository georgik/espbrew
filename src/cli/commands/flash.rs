use crate::cli::args::Cli;
use crate::models::{AppEvent, BuildArtifact, ProjectBoardConfig};
use crate::projects::ProjectRegistry;
use crate::projects::registry::ProjectHandler;
use anyhow::Result;
use std::path::PathBuf;
use tokio::sync::mpsc;

pub async fn execute_flash_command(
    cli: &Cli,
    binary: Option<PathBuf>,
    config: Option<PathBuf>,
    port: Option<String>,
) -> Result<()> {
    println!("⚡ ESPBrew Local Flash Command");

    // Get project directory
    let project_dir = cli
        .project_dir
        .as_ref()
        .map(|p| p.as_path())
        .unwrap_or_else(|| std::path::Path::new("."));

    println!("📁 Project directory: {}", project_dir.display());

    // Create event channel for progress tracking
    let (tx, mut rx) = mpsc::unbounded_channel::<AppEvent>();

    // Spawn a task to handle progress events
    let progress_handle = tokio::spawn(async move {
        while let Some(event) = rx.recv().await {
            match event {
                AppEvent::BuildOutput(board_name, message) => {
                    println!("[{}] {}", board_name, message);
                }
                AppEvent::ActionFinished(board_name, action, success) => {
                    if success {
                        println!("✅ Flash completed successfully for {}", board_name);
                    } else {
                        println!("❌ Flash failed for {}: {}", board_name, action);
                    }
                }
                _ => {} // Ignore other event types
            }
        }
    });

    // Try to detect project type and get appropriate handler
    let registry = ProjectRegistry::new();
    let project_handler = registry.detect_project(project_dir);

    if let Some(handler) = project_handler {
        println!("🔍 Detected project type: {:?}", handler.project_type());
        flash_with_project_handler(handler, project_dir, binary, config, port, tx).await?
    } else {
        println!("🔍 No specific project type detected, trying ESP-IDF fallback...");
        flash_esp_idf_fallback(project_dir, binary, config, port, tx).await?
    }

    // Wait for progress handling to complete
    progress_handle.abort();

    println!("🎉 Flash operation completed!");
    Ok(())
}

async fn flash_with_project_handler(
    handler: &dyn ProjectHandler,
    project_dir: &std::path::Path,
    binary: Option<PathBuf>,
    config: Option<PathBuf>,
    port: Option<String>,
    tx: mpsc::UnboundedSender<AppEvent>,
) -> Result<()> {
    // First, try to discover boards from the project
    let discovered_boards = handler.discover_boards(project_dir)?;

    let board_config = if let Some(config_path) = config {
        println!(
            "📋 Loading board configuration from: {}",
            config_path.display()
        );
        // Try to find a board config that matches the given config file
        discovered_boards
            .into_iter()
            .find(|board| board.config_file == config_path)
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "No board configuration found for config file: {}",
                    config_path.display()
                )
            })?
    } else if !discovered_boards.is_empty() {
        println!(
            "📋 Using first discovered board configuration: {}",
            discovered_boards[0].name
        );
        discovered_boards[0].clone()
    } else {
        // Create a minimal default config for projects without board discovery
        println!("📋 Creating default board configuration");
        ProjectBoardConfig {
            name: "default".to_string(),
            config_file: project_dir.join("sdkconfig.defaults"),
            build_dir: project_dir.join("build"),
            target: None,
            project_type: handler.project_type(),
        }
    };

    println!("🔨 Starting flash process for board: {}", board_config.name);

    // First build the project to get artifacts
    let artifacts = if binary.is_some() {
        // If binary is specified, create a build artifact from it
        vec![BuildArtifact {
            name: "user-binary".to_string(),
            file_path: binary.unwrap(),
            artifact_type: crate::models::ArtifactType::Binary,
            offset: Some(0x10000), // Default app offset for ESP32
        }]
    } else {
        println!("🔧 Building project to generate artifacts...");
        handler
            .build_board(project_dir, &board_config, tx.clone())
            .await?
    };

    // Convert port to Option<&str> for flash_board call
    let port_ref = port.as_deref();

    // Call the project handler's flash method
    handler
        .flash_board(project_dir, &board_config, &artifacts, port_ref, tx)
        .await
        .map_err(|e| anyhow::anyhow!("Flash failed: {}", e))
}

async fn flash_esp_idf_fallback(
    project_dir: &std::path::Path,
    binary: Option<PathBuf>,
    config: Option<PathBuf>,
    port: Option<String>,
    tx: mpsc::UnboundedSender<AppEvent>,
) -> Result<()> {
    use crate::projects::handlers::esp_idf::EspIdfHandler;

    println!("🔄 Attempting ESP-IDF flash...");

    // Create ESP-IDF handler as fallback
    let esp_idf_handler = EspIdfHandler;

    flash_with_project_handler(&esp_idf_handler, project_dir, binary, config, port, tx).await
}
