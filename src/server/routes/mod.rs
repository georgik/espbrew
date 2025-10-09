//! HTTP routes for the ESPBrew server

pub mod board_types;
pub mod boards;
pub mod flash;
pub mod health;
pub mod monitor;
pub mod static_files;
pub mod websocket;

use crate::server::app::ServerState;
use std::sync::Arc;
use tokio::sync::RwLock;
use warp::Filter;

/// Create all server routes
pub fn create_routes(
    state: Arc<RwLock<ServerState>>,
) -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
    boards::create_board_routes(state.clone())
        .or(flash::create_flash_routes(state.clone()))
        .or(monitor::create_monitor_routes(state.clone()))
        .or(websocket::create_websocket_routes(state.clone()))
        .or(static_files::create_static_routes())
}
