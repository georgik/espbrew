//! HTTP request logging middleware

/// Create a request logging filter using warp's built-in logging
pub fn with_request_logging() -> warp::filters::log::Log<impl Fn(warp::filters::log::Info) + Clone>
{
    warp::log::custom(|info| {
        let status = info.status();
        let status_icon = match status.as_u16() {
            200..=299 => "✅",
            300..=399 => "🔀",
            400..=499 => "⚠️",
            500..=599 => "❌",
            _ => "❓",
        };

        let elapsed_ms = info.elapsed().as_millis();
        let timing_icon = if elapsed_ms > 5000 {
            "🐌" // Very slow
        } else if elapsed_ms > 1000 {
            "⏳" // Slow  
        } else if elapsed_ms > 500 {
            "⏱️" // Medium
        } else {
            "⚡" // Fast
        };

        let remote_addr = info
            .remote_addr()
            .map(|addr| addr.to_string())
            .unwrap_or_else(|| "unknown".to_string());

        // Format content length for display
        let content_info = if info.method() == "POST" || info.method() == "PUT" {
            // Note: warp's log::Info doesn't provide content-length directly
            // We'll enhance this in individual route handlers
            " [POST data]".to_string()
        } else {
            String::new()
        };

        println!(
            "{} {} {} {} {}{} - {} {}ms - {} - User-Agent: \"{}\"",
            status_icon,
            timing_icon,
            chrono::Local::now().format("%H:%M:%S"),
            info.method(),
            info.path(),
            content_info,
            status,
            elapsed_ms,
            remote_addr,
            info.user_agent().unwrap_or("unknown")
        );

        // Log additional details for errors
        if status.is_client_error() || status.is_server_error() {
            println!(
                "  🔍 Error details: {} {} from {}",
                info.method(),
                info.path(),
                remote_addr
            );
            if let Some(query) = info.path().split('?').nth(1) {
                println!("  🔍 Query params: {}", query);
            }
        }
    })
}

/// Create a simple request logging filter
pub fn with_simple_request_logging()
-> warp::filters::log::Log<impl Fn(warp::filters::log::Info) + Clone> {
    warp::log::custom(|info| {
        println!(
            "📥 {} {} {}",
            chrono::Local::now().format("%H:%M:%S"),
            info.method(),
            info.path()
        );
    })
}

/// Create a request size logging filter for debugging multipart uploads
use warp::Filter;
pub fn with_request_size_logging() -> impl Filter<Extract = (), Error = warp::Rejection> + Clone {
    warp::header::optional::<String>("content-length")
        .and(warp::path::full())
        .and(warp::method())
        .map(
            |content_length: Option<String>,
             path: warp::path::FullPath,
             method: warp::http::Method| {
                if method == "POST" || method == "PUT" {
                    let size_info = content_length
                        .map(|cl| {
                            if let Ok(size) = cl.parse::<usize>() {
                                if size > 1024 * 1024 {
                                    format!(" [{:.1} MB]", size as f64 / (1024.0 * 1024.0))
                                } else if size > 1024 {
                                    format!(" [{:.1} KB]", size as f64 / 1024.0)
                                } else {
                                    format!(" [{}B]", size)
                                }
                            } else {
                                " [unknown size]".to_string()
                            }
                        })
                        .unwrap_or_else(|| " [no content-length]".to_string());
                    println!(
                        "📦 {} {} {}{}",
                        chrono::Local::now().format("%H:%M:%S"),
                        method,
                        path.as_str(),
                        size_info
                    );
                }
            },
        )
        .untuple_one()
}
