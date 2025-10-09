//! Board types management routes

use serde_json::json;
use std::sync::Arc;
use tokio::sync::RwLock;
use warp::Filter;

use crate::models::responses::{AssignBoardRequest, AssignmentResponse, BoardTypesResponse};
use crate::server::app::ServerState;

/// Create all board types management routes
pub fn create_board_types_routes(
    state: Arc<RwLock<ServerState>>,
) -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
    let board_types = board_types_route(state.clone());
    let assign_board = assign_board_route(state.clone());
    let unassign_board = unassign_board_route(state.clone());

    warp::path("api")
        .and(warp::path("v1"))
        .and(board_types.or(assign_board).or(unassign_board))
}

/// GET /api/v1/board-types - Get available board types
fn board_types_route(
    state: Arc<RwLock<ServerState>>,
) -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
    warp::path("board-types")
        .and(warp::get())
        .and(warp::path::end())
        .and(with_server_state(state))
        .and_then(get_board_types_handler)
}

/// POST /api/v1/assign-board - Assign a board to a board type
fn assign_board_route(
    state: Arc<RwLock<ServerState>>,
) -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
    warp::path("assign-board")
        .and(warp::post())
        .and(warp::path::end())
        .and(warp::body::json())
        .and(with_server_state(state))
        .and_then(assign_board_handler)
}

/// DELETE /api/v1/assign-board/{unique_id} - Unassign a board
fn unassign_board_route(
    state: Arc<RwLock<ServerState>>,
) -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
    warp::path("assign-board")
        .and(warp::path::param::<String>())
        .and(warp::delete())
        .and(warp::path::end())
        .and(with_server_state(state))
        .and_then(unassign_board_handler)
}

/// Helper function to pass server state to handlers
fn with_server_state(
    state: Arc<RwLock<ServerState>>,
) -> impl Filter<Extract = (Arc<RwLock<ServerState>>,), Error = std::convert::Infallible> + Clone {
    warp::any().map(move || Arc::clone(&state))
}

/// Handler for GET /api/v1/board-types
async fn get_board_types_handler(
    state: Arc<RwLock<ServerState>>,
) -> Result<impl warp::Reply, warp::Rejection> {
    let state_lock = state.read().await;

    let response = BoardTypesResponse {
        board_types: state_lock.persistent_config.board_types.clone(),
    };

    Ok(warp::reply::json(&response))
}

/// Handler for POST /api/v1/assign-board
async fn assign_board_handler(
    request: AssignBoardRequest,
    state: Arc<RwLock<ServerState>>,
) -> Result<impl warp::Reply, warp::Rejection> {
    let mut state_lock = state.write().await;

    match state_lock
        .assign_board_type(
            request.board_unique_id,
            request.board_type_id,
            request.logical_name,
            request.chip_type_override,
        )
        .await
    {
        Ok(()) => {
            let response = AssignmentResponse {
                success: true,
                message: "Board assignment successful".to_string(),
            };
            Ok(warp::reply::json(&response))
        }
        Err(e) => {
            let response = AssignmentResponse {
                success: false,
                message: format!("Board assignment failed: {}", e),
            };
            Ok(warp::reply::json(&response))
        }
    }
}

/// Handler for DELETE /api/v1/assign-board/{unique_id}
async fn unassign_board_handler(
    unique_id: String,
    state: Arc<RwLock<ServerState>>,
) -> Result<impl warp::Reply, warp::Rejection> {
    let mut state_lock = state.write().await;

    match state_lock.unassign_board(unique_id).await {
        Ok(()) => {
            let response = AssignmentResponse {
                success: true,
                message: "Board unassignment successful".to_string(),
            };
            Ok(warp::reply::json(&response))
        }
        Err(e) => {
            let response = AssignmentResponse {
                success: false,
                message: format!("Board unassignment failed: {}", e),
            };
            Ok(warp::reply::json(&response))
        }
    }
}
