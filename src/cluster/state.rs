//! Cluster state management
//!
//! Centralized state for cluster coordination

use crate::cluster::backends::BackendType;
use crate::cluster::messaging::*;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Node role in cluster
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeRole {
    /// Master coordinator
    Master,
    /// Worker node
    Worker,
    /// Auto-determine (first node becomes master)
    Auto,
}

impl NodeRole {
    pub fn as_str(&self) -> &str {
        match self {
            NodeRole::Master => "master",
            NodeRole::Worker => "worker",
            NodeRole::Auto => "auto",
        }
    }
}

/// Cluster configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterConfig {
    /// Cluster name for isolation
    pub cluster_name: String,
    /// Node role
    pub role: NodeRole,
    /// Bind address for WebSocket
    pub bind_address: String,
    /// Heartbeat interval in seconds
    pub heartbeat_interval: u64,
    /// Node timeout in seconds
    pub node_timeout: u64,
}

impl Default for ClusterConfig {
    fn default() -> Self {
        Self {
            cluster_name: crate::cluster::DEFAULT_CLUSTER_NAME.to_string(),
            role: NodeRole::Auto,
            bind_address: "0.0.0.0:8081".to_string(),
            heartbeat_interval: crate::cluster::DEFAULT_HEARTBEAT_INTERVAL,
            node_timeout: crate::cluster::DEFAULT_NODE_TIMEOUT,
        }
    }
}

/// Cluster state (managed by master)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterState {
    pub cluster_name: String,
    pub nodes: HashMap<NodeId, NodeState>,
    pub devices: HashMap<DeviceId, DeviceState>,
    pub jobs: HashMap<JobId, JobState>,
    pub created_at: DateTime<Utc>,
}

impl ClusterState {
    pub fn new(cluster_name: String) -> Self {
        Self {
            cluster_name,
            nodes: HashMap::new(),
            devices: HashMap::new(),
            jobs: HashMap::new(),
            created_at: Utc::now(),
        }
    }

    pub fn add_node(&mut self, node: NodeState) {
        self.nodes.insert(node.id.clone(), node);
    }

    pub fn remove_node(&mut self, node_id: &str) {
        self.nodes.remove(node_id);
        // Remove devices from this node
        self.devices.retain(|_, d| d.node_id != node_id);
    }

    pub fn add_device(&mut self, device: DeviceState) {
        self.devices.insert(device.id.clone(), device);
    }

    pub fn remove_device(&mut self, device_id: &str) {
        self.devices.remove(device_id);
    }

    pub fn add_job(&mut self, job: JobState) {
        self.jobs.insert(job.id.clone(), job);
    }

    pub fn get_available_devices(&self) -> Vec<DeviceId> {
        self.devices
            .iter()
            .filter(|(_, d)| d.status == DeviceStatus::Available)
            .map(|(id, _)| id.clone())
            .collect()
    }

    pub fn total_devices(&self) -> usize {
        self.devices.len()
    }

    pub fn available_devices(&self) -> usize {
        self.devices.values().filter(|d| d.status == DeviceStatus::Available).count()
    }
}

/// Node state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeState {
    pub id: NodeId,
    pub role: NodeRole,
    pub address: String,
    pub last_seen: DateTime<Utc>,
    pub capabilities: Vec<String>,
    pub device_count: usize,
    pub active_jobs: usize,
}

impl NodeState {
    pub fn new(id: NodeId, role: NodeRole, address: String) -> Self {
        Self {
            id,
            role,
            address,
            last_seen: Utc::now(),
            capabilities: Vec::new(),
            device_count: 0,
            active_jobs: 0,
        }
    }

    pub fn is_expired(&self, timeout_secs: u64) -> bool {
        let elapsed = Utc::now().signed_duration_since(self.last_seen);
        elapsed.num_seconds() > timeout_secs as i64
    }
}

/// Device state in cluster
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceState {
    pub id: DeviceId,
    pub node_id: NodeId,
    pub backend: BackendType,
    pub board_type: String,
    pub status: DeviceStatus,
    pub capabilities: Vec<String>,
    pub location: String,
    pub logical_name: Option<String>,
    pub last_seen: DateTime<Utc>,
}

impl DeviceState {
    pub fn from_announcement(ann: DeviceAnnouncement) -> Self {
        Self {
            id: ann.device_id,
            node_id: ann.node_id,
            backend: ann.backend,
            board_type: ann.board_type,
            status: DeviceStatus::Available,
            capabilities: ann.capabilities,
            location: ann.location,
            logical_name: ann.logical_name,
            last_seen: Utc::now(),
        }
    }
}

/// Job state in cluster
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobState {
    pub id: JobId,
    pub command: CommandType,
    pub target_device: DeviceId,
    pub assigned_node: NodeId,
    pub status: JobStatus,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub result: Option<JobResult>,
}

/// Job status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum JobStatus {
    /// Job queued
    Queued,
    /// Job assigned to worker
    Assigned,
    /// Job in progress
    InProgress,
    /// Job completed
    Completed,
    /// Job failed
    Failed,
    /// Job cancelled
    Cancelled,
}

/// Shared cluster state wrapper
pub type SharedClusterState = Arc<RwLock<ClusterState>>;

/// Create new shared cluster state
pub fn new_shared_state(cluster_name: String) -> SharedClusterState {
    Arc::new(RwLock::new(ClusterState::new(cluster_name)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cluster::backends::BackendType;
    use chrono::Utc;

    #[test]
    fn test_cluster_state_creation() {
        let state = ClusterState::new("test-cluster".to_string());
        assert_eq!(state.cluster_name, "test-cluster");
        assert_eq!(state.nodes.len(), 0);
        assert_eq!(state.devices.len(), 0);
        assert_eq!(state.jobs.len(), 0);
    }

    #[test]
    fn test_add_remove_node() {
        let mut state = ClusterState::new("test-cluster".to_string());

        let node = NodeState::new(
            "node-1".to_string(),
            NodeRole::Worker,
            "192.168.1.100:8081".to_string(),
        );

        state.add_node(node.clone());
        assert_eq!(state.nodes.len(), 1);
        assert!(state.nodes.contains_key("node-1"));

        state.remove_node("node-1");
        assert_eq!(state.nodes.len(), 0);
    }

    #[test]
    fn test_add_remove_device() {
        let mut state = ClusterState::new("test-cluster".to_string());

        let device = DeviceState {
            id: "device-1".to_string(),
            node_id: "node-1".to_string(),
            backend: BackendType::USB {
                port: "/dev/ttyUSB0".to_string(),
            },
            board_type: "esp32s3".to_string(),
            status: DeviceStatus::Available,
            capabilities: vec!["flash".to_string()],
            location: "/dev/ttyUSB0".to_string(),
            logical_name: None,
            last_seen: Utc::now(),
        };

        state.add_device(device);
        assert_eq!(state.devices.len(), 1);
        assert_eq!(state.total_devices(), 1);
        assert_eq!(state.available_devices(), 1);

        state.remove_device("device-1");
        assert_eq!(state.devices.len(), 0);
    }

    #[test]
    fn test_get_available_devices() {
        let mut state = ClusterState::new("test-cluster".to_string());

        // Add available device
        state.add_device(DeviceState {
            id: "device-1".to_string(),
            node_id: "node-1".to_string(),
            backend: BackendType::USB {
                port: "/dev/ttyUSB0".to_string(),
            },
            board_type: "esp32s3".to_string(),
            status: DeviceStatus::Available,
            capabilities: vec!["flash".to_string()],
            location: "/dev/ttyUSB0".to_string(),
            logical_name: None,
            last_seen: Utc::now(),
        });

        // Add busy device
        state.add_device(DeviceState {
            id: "device-2".to_string(),
            node_id: "node-1".to_string(),
            backend: BackendType::USB {
                port: "/dev/ttyUSB1".to_string(),
            },
            board_type: "esp32s3".to_string(),
            status: DeviceStatus::Busy {
                job_id: "job-1".to_string(),
            },
            capabilities: vec!["flash".to_string()],
            location: "/dev/ttyUSB1".to_string(),
            logical_name: None,
            last_seen: Utc::now(),
        });

        let available = state.get_available_devices();
        assert_eq!(available.len(), 1);
        assert_eq!(available[0], "device-1");
    }

    #[test]
    fn test_node_state_expiry() {
        let mut node = NodeState::new(
            "node-1".to_string(),
            NodeRole::Worker,
            "192.168.1.100:8081".to_string(),
        );

        // Node should not be expired immediately
        assert!(!node.is_expired(30));

        // Set last_seen to 60 seconds ago
        node.last_seen = Utc::now() - chrono::Duration::seconds(60);

        // Node should be expired with 30 second timeout
        assert!(node.is_expired(30));

        // Node should not be expired with 120 second timeout
        assert!(!node.is_expired(120));
    }

    #[test]
    fn test_device_from_announcement() {
        let ann = DeviceAnnouncement {
            device_id: "device-1".to_string(),
            node_id: "node-1".to_string(),
            backend: BackendType::USB {
                port: "/dev/ttyUSB0".to_string(),
            },
            board_type: "esp32s3".to_string(),
            capabilities: vec!["flash".to_string()],
            location: "/dev/ttyUSB0".to_string(),
            logical_name: Some("dut-1".to_string()),
        };

        let device = DeviceState::from_announcement(ann.clone());

        assert_eq!(device.id, ann.device_id);
        assert_eq!(device.node_id, ann.node_id);
        assert_eq!(device.status, DeviceStatus::Available);
        assert_eq!(device.logical_name, Some("dut-1".to_string()));
    }

    #[test]
    fn test_cluster_config_default() {
        let config = ClusterConfig::default();
        assert_eq!(config.cluster_name, crate::cluster::DEFAULT_CLUSTER_NAME);
        assert_eq!(config.role, NodeRole::Auto);
        assert_eq!(config.heartbeat_interval, crate::cluster::DEFAULT_HEARTBEAT_INTERVAL);
        assert_eq!(config.node_timeout, crate::cluster::DEFAULT_NODE_TIMEOUT);
    }

    #[test]
    fn test_job_state_transitions() {
        let mut job = JobState {
            id: "job-1".to_string(),
            command: CommandType::Reset,
            target_device: "device-1".to_string(),
            assigned_node: "node-1".to_string(),
            status: JobStatus::Queued,
            created_at: Utc::now(),
            started_at: None,
            completed_at: None,
            result: None,
        };

        assert_eq!(job.status, JobStatus::Queued);

        job.status = JobStatus::Assigned;
        job.started_at = Some(Utc::now());
        assert_eq!(job.status, JobStatus::Assigned);

        job.status = JobStatus::Completed;
        job.completed_at = Some(Utc::now());
        assert_eq!(job.status, JobStatus::Completed);
    }

    #[tokio::test]
    async fn test_shared_state_concurrent_access() {
        let state = new_shared_state("test-cluster".to_string());

        // Spawn multiple tasks to write to state
        let handles: Vec<_> = (0..10)
            .map(|i| {
                let state = state.clone();
                tokio::spawn(async move {
                    let mut s = state.write().await;
                    s.nodes.insert(
                        format!("node-{}", i),
                        NodeState::new(
                            format!("node-{}", i),
                            NodeRole::Worker,
                            format!("192.168.1.{}:8081", 100 + i),
                        ),
                    );
                })
            })
            .collect();

        // Wait for all tasks
        for handle in handles {
            handle.await.unwrap();
        }

        // Verify all nodes were added
        let s = state.read().await;
        assert_eq!(s.nodes.len(), 10);
    }
}

