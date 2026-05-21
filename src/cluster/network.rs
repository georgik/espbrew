//! Cluster network communication via WebSocket
//!
//! Handles WebSocket connections between master and worker nodes

use crate::cluster::messaging::*;
use crate::cluster::state::ClusterConfig;
use anyhow::Result;
use futures_util::{SinkExt, StreamExt};
use log::{debug, info, warn};
use serde_json::json;
use warp::Filter;
use warp::ws::{Message, WebSocket};

/// WebSocket message wrapper for cluster communication
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ClusterMessage {
    pub msg_type: MessageType,
    pub payload: serde_json::Value,
}

/// Message types for cluster communication
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum MessageType {
    // Node management
    NodeJoin,
    NodeLeave,
    Heartbeat,

    // Device management
    DeviceAnnounce,
    DeviceUpdate,

    // Job management
    JobAssign,
    JobProgress,
    JobComplete,
    JobFailed,

    // State sync
    StateSync,
    StateRequest,
}

/// Cluster WebSocket client
pub struct ClusterClient {
    node_id: String,
    #[allow(dead_code)]
    cluster_name: String,
    master_addr: String,
}

impl ClusterClient {
    pub fn new(node_id: String, cluster_name: String, master_addr: String) -> Self {
        Self {
            node_id,
            cluster_name,
            master_addr,
        }
    }

    /// Connect to master and handle messages
    pub async fn connect_and_run<F>(&self, _handler: F) -> Result<()>
    where
        F: FnMut(ClusterMessage) -> anyhow::Result<Option<ClusterMessage>>,
    {
        let url = format!("ws://{}/cluster/ws", self.master_addr);
        info!("Connecting to cluster master at {}", url);

        // TODO: Implement WebSocket client connection
        // For now, this is a placeholder for the future implementation
        warn!("WebSocket client not yet implemented - placeholder");

        Ok(())
    }

    /// Send node join message
    pub fn send_node_join(&self, node_info: NodeInfo) -> ClusterMessage {
        ClusterMessage {
            msg_type: MessageType::NodeJoin,
            payload: json!(node_info),
        }
    }

    /// Send heartbeat
    pub fn send_heartbeat(&self, device_count: usize, active_jobs: usize) -> ClusterMessage {
        ClusterMessage {
            msg_type: MessageType::Heartbeat,
            payload: json!({
                "node_id": self.node_id,
                "timestamp": chrono::Utc::now().to_rfc3339(),
                "device_count": device_count,
                "active_jobs": active_jobs,
            }),
        }
    }

    /// Send device announcement
    pub fn send_device_announcement(&self, ann: DeviceAnnouncement) -> ClusterMessage {
        ClusterMessage {
            msg_type: MessageType::DeviceAnnounce,
            payload: json!(ann),
        }
    }

    /// Send job progress update
    pub fn send_job_progress(&self, job_id: &str, progress: f32, message: &str) -> ClusterMessage {
        ClusterMessage {
            msg_type: MessageType::JobProgress,
            payload: json!({
                "job_id": job_id,
                "progress": progress,
                "message": message,
                "node_id": self.node_id,
            }),
        }
    }

    /// Send job completion
    pub fn send_job_complete(&self, job_id: &str, result: JobResult) -> ClusterMessage {
        ClusterMessage {
            msg_type: MessageType::JobComplete,
            payload: json!({
                "job_id": job_id,
                "result": result,
                "node_id": self.node_id,
            }),
        }
    }
}

/// Handle WebSocket connection from a worker node
pub async fn handle_worker_connection(
    ws: WebSocket,
    node_id: String,
    _config: ClusterConfig,
    master: Option<std::sync::Arc<crate::cluster::master::MasterNode>>,
) {
    let (mut ws_sender, mut ws_receiver) = ws.split();

    info!("Worker {} connected to cluster", node_id);

    // Create channel for master to send messages to this worker
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<ClusterMessage>();

    // Register worker with master if provided
    if let Some(ref master) = master {
        master.register_worker(node_id.clone(), tx).await;
    }

    // Send connection acknowledgment
    let ack = ClusterMessage {
        msg_type: MessageType::StateSync,
        payload: json!({"status": "connected", "node_id": node_id}),
    };
    if let Ok(ack_json) = serde_json::to_string(&ack) {
        let _ = ws_sender.send(Message::text(ack_json)).await;
    }

    // Handle incoming messages and outgoing messages
    loop {
        tokio::select! {
            // Incoming from worker
            result = ws_receiver.next() => {
                match result {
                    Some(Ok(msg)) => {
                        if msg.is_close() {
                            break;
                        }

                        if let Ok(text) = msg.to_str() {
                            debug!("Received message from worker {}: {}", node_id, text);

                            // Parse and handle cluster message
                            if let Ok(cluster_msg) = serde_json::from_str::<ClusterMessage>(text) {
                                // Handle message inline
                                match cluster_msg.msg_type {
                                    MessageType::NodeJoin => {
                                        if let Ok(node_info) = serde_json::from_value::<NodeInfo>(cluster_msg.payload) {
                                            info!("Node join from {}: {}", node_id, node_info.node_id);
                                        }
                                    }
                                    MessageType::Heartbeat => {
                                        debug!("Heartbeat from {}", node_id);
                                    }
                                    MessageType::DeviceAnnounce => {
                                        if let Ok(ann) = serde_json::from_value::<DeviceAnnouncement>(cluster_msg.payload) {
                                            debug!("Device announcement from {}: {}", node_id, ann.device_id);
                                        }
                                    }
                                    MessageType::JobProgress => {
                                        debug!("Job progress from {}", node_id);
                                    }
                                    MessageType::JobComplete => {
                                        debug!("Job complete from {}", node_id);
                                    }
                                    MessageType::JobFailed => {
                                        warn!("Job failed from {}", node_id);
                                    }
                                    _ => {
                                        debug!("Unhandled message type from worker: {:?}", cluster_msg.msg_type);
                                    }
                                }
                            }
                        }
                    }
                    Some(Err(e)) => {
                        warn!("WebSocket error from {}: {}", node_id, e);
                        break;
                    }
                    None => break,
                }
            }
            // Outgoing from master
            Some(msg) = rx.recv() => {
                if let Ok(json) = serde_json::to_string(&msg) {
                    if ws_sender.send(Message::text(json)).await.is_err() {
                        warn!("Failed to send message to worker {}", node_id);
                        break;
                    }
                }
            }
        }
    }

    info!("Worker {} disconnected", node_id);
}

/// Create health check route
pub fn create_health_route()
-> impl Filter<Extract = (warp::reply::Json,), Error = warp::Rejection> + Clone {
    warp::path("health").and(warp::get()).map(|| {
        warp::reply::json(&json!({
            "status": "healthy",
            "service": "espbrew-cluster",
            "version": crate::CLUSTER_VERSION,
            "timestamp": chrono::Utc::now().to_rfc3339(),
        }))
    })
}

/// Create cluster status API route
pub fn create_status_route(
    master: std::sync::Arc<crate::cluster::master::MasterNode>,
) -> impl Filter<Extract = (warp::reply::Json,), Error = warp::Rejection> + Clone {
    let master_clone = master.clone();
    warp::path!("api" / "v1" / "cluster" / "status")
        .and(warp::get())
        .and(warp::any().map(move || master_clone.clone()))
        .and_then(
            |master: std::sync::Arc<crate::cluster::master::MasterNode>| async move {
                let status = master.status_summary().await;
                Ok::<_, warp::Rejection>(warp::reply::json(&status))
            },
        )
}

/// Create WebSocket upgrade handler for cluster workers
pub fn create_cluster_ws_route(
    config: ClusterConfig,
    master: Option<std::sync::Arc<crate::cluster::master::MasterNode>>,
) -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
    warp::path("cluster")
        .and(warp::path("ws"))
        .and(warp::ws())
        .map(move |ws: warp::ws::Ws| {
            let config = config.clone();
            let master = master.clone();
            ws.on_upgrade(move |websocket| {
                let node_id = format!("worker-{}", uuid::Uuid::new_v4());
                async move { handle_worker_connection(websocket, node_id, config, master).await }
            })
        })
}

/// Create all cluster HTTP routes
pub fn create_cluster_routes(
    config: ClusterConfig,
    master: std::sync::Arc<crate::cluster::master::MasterNode>,
) -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
    let health = create_health_route();
    let status = create_status_route(master.clone());
    let ws = create_cluster_ws_route(config, Some(master));

    health.or(status).or(ws)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cluster_message_serialization() {
        let msg = ClusterMessage {
            msg_type: MessageType::Heartbeat,
            payload: json!({"node_id": "test-node"}),
        };

        let serialized = serde_json::to_string(&msg).unwrap();
        let deserialized: ClusterMessage = serde_json::from_str(&serialized).unwrap();

        assert!(matches!(deserialized.msg_type, MessageType::Heartbeat));
    }

    #[test]
    fn test_client_send_heartbeat() {
        let client = ClusterClient::new(
            "node-1".to_string(),
            "test-cluster".to_string(),
            "localhost:8081".to_string(),
        );

        let msg = client.send_heartbeat(5, 2);
        assert!(matches!(msg.msg_type, MessageType::Heartbeat));

        let payload = msg.payload;
        assert_eq!(payload["node_id"], "node-1");
        assert_eq!(payload["device_count"], 5);
        assert_eq!(payload["active_jobs"], 2);
    }

    #[test]
    fn test_message_type_variants() {
        // Test that all message types can be serialized
        let types = vec![
            MessageType::NodeJoin,
            MessageType::NodeLeave,
            MessageType::Heartbeat,
            MessageType::DeviceAnnounce,
            MessageType::DeviceUpdate,
            MessageType::JobAssign,
            MessageType::JobProgress,
            MessageType::JobComplete,
            MessageType::JobFailed,
            MessageType::StateSync,
            MessageType::StateRequest,
        ];

        for msg_type in types {
            let serialized = serde_json::to_string(&msg_type).unwrap();
            let _: MessageType = serde_json::from_str(&serialized).unwrap();
        }
    }

    #[test]
    fn test_client_send_device_announcement() {
        let client = ClusterClient::new(
            "node-1".to_string(),
            "test-cluster".to_string(),
            "localhost:8081".to_string(),
        );

        let ann = DeviceAnnouncement {
            device_id: "device-1".to_string(),
            node_id: "node-1".to_string(),
            backend: crate::cluster::backends::BackendType::USB {
                port: "/dev/ttyUSB0".to_string(),
            },
            board_type: "esp32s3".to_string(),
            capabilities: vec!["flash".to_string()],
            location: "/dev/ttyUSB0".to_string(),
            logical_name: None,
        };

        let msg = client.send_device_announcement(ann);
        assert!(matches!(msg.msg_type, MessageType::DeviceAnnounce));
    }
}
