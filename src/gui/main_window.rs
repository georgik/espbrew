//! Main GUI window implementation using Slint
//!
//! This module provides the main application window for GUI mode,
//! integrating with the existing TUI App state and business logic.

use anyhow::Result;
use chrono;
use slint::{ComponentHandle, Model, ModelRc, VecModel};
use std::{rc::Rc, sync::Arc};
use tokio::sync::{Mutex, mpsc};

use crate::cli::tui::main_app::App;
use crate::models::AppEvent;
use crate::models::board::BoardAction;
use crate::models::project::BuildStatus;

// Include the generated Slint code
slint::include_modules!();

/// Convert TUI BuildStatus to GUI status color
fn status_to_color(status: &BuildStatus) -> slint::Color {
    match status {
        BuildStatus::Pending => slint::Color::from_rgb_u8(128, 128, 128), // Gray
        BuildStatus::Building => slint::Color::from_rgb_u8(0, 123, 255),  // Blue
        BuildStatus::Success => slint::Color::from_rgb_u8(40, 167, 69),   // Green
        BuildStatus::Failed => slint::Color::from_rgb_u8(220, 53, 69),    // Red
        BuildStatus::Flashing => slint::Color::from_rgb_u8(0, 188, 212),  // Cyan
        BuildStatus::Flashed => slint::Color::from_rgb_u8(63, 81, 181),   // Indigo
        BuildStatus::Monitoring => slint::Color::from_rgb_u8(156, 39, 176), // Purple
    }
}

/// Convert TUI BuildStatus to status string
fn status_to_string(status: &BuildStatus) -> String {
    match status {
        BuildStatus::Pending => "Pending".to_string(),
        BuildStatus::Building => "Building".to_string(),
        BuildStatus::Success => "Success".to_string(),
        BuildStatus::Failed => "Failed".to_string(),
        BuildStatus::Flashing => "Flashing".to_string(),
        BuildStatus::Flashed => "Flashed".to_string(),
        BuildStatus::Monitoring => "Monitoring".to_string(),
    }
}

/// Run the main GUI window with proper async event handling
pub async fn run_main_window(app: App) -> Result<()> {
    // Create the main window
    let main_window = MainWindow::new()?;

    // Set up initial window properties
    setup_initial_state(&main_window, &app)?;

    // Create event channel for async communication
    let (tx, mut rx) = mpsc::unbounded_channel::<AppEvent>();

    // Set up event handlers with app and event sender
    setup_event_handlers(&main_window, app, tx)?;

    // Show the window
    main_window.show()?;

    // Start the event processing task
    let main_window_weak = main_window.as_weak();
    tokio::spawn(async move {
        while let Some(event) = rx.recv().await {
            if let Some(window) = main_window_weak.upgrade() {
                handle_app_event(&window, event);
            }
        }
    });

    // Run the GUI event loop
    slint::run_event_loop()?;

    Ok(())
}

/// Set up initial window state from TUI App
fn setup_initial_state(main_window: &MainWindow, app: &App) -> Result<()> {
    // Convert boards to GUI model
    let boards_model = Rc::new(VecModel::default());
    for board in &app.boards {
        let build_time = if let Some(duration) = board.build_time {
            format!("({}s)", duration.as_secs())
        } else {
            String::new()
        };

        let target = board
            .target
            .clone()
            .unwrap_or_else(|| "auto-detect".to_string());

        boards_model.push(BoardItem {
            name: board.name.clone().into(),
            status: status_to_string(&board.status).into(),
            status_color: status_to_color(&board.status),
            build_time: build_time.into(),
            target: target.into(),
        });
    }
    main_window.set_boards(ModelRc::new(boards_model));

    // Convert components to GUI model
    let components_model = Rc::new(VecModel::default());
    for component in &app.components {
        components_model.push(ComponentItem {
            name: component.name.clone().into(),
            r#type: if component.is_managed {
                "managed"
            } else {
                "local"
            }
            .into(),
            is_managed: component.is_managed,
            status: component.action_status.clone().unwrap_or_default().into(),
        });
    }
    main_window.set_components(ModelRc::new(components_model));

    // Set project info
    let project_info = if let Some(project_type) = &app.project_type {
        format!(
            "🍺 {} project in {}",
            project_type.name(),
            app.project_dir.display()
        )
    } else {
        format!("🍺 Unknown project in {}", app.project_dir.display())
    };
    main_window.set_project_info(project_info.into());

    // Set server status
    let server_status = if app.server_url.is_some() && !app.discovered_servers.is_empty() {
        "Connected"
    } else {
        "Disconnected"
    };
    main_window.set_server_status(server_status.into());

    // Initialize empty logs (will be populated by log updates)
    let logs_model = Rc::new(VecModel::<LogEntry>::default());
    main_window.set_logs(ModelRc::new(logs_model));

    // Set build status
    main_window.set_build_in_progress(app.build_in_progress);

    Ok(())
}

/// Set up event handlers for the main window
fn setup_event_handlers(
    main_window: &MainWindow,
    app: App,
    tx: mpsc::UnboundedSender<AppEvent>,
) -> Result<()> {
    // Board selection handler
    let main_window_weak = main_window.as_weak();
    main_window.on_board_selected(move |index| {
        if main_window_weak.upgrade().is_some() {
            println!("Board selected: {}", index);
            // Update selected board index (already handled by property binding)
        }
    });

    // Component selection handler
    let main_window_weak = main_window.as_weak();
    main_window.on_component_selected(move |index| {
        if main_window_weak.upgrade().is_some() {
            println!("Component selected: {}", index);
            // Update selected component index (already handled by property binding)
        }
    });

    // Create shared app instance that can be used by multiple handlers
    let app_shared = Arc::new(Mutex::new(app));

    // Build board handler
    let main_window_weak = main_window.as_weak();
    let app_clone = app_shared.clone();
    let tx_clone = tx.clone();

    main_window.on_build_board(move |index| {
        if main_window_weak.upgrade().is_some() {
            let app_arc = app_clone.clone();
            let tx = tx_clone.clone();

            // Execute build in background task
            tokio::spawn(async move {
                let mut app_ref = app_arc.lock().await;
                // Set selected board
                if (index as usize) < app_ref.boards.len() {
                    app_ref.selected_board = index as usize;
                    app_ref.list_state.select(Some(index as usize));

                    // Execute build action
                    if let Err(e) = app_ref.execute_action(BoardAction::Build, tx.clone()).await {
                        let _ = tx.send(AppEvent::Error(format!("Build failed: {}", e)));
                    }
                } else {
                    let _ = tx.send(AppEvent::Error("Invalid board index".to_string()));
                }
            });
        }
    });

    // Build all handler
    let main_window_weak = main_window.as_weak();
    let app_clone2 = app_shared.clone();
    let tx_clone2 = tx.clone();

    main_window.on_build_all(move || {
        if main_window_weak.upgrade().is_some() {
            let app_arc = app_clone2.clone();
            let tx = tx_clone2.clone();

            // Execute build all in background task
            tokio::spawn(async move {
                let mut app_ref = app_arc.lock().await;
                // Build each board sequentially
                let board_count = app_ref.boards.len();
                for i in 0..board_count {
                    app_ref.selected_board = i;
                    app_ref.list_state.select(Some(i));

                    if let Err(e) = app_ref.execute_action(BoardAction::Build, tx.clone()).await {
                        let _ = tx.send(AppEvent::Error(format!(
                            "Build failed for board {}: {}",
                            app_ref
                                .boards
                                .get(i)
                                .map(|b| &b.name)
                                .unwrap_or(&"unknown".to_string()),
                            e
                        )));
                        break; // Stop on first failure
                    }
                }
            });
        }
    });

    // Flash board handler
    let main_window_weak = main_window.as_weak();
    main_window.on_flash_board(move |index| {
        if main_window_weak.upgrade().is_some() {
            println!("Flash board requested for index: {}", index);
            // TODO: Implement flash logic
            // This will be implemented in Phase 2 with proper async handling
        }
    });

    // Clean board handler
    let main_window_weak = main_window.as_weak();
    main_window.on_clean_board(move |index| {
        if main_window_weak.upgrade().is_some() {
            println!("Clean board requested for index: {}", index);
            // TODO: Implement clean logic
            // This will be implemented in Phase 2 with proper async handling
        }
    });

    // Discover servers handler
    let main_window_weak = main_window.as_weak();
    main_window.on_discover_servers(move || {
        if main_window_weak.upgrade().is_some() {
            println!("Server discovery requested");
            // TODO: Implement server discovery logic
            // This will be implemented in Phase 2 with proper async handling
        }
    });

    // Refresh project handler
    let main_window_weak = main_window.as_weak();
    main_window.on_refresh_project(move || {
        if main_window_weak.upgrade().is_some() {
            println!("Project refresh requested");
            // TODO: Implement project refresh logic
            // This will be implemented in Phase 2 with proper async handling
        }
    });

    // App is now properly shared via Arc<Mutex>, so handlers can use it
    // The Arc will keep the App alive as long as handlers exist

    Ok(())
}

/// Handle application events and update the GUI
fn handle_app_event(main_window: &MainWindow, event: AppEvent) {
    match event {
        AppEvent::BuildOutput(board_name, log_line) => {
            // Add log line to the logs model
            let logs_model = main_window.get_logs();
            let vec_model = logs_model
                .as_any()
                .downcast_ref::<VecModel<LogEntry>>()
                .unwrap();

            // Create LogEntry with current timestamp
            let timestamp = chrono::Local::now().format("%H:%M:%S").to_string();
            let log_entry = LogEntry {
                timestamp: timestamp.into(),
                level: "INFO".into(),
                message: log_line.into(),
                board_name: board_name.into(),
            };

            vec_model.push(log_entry);
        }
        AppEvent::BuildFinished(board_name, success) => {
            // Update board status in the model
            let boards_model = main_window.get_boards();
            let vec_model = boards_model
                .as_any()
                .downcast_ref::<VecModel<BoardItem>>()
                .unwrap();

            for i in 0..vec_model.row_count() {
                if let Some(mut board_item) = vec_model.row_data(i) {
                    if board_item.name.as_str() == board_name {
                        // Update status and color
                        let new_status = if success {
                            BuildStatus::Success
                        } else {
                            BuildStatus::Failed
                        };
                        board_item.status = status_to_string(&new_status).into();
                        board_item.status_color = status_to_color(&new_status);
                        vec_model.set_row_data(i, board_item);
                        break;
                    }
                }
            }

            // Set build in progress to false
            main_window.set_build_in_progress(false);
        }
        AppEvent::ActionFinished(board_name, action_name, success) => {
            // Add completion message to logs
            let logs_model = main_window.get_logs();
            let vec_model = logs_model
                .as_any()
                .downcast_ref::<VecModel<LogEntry>>()
                .unwrap();

            let timestamp = chrono::Local::now().format("%H:%M:%S").to_string();
            let level = if success { "INFO" } else { "ERROR" };
            let status_icon = if success { "✅" } else { "❌" };
            let message = format!(
                "{} {} {}",
                status_icon,
                action_name,
                if success { "completed" } else { "failed" }
            );

            let log_entry = LogEntry {
                timestamp: timestamp.into(),
                level: level.into(),
                message: message.into(),
                board_name: board_name.into(),
            };

            vec_model.push(log_entry);

            // Update build in progress status
            main_window.set_build_in_progress(false);
        }
        AppEvent::Error(message) => {
            // Add error to logs
            let logs_model = main_window.get_logs();
            let vec_model = logs_model
                .as_any()
                .downcast_ref::<VecModel<LogEntry>>()
                .unwrap();

            let timestamp = chrono::Local::now().format("%H:%M:%S").to_string();
            let log_entry = LogEntry {
                timestamp: timestamp.into(),
                level: "ERROR".into(),
                message: format!("❌ {}", message).into(),
                board_name: "system".into(),
            };
            vec_model.push(log_entry);
        }
        AppEvent::Warning(message) => {
            // Add warning to logs
            let logs_model = main_window.get_logs();
            let vec_model = logs_model
                .as_any()
                .downcast_ref::<VecModel<LogEntry>>()
                .unwrap();

            let timestamp = chrono::Local::now().format("%H:%M:%S").to_string();
            let log_entry = LogEntry {
                timestamp: timestamp.into(),
                level: "WARN".into(),
                message: format!("⚠️ {}", message).into(),
                board_name: "system".into(),
            };
            vec_model.push(log_entry);
        }
        AppEvent::Info(message) => {
            // Add info to logs
            let logs_model = main_window.get_logs();
            let vec_model = logs_model
                .as_any()
                .downcast_ref::<VecModel<LogEntry>>()
                .unwrap();

            let timestamp = chrono::Local::now().format("%H:%M:%S").to_string();
            let log_entry = LogEntry {
                timestamp: timestamp.into(),
                level: "INFO".into(),
                message: format!("ℹ️ {}", message).into(),
                board_name: "system".into(),
            };
            vec_model.push(log_entry);
        }
        _ => {
            // Handle other events as needed
            // For now, we'll ignore unhandled events
        }
    }
}
