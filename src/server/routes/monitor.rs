//! Monitor endpoint routes

use futures_util::{SinkExt, StreamExt};
use serde_json::json;
use std::sync::Arc;
use tokio::sync::RwLock;
use warp::Filter;

use crate::models::monitor::{KeepAliveRequest, MonitorRequest, StopMonitorRequest};
use crate::server::app::ServerState;
use crate::server::services::MonitoringService;

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

        // Spawn task to handle incoming WebSocket messages (for keep-alive, etc.)
        let session_id_clone = session_id.clone();
        let monitoring_service_clone = monitoring_service.clone();
        let ping_task = tokio::spawn(async move {
            while let Some(result) = ws_receiver.next().await {
                match result {
                    Ok(msg) => {
                        if msg.is_text() || msg.is_binary() {
                            // Handle incoming messages if needed (keep-alive, etc.)
                            println!(
                                "📨 WebSocket message received for session {}",
                                session_id_clone
                            );
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

        // Main loop to forward log messages to WebSocket
        while let Ok(log_message) = receiver.recv().await {
            if let Err(e) = ws_sender.send(warp::ws::Message::text(log_message)).await {
                println!(
                    "❌ Failed to send WebSocket message for session {}: {}",
                    session_id, e
                );
                break;
            }
        }

        // Clean up
        ping_task.abort();
        println!("🔌 WebSocket connection closed for session {}", session_id);
    } else {
        println!(
            "❌ WebSocket connection failed - session not found: {}",
            session_id
        );

        // Send error message and close
        let (mut ws_sender, _) = ws.split();
        let error_msg = json!({
            "type": "error",
            "message": "Monitoring session not found",
            "session_id": session_id
        });

        if let Ok(error_json) = serde_json::to_string(&error_msg) {
            let _ = ws_sender.send(warp::ws::Message::text(error_json)).await;
        }
    }
}
