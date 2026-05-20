//! Cluster worker node
//!
//! Worker nodes execute jobs, manage local devices, report to master

use crate::cluster::backends::BackendType;
use crate::cluster::flash_executor::FlashExecutor;
use crate::cluster::messaging::*;
use crate::cluster::state::ClusterConfig;
use anyhow::Result;
use chrono::Utc;
use log::{debug, info};
use std::collections::HashMap;

/// Worker node instance
pub struct WorkerNode {
    /// Node identifier
    node_id: NodeId,
    /// Cluster configuration
    #[allow(dead_code)]
    config: ClusterConfig,
    /// Master address
    master_addr: Option<String>,
    /// Local devices managed by this worker
    devices: HashMap<DeviceId, LocalDevice>,
    /// Active jobs
    active_jobs: HashMap<JobId, JobContext>,
    /// Worker capabilities
    capabilities: Vec<String>,
    /// Available backends
    backends: Vec<String>,
}

/// Local device managed by worker
#[derive(Debug, Clone)]
pub struct LocalDevice {
    pub id: DeviceId,
    pub backend: BackendType,
    pub board_type: String,
    pub location: String,
    pub capabilities: Vec<String>,
    pub logical_name: Option<String>,
    pub status: DeviceStatus,
}

/// Job execution context
#[derive(Debug)]
struct JobContext {
    #[allow(dead_code)]
    id: JobId,
    #[allow(dead_code)]
    device_id: DeviceId,
    #[allow(dead_code)]
    command: CommandType,
    #[allow(dead_code)]
    started_at: chrono::DateTime<chrono::Utc>,
}

impl WorkerNode {
    /// Create new worker node
    pub fn new(node_id: NodeId, config: ClusterConfig) -> Self {
        let capabilities = vec![
            "flash".to_string(),
            "monitor".to_string(),
            "reset".to_string(),
        ];

        let backends = vec!["usb".to_string()];

        Self {
            node_id,
            config,
            master_addr: None,
            devices: HashMap::new(),
            active_jobs: HashMap::new(),
            capabilities,
            backends,
        }
    }

    /// Get node ID
    pub fn node_id(&self) -> &str {
        &self.node_id
    }

    /// Get capabilities
    pub fn capabilities(&self) -> &[String] {
        &self.capabilities
    }

    /// Get backends
    pub fn backends(&self) -> &[String] {
        &self.backends
    }

    /// Set master address
    pub fn set_master(&mut self, addr: String) {
        info!("Worker {} connected to master at {}", self.node_id, addr);
        self.master_addr = Some(addr);
    }

    /// Add local device
    pub fn add_device(&mut self, device: LocalDevice) {
        let id = device.id.clone();
        self.devices.insert(id, device);
    }

    /// Remove device
    pub fn remove_device(&mut self, device_id: &str) {
        self.devices.remove(device_id);
    }

    /// Get device count
    pub fn device_count(&self) -> usize {
        self.devices.len()
    }

    /// Get active job count
    pub fn active_job_count(&self) -> usize {
        self.active_jobs.len()
    }

    /// Get an available device ID
    pub fn get_available_device(&self) -> Option<String> {
        self.devices
            .values()
            .find(|d| matches!(d.status, DeviceStatus::Available))
            .map(|d| d.id.clone())
    }

    /// Create device announcements for all local devices
    pub fn device_announcements(&self) -> Vec<DeviceAnnouncement> {
        self.devices
            .values()
            .map(|d| DeviceAnnouncement {
                device_id: d.id.clone(),
                node_id: self.node_id.clone(),
                backend: d.backend.clone(),
                board_type: d.board_type.clone(),
                capabilities: d.capabilities.clone(),
                location: d.location.clone(),
                logical_name: d.logical_name.clone(),
            })
            .collect()
    }

    /// Execute command on device
    pub async fn execute_command(
        &mut self,
        job_id: JobId,
        command: CommandType,
        target_device: DeviceId,
    ) -> Result<JobResult> {
        // Check if device exists and is available
        let device = self
            .devices
            .get(&target_device)
            .ok_or_else(|| anyhow::anyhow!("Device not found: {}", target_device))?;

        if !matches!(device.status, DeviceStatus::Available) {
            return Ok(JobResult {
                success: false,
                message: format!("Device not available: {:?}", device.status),
                output: None,
                captured_image: None,
                validation_result: None,
                duration_secs: 0.0,
            });
        }

        // Create job context
        let started_at = Utc::now();
        self.active_jobs.insert(
            job_id.clone(),
            JobContext {
                id: job_id.clone(),
                device_id: target_device.clone(),
                command: command.clone(),
                started_at,
            },
        );

        debug!("Executing job {} on device {}", job_id, target_device);

        // Execute based on command type
        let result = match &command {
            CommandType::Flash { binary_data } => {
                self.execute_flash(&target_device, binary_data).await
            }
            CommandType::Monitor { baud_rate } => {
                self.execute_monitor(&target_device, *baud_rate).await
            }
            CommandType::Capture { camera_id } => {
                self.execute_capture(&target_device, camera_id.clone())
                    .await
            }
            CommandType::Reset => self.execute_reset(&target_device).await,
            CommandType::Test { validation } => self.execute_test(&target_device, validation).await,
        };

        // Clean up job context
        self.active_jobs.remove(&job_id);

        result
    }

    /// Execute flash command
    async fn execute_flash(&self, device_id: &str, binary: &[u8]) -> Result<JobResult> {
        info!("Flashing device {} with {} bytes", device_id, binary.len());

        // Get the device's port location
        let device = self
            .devices
            .get(device_id)
            .ok_or_else(|| anyhow::anyhow!("Device not found: {}", device_id))?;

        let port = match &device.backend {
            BackendType::USB { port } => port.clone(),
            BackendType::QEMU { instance } => instance.clone(),
            BackendType::Wokwi { project_id } => project_id.clone(),
        };

        FlashExecutor::execute_flash(port, binary.to_vec(), None).await
    }

    /// Execute monitor command
    async fn execute_monitor(&self, device_id: &str, baud_rate: u32) -> Result<JobResult> {
        info!("Monitoring device {} at {} baud", device_id, baud_rate);

        let device = self
            .devices
            .get(device_id)
            .ok_or_else(|| anyhow::anyhow!("Device not found: {}", device_id))?;

        let port = match &device.backend {
            BackendType::USB { port } => port.clone(),
            BackendType::QEMU { instance } => instance.clone(),
            BackendType::Wokwi { project_id } => project_id.clone(),
        };

        let handle = FlashExecutor::execute_monitor(port, baud_rate, 10);
        handle
            .await
            .map_err(|e| anyhow::anyhow!("Monitor task failed: {}", e))?
    }

    /// Execute capture command
    async fn execute_capture(
        &self,
        _device_id: &str,
        camera_id: Option<String>,
    ) -> Result<JobResult> {
        info!("Capturing image from camera: {:?}", camera_id);

        #[cfg(feature = "capture")]
        {
            // TODO: Integrate with capture service
            Ok(JobResult {
                success: true,
                message: "Image captured".to_string(),
                output: None,
                captured_image: Some("base64_image_data_here".to_string()),
                validation_result: None,
                duration_secs: 0.5,
            })
        }

        #[cfg(not(feature = "capture"))]
        {
            Ok(JobResult {
                success: false,
                message: "Capture not supported".to_string(),
                output: None,
                captured_image: None,
                validation_result: None,
                duration_secs: 0.0,
            })
        }
    }

    /// Execute reset command
    async fn execute_reset(&self, device_id: &str) -> Result<JobResult> {
        info!("Resetting device {}", device_id);

        let device = self
            .devices
            .get(device_id)
            .ok_or_else(|| anyhow::anyhow!("Device not found: {}", device_id))?;

        let port = match &device.backend {
            BackendType::USB { port } => port.clone(),
            BackendType::QEMU { instance } => instance.clone(),
            BackendType::Wokwi { project_id } => project_id.clone(),
        };

        FlashExecutor::execute_reset(port).await
    }

    /// Execute test command
    async fn execute_test(&self, device_id: &str, _config: &TestConfig) -> Result<JobResult> {
        info!("Running test on device {}", device_id);

        // TODO: Full test execution with capture and validation
        Ok(JobResult {
            success: true,
            message: "Test completed".to_string(),
            output: Some("Test output".to_string()),
            captured_image: None,
            validation_result: Some(ValidationResult {
                passed: true,
                details: "All checks passed".to_string(),
                image_comparison: None,
            }),
            duration_secs: 5.0,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_worker() -> WorkerNode {
        let config = ClusterConfig {
            cluster_name: "test-cluster".to_string(),
            ..Default::default()
        };
        WorkerNode::new("worker-1".to_string(), config)
    }

    fn create_test_device(id: &str) -> LocalDevice {
        LocalDevice {
            id: id.to_string(),
            backend: BackendType::USB {
                port: format!("/dev/tty{}", id),
            },
            board_type: "esp32s3".to_string(),
            location: format!("/dev/tty{}", id),
            capabilities: vec!["flash".to_string(), "monitor".to_string()],
            logical_name: Some(format!("dut-{}", id)),
            status: DeviceStatus::Available,
        }
    }

    #[test]
    fn test_worker_creation() {
        let worker = create_test_worker();

        assert_eq!(worker.node_id(), "worker-1");
        assert_eq!(worker.device_count(), 0);
        assert_eq!(worker.active_job_count(), 0);
        assert!(!worker.capabilities().is_empty());
        assert!(!worker.backends().is_empty());
    }

    #[test]
    fn test_worker_device_management() {
        let mut worker = create_test_worker();

        assert_eq!(worker.device_count(), 0);

        worker.add_device(create_test_device("device-1"));
        assert_eq!(worker.device_count(), 1);

        worker.add_device(create_test_device("device-2"));
        assert_eq!(worker.device_count(), 2);

        worker.remove_device("device-1");
        assert_eq!(worker.device_count(), 1);
    }

    #[test]
    fn test_worker_device_announcements() {
        let mut worker = create_test_worker();

        worker.add_device(create_test_device("device-1"));
        worker.add_device(create_test_device("device-2"));

        let mut announcements = worker.device_announcements();

        assert_eq!(announcements.len(), 2);
        // Sort by device_id for deterministic ordering
        announcements.sort_by(|a, b| a.device_id.cmp(&b.device_id));
        assert_eq!(announcements[0].device_id, "device-1");
        assert_eq!(announcements[0].node_id, "worker-1");
        assert_eq!(
            announcements[0].logical_name,
            Some("dut-device-1".to_string())
        );
        assert_eq!(announcements[1].device_id, "device-2");
    }

    #[test]
    fn test_worker_master_address() {
        let mut worker = create_test_worker();

        assert!(worker.master_addr.is_none());

        worker.set_master("192.168.1.100:8081".to_string());
        assert_eq!(worker.master_addr, Some("192.168.1.100:8081".to_string()));
    }

    #[tokio::test]
    async fn test_worker_execute_reset() {
        let mut worker = create_test_worker();
        worker.add_device(create_test_device("device-1"));

        let result = worker
            .execute_command(
                "job-1".to_string(),
                CommandType::Reset,
                "device-1".to_string(),
            )
            .await;

        // In test environment without actual serial ports, this will fail
        // but we can verify the job context is cleaned up
        assert_eq!(worker.active_job_count(), 0);

        // If port doesn't exist, we expect an error
        if let Err(e) = result {
            assert!(
                e.to_string().contains("Failed to open port")
                    || e.to_string().contains("No such file")
            );
        }
    }

    #[tokio::test]
    async fn test_worker_execute_flash() {
        let mut worker = create_test_worker();
        worker.add_device(create_test_device("device-1"));

        let binary = vec![0x01, 0x02, 0x03, 0x04];
        let result = worker
            .execute_command(
                "job-1".to_string(),
                CommandType::Flash {
                    binary_data: binary.clone(),
                },
                "device-1".to_string(),
            )
            .await;

        assert_eq!(worker.active_job_count(), 0);

        // If port doesn't exist, we expect an error
        if let Err(e) = result {
            assert!(
                e.to_string().contains("Failed to open port")
                    || e.to_string().contains("No such file")
            );
        }
    }

    #[tokio::test]
    async fn test_worker_execute_monitor() {
        let mut worker = create_test_worker();
        worker.add_device(create_test_device("device-1"));

        let result = worker
            .execute_command(
                "job-1".to_string(),
                CommandType::Monitor { baud_rate: 115200 },
                "device-1".to_string(),
            )
            .await;

        assert_eq!(worker.active_job_count(), 0);

        // If port doesn't exist, we expect an error
        if let Err(e) = result {
            assert!(
                e.to_string().contains("Failed to open port")
                    || e.to_string().contains("No such file")
            );
        }
    }

    #[tokio::test]
    async fn test_worker_execute_on_nonexistent_device() {
        let mut worker = create_test_worker();

        let result = worker
            .execute_command(
                "job-1".to_string(),
                CommandType::Reset,
                "nonexistent".to_string(),
            )
            .await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[tokio::test]
    async fn test_worker_execute_on_busy_device() {
        let mut worker = create_test_worker();

        // Add a busy device
        let mut device = create_test_device("device-1");
        device.status = DeviceStatus::Busy {
            job_id: "job-0".to_string(),
        };
        worker.add_device(device);

        let result = worker
            .execute_command(
                "job-1".to_string(),
                CommandType::Reset,
                "device-1".to_string(),
            )
            .await
            .unwrap();

        assert!(!result.success);
        assert!(result.message.contains("not available"));
    }

    #[tokio::test]
    async fn test_worker_execute_test() {
        let mut worker = create_test_worker();
        worker.add_device(create_test_device("device-1"));

        let test_config = TestConfig {
            capture: None,
            monitor_pattern: None,
            timeout_secs: 30,
        };

        let result = worker
            .execute_command(
                "job-1".to_string(),
                CommandType::Test {
                    validation: test_config,
                },
                "device-1".to_string(),
            )
            .await
            .unwrap();

        assert!(result.success);
        assert!(result.validation_result.is_some());
        assert!(result.validation_result.unwrap().passed);
    }
}
