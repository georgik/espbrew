//! TUI event loop and handling

use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::{io, time::Duration};
use tokio::sync::mpsc;

use crate::cli::tui::main_app::App;
use crate::cli::tui::ui::ui;
use crate::models::board::BoardAction;
use crate::models::project::BuildStatus;
use crate::models::{AppEvent, FocusedPane};

/// Run the main TUI event loop
pub async fn run_tui_event_loop(mut app: App) -> Result<()> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create event channel
    let (tx, mut rx) = mpsc::unbounded_channel();

    // Spawn tick generator
    let tx_tick = tx.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_millis(250));
        loop {
            interval.tick().await;
            let _ = tx_tick.send(AppEvent::Tick);
        }
    });

    // Start server discovery
    app.start_server_discovery(tx.clone());

    // Main loop
    let result = loop {
        terminal.draw(|f| ui(f, &app))?;

        // Handle events
        tokio::select! {
            // Handle crossterm events
            _ = tokio::task::spawn_blocking(|| event::poll(Duration::from_millis(50))) => {
                if event::poll(Duration::from_millis(0))? {
                    match event::read()? {
                        Event::Key(key) => {
                            if key.kind == KeyEventKind::Press {
                                // Handle tool warning modal first
                                if app.show_tool_warning && !app.tool_warning_acknowledged {
                                    match key.code {
                                        KeyCode::Enter => {
                                            app.acknowledge_tool_warning();
                                        }
                                        KeyCode::Char('q') | KeyCode::Esc => break Ok(()),
                                        _ => {}
                                    }
                                    continue;
                                }

                                // Handle action menus
                                if app.show_action_menu {
                                    match key.code {
                                        KeyCode::Up | KeyCode::Char('k') => {
                                            if app.action_menu_selected > 0 {
                                                app.action_menu_selected -= 1;
                                            } else {
                                                app.action_menu_selected = app.available_actions.len().saturating_sub(1);
                                            }
                                        }
                                        KeyCode::Down | KeyCode::Char('j') => {
                                            app.action_menu_selected = (app.action_menu_selected + 1) % app.available_actions.len();
                                        }
                                        KeyCode::Enter => {
                                            if app.action_menu_selected < app.available_actions.len() {
                                                let action = app.available_actions[app.action_menu_selected].clone();
                                                app.show_action_menu = false;

                                                // Extract data needed for action execution
                                                if let Some(board) = app.boards.get(app.selected_board) {
                                                    let board_name = board.name.clone();
                                                    let config_file = board.config_file.clone();
                                                    let build_dir = board.build_dir.clone();
                                                    let project_dir = app.project_dir.clone();
                                                    let logs_dir = app.logs_dir.clone();
                    let _project_type = app.project_handler.as_ref().map(|h| h.project_type());

                                                    let tx_action = tx.clone();

                                                    // Use the centralized execute_action method that handles all actions including RemoteFlash
                                                    if let Err(e) = app.execute_action(action, tx_action).await {
                                                        eprintln!("Action execution failed: {}", e);
                                                    }
                                                }
                                            }
                                        }
                                        KeyCode::Esc => {
                                            app.show_action_menu = false;
                                        }
                                        _ => {}
                                    }
                                    continue;
                                }

                                if app.show_component_action_menu {
                                    match key.code {
                                        KeyCode::Up | KeyCode::Char('k') => {
                                            if app.component_action_menu_selected > 0 {
                                                app.component_action_menu_selected -= 1;
                                            } else {
                                                app.component_action_menu_selected = app.available_component_actions.len().saturating_sub(1);
                                            }
                                        }
                                        KeyCode::Down | KeyCode::Char('j') => {
                                            app.component_action_menu_selected = (app.component_action_menu_selected + 1) % app.available_component_actions.len();
                                        }
                                        KeyCode::Enter => {
                                            // TODO: Execute selected component action
                                            app.show_component_action_menu = false;
                                        }
                                        KeyCode::Esc => {
                                            app.show_component_action_menu = false;
                                        }
                                        _ => {}
                                    }
                                    continue;
                                }

                                // Handle remote board dialog
                                if app.show_remote_board_dialog {
                                    match key.code {
                                        KeyCode::Up | KeyCode::Char('k') => {
                                            app.previous_remote_board();
                                        }
                                        KeyCode::Down | KeyCode::Char('j') => {
                                            app.next_remote_board();
                                        }
                                        KeyCode::Enter => {
                                            if !app.remote_boards.is_empty() {
                                                // Execute remote flash for selected board
                                                let tx_remote = tx.clone();
                                                let result = app.execute_remote_flash(tx_remote).await;
                                                if let Err(e) = result {
                                                    eprintln!("Remote flash failed: {}", e);
                                                }
                                                app.hide_remote_board_dialog();
                                            }
                                        }
                                        KeyCode::Esc => {
                                            app.hide_remote_board_dialog();
                                        }
                                        _ => {}
                                    }
                                    continue;
                                }

                                match key.code {
                                    KeyCode::Char('q') => break Ok(()),
                                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                                        break Ok(());
                                    }
                                    KeyCode::Tab => {
                                        app.toggle_focused_pane();
                                    }
                                    KeyCode::Char('h') | KeyCode::Char('?') => {
                                        app.show_help = !app.show_help;
                                    }
                                    KeyCode::Up | KeyCode::Char('k') => {
                                        match app.focused_pane {
                                            FocusedPane::BoardList => {
                                                let old_board = app.selected_board;
                                                app.previous_board();
                                                if old_board != app.selected_board {
                                                    app.reset_log_scroll();
                                                }
                                            }
                                            FocusedPane::ComponentList => {
                                                app.previous_component();
                                            }
                                            FocusedPane::LogPane => {
                                                app.scroll_log_up();
                                            }
                                        }
                                    }
                                    KeyCode::Down | KeyCode::Char('j') => {
                                        match app.focused_pane {
                                            FocusedPane::BoardList => {
                                                let old_board = app.selected_board;
                                                app.next_board();
                                                if old_board != app.selected_board {
                                                    app.reset_log_scroll();
                                                }
                                            }
                                            FocusedPane::ComponentList => {
                                                app.next_component();
                                            }
                                            FocusedPane::LogPane => {
                                                app.scroll_log_down();
                                            }
                                        }
                                    }
                                    // Page scrolling for logs
                                    KeyCode::PageUp => {
                                        if app.focused_pane == FocusedPane::LogPane {
                                            for _ in 0..10 {
                                                app.scroll_log_up();
                                            }
                                        }
                                    }
                                    KeyCode::PageDown => {
                                        if app.focused_pane == FocusedPane::LogPane {
                                            for _ in 0..10 {
                                                app.scroll_log_down();
                                            }
                                        }
                                    }
                                    KeyCode::Home => {
                                        if app.focused_pane == FocusedPane::LogPane {
                                            app.log_scroll_offset = 0;
                                        }
                                    }
                                    KeyCode::End => {
                                        if app.focused_pane == FocusedPane::LogPane {
                                            if let Some(board) = app.boards.get(app.selected_board) {
                                                app.log_scroll_offset = board.log_lines.len().saturating_sub(1);
                                            }
                                        }
                                    }
                                    // Build actions
                                    KeyCode::Char(' ') | KeyCode::Char('b') => {
                                        if !app.build_in_progress {
                                            // TODO: Trigger single board build
                                            println!("\r\n🔨 Building selected board (not implemented yet)\r\n");
                                        }
                                    }
                                    KeyCode::Char('x') => {
                                        if !app.build_in_progress {
                                            // TODO: Trigger build all boards
                                            println!("\r\n🔨 Building all boards (not implemented yet)\r\n");
                                        }
                                    }
                                    // Action menus
                                    KeyCode::Enter => {
                                        match app.focused_pane {
                                            FocusedPane::BoardList => {
                                                if app.show_action_menu {
                                                    // TODO: Execute selected board action
                                                    app.show_action_menu = false;
                                                } else {
                                                    app.show_action_menu = true;
                                                    app.action_menu_selected = 0;
                                                }
                                            }
                                            FocusedPane::ComponentList => {
                                                if app.show_component_action_menu {
                                                    // TODO: Execute selected component action
                                                    app.show_component_action_menu = false;
                                                } else {
                                                    app.show_component_action_menu = true;
                                                    app.component_action_menu_selected = 0;
                                                }
                                            }
                                            _ => {}
                                        }
                                    }
                                    KeyCode::Esc => {
                                        if app.show_action_menu {
                                            app.show_action_menu = false;
                                        } else if app.show_component_action_menu {
                                            app.show_component_action_menu = false;
                                        } else if app.show_help {
                                            app.show_help = false;
                                        } else {
                                            break Ok(());
                                        }
                                    }
                                    // Refresh
                                    KeyCode::Char('r') => {
                                        if !app.build_in_progress {
                                            // TODO: Refresh board list
                                            println!("\r\n🔄 Refreshing board list (not implemented yet)\r\n");
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        }
                        Event::Mouse(_mouse) => {
                            // Mouse events are not captured
                        }
                        _ => {}
                    }
                }
            }

            // Handle app events
            Some(event) = rx.recv() => {
                match event {
                    AppEvent::BuildOutput(board_name, line) => {
                        app.add_log_line(&board_name, line);
                    }
                    AppEvent::BuildFinished(board_name, success) => {
                        let status = if success {
                            BuildStatus::Success
                        } else {
                            BuildStatus::Failed
                        };
                        app.update_board_status(&board_name, status);
                    }
                    AppEvent::ActionFinished(board_name, action_name, success) => {
                        let status = if success {
                            BuildStatus::Success
                        } else {
                            BuildStatus::Failed
                        };
                        app.update_board_status(&board_name, status);

                        // Add completion message to logs
                        let completion_msg = if success {
                            format!("✅ {} completed successfully!", action_name)
                        } else {
                            format!("❌ {} failed!", action_name)
                        };
                        app.add_log_line(&board_name, completion_msg);
                    }
                    AppEvent::RemoteBoardsFetched(remote_boards) => {
                        app.handle_remote_boards_fetched(remote_boards);
                    }
                    AppEvent::RemoteBoardsFetchFailed(error) => {
                        app.handle_remote_boards_fetch_failed(error);
                    }
                    AppEvent::ServerDiscoveryCompleted(servers) => {
                        app.handle_server_discovery_completed(servers);
                    }
                    AppEvent::ServerDiscoveryFailed(error) => {
                        app.handle_server_discovery_failed(error);
                    }
                    AppEvent::RemoteFlashCompleted => {
                        app.handle_remote_flash_completed();
                    }
                    AppEvent::RemoteFlashFailed(error) => {
                        app.handle_remote_flash_failed(error);
                    }
                    AppEvent::Tick => {
                        // Regular tick for UI updates
                    }
                    _ => {}
                }
            }
        }
    };

    // Cleanup
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}
