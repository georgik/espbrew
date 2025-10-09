use warp::Filter;

pub fn create_websocket_routes(
    _state: std::sync::Arc<tokio::sync::RwLock<crate::server::app::ServerState>>,
) -> impl warp::Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
    warp::path("ws").map(|| "TODO")
}
