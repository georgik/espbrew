//! Build command implementation

use crate::cli::args::Cli;
use crate::models::AppEvent;
use crate::projects::ProjectRegistry;
use anyhow::Result;
use tokio::sync::mpsc;

pub async fn execute_build_command(cli: &Cli, board_filter: Option<&str>) -> Result<()> {
    let current_dir = std::env::current_dir()?;
    let project_dir = cli.project_dir.as_ref().unwrap_or(&current_dir);

    if !project_dir.exists() {
        return Err(anyhow::anyhow!(
            "Project directory does not exist: {:?}",
            project_dir
        ));
    }

    log::info!("🔨 ESPBrew Build Command");
    log::info!("📁 Project directory: {}", project_dir.display());

    // Detect project type using proper project detection
    let registry = ProjectRegistry::new();
    let handler = registry.detect_project_boxed(project_dir).ok_or_else(|| {
        anyhow::anyhow!(
            "Unable to detect project type in: {}",
            project_dir.display()
        )
    })?;

    log::info!(
        "🔍 Detected project type: {}",
        handler.project_type().name()
    );
    log::info!("📝 Description: {}", handler.project_type().description());

    // Check if required tools are available
    if let Err(error_msg) = handler.check_tools_available() {
        log::warn!("⚠️  Tool check failed: {}", error_msg);
        log::info!("\n{}", handler.get_missing_tools_message());
        return Err(anyhow::anyhow!(
            "Required tools not available: {}",
            error_msg
        ));
    }

    // Discover board configurations
    let all_board_configs = handler.discover_boards(project_dir)?;

    if all_board_configs.is_empty() {
        return Err(anyhow::anyhow!(
            "No board configurations found in project directory"
        ));
    }

    // Filter board configurations if specified
    let board_configs = if let Some(board_name) = board_filter {
        let available_boards: Vec<String> =
            all_board_configs.iter().map(|c| c.name.clone()).collect();
        let filtered: Vec<_> = all_board_configs
            .into_iter()
            .filter(|config| config.name == board_name)
            .collect();

        if filtered.is_empty() {
            return Err(anyhow::anyhow!(
                "Board configuration '{}' not found. Available boards: {}",
                board_name,
                available_boards.join(", ")
            ));
        }

        log::info!("🎯 Building specific board: {}", board_name);
        filtered
    } else {
        log::info!(
            "🎯 Found {} board configuration(s) (building all):",
            all_board_configs.len()
        );
        all_board_configs
    };

    for config in &board_configs {
        let target_info = config.target.as_deref().unwrap_or("unknown");
        log::info!("  - {} ({})", config.name, target_info);
    }

    // Create a channel for build events
    let (tx, mut rx) = mpsc::unbounded_channel::<AppEvent>();

    // Spawn a task to handle build events and log them
    let log_handler = tokio::spawn(async move {
        while let Some(event) = rx.recv().await {
            match event {
                AppEvent::BuildOutput(board_name, message) => {
                    log::info!("[{}] {}", board_name, message);
                }
                _ => {}
            }
        }
    });

    // Build all board configurations
    let mut build_results = Vec::new();
    let mut failed_builds = Vec::new();

    for board_config in &board_configs {
        log::info!("🔨 Building board configuration: {}", board_config.name);

        match handler
            .build_board(project_dir, board_config, tx.clone())
            .await
        {
            Ok(artifacts) => {
                log::info!(
                    "✅ Build successful for {}: {} artifacts generated",
                    board_config.name,
                    artifacts.len()
                );
                for artifact in &artifacts {
                    log::debug!(
                        "   📦 {}: {} ({:?})",
                        artifact.name,
                        artifact.file_path.display(),
                        artifact.artifact_type
                    );
                }
                build_results.push((board_config.name.clone(), artifacts));
            }
            Err(e) => {
                log::error!("❌ Build failed for {}: {}", board_config.name, e);
                failed_builds.push(board_config.name.clone());
            }
        }
    }

    // Close the channel and wait for log handler to finish
    drop(tx);
    log_handler.await?;

    // Report results
    if !failed_builds.is_empty() {
        log::error!(
            "❌ {} build(s) failed: {}",
            failed_builds.len(),
            failed_builds.join(", ")
        );
        return Err(anyhow::anyhow!("Some builds failed"));
    }

    log::info!(
        "🎉 All {} build(s) completed successfully!",
        build_results.len()
    );
    log::info!(
        "📦 Total artifacts generated: {}",
        build_results
            .iter()
            .map(|(_, artifacts)| artifacts.len())
            .sum::<usize>()
    );

    Ok(())
}
