//! Cluster messaging types
//!
//! Messages exchanged between nodes for cluster coordination

use crate::cluster::backends::BackendType;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Unique node identifier
pub type NodeId = String;

/// Unique device identifier (MAC address or similar)
pub type DeviceId = String;

/// Unique job identifier
pub type JobId = String;

/// Cluster event - node state changes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterEvent {
    pub node_id: NodeId,
    pub cluster_name: String,
    pub event_type: EventType,
    pub timestamp: DateTime<Utc>,
}

/// Event types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EventType {
    /// Node joined cluster
    NodeJoined { role: String },
    /// Node leaving cluster
    NodeLeaving,
    /// Device connected
    DeviceConnected { device: ConnectedDevice },
    /// Device disconnected
    DeviceDisconnected { device_id: DeviceId },
    /// Job started
    JobStarted { job_id: JobId, device_id: DeviceId },
    /// Job completed
    JobCompleted { job_id: JobId, result: JobResult },
    /// Job failed
    JobFailed { job_id: JobId, error: String },
    /// Heartbeat
    Heartbeat,
}

/// Cluster command - job assignment from master
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterCommand {
    pub job_id: JobId,
    pub command: CommandType,
    pub target_device: DeviceSelector,
    pub timeout_secs: u64,
    pub timestamp: DateTime<Utc>,
}

/// Command types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CommandType {
    /// Flash firmware
    Flash { binary_data: Vec<u8> },
    /// Start monitoring
    Monitor { baud_rate: u32 },
    /// Capture image
    Capture { camera_id: Option<String> },
    /// Reset device
    Reset,
    /// Run test
    Test { validation: TestConfig },
}

/// Test configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestConfig {
    pub capture: Option<CaptureConfig>,
    pub monitor_pattern: Option<PatternConfig>,
    pub timeout_secs: u64,
}

/// Capture configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptureConfig {
    pub camera_id: String,
    pub compare_with: Option<String>,
    pub threshold: f32,
}

/// Pattern matching configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatternConfig {
    pub success_pattern: Option<String>,
    pub failure_pattern: Option<String>,
    pub timeout_secs: u64,
}

/// Device selector for targeting
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DeviceSelector {
    /// Any available device
    Any,
    /// Specific device by ID
    Specific(DeviceId),
    /// By board type
    ByType(String),
    /// By backend type
    ByBackend(String),
    /// By node
    ByNode(NodeId),
    /// By MAC prefix
    ByMacPrefix(String),
    /// By logical name
    ByName(String),
}

/// Device announcement from worker
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceAnnouncement {
    pub device_id: DeviceId,
    pub node_id: NodeId,
    pub backend: BackendType,
    pub board_type: String,
    pub capabilities: Vec<String>,
    pub location: String,
    pub logical_name: Option<String>,
}

/// Connected device information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectedDevice {
    pub device_id: DeviceId,
    pub node_id: NodeId,
    pub backend: BackendType,
    pub board_type: String,
    pub capabilities: Vec<String>,
    pub location: String,
    pub logical_name: Option<String>,
    pub status: DeviceStatus,
}

/// Device status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum DeviceStatus {
    /// Device available
    Available,
    /// Device busy with job
    Busy { job_id: JobId },
    /// Device error
    Error { message: String },
    /// Device offline
    Offline,
}

/// Job result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobResult {
    pub success: bool,
    pub message: String,
    pub output: Option<String>,
    pub captured_image: Option<String>,
    pub validation_result: Option<ValidationResult>,
    pub duration_secs: f64,
}

/// Validation result for tests
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationResult {
    pub passed: bool,
    pub details: String,
    pub image_comparison: Option<ImageComparison>,
}

/// Image comparison result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageComparison {
    pub similarity: f32,
    pub matches: bool,
    pub diff_available: bool,
}

/// Heartbeat message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Heartbeat {
    pub node_id: NodeId,
    pub timestamp: DateTime<Utc>,
    pub device_count: usize,
    pub active_jobs: usize,
}

/// Node info response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeInfo {
    pub node_id: NodeId,
    pub cluster_name: String,
    pub role: String,
    pub address: String,
    pub capabilities: Vec<String>,
    pub device_count: usize,
    pub version: String,
}

#[cfg(test)]
mod tests {
    use crate::cluster::backends::BackendType;
    use crate::cluster::messaging::*;
    use chrono::Utc;

    #[test]
    fn test_device_selector_serialization() {
        let selectors = vec![
            DeviceSelector::Any,
            DeviceSelector::Specific("device-123".to_string()),
            DeviceSelector::ByType("esp32s3".to_string()),
            DeviceSelector::ByBackend("usb".to_string()),
            DeviceSelector::ByNode("node-1".to_string()),
            DeviceSelector::ByMacPrefix("84:f7:03".to_string()),
            DeviceSelector::ByName("dut-1".to_string()),
        ];

        for selector in selectors {
            let serialized = serde_json::to_string(&selector).unwrap();
            let deserialized: DeviceSelector = serde_json::from_str(&serialized).unwrap();

            assert_eq!(
                std::mem::discriminant(&selector),
                std::mem::discriminant(&deserialized)
            );
        }
    }

    #[test]
    fn test_cluster_event_serialization() {
        let event = ClusterEvent {
            node_id: "node-1".to_string(),
            cluster_name: "test-cluster".to_string(),
            event_type: EventType::NodeJoined {
                role: "worker".to_string(),
            },
            timestamp: Utc::now(),
        };

        let serialized = serde_json::to_string(&event).unwrap();
        let deserialized: ClusterEvent = serde_json::from_str(&serialized).unwrap();

        assert_eq!(event.node_id, deserialized.node_id);
        assert_eq!(event.cluster_name, deserialized.cluster_name);
    }

    #[test]
    fn test_backend_type_from_str() {
        assert!(matches!(
            BackendType::from_str("usb:/dev/ttyUSB0"),
            Some(BackendType::USB { .. })
        ));

        assert!(matches!(
            BackendType::from_str("qemu:instance-1"),
            Some(BackendType::QEMU { .. })
        ));

        assert!(matches!(
            BackendType::from_str("wokwi:project-123"),
            Some(BackendType::Wokwi { .. })
        ));

        assert!(BackendType::from_str("invalid:format").is_none());
    }

    #[test]
    fn test_device_status_equality() {
        let status1 = DeviceStatus::Available;
        let status2 = DeviceStatus::Available;
        assert_eq!(status1, status2);

        let status3 = DeviceStatus::Busy {
            job_id: "job-1".to_string(),
        };
        let status4 = DeviceStatus::Busy {
            job_id: "job-1".to_string(),
        };
        assert_eq!(status3, status4);

        let status5 = DeviceStatus::Busy {
            job_id: "job-2".to_string(),
        };
        assert_ne!(status3, status5);
    }
}

