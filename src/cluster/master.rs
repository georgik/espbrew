//! Cluster master coordinator
//!
//! Master node coordinates cluster, distributes jobs, aggregates state

use crate::cluster::messaging::*;
use crate::cluster::network::ClusterMessage;
use crate::cluster::reservation::DevicePool;
use crate::cluster::state::{ClusterConfig, NodeRole, SharedClusterState, new_shared_state};
use anyhow::Result;
use chrono::Utc;
use log::{debug, info, warn};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use uuid::Uuid;

/// Master coordinator
#[derive(Debug)]
pub struct MasterNode {
    /// Node identifier
    node_id: String,
    /// Cluster configuration
    config: ClusterConfig,
    /// Shared cluster state
    state: SharedClusterState,
    /// Job queue
    job_queue: Vec<QueuedJob>,
    /// Active jobs being executed
    running_jobs: HashMap<JobId, RunningJob>,
    /// Next job ID counter
    #[allow(dead_code)]
    next_job_id: u64,
    /// Connected worker senders (mpsc channels)
    worker_senders: Arc<RwLock<HashMap<String, mpsc::UnboundedSender<ClusterMessage>>>>,
    /// Device reservation pool
    device_pool: Arc<RwLock<DevicePool>>,
}

/// Queued job waiting for assignment
#[derive(Debug, Clone)]
struct QueuedJob {
    id: JobId,
    command: CommandType,
    selector: DeviceSelector,
    #[allow(dead_code)]
    submitted_at: chrono::DateTime<chrono::Utc>,
}

/// Running job with assigned worker
#[derive(Debug)]
struct RunningJob {
    #[allow(dead_code)]
    id: JobId,
    #[allow(dead_code)]
    command: CommandType,
    #[allow(dead_code)]
    device_id: DeviceId,
    #[allow(dead_code)]
    assigned_node: String,
    #[allow(dead_code)]
    started_at: chrono::DateTime<chrono::Utc>,
}

impl MasterNode {
    /// Create new master node
    pub fn new(node_id: String, config: ClusterConfig) -> Self {
        let state = new_shared_state(config.cluster_name.clone());

        Self {
            node_id,
            config,
            state,
            job_queue: Vec::new(),
            running_jobs: HashMap::new(),
            next_job_id: 1,
            worker_senders: Arc::new(RwLock::new(HashMap::new())),
            device_pool: Arc::new(RwLock::new(DevicePool::new())),
        }
    }

    /// Get device pool reference
    pub fn device_pool(&self) -> Arc<RwLock<DevicePool>> {
        self.device_pool.clone()
    }

    /// Register worker connection
    pub async fn register_worker(&self, node_id: String, sender: mpsc::UnboundedSender<ClusterMessage>) {
        self.worker_senders
            .write()
            .await
            .insert(node_id.clone(), sender);
        info!("Registered worker connection: {}", node_id);
    }

    /// Unregister worker connection
    pub async fn unregister_worker(&self, node_id: &str) {
        self.worker_senders.write().await.remove(node_id);
        info!("Unregistered worker connection: {}", node_id);
    }

    /// Dispatch job to worker
    pub async fn dispatch_job(
        &self,
        job_id: JobId,
        command: CommandType,
        device_id: DeviceId,
        node_id: String,
    ) -> Result<()> {
        let cluster_command = ClusterCommand {
            job_id: job_id.clone(),
            command,
            target_device: DeviceSelector::Specific(device_id),
            timeout_secs: 300,
            timestamp: Utc::now(),
        };

        let senders = self.worker_senders.read().await;
        if let Some(sender) = senders.get(&node_id) {
            let cluster_msg = ClusterMessage {
                msg_type: crate::cluster::network::MessageType::JobAssign,
                payload: serde_json::to_value(cluster_command)?,
            };

            sender
                .send(cluster_msg)
                .map_err(|e| anyhow::anyhow!("Failed to send job to worker: {}", e))?;

            info!("Dispatched job {} to worker {}", job_id, node_id);
            Ok(())
        } else {
            warn!("Worker {} not connected, cannot dispatch job {}", node_id, job_id);
            Err(anyhow::anyhow!("Worker {} not connected", node_id))
        }
    }

    /// Get master node ID
    pub fn node_id(&self) -> &str {
        &self.node_id
    }

    /// Get cluster state
    pub fn state(&self) -> SharedClusterState {
        self.state.clone()
    }

    /// Get cluster configuration
    pub fn config(&self) -> &ClusterConfig {
        &self.config
    }

    /// Handle node joining cluster
    pub async fn handle_node_join(&self, node_info: NodeInfo) -> Result<()> {
        info!("Node {} joining cluster: {}", node_info.node_id, node_info.cluster_name);

        let mut state = self.state.write().await;

        let node_state = crate::cluster::state::NodeState {
            id: node_info.node_id.clone(),
            role: if node_info.role == "master" {
                NodeRole::Master
            } else {
                NodeRole::Worker
            },
            address: node_info.address.clone(),
            last_seen: Utc::now(),
            capabilities: node_info.capabilities.clone(),
            device_count: node_info.device_count,
            active_jobs: 0,
        };

        state.add_node(node_state);
        debug!("Node {} added to cluster state", node_info.node_id);

        Ok(())
    }

    /// Handle node leaving cluster
    pub async fn handle_node_leave(&mut self, node_id: &str) -> Result<()> {
        info!("Node {} leaving cluster", node_id);

        let mut state = self.state.write().await;
        state.remove_node(node_id);

        // Cancel jobs assigned to this node
        let node_id_str = node_id.to_string();
        self.running_jobs.retain(|job_id, job| {
            if job.assigned_node == node_id_str {
                warn!("Cancelling job {} - node {} left", job_id, node_id_str);
                false
            } else {
                true
            }
        });

        Ok(())
    }

    /// Handle device announcement
    pub async fn handle_device_announcement(&self, ann: DeviceAnnouncement) -> Result<()> {
        debug!(
            "Device {} announced on node {}",
            ann.device_id, ann.node_id
        );

        let mut state = self.state.write().await;
        let device_state = crate::cluster::state::DeviceState::from_announcement(ann.clone());
        state.add_device(device_state);

        // Update node device count
        let device_count = state.devices.values().filter(|d| &d.node_id == &ann.node_id).count();
        if let Some(node) = state.nodes.get_mut(&ann.node_id) {
            node.device_count = device_count;
        }

        Ok(())
    }

    /// Handle heartbeat
    pub async fn handle_heartbeat(&self, heartbeat: Heartbeat) -> Result<()> {
        let mut state = self.state.write().await;

        if let Some(node) = state.nodes.get_mut(&heartbeat.node_id) {
            node.last_seen = Utc::now();
            node.device_count = heartbeat.device_count;
            node.active_jobs = heartbeat.active_jobs;
        }

        Ok(())
    }

    /// Submit job to cluster
    pub async fn submit_job(
        &mut self,
        command: CommandType,
        selector: DeviceSelector,
        _timeout_secs: u64,
    ) -> Result<JobId> {
        let job_id = format!("job-{}", Uuid::new_v4());

        let job = QueuedJob {
            id: job_id.clone(),
            command,
            selector,
            submitted_at: Utc::now(),
        };

        self.job_queue.push(job);

        // Try to assign immediately
        self.assign_jobs().await?;

        Ok(job_id)
    }

    /// Assign queued jobs to available devices
    async fn assign_jobs(&mut self) -> Result<()> {
        let state = self.state.read().await;
        let available_devices = state.get_available_devices();
        drop(state);

        let mut pool = self.device_pool.write().await;
        let mut assigned = Vec::new();

        for (idx, job) in self.job_queue.iter().enumerate() {
            // Find matching device
            if let Some(device_id) =
                self.find_device_for_selector(&job.selector, &available_devices).await
            {
                let state = self.state.read().await;
                if let Some(device) = state.devices.get(&device_id) {
                    let node_id = device.node_id.clone();

                    // Reserve device
                    match pool.reserve(device_id.clone(), node_id.clone(), job.id.clone()) {
                        Ok(_) => {
                            // Create running job
                            let running = RunningJob {
                                id: job.id.clone(),
                                command: job.command.clone(),
                                device_id: device_id.clone(),
                                assigned_node: node_id.clone(),
                                started_at: Utc::now(),
                            };

                            self.running_jobs.insert(job.id.clone(), running);
                            assigned.push((
                                idx,
                                job.id.clone(),
                                job.command.clone(),
                                device_id.clone(),
                                node_id,
                            ));

                            // Update device status
                            drop(state);
                            let mut state = self.state.write().await;
                            if let Some(dev) = state.devices.get_mut(&device_id) {
                                dev.status = DeviceStatus::Busy {
                                    job_id: job.id.clone(),
                                };
                            }
                        }
                        Err(e) => {
                            debug!("Could not reserve device {}: {}", device_id, e);
                        }
                    }
                }
            }
        }
        drop(pool);

        // Remove assigned jobs from queue (in reverse order)
        for (idx, job_id, command, device_id, node_id) in assigned.into_iter().rev() {
            self.job_queue.remove(idx);

            // Dispatch to worker
            if let Err(e) = self
                .dispatch_job(job_id.clone(), command, device_id, node_id)
                .await
            {
                warn!("Failed to dispatch job {}: {}", job_id, e);
            }
        }

        Ok(())
    }

    /// Find device matching selector (pub for testing)
    pub async fn find_device_for_selector(
        &self,
        selector: &DeviceSelector,
        available: &[DeviceId],
    ) -> Option<DeviceId> {
        let state = self.state.read().await;

        match selector {
            DeviceSelector::Any => available.first().cloned(),
            DeviceSelector::Specific(id) => {
                if available.contains(id) {
                    Some(id.clone())
                } else {
                    None
                }
            }
            DeviceSelector::ByType(board_type) => available
                .iter()
                .find(|id| {
                    state
                        .devices
                        .get(*id)
                        .map(|d| &d.board_type == board_type)
                        .unwrap_or(false)
                })
                .cloned(),
            DeviceSelector::ByBackend(backend) => available
                .iter()
                .find(|id| {
                    state
                        .devices
                        .get(*id)
                        .map(|d| d.backend.as_str() == *backend)
                        .unwrap_or(false)
                })
                .cloned(),
            DeviceSelector::ByNode(node_id) => available
                .iter()
                .find(|id| {
                    state
                        .devices
                        .get(*id)
                        .map(|d| &d.node_id == node_id)
                        .unwrap_or(false)
                })
                .cloned(),
            DeviceSelector::ByMacPrefix(prefix) => available
                .iter()
                .find(|id| id.starts_with(prefix))
                .cloned(),
            DeviceSelector::ByName(name) => available
                .iter()
                .find(|id| {
                    state
                        .devices
                        .get(*id)
                        .map(|d| d.logical_name.as_ref().map(|n| n == name).unwrap_or(false))
                        .unwrap_or(false)
                })
                .cloned(),
        }
    }

    /// Handle job completion
    pub async fn handle_job_complete(&self, job_id: &JobId, result: JobResult) -> Result<()> {
        info!("Job {} completed: {}", job_id, result.message);

        // Get device ID from running job before releasing
        let device_id = self
            .running_jobs
            .get(job_id)
            .map(|j| j.device_id.clone());

        let mut state = self.state.write().await;

        // Update device status
        for (_, device) in state.devices.iter_mut() {
            if matches!(&device.status, DeviceStatus::Busy { job_id: id } if id == job_id) {
                device.status = DeviceStatus::Available;
            }
        }

        // Update node job count
        if let Some(running) = self.running_jobs.get(job_id) {
            if let Some(node) = state.nodes.get_mut(&running.assigned_node) {
                node.active_jobs = node.active_jobs.saturating_sub(1);
            }
        }

        // Store result in state
        if let Some(job) = state.jobs.get_mut(job_id) {
            job.result = Some(result.clone());
            job.status = if result.success {
                crate::cluster::state::JobStatus::Completed
            } else {
                crate::cluster::state::JobStatus::Failed
            };
            job.completed_at = Some(Utc::now());
        }
        drop(state);

        // Release device reservation
        if let Some(device_id) = device_id {
            let mut pool = self.device_pool.write().await;
            let _ = pool.release(&device_id);
            debug!("Released device {} reservation for job {}", device_id, job_id);
        }

        Ok(())
    }

    /// Clean up expired nodes
    pub async fn cleanup_expired_nodes(&self) -> Result<usize> {
        let mut state = self.state.write().await;
        let timeout_secs = self.config.node_timeout;

        let expired: Vec<_> = state
            .nodes
            .iter()
            .filter(|(_, n)| n.is_expired(timeout_secs) && n.role != NodeRole::Master)
            .map(|(id, _)| id.clone())
            .collect();

        let count = expired.len();
        for node_id in &expired {
            warn!("Node {} expired, removing from cluster", node_id);

            // Release all reservations for this node
            let mut pool = self.device_pool.write().await;
            let released = pool.release_node(node_id);
            debug!("Released {} reservations for expired node {}", released.len(), node_id);
            drop(pool);

            state.remove_node(node_id);
        }

        Ok(count)
    }

    /// Clean up expired reservations
    pub async fn cleanup_expired_reservations(&self) -> Result<usize> {
        let mut pool = self.device_pool.write().await;
        let expired = pool.cleanup_expired();
        let count = expired.len();

        for device_id in &expired {
            debug!("Expired reservation for device {}", device_id);
        }

        Ok(count)
    }

    /// Get cluster status summary
    pub async fn status_summary(&self) -> ClusterStatus {
        let state = self.state.read().await;
        let pool = self.device_pool.read().await;

        ClusterStatus {
            cluster_name: state.cluster_name.clone(),
            node_count: state.nodes.len(),
            device_count: state.devices.len(),
            available_devices: state.available_devices(),
            queued_jobs: self.job_queue.len(),
            running_jobs: self.running_jobs.len(),
            reserved_devices: pool.active_count(),
        }
    }
}

/// Cluster status summary
#[derive(Debug, Clone, serde::Serialize)]
pub struct ClusterStatus {
    pub cluster_name: String,
    pub node_count: usize,
    pub device_count: usize,
    pub available_devices: usize,
    pub queued_jobs: usize,
    pub running_jobs: usize,
    pub reserved_devices: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cluster::backends::BackendType;
    use crate::cluster::state::NodeState;

    #[test]
    fn test_master_creation() {
        let config = ClusterConfig {
            cluster_name: "test-cluster".to_string(),
            ..Default::default()
        };

        let master = MasterNode::new("master-1".to_string(), config);

        assert_eq!(master.node_id(), "master-1");
        assert_eq!(master.config().cluster_name, "test-cluster");
    }

    #[tokio::test]
    async fn test_master_node_join() {
        let config = ClusterConfig {
            cluster_name: "test-cluster".to_string(),
            ..Default::default()
        };

        let master = MasterNode::new("master-1".to_string(), config);

        let node_info = NodeInfo {
            node_id: "worker-1".to_string(),
            cluster_name: "test-cluster".to_string(),
            role: "worker".to_string(),
            address: "192.168.1.100:8081".to_string(),
            capabilities: vec!["flash".to_string()],
            device_count: 2,
            version: "0.1.0".to_string(),
        };

        master.handle_node_join(node_info).await.unwrap();

        {
            let state = master.state();
            let guard = state.read().await;
            assert_eq!(guard.nodes.len(), 1);
            assert!(guard.nodes.contains_key("worker-1"));
        }
    }

    #[tokio::test]
    async fn test_master_node_leave() {
        let config = ClusterConfig {
            cluster_name: "test-cluster".to_string(),
            ..Default::default()
        };

        let mut master = MasterNode::new("master-1".to_string(), config);

        // Add a node first
        let node_info = NodeInfo {
            node_id: "worker-1".to_string(),
            cluster_name: "test-cluster".to_string(),
            role: "worker".to_string(),
            address: "192.168.1.100:8081".to_string(),
            capabilities: vec!["flash".to_string()],
            device_count: 2,
            version: "0.1.0".to_string(),
        };

        master.handle_node_join(node_info).await.unwrap();
        assert_eq!(master.state().read().await.nodes.len(), 1);

        // Remove the node
        master.handle_node_leave("worker-1").await.unwrap();
        assert_eq!(master.state().read().await.nodes.len(), 0);
    }

    #[tokio::test]
    async fn test_master_device_announcement() {
        let config = ClusterConfig {
            cluster_name: "test-cluster".to_string(),
            ..Default::default()
        };

        let master = MasterNode::new("master-1".to_string(), config);

        // Add a worker node first
        let node_info = NodeInfo {
            node_id: "worker-1".to_string(),
            cluster_name: "test-cluster".to_string(),
            role: "worker".to_string(),
            address: "192.168.1.100:8081".to_string(),
            capabilities: vec!["flash".to_string()],
            device_count: 0,
            version: "0.1.0".to_string(),
        };

        master.handle_node_join(node_info).await.unwrap();

        // Announce a device
        let ann = DeviceAnnouncement {
            device_id: "device-1".to_string(),
            node_id: "worker-1".to_string(),
            backend: BackendType::USB {
                port: "/dev/ttyUSB0".to_string(),
            },
            board_type: "esp32s3".to_string(),
            capabilities: vec!["flash".to_string()],
            location: "/dev/ttyUSB0".to_string(),
            logical_name: None,
        };

        master.handle_device_announcement(ann).await.unwrap();

        {
            let state = master.state();
            let guard = state.read().await;
            assert_eq!(guard.devices.len(), 1);
            assert_eq!(guard.nodes.get("worker-1").unwrap().device_count, 1);
        }
    }

    #[tokio::test]
    async fn test_master_heartbeat() {
        let config = ClusterConfig {
            cluster_name: "test-cluster".to_string(),
            ..Default::default()
        };

        let master = MasterNode::new("master-1".to_string(), config);

        // Add node first (heartbeat only updates existing nodes)
        let node_info = NodeInfo {
            node_id: "worker-1".to_string(),
            cluster_name: "test-cluster".to_string(),
            role: "worker".to_string(),
            address: "192.168.1.100:8081".to_string(),
            capabilities: vec!["flash".to_string()],
            device_count: 0,
            version: "0.1.0".to_string(),
        };

        master.handle_node_join(node_info).await.unwrap();

        let heartbeat = Heartbeat {
            node_id: "worker-1".to_string(),
            timestamp: Utc::now(),
            device_count: 5,
            active_jobs: 2,
        };

        master.handle_heartbeat(heartbeat).await.unwrap();

        {
            let state = master.state();
            let guard = state.read().await;
            let node = guard.nodes.get("worker-1").unwrap();
            assert_eq!(node.device_count, 5);
            assert_eq!(node.active_jobs, 2);
        }
    }

    #[tokio::test]
    async fn test_master_status_summary() {
        let config = ClusterConfig {
            cluster_name: "test-cluster".to_string(),
            ..Default::default()
        };

        let master = MasterNode::new("master-1".to_string(), config);

        let status = master.status_summary().await;

        assert_eq!(status.cluster_name, "test-cluster");
        assert_eq!(status.node_count, 0); // Master no longer auto-registers
        assert_eq!(status.device_count, 0);
        assert_eq!(status.available_devices, 0);
    }

    #[tokio::test]
    async fn test_master_cleanup_expired_nodes() {
        let config = ClusterConfig {
            cluster_name: "test-cluster".to_string(),
            node_timeout: 30,
            ..Default::default()
        };

        let master = MasterNode::new("master-1".to_string(), config);

        // Add a node with old timestamp
        {
            let state_lock = master.state();
            let mut state = state_lock.write().await;
            let mut node = NodeState::new(
                "expired-node".to_string(),
                NodeRole::Worker,
                "192.168.1.100:8081".to_string(),
            );
            node.last_seen = Utc::now() - chrono::Duration::seconds(60);
            state.add_node(node);
        }

        // Cleanup should remove the expired node
        let count = master.cleanup_expired_nodes().await.unwrap();
        assert_eq!(count, 1);

        {
            let state_lock = master.state();
            let state = state_lock.read().await;
            assert!(!state.nodes.contains_key("expired-node"));
        }
    }

    #[tokio::test]
    async fn test_find_device_by_selector() {
        let config = ClusterConfig {
            cluster_name: "test-cluster".to_string(),
            ..Default::default()
        };

        let master = MasterNode::new("master-1".to_string(), config);

        // Add some devices
        let available = vec!["device-1".to_string(), "device-2".to_string()];

        // Find by specific device
        let result = master
            .find_device_for_selector(&DeviceSelector::Specific("device-1".to_string()), &available)
            .await;
        assert_eq!(result, Some("device-1".to_string()));

        // Find by any
        let result = master
            .find_device_for_selector(&DeviceSelector::Any, &available)
            .await;
        assert!(result.is_some());
    }
}

