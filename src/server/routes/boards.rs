//! Board management routes

use serde_json::json;
use std::sync::Arc;
use tokio::sync::RwLock;
use warp::Filter;

use crate::models::responses::{BoardListResponse, ServerInfo};
use crate::server::app::ServerState;
use crate::server::services::board_scanner::BoardScanner;

/// Create all board-related routes
pub fn create_board_routes(
    state: Arc<RwLock<ServerState>>,
) -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
    let boards_list = boards_list_route(state.clone());
    let boards_scan = boards_scan_route(state.clone());
    let board_info = board_info_route(state.clone());

    warp::path("api")
        .and(warp::path("v1"))
        .and(boards_list.or(boards_scan).or(board_info))
}

/// Create reset route (separate from boards routes per original API)
pub fn create_reset_route(
    state: Arc<RwLock<ServerState>>,
) -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
    warp::path("api")
        .and(warp::path("v1"))
        .and(warp::path("reset"))
        .and(warp::post())
        .and(warp::path::end())
        .and(warp::body::json())
        .and(with_server_state(state))
        .and_then(reset_board_handler)
}

/// GET /api/v1/boards - List all connected boards
fn boards_list_route(
    state: Arc<RwLock<ServerState>>,
) -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
    warp::path("boards")
        .and(warp::get())
        .and(warp::path::end())
        .and(with_server_state(state))
        .and_then(list_boards_handler)
}

/// POST /api/v1/boards/scan - Trigger board scan
fn boards_scan_route(
    state: Arc<RwLock<ServerState>>,
) -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
    warp::path("boards")
        .and(warp::path("scan"))
        .and(warp::post())
        .and(warp::path::end())
        .and(with_server_state(state))
        .and_then(scan_boards_handler)
}

/// GET /api/v1/boards/{id} - Get board info
fn board_info_route(
    state: Arc<RwLock<ServerState>>,
) -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
    warp::path("boards")
        .and(warp::path::param::<String>())
        .and(warp::get())
        .and(warp::path::end())
        .and(with_server_state(state))
        .and_then(get_board_info_handler)
}

/// Helper function to pass server state to handlers
fn with_server_state(
    state: Arc<RwLock<ServerState>>,
) -> impl Filter<Extract = (Arc<RwLock<ServerState>>,), Error = std::convert::Infallible> + Clone {
    warp::any().map(move || Arc::clone(&state))
}

/// Handler for GET /api/v1/boards
async fn list_boards_handler(
    state: Arc<RwLock<ServerState>>,
) -> Result<impl warp::Reply, warp::Rejection> {
    let state_lock = state.read().await;

    let boards: Vec<_> = state_lock.boards.values().cloned().collect();

    // Get hostname for server info
    let hostname = hostname::get()
        .unwrap_or_else(|_| "espbrew-server".into())
        .to_string_lossy()
        .to_string();

    let response = BoardListResponse {
        boards,
        server_info: ServerInfo {
            version: env!("CARGO_PKG_VERSION").to_string(),
            hostname,
            last_scan: state_lock.last_scan,
            total_boards: state_lock.boards.len(),
        },
    };

    Ok(warp::reply::json(&response))
}

/// Handler for POST /api/v1/boards/scan
async fn scan_boards_handler(
    state: Arc<RwLock<ServerState>>,
) -> Result<impl warp::Reply, warp::Rejection> {
    let scanner = BoardScanner::new(state.clone());

    match scanner.scan_boards().await {
        Ok(count) => {
            let response = json!({
                "message": format!("Board scan completed, found {} boards", count),
                "boards_found": count,
                "success": true
            });
            Ok(warp::reply::json(&response))
        }
        Err(e) => {
            let response = json!({
                "error": format!("Board scan failed: {}", e),
                "success": false
            });
            Ok(warp::reply::json(&response))
        }
    }
}

/// Handler for GET /api/v1/boards/{id}
async fn get_board_info_handler(
    board_id: String,
    state: Arc<RwLock<ServerState>>,
) -> Result<impl warp::Reply, warp::Rejection> {
    let state_lock = state.read().await;

    if let Some(board) = state_lock.boards.get(&board_id) {
        Ok(warp::reply::json(board))
    } else {
        let response = json!({
            "error": format!("Board not found: {}", board_id),
            "success": false
        });
        Ok(warp::reply::json(&response))
    }
}

/// Handler for POST /api/v1/reset
async fn reset_board_handler(
    reset_request: crate::models::board::ResetRequest,
    state: Arc<RwLock<ServerState>>,
) -> Result<impl warp::Reply, warp::Rejection> {
    let state_lock = state.read().await;

    if let Some(_board) = state_lock.boards.get(&reset_request.board_id) {
        // TODO: Implement actual board reset functionality
        // This would involve sending a reset signal to the board
        let response = crate::models::board::ResetResponse {
            success: true,
            message: format!(
                "Board {} reset requested (not yet implemented)",
                reset_request.board_id
            ),
        };
        Ok(warp::reply::json(&response))
    } else {
        let response = crate::models::board::ResetResponse {
            success: false,
            message: format!("Board not found: {}", reset_request.board_id),
        };
        Ok(warp::reply::json(&response))
    }
}
