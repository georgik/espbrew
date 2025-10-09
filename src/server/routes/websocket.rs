use warp::Filter;

/// Additional WebSocket routes (monitoring WebSocket is handled in monitor routes)
pub fn create_websocket_routes(
    _state: std::sync::Arc<tokio::sync::RwLock<crate::server::app::ServerState>>,
) -> impl warp::Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
    // WebSocket routes for monitoring are handled in monitor::create_monitor_routes
    // This could be used for other WebSocket functionality in the future
    warp::path("ws").and(warp::path("info")).map(|| {
        warp::reply::with_status(
            "WebSocket monitoring available at /ws/monitor/{session_id}",
            warp::http::StatusCode::OK,
        )
    })
}
