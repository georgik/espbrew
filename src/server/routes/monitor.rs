//! Monitor endpoint routes

use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;
use tokio::sync::RwLock;
use warp::Filter;

use crate::models::monitor::{KeepAliveRequest, MonitorRequest, StopMonitorRequest};
use crate::server::app::ServerState;
use crate::server::services::MonitoringService;

/// WebSocket message types for client-server communication
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
enum WebSocketMessage {
    #[serde(rename = "auth")]
    Auth { session_id: String },
    #[serde(rename = "ping")]
    Ping,
    #[serde(rename = "pong")]
    Pong,
    #[serde(rename = "keepalive")]
    KeepAlive { session_id: String },
}

/// WebSocket response message types
#[derive(Debug, Serialize)]
#[serde(tag = "type")]
enum WebSocketResponse {
    #[serde(rename = "connected")]
    Connected { session_id: String, message: String },
    #[serde(rename = "error")]
    Error {
        message: String,
        session_id: Option<String>,
    },
    #[serde(rename = "pong")]
    Pong,
    #[serde(rename = "keepalive_ack")]
    KeepAliveAck { success: bool, message: String },
}

/// Create all monitoring-related routes
pub fn create_monitor_routes(
    state: Arc<RwLock<ServerState>>,
) -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
    let monitor_start = monitor_start_route(state.clone());
    let monitor_stop = monitor_stop_route(state.clone());
    let monitor_keepalive = monitor_keepalive_route(state.clone());
    let monitor_sessions = monitor_sessions_route(state.clone());
    let websocket_route = websocket_monitor_route(state.clone());

    let api_routes = warp::path("api")
        .and(warp::path("v1"))
        .and(warp::path("monitor"))
        .and(
            monitor_start
                .or(monitor_stop)
                .or(monitor_keepalive)
                .or(monitor_sessions),
        );

    // Combine API routes with WebSocket route
    api_routes.or(websocket_route)
}

/// POST /api/v1/monitor/start - Start monitoring a board
fn monitor_start_route(
    state: Arc<RwLock<ServerState>>,
) -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
    warp::post()
        .and(warp::path("start"))
        .and(warp::path::end())
        .and(warp::body::json())
        .and(with_server_state(state))
        .and_then(monitor_start_handler)
}

/// POST /api/v1/monitor/stop - Stop monitoring session
fn monitor_stop_route(
    state: Arc<RwLock<ServerState>>,
) -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
    warp::post()
        .and(warp::path("stop"))
        .and(warp::path::end())
        .and(warp::body::json())
        .and(with_server_state(state))
        .and_then(monitor_stop_handler)
}

/// POST /api/v1/monitor/keepalive - Keep monitoring session alive
fn monitor_keepalive_route(
    state: Arc<RwLock<ServerState>>,
) -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
    warp::post()
        .and(warp::path("keepalive"))
        .and(warp::path::end())
        .and(warp::body::json())
        .and(with_server_state(state))
        .and_then(monitor_keepalive_handler)
}

/// GET /api/v1/monitor/sessions - List active monitoring sessions
fn monitor_sessions_route(
    state: Arc<RwLock<ServerState>>,
) -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
    warp::get()
        .and(warp::path("sessions"))
        .and(warp::path::end())
        .and(with_server_state(state))
        .and_then(monitor_sessions_handler)
}

/// WS /ws/monitor/{session_id} - WebSocket for receiving logs
fn websocket_monitor_route(
    state: Arc<RwLock<ServerState>>,
) -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
    warp::path("ws")
        .and(warp::path("monitor"))
        .and(warp::path::param::<String>())
        .and(warp::path::end())
        .and(warp::ws())
        .and(with_server_state(state))
        .map(
            |session_id: String, ws: warp::ws::Ws, state: Arc<RwLock<ServerState>>| {
                ws.on_upgrade(move |socket| websocket_handler(socket, session_id, state))
            },
        )
}

/// Helper function to pass server state to handlers
fn with_server_state(
    state: Arc<RwLock<ServerState>>,
) -> impl Filter<Extract = (Arc<RwLock<ServerState>>,), Error = std::convert::Infallible> + Clone {
    warp::any().map(move || Arc::clone(&state))
}

/// Handler for POST /api/v1/monitor/start
async fn monitor_start_handler(
    request: MonitorRequest,
    state: Arc<RwLock<ServerState>>,
) -> Result<impl warp::Reply, warp::Rejection> {
    let monitoring_service = MonitoringService::new(state);

    match monitoring_service.start_monitoring(request).await {
        Ok(response) => Ok(warp::reply::json(&response)),
        Err(e) => {
            let error_response = json!({
                "success": false,
                "message": format!("Failed to start monitoring: {}", e),
                "websocket_url": serde_json::Value::Null,
                "session_id": serde_json::Value::Null
            });
            Ok(warp::reply::json(&error_response))
        }
    }
}

/// Handle incoming WebSocket messages
async fn handle_websocket_message(
    text: &str,
    session_id: &str,
    monitoring_service: &MonitoringService,
    response_tx: &tokio::sync::mpsc::UnboundedSender<String>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Try to parse the incoming message
    if let Ok(message) = serde_json::from_str::<WebSocketMessage>(text) {
        match message {
            WebSocketMessage::Auth {
                session_id: auth_session_id,
            } => {
                println!("🔐 WebSocket auth request for session: {}", auth_session_id);

                // Verify the session ID matches
                if auth_session_id == session_id {
                    let response = WebSocketResponse::Connected {
                        session_id: session_id.to_string(),
                        message: "Authentication successful".to_string(),
                    };
                    let response_json = serde_json::to_string(&response)?;
                    response_tx.send(response_json)?;
                } else {
                    let response = WebSocketResponse::Error {
                        message: "Invalid session ID".to_string(),
                        session_id: Some(session_id.to_string()),
                    };
                    let response_json = serde_json::to_string(&response)?;
                    response_tx.send(response_json)?;
                }
            }
            WebSocketMessage::Ping => {
                println!("🏓 WebSocket ping from session: {}", session_id);
                let response = WebSocketResponse::Pong;
                let response_json = serde_json::to_string(&response)?;
                response_tx.send(response_json)?;
            }
            WebSocketMessage::Pong => {
                println!("🏓 WebSocket pong from session: {}", session_id);
                // Just acknowledge the pong, no response needed
            }
            WebSocketMessage::KeepAlive {
                session_id: keepalive_session_id,
            } => {
                println!(
                    "❤️ WebSocket keepalive from session: {}",
                    keepalive_session_id
                );

                // Update the session's last activity
                let keepalive_req = KeepAliveRequest {
                    session_id: keepalive_session_id.clone(),
                };

                match monitoring_service.keep_alive(keepalive_req).await {
                    Ok(keepalive_resp) => {
                        let response = WebSocketResponse::KeepAliveAck {
                            success: keepalive_resp.success,
                            message: keepalive_resp.message,
                        };
                        let response_json = serde_json::to_string(&response)?;
                        response_tx.send(response_json)?;
                    }
                    Err(e) => {
                        let response = WebSocketResponse::KeepAliveAck {
                            success: false,
                            message: format!("Keep-alive failed: {}", e),
                        };
                        let response_json = serde_json::to_string(&response)?;
                        response_tx.send(response_json)?;
                    }
                }
            }
        }
    } else {
        // Handle non-JSON messages (could be raw text for backwards compatibility)
        println!(
            "📨 WebSocket raw message from session {}: {}",
            session_id, text
        );
    }

    Ok(())
}

/// Handler for POST /api/v1/monitor/stop
async fn monitor_stop_handler(
    request: StopMonitorRequest,
    state: Arc<RwLock<ServerState>>,
) -> Result<impl warp::Reply, warp::Rejection> {
    let monitoring_service = MonitoringService::new(state);

    match monitoring_service.stop_monitoring(request).await {
        Ok(response) => Ok(warp::reply::json(&response)),
        Err(e) => {
            let error_response = json!({
                "success": false,
                "message": format!("Failed to stop monitoring: {}", e)
            });
            Ok(warp::reply::json(&error_response))
        }
    }
}

/// Handler for POST /api/v1/monitor/keepalive
async fn monitor_keepalive_handler(
    request: KeepAliveRequest,
    state: Arc<RwLock<ServerState>>,
) -> Result<impl warp::Reply, warp::Rejection> {
    let monitoring_service = MonitoringService::new(state);

    match monitoring_service.keep_alive(request).await {
        Ok(response) => Ok(warp::reply::json(&response)),
        Err(e) => {
            let error_response = json!({
                "success": false,
                "message": format!("Failed to update keep-alive: {}", e)
            });
            Ok(warp::reply::json(&error_response))
        }
    }
}

/// Handler for GET /api/v1/monitor/sessions
async fn monitor_sessions_handler(
    state: Arc<RwLock<ServerState>>,
) -> Result<impl warp::Reply, warp::Rejection> {
    let monitoring_service = MonitoringService::new(state);

    match monitoring_service.list_sessions().await {
        Ok(sessions) => {
            let response = json!({
                "success": true,
                "sessions": sessions
            });
            Ok(warp::reply::json(&response))
        }
        Err(e) => {
            let error_response = json!({
                "success": false,
                "message": format!("Failed to list sessions: {}", e),
                "sessions": []
            });
            Ok(warp::reply::json(&error_response))
        }
    }
}

/// WebSocket handler for log streaming
async fn websocket_handler(
    ws: warp::ws::WebSocket,
    session_id: String,
    state: Arc<RwLock<ServerState>>,
) {
    println!(
        "🔌 WebSocket connection established for session {}",
        session_id
    );

    let monitoring_service = MonitoringService::new(state);

    // Get the monitoring session
    if let Some(session_arc) = monitoring_service.get_session(&session_id).await {
        let session = session_arc.read().await;
        let mut receiver = session.sender.subscribe();
        drop(session); // Release the lock

        // Split the WebSocket into sender and receiver
        let (mut ws_sender, mut ws_receiver) = ws.split();

        // Send connection confirmation
        let connected_msg = WebSocketResponse::Connected {
            session_id: session_id.clone(),
            message: "WebSocket connected to monitoring session".to_string(),
        };
        if let Ok(connected_json) = serde_json::to_string(&connected_msg) {
            let _ = ws_sender
                .send(warp::ws::Message::text(connected_json))
                .await;
        }

        // Create a channel for sending responses back to the WebSocket
        let (response_tx, mut response_rx) = tokio::sync::mpsc::unbounded_channel::<String>();

        // Spawn task to handle incoming WebSocket messages
        let session_id_clone = session_id.clone();
        let monitoring_service_clone = monitoring_service.clone();
        let message_handler = tokio::spawn(async move {
            while let Some(result) = ws_receiver.next().await {
                match result {
                    Ok(msg) => {
                        if msg.is_text() {
                            if let Ok(text) = msg.to_str() {
                                if let Err(e) = handle_websocket_message(
                                    text,
                                    &session_id_clone,
                                    &monitoring_service_clone,
                                    &response_tx,
                                )
                                .await
                                {
                                    println!("❌ Error handling WebSocket message: {}", e);
                                }
                            }
                        } else if msg.is_close() {
                            println!("🔌 WebSocket closed for session {}", session_id_clone);
                            break;
                        }
                    }
                    Err(e) => {
                        println!("❌ WebSocket error for session {}: {}", session_id_clone, e);
                        break;
                    }
                }
            }
        });

        // Main loop to forward log messages and response messages to WebSocket
        loop {
            tokio::select! {
                // Forward log messages from serial port
                log_result = receiver.recv() => {
                    match log_result {
                        Ok(log_message) => {
                            if let Err(e) = ws_sender.send(warp::ws::Message::text(log_message)).await {
                                println!(
                                    "❌ Failed to send log message for session {}: {}",
                                    session_id, e
                                );
                                break;
                            }
                        }
                        Err(_) => {
                            println!("🔌 Log channel closed for session {}", session_id);
                            break;
                        }
                    }
                }
                // Forward response messages from message handler
                response_result = response_rx.recv() => {
                    match response_result {
                        Some(response_message) => {
                            if let Err(e) = ws_sender.send(warp::ws::Message::text(response_message)).await {
                                println!(
                                    "❌ Failed to send response message for session {}: {}",
                                    session_id, e
                                );
                                break;
                            }
                        }
                        None => {
                            // Response channel closed, continue with log forwarding only
                        }
                    }
                }
            }
        }

        // Clean up
        message_handler.abort();
        println!("🔌 WebSocket connection closed for session {}", session_id);
    } else {
        println!(
            "❌ WebSocket connection failed - session not found: {}",
            session_id
        );

        // Send error message and close
        let (mut ws_sender, _) = ws.split();
        let error_msg = WebSocketResponse::Error {
            message: "Monitoring session not found".to_string(),
            session_id: Some(session_id.clone()),
        };

        if let Ok(error_json) = serde_json::to_string(&error_msg) {
            let _ = ws_sender.send(warp::ws::Message::text(error_json)).await;
        }
    }
}
