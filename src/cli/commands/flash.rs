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
    force_rebuild: bool,
) -> Result<()> {
    log::info!("⚡ ESPBrew Local Flash Command");

    // Get project directory
    let project_dir = cli
        .project_dir
        .as_deref()
        .unwrap_or_else(|| std::path::Path::new("."));

    log::info!("📁 Project directory: {}", project_dir.display());

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
        log::info!("🔍 Detected project type: {:?}", handler.project_type());
        flash_with_project_handler(
            handler,
            project_dir,
            binary,
            config,
            port,
            force_rebuild,
            tx,
        )
        .await?
    } else {
        log::info!("🔍 No specific project type detected, trying ESP-IDF fallback...");
        flash_esp_idf_fallback(project_dir, binary, config, port, tx).await?
    }

    // Wait for progress handling to complete
    progress_handle.abort();

    log::info!("🎉 Flash operation completed!");
    Ok(())
}

async fn flash_with_project_handler(
    handler: &dyn ProjectHandler,
    project_dir: &std::path::Path,
    binary: Option<PathBuf>,
    config: Option<PathBuf>,
    port: Option<String>,
    force_rebuild: bool,
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

    // First check for existing artifacts before building
    let artifacts = if binary.is_some() {
        // If binary is specified, create a build artifact from it
        vec![BuildArtifact {
            name: "user-binary".to_string(),
            file_path: binary.unwrap(),
            artifact_type: crate::models::ArtifactType::Binary,
            offset: Some(0x10000), // Default app offset for ESP32
        }]
    } else {
        // Check if we should force rebuild or try to find existing artifacts
        if force_rebuild {
            log::info!("🔄 Force rebuild requested, building project...");
            handler
                .build_board(project_dir, &board_config, tx.clone())
                .await?
        } else {
            // Try to find existing build artifacts first
            let existing_artifacts =
                try_find_existing_artifacts(handler, project_dir, &board_config);

            match existing_artifacts {
                Ok(artifacts) if !artifacts.is_empty() => {
                    log::info!("🎯 Found existing build artifacts, skipping build:");
                    for artifact in &artifacts {
                        log::info!("  - {}: {}", artifact.name, artifact.file_path.display());
                    }
                    artifacts
                }
                _ => {
                    log::info!("🔧 No existing artifacts found, building project...");
                    handler
                        .build_board(project_dir, &board_config, tx.clone())
                        .await?
                }
            }
        }
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
    use crate::services::UnifiedFlashService;

    println!("🔄 Attempting ESP-IDF flash using unified service...");

    let flash_service = UnifiedFlashService::new();

    // Determine port to use
    let flash_port = if let Some(p) = port {
        p
    } else {
        crate::utils::espflash_utils::select_esp_port()?
    };

    println!("🔌 Using flash port: {}", flash_port);

    if let Some(binary_path) = binary {
        // Flash single binary
        let result = flash_service
            .flash_single_binary(
                &flash_port,
                &binary_path,
                0x10000, // Default app offset
                Some(tx.clone()),
                Some("fallback".to_string()),
            )
            .await?;

        if !result.success {
            return Err(anyhow::anyhow!("Flash failed: {}", result.message));
        }
    } else {
        // Flash ESP-IDF project
        let build_dir = config
            .as_ref()
            .and_then(|c| c.parent())
            .map(|p| p.join("build"));
        let result = flash_service
            .flash_esp_idf_project(
                project_dir,
                &flash_port,
                build_dir,
                Some(tx.clone()),
                Some("ESP-IDF".to_string()),
            )
            .await?;

        if !result.success {
            return Err(anyhow::anyhow!("Flash failed: {}", result.message));
        }
    }

    println!("✅ ESP-IDF flash completed successfully");
    Ok(())
}

/// Try to find existing build artifacts using handler-specific methods
fn try_find_existing_artifacts(
    handler: &dyn ProjectHandler,
    project_dir: &std::path::Path,
    board_config: &ProjectBoardConfig,
) -> Result<Vec<BuildArtifact>> {
    // Try to downcast to specific handler types that have find_build_artifacts methods
    if let Some(rust_handler) = handler
        .as_any()
        .downcast_ref::<crate::projects::handlers::rust_nostd::RustNoStdHandler>(
    ) {
        return rust_handler.find_build_artifacts(project_dir, board_config);
    }

    if let Some(esp_idf_handler) = handler
        .as_any()
        .downcast_ref::<crate::projects::handlers::esp_idf::EspIdfHandler>()
    {
        return esp_idf_handler.find_build_artifacts(project_dir, board_config);
    }

    // For other handlers, return empty artifacts (will trigger a build)
    Ok(Vec::new())
}
