//! TUI rendering logic

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap},
};

use crate::cli::tui::main_app::App;
use crate::models::FocusedPane;

/// Main UI rendering function
pub fn ui(f: &mut Frame, app: &App) {
    // Main layout with help bar at bottom
    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(3)])
        .split(f.area());

    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(32), Constraint::Percentage(68)])
        .split(main_chunks[0]);

    // Split left panel into boards (top) and components (bottom)
    let left_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
        .split(chunks[0]);

    // Board list (top of left panel)
    let board_items: Vec<ListItem> = app
        .boards
        .iter()
        .map(|board| {
            let status_symbol = board.status.symbol();
            let time_info = if let Some(duration) = board.build_time {
                format!(" ({}s)", duration.as_secs())
            } else {
                String::new()
            };

            ListItem::new(Line::from(vec![
                Span::styled(status_symbol, Style::default().fg(board.status.color())),
                Span::raw(" "),
                Span::raw(&board.name),
                Span::styled(time_info, Style::default().fg(Color::Gray)),
            ]))
        })
        .collect();

    let project_type_display = if let Some(project_type) = &app.project_type {
        format!(" ({})", project_type.name())
    } else {
        String::new()
    };

    let server_indicator = if app.server_discovery_in_progress {
        " 🔄"
    } else if !app.discovered_servers.is_empty() {
        " ✅"
    } else {
        " ❌"
    };

    let board_list_title = if app.focused_pane == FocusedPane::BoardList {
        format!(
            "🍺 Boards{}{} [FOCUSED]",
            project_type_display, server_indicator
        )
    } else {
        format!("🍺 Boards{}{}", project_type_display, server_indicator)
    };

    let board_list_block = if app.focused_pane == FocusedPane::BoardList {
        Block::default()
            .title(board_list_title.clone())
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan))
    } else {
        Block::default()
            .title(board_list_title.clone())
            .borders(Borders::ALL)
    };

    let board_list = List::new(board_items)
        .block(board_list_block)
        .highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        );

    f.render_stateful_widget(board_list, left_chunks[0], &mut app.list_state.clone());

    // Component list (bottom of left panel)
    let component_items: Vec<ListItem> = app
        .components
        .iter()
        .map(|component| {
            let type_indicator = if component.is_managed {
                "📦" // Package icon for managed components
            } else {
                "🔧" // Tool icon for regular components
            };

            let mut spans = vec![
                Span::styled(type_indicator, Style::default().fg(Color::White)),
                Span::raw(" "),
                Span::raw(&component.name),
            ];

            // Add action status if present
            if let Some(action_status) = &component.action_status {
                spans.push(Span::styled(
                    format!(" [{}]", action_status),
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::ITALIC),
                ));
            } else {
                // Add component type status
                if component.is_managed {
                    spans.push(Span::styled(
                        " (managed)",
                        Style::default().fg(Color::Yellow),
                    ));
                } else {
                    spans.push(Span::styled(" (local)", Style::default().fg(Color::Green)));
                }
            }

            ListItem::new(Line::from(spans))
        })
        .collect();

    let component_list_title = if app.focused_pane == FocusedPane::ComponentList {
        "🧩 Components [FOCUSED]"
    } else {
        "🧩 Components"
    };

    let component_list_block = if app.focused_pane == FocusedPane::ComponentList {
        Block::default()
            .title(component_list_title)
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan))
    } else {
        Block::default()
            .title(component_list_title)
            .borders(Borders::ALL)
    };

    let component_list = List::new(component_items)
        .block(component_list_block)
        .highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        );

    f.render_stateful_widget(
        component_list,
        left_chunks[1],
        &mut app.component_list_state.clone(),
    );

    // Right panel - Details
    let right_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(8), Constraint::Min(0)])
        .split(chunks[1]);

    // Board details
    if let Some(selected_board) = app.boards.get(app.selected_board) {
        let details = vec![
            Line::from(vec![
                Span::styled("Board: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(&selected_board.name),
            ]),
            Line::from(vec![
                Span::styled("Status: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::styled(
                    format!(
                        "{} {:?}",
                        selected_board.status.symbol(),
                        selected_board.status
                    ),
                    Style::default().fg(selected_board.status.color()),
                ),
            ]),
            Line::from(vec![
                Span::styled("Config: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(selected_board.config_file.display().to_string()),
            ]),
            Line::from(vec![
                Span::styled("Build Dir: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(selected_board.build_dir.display().to_string()),
            ]),
            Line::from(vec![
                Span::styled("Updated: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(selected_board.last_updated.format("%H:%M:%S").to_string()),
            ]),
        ];

        let details_paragraph = Paragraph::new(details)
            .block(
                Block::default()
                    .title("Board Details")
                    .borders(Borders::ALL),
            )
            .wrap(Wrap { trim: true });

        f.render_widget(details_paragraph, right_chunks[0]);

        // Build log with scrolling support
        let total_lines = selected_board.log_lines.len();
        let available_height = right_chunks[1].height.saturating_sub(2) as usize; // Account for borders

        // Auto-adjust scroll for real-time streaming (show latest content)
        let adjusted_scroll_offset = if total_lines > available_height {
            let max_scroll = total_lines.saturating_sub(available_height);
            
            if app.log_auto_scroll {
                // Auto-scroll enabled: always show the latest content
                max_scroll
            } else {
                // Manual scroll mode: preserve user's position
                app.log_scroll_offset.min(max_scroll)
            }
        } else {
            0
        };

        let log_lines: Vec<Line> = if total_lines > 0 {
            let start_index = adjusted_scroll_offset;
            let end_index = (start_index + available_height).min(total_lines);

            selected_board
                .log_lines
                .get(start_index..end_index)
                .unwrap_or_default()
                .iter()
                .map(|line| colorize_log_line(line))
                .collect()
        } else {
            vec![Line::from("No logs available")]
        };

        let auto_scroll_indicator = if app.log_auto_scroll { "🔄" } else { "📌" };
        let log_title = if app.focused_pane == FocusedPane::LogPane {
            if total_lines > 0 {
                format!(
                    "Build Log [FOCUSED] ({}/{} lines, scroll: {}) {} {}",
                    (adjusted_scroll_offset + log_lines.len()).min(total_lines),
                    total_lines,
                    adjusted_scroll_offset,
                    auto_scroll_indicator,
                    if app.log_auto_scroll { "Auto-scroll" } else { "Manual" }
                )
            } else {
                "Build Log [FOCUSED] (No logs)".to_string()
            }
        } else if total_lines > 0 {
            format!("Build Log ({} lines) {}", total_lines, if app.log_auto_scroll { "🔄" } else { "📌" })
        } else {
            "Build Log".to_string()
        };

        let log_block = if app.focused_pane == FocusedPane::LogPane {
            Block::default()
                .title(log_title)
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan))
        } else {
            Block::default().title(log_title).borders(Borders::ALL)
        };

        let log_paragraph = Paragraph::new(log_lines)
            .block(log_block)
            .wrap(Wrap { trim: true });

        f.render_widget(log_paragraph, right_chunks[1]);
    }

    // Tool warning modal (project-specific)
    if app.show_tool_warning && !app.tool_warning_acknowledged {
        let area = centered_rect(70, 20, f.area());
        f.render_widget(Clear, area);

        let warning_lines: Vec<Line> = app
            .tool_warning_message
            .split('\n')
            .map(Line::from)
            .collect();

        let warning_paragraph = Paragraph::new(warning_lines)
            .block(
                Block::default()
                    .title("Build Tools Notice - Flashing Still Available!")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Yellow)),
            )
            .style(Style::default().bg(Color::Black))
            .wrap(Wrap { trim: true });

        f.render_widget(warning_paragraph, area);
    }
    // Help popup
    else if app.show_help {
        let area = centered_rect(60, 20, f.area());
        f.render_widget(Clear, area);

        let help_text = vec![
            Line::from("🍺 ESPBrew Help"),
            Line::from(""),
            Line::from("Navigation:"),
            Line::from("↑/↓ or j/k    Navigate boards (Board List) / Scroll logs (Log Pane)"),
            Line::from("Tab           Switch between Board List and Log Pane"),
            Line::from("PgUp/PgDn     Scroll logs by page (Log Pane only)"),
            Line::from("Home/End      Jump to top/bottom of logs (Log Pane only)"),
            Line::from(""),
            Line::from("Building:"),
            Line::from("Space or b    Build selected board only"),
            Line::from("x             Build all boards (rebuild all)"),
            Line::from(""),
            Line::from("Other Actions:"),
            Line::from("Enter         Show action menu (Build/Flash/Monitor/Clean/Purge)"),
            Line::from("r             Refresh board list"),
            Line::from("h or ?        Toggle this help"),
            Line::from("q/Ctrl+C/ESC Quit"),
            Line::from(""),
            Line::from("Note: Focused pane is highlighted with cyan border"),
            Line::from("Logs are saved in ./logs/ | Scripts in ./support/"),
            Line::from("Text selection: Mouse support enabled for copy/paste"),
        ];

        let help_paragraph = Paragraph::new(help_text)
            .block(Block::default().title("Help").borders(Borders::ALL))
            .style(Style::default().bg(Color::Black));

        f.render_widget(help_paragraph, area);
    }

    render_help_bar(f, app, main_chunks[1]);
    render_action_menu(f, app);
    render_component_action_menu(f, app);
    render_remote_board_dialog(f, app);
    render_local_board_dialog(f, app);
}

/// Colorize log lines based on content
fn colorize_log_line(line: &str) -> Line<'_> {
    let line_lower = line.to_lowercase();

    if line_lower.contains("error") || line_lower.contains("failed") || line_lower.contains("❌") {
        Line::from(Span::styled(line, Style::default().fg(Color::Red)))
    } else if line_lower.contains("warning") || line_lower.contains("warn") {
        Line::from(Span::styled(line, Style::default().fg(Color::Yellow)))
    } else if line_lower.contains("success")
        || line_lower.contains("✅")
        || line_lower.contains("completed")
    {
        Line::from(Span::styled(line, Style::default().fg(Color::Green)))
    } else if line_lower.contains("info") || line_lower.contains("note") {
        Line::from(Span::styled(line, Style::default().fg(Color::Cyan)))
    } else {
        Line::from(line)
    }
}

/// Create a centered rectangle
fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

/// Render the help bar at the bottom
fn render_help_bar(f: &mut Frame, app: &App, area: Rect) {
    let mut help_text = if app.focused_pane == FocusedPane::LogPane {
        vec![
            Span::styled("[↑↓]Scroll ", Style::default().fg(Color::Cyan)),
            Span::styled("[PgUp/PgDn]Page ", Style::default().fg(Color::Cyan)),
            Span::styled("[Home/End]Top/Bottom ", Style::default().fg(Color::Cyan)),
            Span::styled("[Tab]Switch Pane ", Style::default().fg(Color::White)),
            Span::styled("[Enter]Actions ", Style::default().fg(Color::Green)),
        ]
    } else {
        vec![
            Span::styled("[↑↓]Navigate ", Style::default().fg(Color::Cyan)),
            Span::styled("[Tab]Switch Pane ", Style::default().fg(Color::White)),
            Span::styled("[Enter]Actions ", Style::default().fg(Color::Green)),
        ]
    };

    // Add build status and controls
    if app.build_in_progress {
        help_text.extend(vec![Span::styled(
            "🔨 Building... ",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )]);
    } else {
        help_text.extend(vec![
            Span::styled(
                "[Space/B]Build Selected ",
                Style::default().fg(Color::LightYellow),
            ),
            Span::styled("[X]Build All ", Style::default().fg(Color::Yellow)),
        ]);
    }

    // Add remaining controls
    if !app.build_in_progress {
        help_text.push(Span::styled(
            "[R]Refresh ",
            Style::default().fg(Color::Magenta),
        ));
    }
    help_text.extend(vec![
        Span::styled("[H/?]Help ", Style::default().fg(Color::Blue)),
        Span::styled("[Q/Ctrl+C/ESC]Quit ", Style::default().fg(Color::Red)),
    ]);

    // Add server discovery status with visual indicators
    let server_status_color = if app.server_discovery_in_progress {
        Color::Yellow
    } else if app.discovered_servers.is_empty() {
        Color::Gray
    } else {
        Color::Green
    };

    // Add animated indicator for discovery in progress
    let discovery_indicator = if app.server_discovery_in_progress {
        "🔄 "
    } else if app.discovered_servers.is_empty() {
        "❌ "
    } else {
        "✅ "
    };

    // Add a separator and prominent server discovery status
    help_text.push(Span::styled(" | ", Style::default().fg(Color::White)));

    if !app.discovered_servers.is_empty() {
        let server = &app.discovered_servers[0];
        help_text.push(Span::styled(
            format!(
                "{}Server: {} ({}:{}) ",
                discovery_indicator, server.name, server.ip, server.port
            ),
            Style::default()
                .fg(server_status_color)
                .add_modifier(Modifier::BOLD),
        ));
    } else {
        let status_with_timing = if app.server_discovery_in_progress {
            if let Some(start_time) = app.server_discovery_start_time {
                let now = chrono::Local::now();
                let elapsed_secs = (now - start_time).num_seconds();
                format!("{} ({}/3s)", app.server_discovery_status, elapsed_secs)
            } else {
                app.server_discovery_status.clone()
            }
        } else {
            app.server_discovery_status.clone()
        };
        help_text.push(Span::styled(
            format!("{}{} ", discovery_indicator, status_with_timing),
            Style::default()
                .fg(server_status_color)
                .add_modifier(Modifier::BOLD),
        ));
    }

    let help_bar = Paragraph::new(Line::from(help_text))
        .block(Block::default().borders(Borders::ALL))
        .style(Style::default().bg(Color::DarkGray));

    f.render_widget(help_bar, area);
}

/// Render the board action menu modal
fn render_action_menu(f: &mut Frame, app: &App) {
    if !app.show_action_menu {
        return;
    }

    let area = centered_rect(50, 40, f.area());
    f.render_widget(Clear, area);

    let selected_board_name = if let Some(board) = app.boards.get(app.selected_board) {
        &board.name
    } else {
        "Unknown"
    };

    let action_items: Vec<ListItem> = app
        .available_actions
        .iter()
        .map(|action| {
            ListItem::new(Line::from(vec![
                Span::raw(action.name()),
                Span::styled(
                    format!(" - {}", action.description()),
                    Style::default().fg(Color::Gray),
                ),
            ]))
        })
        .collect();

    let mut action_list_state = ListState::default();
    action_list_state.select(Some(app.action_menu_selected));

    let action_list = List::new(action_items)
        .block(
            Block::default()
                .title(format!("Actions for: {}", selected_board_name))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Green)),
        )
        .highlight_style(
            Style::default()
                .bg(Color::Green)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD),
        );

    f.render_stateful_widget(action_list, area, &mut action_list_state);

    // Instructions at the bottom of the modal
    let instruction_area = Rect {
        x: area.x + 1,
        y: area.y + area.height - 3,
        width: area.width - 2,
        height: 1,
    };

    let instructions = Paragraph::new(Line::from(vec![
        Span::styled("[↑↓]", Style::default().fg(Color::Cyan)),
        Span::raw(" Navigate "),
        Span::styled("[Enter]", Style::default().fg(Color::Green)),
        Span::raw(" Execute "),
        Span::styled("[ESC]", Style::default().fg(Color::Red)),
        Span::raw(" Cancel"),
    ]));

    f.render_widget(instructions, instruction_area);
}

/// Render the component action menu modal
fn render_component_action_menu(f: &mut Frame, app: &App) {
    if !app.show_component_action_menu {
        return;
    }

    let area = centered_rect(50, 40, f.area());
    f.render_widget(Clear, area);

    let selected_component_name =
        if let Some(component) = app.components.get(app.selected_component) {
            &component.name
        } else {
            "Unknown"
        };

    let selected_component = app.components.get(app.selected_component);
    let available_actions: Vec<_> = app
        .available_component_actions
        .iter()
        .filter(|action| {
            if let Some(comp) = selected_component {
                action.is_available_for(comp)
            } else {
                false
            }
        })
        .collect();

    let action_items: Vec<ListItem> = available_actions
        .iter()
        .map(|action| {
            ListItem::new(Line::from(vec![
                Span::raw(action.name()),
                Span::styled(
                    format!(" - {}", action.description()),
                    Style::default().fg(Color::Gray),
                ),
            ]))
        })
        .collect();

    let mut component_action_list_state = ListState::default();
    // Ensure the selected index is within bounds of available actions
    let adjusted_selected = app
        .component_action_menu_selected
        .min(available_actions.len().saturating_sub(1));
    if !available_actions.is_empty() {
        component_action_list_state.select(Some(adjusted_selected));
    }

    let component_action_list = List::new(action_items)
        .block(
            Block::default()
                .title(format!(
                    "Component Actions for: {}",
                    selected_component_name
                ))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Magenta)),
        )
        .highlight_style(
            Style::default()
                .bg(Color::Magenta)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD),
        );

    f.render_stateful_widget(
        component_action_list,
        area,
        &mut component_action_list_state,
    );

    // Instructions at the bottom of the modal
    let instruction_area = Rect {
        x: area.x + 1,
        y: area.y + area.height - 3,
        width: area.width - 2,
        height: 1,
    };

    let instructions = Paragraph::new(Line::from(vec![
        Span::styled("[↑↓]", Style::default().fg(Color::Cyan)),
        Span::raw(" Navigate "),
        Span::styled("[Enter]", Style::default().fg(Color::Green)),
        Span::raw(" Execute "),
        Span::styled("[ESC]", Style::default().fg(Color::Red)),
        Span::raw(" Cancel"),
    ]));

    f.render_widget(instructions, instruction_area);
}

/// Render the remote board selection dialog
fn render_remote_board_dialog(f: &mut Frame, app: &App) {
    if !app.show_remote_board_dialog {
        return;
    }

    let area = centered_rect(70, 60, f.area());
    f.render_widget(Clear, area);

    // Show loading state
    if app.remote_boards_loading {
        let loading_text = vec![
            Line::from("🔄 Connecting to ESPBrew server..."),
            Line::from(""),
            Line::from("Please wait while we fetch available boards."),
        ];

        let loading_paragraph = Paragraph::new(loading_text)
            .block(
                Block::default()
                    .title("Remote Flash - Loading")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Yellow)),
            )
            .style(Style::default().bg(Color::Black))
            .wrap(Wrap { trim: true });

        f.render_widget(loading_paragraph, area);
        return;
    }

    // Show error state
    if let Some(error) = &app.remote_boards_fetch_error {
        let error_text = vec![
            Line::from("❌ Failed to connect to ESPBrew server"),
            Line::from(""),
            Line::from(error.clone()),
            Line::from(""),
            Line::from("Press [ESC] to close this dialog"),
        ];

        let error_paragraph = Paragraph::new(error_text)
            .block(
                Block::default()
                    .title("Remote Flash - Connection Error")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Red)),
            )
            .style(Style::default().bg(Color::Black))
            .wrap(Wrap { trim: true });

        f.render_widget(error_paragraph, area);
        return;
    }

    // Show board list
    if app.remote_boards.is_empty() {
        let empty_text = vec![
            Line::from("📋 No remote boards found"),
            Line::from(""),
            Line::from("No boards are currently available on the server."),
            Line::from("Please ensure that:"),
            Line::from("• ESPBrew server is running"),
            Line::from("• Boards are connected to the server"),
            Line::from(""),
            Line::from("Press [ESC] to close this dialog"),
        ];

        let empty_paragraph = Paragraph::new(empty_text)
            .block(
                Block::default()
                    .title("Remote Flash - No Boards")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Yellow)),
            )
            .style(Style::default().bg(Color::Black))
            .wrap(Wrap { trim: true });

        f.render_widget(empty_paragraph, area);
        return;
    }

    // Create the board list layout
    let dialog_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(3)])
        .split(area);

    // Board list items
    let board_items: Vec<ListItem> = app
        .remote_boards
        .iter()
        .map(|board| {
            let display_name = board.logical_name.as_ref().unwrap_or(&board.id);
            ListItem::new(Line::from(vec![
                Span::styled("📟 ", Style::default().fg(Color::Cyan)),
                Span::styled(
                    display_name.clone(),
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!(" ({})", board.chip_type),
                    Style::default().fg(Color::Green),
                ),
                Span::raw(" - "),
                Span::styled(&board.device_description, Style::default().fg(Color::Gray)),
            ]))
        })
        .collect();

    let mut remote_board_list_state = app.remote_board_list_state.clone();

    let remote_board_list = List::new(board_items)
        .block(
            Block::default()
                .title(format!(
                    "Remote Boards ({}) - Select board to flash",
                    app.remote_boards.len()
                ))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Green)),
        )
        .highlight_style(
            Style::default()
                .bg(Color::Green)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD),
        );

    f.render_stateful_widget(
        remote_board_list,
        dialog_chunks[0],
        &mut remote_board_list_state,
    );

    // Show selected board details and instructions
    if let Some(selected_board) = app.remote_boards.get(app.selected_remote_board) {
        let detail_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
            .split(dialog_chunks[1]);

        // Board details
        let details = format!(
            "MAC: {} | Port: {} | Status: {} | ID: {}",
            selected_board.mac_address,
            selected_board.port,
            selected_board.status,
            selected_board.id
        );

        let details_paragraph = Paragraph::new(details)
            .style(Style::default().fg(Color::White))
            .wrap(Wrap { trim: true });

        f.render_widget(details_paragraph, detail_chunks[0]);

        // Instructions
        let instructions = Paragraph::new(Line::from(vec![
            Span::styled("[↑↓]", Style::default().fg(Color::Cyan)),
            Span::raw(" Navigate "),
            Span::styled("[Enter]", Style::default().fg(Color::Green)),
            Span::raw(" Flash "),
            Span::styled("[ESC]", Style::default().fg(Color::Red)),
            Span::raw(" Cancel"),
        ]));

        f.render_widget(instructions, detail_chunks[1]);
    }
}

/// Render the local board selection dialog
fn render_local_board_dialog(f: &mut Frame, app: &App) {
    if !app.show_local_board_dialog {
        return;
    }

    let area = centered_rect(70, 60, f.area());
    f.render_widget(Clear, area);

    // Show loading state
    if app.local_boards_loading {
        let loading_text = vec![
            Line::from("🔍 Scanning for local boards..."),
            Line::from(""),
            Line::from("Please wait while we detect connected ESP32 devices."),
        ];

        let loading_paragraph = Paragraph::new(loading_text)
            .block(
                Block::default()
                    .title("Local Flash - Scanning")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Yellow)),
            )
            .style(Style::default().bg(Color::Black))
            .wrap(Wrap { trim: true });

        f.render_widget(loading_paragraph, area);
        return;
    }

    // Show error state
    if let Some(error) = &app.local_boards_fetch_error {
        let error_text = vec![
            Line::from("❌ Failed to scan for local boards"),
            Line::from(""),
            Line::from(error.clone()),
            Line::from(""),
            Line::from("Press [ESC] to close this dialog"),
        ];

        let error_paragraph = Paragraph::new(error_text)
            .block(
                Block::default()
                    .title("Local Flash - Scan Error")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Red)),
            )
            .style(Style::default().bg(Color::Black))
            .wrap(Wrap { trim: true });

        f.render_widget(error_paragraph, area);
        return;
    }

    // Show board list
    if app.local_boards.is_empty() {
        let empty_text = vec![
            Line::from("📱 No local boards found"),
            Line::from(""),
            Line::from("No ESP32 boards are currently connected via USB."),
            Line::from("Please ensure that:"),
            Line::from("• ESP32 board is connected via USB cable"),
            Line::from("• USB drivers are installed"),
            Line::from("• Board is not in use by another program"),
            Line::from(""),
            Line::from("Press [ESC] to close this dialog"),
        ];

        let empty_paragraph = Paragraph::new(empty_text)
            .block(
                Block::default()
                    .title("Local Flash - No Boards")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Yellow)),
            )
            .style(Style::default().bg(Color::Black))
            .wrap(Wrap { trim: true });

        f.render_widget(empty_paragraph, area);
        return;
    }

    // Create the board list layout
    let dialog_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(3)])
        .split(area);

    // Board list items
    let board_items: Vec<ListItem> = app
        .local_boards
        .iter()
        .map(|board| {
            let port_name = board.port.split('/').next_back().unwrap_or(&board.port);
            ListItem::new(Line::from(vec![
                Span::styled("🔌 ", Style::default().fg(Color::Blue)),
                Span::styled(
                    port_name.to_string(),
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!(" ({})", board.chip_type),
                    Style::default().fg(Color::Green),
                ),
                Span::raw(" - "),
                Span::styled(&board.device_description, Style::default().fg(Color::Gray)),
            ]))
        })
        .collect();

    let mut local_board_list_state = app.local_board_list_state.clone();

    let local_board_list = List::new(board_items)
        .block(
            Block::default()
                .title(format!(
                    "Local Boards ({}) - Select board to flash",
                    app.local_boards.len()
                ))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Blue)),
        )
        .highlight_style(
            Style::default()
                .bg(Color::Blue)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD),
        );

    f.render_stateful_widget(
        local_board_list,
        dialog_chunks[0],
        &mut local_board_list_state,
    );

    // Show selected board details and instructions
    if let Some(selected_board) = app.local_boards.get(app.selected_local_board) {
        let detail_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
            .split(dialog_chunks[1]);

        // Board details
        let details = format!(
            "Port: {} | MAC: {} | ID: {}",
            selected_board.port, selected_board.mac_address, selected_board.unique_id
        );

        let details_paragraph = Paragraph::new(details)
            .style(Style::default().fg(Color::White))
            .wrap(Wrap { trim: true });

        f.render_widget(details_paragraph, detail_chunks[0]);

        // Instructions
        let instructions = Paragraph::new(Line::from(vec![
            Span::styled("[↑↓]", Style::default().fg(Color::Cyan)),
            Span::raw(" Navigate "),
            Span::styled("[Enter]", Style::default().fg(Color::Green)),
            Span::raw(" Flash "),
            Span::styled("[ESC]", Style::default().fg(Color::Red)),
            Span::raw(" Cancel"),
        ]));

        f.render_widget(instructions, detail_chunks[1]);
    }
}
