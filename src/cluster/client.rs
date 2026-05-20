//! Worker WebSocket client for cluster communication
//!
//! Handles connection from worker to master, receiving job assignments
//! and reporting results back.

use crate::cluster::messaging::*;
use crate::cluster::network::ClusterMessage;
use crate::cluster::worker::WorkerNode;
use anyhow::Result;
use futures_util::{SinkExt, StreamExt};
use log::{debug, error, info, warn};
use std::time::Duration;
use tokio_tungstenite::{connect_async, tungstenite::Message as WsMessage};

/// Worker WebSocket client
pub struct WorkerClient {
    /// Worker node instance
    worker: WorkerNode,
    /// Master address
    master_addr: String,
    /// Reconnect configuration
    max_retries: u32,
    retry_delay: Duration,
}

impl WorkerClient {
    /// Create new worker client
    pub fn new(worker: WorkerNode, master_addr: String) -> Self {
        Self {
            worker,
            master_addr,
            max_retries: 5,
            retry_delay: Duration::from_secs(2),
        }
    }

    /// Connect and run the worker client
    pub async fn connect_and_run(mut self) -> Result<()> {
        let mut retries = 0;
        let url = format!("ws://{}/cluster/ws", self.master_addr);

        while retries < self.max_retries {
            info!(
                "Worker {} connecting to master at {}",
                self.worker.node_id(),
                url
            );

            match connect_async(&url).await {
                Ok((ws_stream, _)) => {
                    info!("Worker {} connected to master", self.worker.node_id());
                    retries = 0;

                    let result = self.run_connection(ws_stream).await;

                    if let Err(e) = result {
                        error!("Connection error: {}", e);
                    }

                    info!("Connection closed, reconnecting in {:?}", self.retry_delay);
                }
                Err(e) => {
                    warn!("Failed to connect to master: {}", e);
                    retries += 1;
                    if retries >= self.max_retries {
                        return Err(anyhow::anyhow!(
                            "Max retries ({}) reached",
                            self.max_retries
                        ));
                    }
                }
            }

            tokio::time::sleep(self.retry_delay).await;
        }

        Ok(())
    }

    /// Run the WebSocket connection
    async fn run_connection(
        &mut self,
        ws_stream: tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
    ) -> Result<()> {
        let (mut ws_sender, mut ws_receiver) = ws_stream.split();

        // Send node join message
        let join_msg = self.send_node_join()?;
        self.send_message(&mut ws_sender, &join_msg).await?;

        // Send device announcements
        for ann_msg in self.send_device_announcements() {
            self.send_message(&mut ws_sender, &ann_msg).await?;
        }

        // Handle incoming messages
        while let Some(msg_result) = ws_receiver.next().await {
            match msg_result {
                Ok(ws_msg) => {
                    if ws_msg.is_close() {
                        info!("Master initiated close");
                        break;
                    }

                    if let Ok(text) = ws_msg.to_text() {
                        if let Ok(cluster_msg) = serde_json::from_str::<ClusterMessage>(text) {
                            if let Err(e) = self
                                .handle_cluster_message(&mut ws_sender, cluster_msg)
                                .await
                            {
                                error!("Error handling message: {}", e);
                            }
                        }
                    }
                }
                Err(e) => {
                    error!("WebSocket error: {}", e);
                    break;
                }
            }
        }

        Ok(())
    }

    /// Send a message through the WebSocket
    async fn send_message(
        &mut self,
        sender: &mut futures_util::stream::SplitSink<
            tokio_tungstenite::WebSocketStream<
                tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
            >,
            WsMessage,
        >,
        msg: &ClusterMessage,
    ) -> Result<()> {
        let json = serde_json::to_string(msg)?;
        sender.send(WsMessage::Text(json.into())).await?;
        Ok(())
    }

    /// Handle incoming cluster message from master
    async fn handle_cluster_message(
        &mut self,
        sender: &mut futures_util::stream::SplitSink<
            tokio_tungstenite::WebSocketStream<
                tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
            >,
            WsMessage,
        >,
        msg: ClusterMessage,
    ) -> Result<()> {
        use crate::cluster::network::MessageType;

        match msg.msg_type {
            MessageType::JobAssign => {
                if let Ok(job) = serde_json::from_value::<ClusterCommand>(msg.payload) {
                    self.handle_job_assignment(sender, job).await?;
                }
            }
            MessageType::Heartbeat => {
                let heartbeat = self.send_heartbeat();
                self.send_message(sender, &heartbeat).await?;
            }
            MessageType::StateSync => {
                debug!("State sync requested by master");
            }
            _ => {
                debug!("Unhandled message type: {:?}", msg.msg_type);
            }
        }

        Ok(())
    }

    /// Handle incoming job assignment
    async fn handle_job_assignment(
        &mut self,
        sender: &mut futures_util::stream::SplitSink<
            tokio_tungstenite::WebSocketStream<
                tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
            >,
            WsMessage,
        >,
        job: ClusterCommand,
    ) -> Result<()> {
        info!(
            "Received job assignment: {} for device {:?}",
            job.job_id, job.target_device
        );

        // Resolve device ID from selector
        let device_id = match &job.target_device {
            crate::cluster::messaging::DeviceSelector::Specific(id) => id.clone(),
            crate::cluster::messaging::DeviceSelector::Any => {
                // Get first available device
                match self.worker.get_available_device() {
                    Some(id) => id,
                    None => {
                        let failed = ClusterMessage {
                            msg_type: crate::cluster::network::MessageType::JobFailed,
                            payload: serde_json::json!({
                                "job_id": job.job_id,
                                "error": "No available devices",
                                "node_id": self.worker.node_id(),
                            }),
                        };
                        self.send_message(sender, &failed).await?;
                        return Ok(());
                    }
                }
            }
            _ => {
                let failed = ClusterMessage {
                    msg_type: crate::cluster::network::MessageType::JobFailed,
                    payload: serde_json::json!({
                        "job_id": job.job_id,
                        "error": "Unsupported device selector",
                        "node_id": self.worker.node_id(),
                    }),
                };
                self.send_message(sender, &failed).await?;
                return Ok(());
            }
        };

        // Send initial progress
        let progress = ClusterMessage {
            msg_type: crate::cluster::network::MessageType::JobProgress,
            payload: serde_json::json!({
                "job_id": job.job_id,
                "progress": 0.0,
                "message": "Starting job execution",
                "node_id": self.worker.node_id(),
                "device_id": device_id,
            }),
        };
        self.send_message(sender, &progress).await?;

        // Execute the command
        let result = self
            .worker
            .execute_command(job.job_id.clone(), job.command.clone(), device_id.clone())
            .await;

        match result {
            Ok(job_result) => {
                let msg_type = if job_result.success {
                    crate::cluster::network::MessageType::JobComplete
                } else {
                    crate::cluster::network::MessageType::JobFailed
                };

                let response = ClusterMessage {
                    msg_type,
                    payload: serde_json::json!({
                        "job_id": job.job_id,
                        "result": job_result,
                        "node_id": self.worker.node_id(),
                        "device_id": device_id,
                    }),
                };
                self.send_message(sender, &response).await?;
            }
            Err(e) => {
                let failed = ClusterMessage {
                    msg_type: crate::cluster::network::MessageType::JobFailed,
                    payload: serde_json::json!({
                        "job_id": job.job_id,
                        "error": e.to_string(),
                        "node_id": self.worker.node_id(),
                        "device_id": device_id,
                    }),
                };
                self.send_message(sender, &failed).await?;
            }
        }

        Ok(())
    }

    /// Create node join message
    fn send_node_join(&self) -> Result<ClusterMessage> {
        let node_info = NodeInfo {
            node_id: self.worker.node_id().to_string(),
            cluster_name: "espbrew".to_string(),
            role: "worker".to_string(),
            address: format!("{}:8081", hostname::get()?.to_string_lossy()),
            capabilities: self.worker.capabilities().to_vec(),
            device_count: self.worker.device_count(),
            version: crate::cluster::CLUSTER_VERSION.to_string(),
        };

        Ok(ClusterMessage {
            msg_type: crate::cluster::network::MessageType::NodeJoin,
            payload: serde_json::to_value(node_info)?,
        })
    }

    /// Send device announcements to master
    fn send_device_announcements(&self) -> Vec<ClusterMessage> {
        self.worker
            .device_announcements()
            .into_iter()
            .map(|ann| ClusterMessage {
                msg_type: crate::cluster::network::MessageType::DeviceAnnounce,
                payload: serde_json::json!(ann),
            })
            .collect()
    }

    /// Send heartbeat to master
    fn send_heartbeat(&self) -> ClusterMessage {
        ClusterMessage {
            msg_type: crate::cluster::network::MessageType::Heartbeat,
            payload: serde_json::json!({
                "node_id": self.worker.node_id(),
                "timestamp": chrono::Utc::now().to_rfc3339(),
                "device_count": self.worker.device_count(),
                "active_jobs": self.worker.active_job_count(),
            }),
        }
    }
}

/// Simpler worker connector for HTTP-based job polling
/// (Used until full WebSocket client is implemented)
pub struct WorkerPoller {
    #[allow(dead_code)]
    worker: WorkerNode,
    #[allow(dead_code)]
    master_addr: String,
    #[allow(dead_code)]
    poll_interval: Duration,
}

impl WorkerPoller {
    pub fn new(worker: WorkerNode, master_addr: String) -> Self {
        Self {
            worker,
            master_addr,
            poll_interval: Duration::from_secs(5),
        }
    }

    /// Run the poller
    pub async fn run(&self) -> Result<()> {
        info!("Starting job poller for worker {}", self.worker.node_id());

        // TODO: Implement HTTP polling for jobs
        // GET /api/v1/cluster/jobs/pending?node_id={worker.node_id}

        Ok(())
    }

    /// Poll master for pending jobs
    #[allow(dead_code)]
    async fn poll_jobs(&self) -> Result<()> {
        // TODO: Implement HTTP polling for jobs
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cluster::state::ClusterConfig;

    fn create_test_worker() -> WorkerNode {
        let config = ClusterConfig {
            cluster_name: "test-cluster".to_string(),
            ..Default::default()
        };
        WorkerNode::new("worker-test".to_string(), config)
    }

    #[test]
    fn test_worker_client_creation() {
        let worker = create_test_worker();
        let client = WorkerClient::new(worker, "localhost:8081".to_string());

        assert_eq!(client.master_addr, "localhost:8081");
        assert_eq!(client.max_retries, 5);
    }

    #[test]
    fn test_heartbeat_message() {
        let worker = create_test_worker();
        let client = WorkerClient::new(worker, "localhost:8081".to_string());

        let msg = client.send_heartbeat();
        assert_eq!(
            msg.msg_type,
            crate::cluster::network::MessageType::Heartbeat
        );
    }

    #[test]
    fn test_device_announcements() {
        let mut worker = create_test_worker();

        let device = crate::cluster::worker::LocalDevice {
            id: "test-device".to_string(),
            backend: crate::cluster::backends::BackendType::USB {
                port: "/dev/ttyUSB0".to_string(),
            },
            board_type: "esp32s3".to_string(),
            location: "/dev/ttyUSB0".to_string(),
            capabilities: vec!["flash".to_string()],
            logical_name: None,
            status: DeviceStatus::Available,
        };

        worker.add_device(device);

        let client = WorkerClient::new(worker, "localhost:8081".to_string());
        let announcements = client.send_device_announcements();

        assert_eq!(announcements.len(), 1);
        assert_eq!(
            announcements[0].msg_type,
            crate::cluster::network::MessageType::DeviceAnnounce
        );
    }

    #[test]
    fn test_poller_creation() {
        let worker = create_test_worker();
        let poller = WorkerPoller::new(worker, "localhost:8081".to_string());

        assert_eq!(poller.master_addr, "localhost:8081");
        assert_eq!(poller.poll_interval, Duration::from_secs(5));
    }

    #[test]
    fn test_node_join_message() {
        let worker = create_test_worker();
        let client = WorkerClient::new(worker, "localhost:8081".to_string());

        let msg = client.send_node_join();
        assert!(msg.is_ok());
        let msg = msg.unwrap();
        assert_eq!(msg.msg_type, crate::cluster::network::MessageType::NodeJoin);
    }
}
