//! Health check route

use serde_json::json;
use warp::Filter;

/// Create health check route
pub fn create_health_route()
-> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
    warp::path("health")
        .and(warp::get())
        .and(warp::path::end())
        .map(|| {
            warp::reply::json(&json!({
                "status": "healthy",
                "version": env!("CARGO_PKG_VERSION")
            }))
        })
}
