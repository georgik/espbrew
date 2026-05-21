//! HTTP/WebSocket integration tests for cluster functionality
//!
//! Tests actual server startup, HTTP endpoints, and WebSocket connections

use futures_util::{SinkExt, StreamExt};
use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::time::{Duration, timeout};

/// Test harness for running a cluster server
struct TestCluster {
    port: u16,
    shutdown: Arc<AtomicBool>,
    _handle: tokio::task::JoinHandle<()>,
}

impl TestCluster {
    /// Start a test cluster server on a random available port
    async fn start() -> Self {
        let port = portpicker::pick_unused_port().expect("No available ports for testing");

        let shutdown = Arc::new(AtomicBool::new(false));
        let shutdown_clone = shutdown.clone();

        let bind_addr =
            SocketAddr::from_str(&format!("127.0.0.1:{}", port)).expect("Invalid bind address");
        let bind_addr_str = bind_addr.to_string();
        let config = espbrew::cluster::state::ClusterConfig {
            cluster_name: "test-cluster".to_string(),
            bind_address: bind_addr_str.clone(),
            ..Default::default()
        };

        let master = Arc::new(espbrew::cluster::master::MasterNode::new(
            "test-master".to_string(),
            config.clone(),
        ));

        let routes = espbrew::cluster::network::create_cluster_routes(config, master);

        let handle = tokio::spawn(async move {
            let (_, server) =
                warp::serve(routes).bind_with_graceful_shutdown(bind_addr, async move {
                    shutdown_clone.wait_true().await;
                });
            server.await;
        });

        // Give server time to start
        tokio::time::sleep(Duration::from_millis(100)).await;

        Self {
            port,
            shutdown,
            _handle: handle,
        }
    }

    /// Get the base URL for this cluster
    fn base_url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    /// Get the WebSocket URL for this cluster
    fn ws_url(&self) -> String {
        format!("ws://127.0.0.1:{}/cluster/ws", self.port)
    }

    /// Shutdown the cluster server
    async fn stop(self) {
        self.shutdown.store(true, Ordering::SeqCst);
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// Trait extension for AtomicBool to support wait_true
trait AtomicBoolExt {
    async fn wait_true(&self);
}

impl AtomicBoolExt for AtomicBool {
    async fn wait_true(&self) {
        while !self.load(Ordering::SeqCst) {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }
}

#[tokio::test]
async fn test_cluster_http_health_endpoint() {
    let cluster = TestCluster::start().await;

    let client = reqwest::Client::new();
    let url = format!("{}/health", cluster.base_url());

    let response = timeout(Duration::from_secs(5), client.get(&url).send())
        .await
        .expect("Health check timed out")
        .expect("Failed to connect to cluster");

    assert_eq!(response.status(), reqwest::StatusCode::OK);

    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["status"], "healthy");
    assert_eq!(body["service"], "espbrew-cluster");
    assert!(body["version"].is_string());
    assert!(body["timestamp"].is_string());

    cluster.stop().await;
}

#[tokio::test]
async fn test_cluster_http_status_endpoint() {
    let cluster = TestCluster::start().await;

    let client = reqwest::Client::new();
    let url = format!("{}/api/v1/cluster/status", cluster.base_url());

    let response = timeout(Duration::from_secs(5), client.get(&url).send())
        .await
        .expect("Status check timed out")
        .expect("Failed to connect to cluster");

    assert_eq!(response.status(), reqwest::StatusCode::OK);

    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["cluster_name"], "test-cluster");
    assert_eq!(body["node_count"], 0);
    assert_eq!(body["device_count"], 0);

    cluster.stop().await;
}

#[tokio::test]
async fn test_cluster_http_invalid_path_returns_404() {
    let cluster = TestCluster::start().await;

    let client = reqwest::Client::new();
    let url = format!("{}/invalid/path", cluster.base_url());

    let response = client.get(&url).send().await.unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::NOT_FOUND);

    cluster.stop().await;
}

#[tokio::test]
async fn test_cluster_websocket_upgrade() {
    let cluster = TestCluster::start().await;

    let (mut ws_stream, _) = tokio_tungstenite::connect_async(cluster.ws_url())
        .await
        .expect("Failed to connect to WebSocket");

    // Server should send acknowledgment on connection
    let msg = timeout(Duration::from_secs(5), ws_stream.next())
        .await
        .expect("WebSocket acknowledgment timed out")
        .expect("WebSocket stream ended");

    let msg = msg.expect("Failed to receive WebSocket message");

    if let tokio_tungstenite::tungstenite::Message::Text(text) = msg {
        let json: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(json["msg_type"], "StateSync");
        assert_eq!(json["payload"]["status"], "connected");
        assert!(json["payload"]["node_id"].is_string());
    } else {
        panic!("Expected text message, got {:?}", msg);
    }

    cluster.stop().await;
}

#[tokio::test]
async fn test_cluster_websocket_send_heartbeat() {
    let cluster = TestCluster::start().await;

    let (mut ws_stream, _) = tokio_tungstenite::connect_async(cluster.ws_url())
        .await
        .expect("Failed to connect to WebSocket");

    // Wait for acknowledgment
    let _ = timeout(Duration::from_secs(5), ws_stream.next())
        .await
        .expect("WebSocket acknowledgment timed out");

    // Send heartbeat message
    let heartbeat = serde_json::json!({
        "msg_type": "Heartbeat",
        "payload": {
            "node_id": "test-worker",
            "timestamp": chrono::Utc::now().to_rfc3339(),
            "device_count": 3,
            "active_jobs": 1
        }
    });

    ws_stream
        .send(tokio_tungstenite::tungstenite::Message::Text(
            heartbeat.to_string().into(),
        ))
        .await
        .expect("Failed to send heartbeat");

    cluster.stop().await;
}

#[tokio::test]
async fn test_cluster_multiple_clients() {
    let cluster = TestCluster::start().await;

    let url = format!("{}/health", cluster.base_url());
    let client = reqwest::Client::new();

    // Multiple concurrent requests
    let mut handles = Vec::new();
    for _ in 0..5 {
        let client_clone = client.clone();
        let url_clone = url.clone();
        handles.push(tokio::spawn(async move {
            let resp = client_clone.get(&url_clone).send().await.unwrap();
            assert_eq!(resp.status(), reqwest::StatusCode::OK);
        }));
    }

    for handle in handles {
        timeout(Duration::from_secs(5), handle)
            .await
            .expect("Client request timed out")
            .unwrap();
    }

    cluster.stop().await;
}

#[tokio::test]
async fn test_cluster_server_cleanup() {
    let port = portpicker::pick_unused_port().expect("No available ports");

    let shutdown = Arc::new(AtomicBool::new(false));
    let shutdown_clone = shutdown.clone();

    let bind_addr =
        SocketAddr::from_str(&format!("127.0.0.1:{}", port)).expect("Invalid bind address");
    let bind_addr_str = bind_addr.to_string();
    let config = espbrew::cluster::state::ClusterConfig {
        cluster_name: "test-cluster-cleanup".to_string(),
        bind_address: bind_addr_str.clone(),
        ..Default::default()
    };

    let master = Arc::new(espbrew::cluster::master::MasterNode::new(
        "test-master-cleanup".to_string(),
        config.clone(),
    ));

    let routes = espbrew::cluster::network::create_cluster_routes(config, master);

    let handle = tokio::spawn(async move {
        let (_, server) = warp::serve(routes).bind_with_graceful_shutdown(bind_addr, async move {
            shutdown_clone.wait_true().await;
        });
        server.await;
    });

    // Wait for server to start
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Verify server is running
    let client = reqwest::Client::new();
    let url = format!("http://127.0.0.1:{}/health", port);
    let resp = client.get(&url).send().await.unwrap();
    assert_eq!(resp.status(), reqwest::StatusCode::OK);

    // Shutdown server
    shutdown.store(true, Ordering::SeqCst);
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Verify server is no longer accessible
    let result = timeout(Duration::from_secs(1), client.get(&url).send()).await;

    // Connection should fail or timeout after shutdown
    assert!(result.is_err() || result.unwrap().is_err());

    // Clean up task
    handle.abort();
}
