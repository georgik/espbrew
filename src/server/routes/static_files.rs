use include_dir::{Dir, include_dir};
use warp::http::StatusCode;
use warp::{Filter, Reply};

// Include the web assets directory at compile time
static WEB_ASSETS: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/web");

/// Create static file serving routes for web dashboard
pub fn create_static_routes()
-> impl warp::Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
    let static_files = warp::path("static")
        .and(warp::path::tail())
        .and_then(serve_static_file);

    let dashboard_root = warp::path::end().and_then(serve_dashboard_root);

    let dashboard_index = warp::path("dashboard")
        .and(warp::path::end())
        .and_then(serve_dashboard_root);

    dashboard_root.or(dashboard_index).or(static_files)
}

/// Serve static files from embedded directory
async fn serve_static_file(path: warp::path::Tail) -> Result<impl warp::Reply, warp::Rejection> {
    let file_path = path.as_str();

    // Security: prevent directory traversal
    if file_path.contains("..") {
        return Ok(
            warp::reply::with_status("Access denied".to_string(), StatusCode::FORBIDDEN)
                .into_response(),
        );
    }

    // Try to find the file in embedded assets
    if let Some(file) = WEB_ASSETS.get_file(file_path) {
        let content_type = get_content_type(file_path);
        let contents = file.contents();

        Ok(warp::reply::with_header(contents, "content-type", content_type).into_response())
    } else {
        Ok(
            warp::reply::with_status("File not found".to_string(), StatusCode::NOT_FOUND)
                .into_response(),
        )
    }
}

/// Serve the main dashboard HTML page
async fn serve_dashboard_root() -> Result<impl warp::Reply, warp::Rejection> {
    // Try to serve index.html from embedded assets
    if let Some(file) = WEB_ASSETS.get_file("index.html") {
        Ok(
            warp::reply::with_header(file.contents(), "content-type", "text/html; charset=utf-8")
                .into_response(),
        )
    } else {
        // Fallback: serve a minimal dashboard page
        let html = create_minimal_dashboard();
        Ok(
            warp::reply::with_header(html, "content-type", "text/html; charset=utf-8")
                .into_response(),
        )
    }
}

/// Determine MIME type based on file extension
fn get_content_type(file_path: &str) -> &'static str {
    if file_path.ends_with(".html") {
        "text/html; charset=utf-8"
    } else if file_path.ends_with(".css") {
        "text/css"
    } else if file_path.ends_with(".js") {
        "application/javascript"
    } else if file_path.ends_with(".json") {
        "application/json"
    } else if file_path.ends_with(".png") {
        "image/png"
    } else if file_path.ends_with(".jpg") || file_path.ends_with(".jpeg") {
        "image/jpeg"
    } else if file_path.ends_with(".svg") {
        "image/svg+xml"
    } else if file_path.ends_with(".ico") {
        "image/x-icon"
    } else {
        "application/octet-stream"
    }
}

/// Create a minimal dashboard HTML page when assets aren't available
fn create_minimal_dashboard() -> String {
    r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>ESPBrew Dashboard</title>
    <style>
        body {
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
            margin: 0;
            padding: 20px;
            background: #1a1a1a;
            color: #ffffff;
        }
        .container {
            max-width: 1200px;
            margin: 0 auto;
        }
        .header {
            text-align: center;
            margin-bottom: 40px;
            padding: 20px;
            background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
            border-radius: 10px;
        }
        .card {
            background: #2d2d2d;
            border-radius: 8px;
            padding: 20px;
            margin-bottom: 20px;
            border: 1px solid #444;
        }
        .status {
            display: flex;
            align-items: center;
            gap: 10px;
            margin-bottom: 10px;
        }
        .status-dot {
            width: 12px;
            height: 12px;
            border-radius: 50%;
            background: #4ade80;
            animation: pulse 2s infinite;
        }
        @keyframes pulse {
            0%, 100% { opacity: 1; }
            50% { opacity: 0.5; }
        }
        .api-info {
            font-family: 'Monaco', 'Menlo', monospace;
            font-size: 14px;
            background: #1a1a1a;
            padding: 15px;
            border-radius: 5px;
            border-left: 4px solid #667eea;
        }
        .feature-grid {
            display: grid;
            grid-template-columns: repeat(auto-fit, minmax(300px, 1fr));
            gap: 20px;
            margin-top: 20px;
        }
        .feature {
            background: #2d2d2d;
            padding: 20px;
            border-radius: 8px;
            border: 1px solid #444;
        }
        .feature h3 {
            margin: 0 0 10px 0;
            color: #667eea;
        }
    </style>
</head>
<body>
    <div class="container">
        <div class="header">
            <h1>🍺 ESPBrew Dashboard</h1>
            <p>ESP32 Development Server & Board Manager</p>
        </div>
        
        <div class="card">
            <div class="status">
                <div class="status-dot"></div>
                <h2>Server Status: Online</h2>
            </div>
            <p>ESPBrew server is running and ready to accept connections.</p>
        </div>

        <div class="card">
            <h2>📡 API Endpoints</h2>
            <div class="api-info">
                GET  /api/v1/boards          - List available boards<br>
                POST /api/v1/flash           - Flash board firmware<br>
                POST /api/v1/monitor/start   - Start monitoring session<br>
                POST /api/v1/monitor/stop    - Stop monitoring session<br>
                WS   /ws/monitor/{session_id} - WebSocket monitoring
            </div>
        </div>

        <div class="feature-grid">
            <div class="feature">
                <h3>🔧 Board Management</h3>
                <p>Discover, configure, and manage ESP32 development boards with automatic detection and board type assignment.</p>
            </div>
            <div class="feature">
                <h3>⚡ Remote Flashing</h3>
                <p>Upload and flash firmware to remote boards over the network with support for multiple project types.</p>
            </div>
            <div class="feature">
                <h3>📺 Real-time Monitoring</h3>
                <p>Monitor serial output from boards in real-time with WebSocket connections and log filtering.</p>
            </div>
            <div class="feature">
                <h3>🎯 Project Support</h3>
                <p>Full support for ESP-IDF, Rust, Arduino, PlatformIO, MicroPython, and other embedded frameworks.</p>
            </div>
        </div>

        <div class="card">
            <h2>🚀 Getting Started</h2>
            <p>Use the <strong>ESPBrew CLI</strong> or <strong>TUI</strong> to interact with boards:</p>
            <div class="api-info">
                espbrew tui                    # Launch interactive TUI<br>
                espbrew discovery             # Discover ESPBrew servers<br>
                espbrew remote-flash --help   # Remote flashing help<br>
                espbrew remote-monitor --help # Remote monitoring help
            </div>
        </div>
    </div>

    <script>
        // Simple status check
        async function checkServerStatus() {
            try {
                const response = await fetch('/api/v1/boards');
                console.log('Server status check:', response.ok ? 'OK' : 'Error');
            } catch (error) {
                console.error('Server status check failed:', error);
            }
        }
        
        // Check status on load and periodically
        checkServerStatus();
        setInterval(checkServerStatus, 30000);
    </script>
</body>
</html>"#.to_string()
}
