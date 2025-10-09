use warp::Filter;

pub fn create_static_routes()
-> impl warp::Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
    warp::path("static").map(|| "TODO")
}
