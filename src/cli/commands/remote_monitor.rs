use crate::cli::args::Cli;
use crate::models::board::RemoteBoard;
use crate::models::monitor::{LogMessage, MonitorRequest, MonitorResponse, StopMonitorRequest};
use crate::remote::discovery::discover_espbrew_servers;
use anyhow::Result;
use futures_util::{SinkExt, StreamExt};
use reqwest::Client;
use std::io::{self, Write};
use std::time::Duration;
use tokio_tungstenite::{connect_async, tungstenite::Message};

pub async fn execute_remote_monitor_command(
    _cli: &Cli,
    mac: Option<String>,
    name: Option<String>,
    server: Option<String>,
    baud_rate: u32,
    reset: bool,
) -> Result<()> {
    println!("📺 Starting remote monitor session...");

    // Determine server URL
    let server_url = if let Some(url) = server {
        url
    } else {
        println!("🔍 Discovering ESPBrew servers...");
        let servers = discover_espbrew_servers(3).await?;
        if servers.is_empty() {
            return Err(anyhow::anyhow!(
                "No ESPBrew servers found. Please specify --server URL manually."
            ));
        }
        let server = &servers[0];
        let url = format!("http://{}:{}", server.ip, server.port);
        println!("✅ Found server: {} at {}", server.name, url);
        url
    };

    // Get available boards
    println!("🔍 Fetching available boards...");
    let boards = fetch_remote_boards(&server_url).await?;
    if boards.is_empty() {
        return Err(anyhow::anyhow!("No boards available on server"));
    }

    // Find target board
    let target_board = if let Some(mac_addr) = mac {
        boards
            .iter()
            .find(|b| b.id == mac_addr || b.logical_name.as_ref() == Some(&mac_addr))
            .ok_or_else(|| anyhow::anyhow!("Board with MAC {} not found", mac_addr))?
    } else if let Some(board_name) = name {
        boards
            .iter()
            .find(|b| b.logical_name.as_ref() == Some(&board_name) || b.id == board_name)
            .ok_or_else(|| anyhow::anyhow!("Board '{}' not found", board_name))?
    } else {
        // Use first available board
        println!("📊 Available boards:");
        for (i, board) in boards.iter().enumerate() {
            println!(
                "  {}. {} ({}) - {}",
                i + 1,
                board.logical_name.as_ref().unwrap_or(&board.id),
                board.chip_type,
                board.status
            );
        }
        &boards[0]
    };

    println!(
        "🎯 Selected board: {} ({})",
        target_board
            .logical_name
            .as_ref()
            .unwrap_or(&target_board.id),
        target_board.chip_type
    );

    // Reset board if requested
    if reset {
        println!("🔄 Resetting board...");
        reset_board(&server_url, &target_board.id).await?;
        println!("✅ Board reset completed");
    }

    // Start monitoring session
    println!("📺 Starting monitoring session...");
    let monitor_response = start_monitoring(&server_url, &target_board.id, baud_rate).await?;
    let session_id = monitor_response.session_id.unwrap();
    let websocket_url = monitor_response.websocket_url.unwrap();

    println!("✅ Monitoring session started: {}", session_id);
    println!("🔗 WebSocket URL: {}", websocket_url);
    println!();
    println!("📺 === Remote Monitor Output (Press Ctrl+C to stop) ===");
    println!();

    // Create WebSocket URL (convert HTTP to WS)
    let ws_url = server_url
        .replace("http://", "ws://")
        .replace("https://", "wss://")
        + &websocket_url;

    // Connect to WebSocket and stream logs
    let result = stream_monitor_logs(&ws_url, &session_id).await;

    // Stop monitoring session
    println!();
    println!("🛑 Stopping monitoring session...");
    let _ = stop_monitoring(&server_url, &session_id).await;
    println!("✅ Monitoring session stopped");

    result
}

async fn fetch_remote_boards(server_url: &str) -> Result<Vec<RemoteBoard>> {
    let client = Client::new();
    let url = format!("{}/api/v1/boards", server_url.trim_end_matches('/'));
    let response = client.get(&url).send().await?.error_for_status()?;
    let boards_response: crate::models::responses::RemoteBoardsResponse = response.json().await?;
    Ok(boards_response.boards)
}

async fn reset_board(server_url: &str, board_id: &str) -> Result<()> {
    let client = Client::new();
    let url = format!(
        "{}/api/v1/boards/{}/reset",
        server_url.trim_end_matches('/'),
        board_id
    );
    let _response = client.post(&url).send().await?.error_for_status()?;
    Ok(())
}

async fn start_monitoring(
    server_url: &str,
    board_id: &str,
    baud_rate: u32,
) -> Result<MonitorResponse> {
    let client = Client::new();
    let url = format!("{}/api/v1/monitor/start", server_url.trim_end_matches('/'));

    let request = MonitorRequest {
        board_id: board_id.to_string(),
        baud_rate: Some(baud_rate),
        filters: None, // No filters for CLI monitoring
    };

    let response = client
        .post(&url)
        .json(&request)
        .send()
        .await?
        .error_for_status()?;

    let monitor_response: MonitorResponse = response.json().await?;
    if !monitor_response.success {
        return Err(anyhow::anyhow!(
            "Failed to start monitoring: {}",
            monitor_response.message
        ));
    }

    Ok(monitor_response)
}

async fn stop_monitoring(server_url: &str, session_id: &str) -> Result<()> {
    let client = Client::new();
    let url = format!("{}/api/v1/monitor/stop", server_url.trim_end_matches('/'));

    let request = StopMonitorRequest {
        session_id: session_id.to_string(),
    };

    let _response = client
        .post(&url)
        .json(&request)
        .send()
        .await?
        .error_for_status()?;

    Ok(())
}

async fn stream_monitor_logs(ws_url: &str, session_id: &str) -> Result<()> {
    // Connect to WebSocket
    let (ws_stream, _) = connect_async(ws_url)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to connect to WebSocket: {}", e))?;

    let (mut ws_sender, mut ws_receiver) = ws_stream.split();

    // Send session identification
    let auth_message = serde_json::json!({
        "type": "auth",
        "session_id": session_id
    });
    ws_sender
        .send(Message::Text(auth_message.to_string()))
        .await?;

    // Setup Ctrl+C handler
    let mut should_exit = false;
    let mut stdout = io::stdout();

    while !should_exit {
        tokio::select! {
            // Handle WebSocket messages
            msg = ws_receiver.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        // Try to parse as LogMessage
                        if let Ok(log_msg) = serde_json::from_str::<LogMessage>(&text) {
                            // Print log content with timestamp
                            let timestamp = log_msg.timestamp.format("%H:%M:%S%.3f");
                            println!("[{}] {}", timestamp, log_msg.content);
                            let _ = stdout.flush();
                        } else {
                            // Print raw message if not a log message
                            println!("{}", text);
                        }
                    }
                    Some(Ok(Message::Close(_))) => {
                        println!("🔗 WebSocket connection closed by server");
                        should_exit = true;
                    }
                    Some(Err(e)) => {
                        println!("❌ WebSocket error: {}", e);
                        should_exit = true;
                    }
                    None => {
                        println!("🔗 WebSocket stream ended");
                        should_exit = true;
                    }
                    _ => {}
                }
            }

            // Handle Ctrl+C
            _ = tokio::signal::ctrl_c() => {
                println!();
                println!("🛑 Received Ctrl+C, stopping monitor...");
                should_exit = true;
            }
        }
    }

    Ok(())
}
