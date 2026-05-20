//! Integration tests for cluster functionality

#[cfg(test)]
mod tests {
    use crate::cluster::backends::BackendType;
    use crate::cluster::master::MasterNode;
    use crate::cluster::messaging::*;
    use crate::cluster::state::ClusterConfig;
    use crate::cluster::worker::{LocalDevice, WorkerNode};

    async fn setup_test_cluster() -> (MasterNode, WorkerNode) {
        let cluster_name = "test-cluster".to_string();

        let config = ClusterConfig {
            cluster_name: cluster_name.clone(),
            ..Default::default()
        };

        let master = MasterNode::new("master-1".to_string(), config);
        let worker = WorkerNode::new("worker-1".to_string(), ClusterConfig::default());

        (master, worker)
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
            status: crate::cluster::messaging::DeviceStatus::Available,
        }
    }

    #[tokio::test]
    async fn test_cluster_node_registration() {
        let (master, worker) = setup_test_cluster().await;

        // Simulate worker joining cluster
        let node_info = NodeInfo {
            node_id: worker.node_id().to_string(),
            cluster_name: "test-cluster".to_string(),
            role: "worker".to_string(),
            address: "127.0.0.1:8081".to_string(),
            capabilities: worker.capabilities().to_vec(),
            device_count: 0,
            version: crate::CLUSTER_VERSION.to_string(),
        };

        master.handle_node_join(node_info).await.unwrap();

        {
            let state = master.state();
            let guard = state.read().await;
            // Note: Master registers itself asynchronously, so we only see the worker here
            assert_eq!(guard.nodes.len(), 1);
            assert!(guard.nodes.contains_key(worker.node_id()));
        }
    }

    #[tokio::test]
    async fn test_cluster_device_discovery() {
        let (master, mut worker) = setup_test_cluster().await;

        // Add devices to worker
        worker.add_device(create_test_device("device-1"));
        worker.add_device(create_test_device("device-2"));

        // Register worker node
        let node_info = NodeInfo {
            node_id: worker.node_id().to_string(),
            cluster_name: "test-cluster".to_string(),
            role: "worker".to_string(),
            address: "127.0.0.1:8081".to_string(),
            capabilities: worker.capabilities().to_vec(),
            device_count: 2,
            version: crate::CLUSTER_VERSION.to_string(),
        };

        master.handle_node_join(node_info).await.unwrap();

        // Announce devices
        for ann in worker.device_announcements() {
            master.handle_device_announcement(ann).await.unwrap();
        }

        {
            let state = master.state();
            let guard = state.read().await;
            assert_eq!(guard.devices.len(), 2);
            assert_eq!(guard.total_devices(), 2);
            assert_eq!(guard.available_devices(), 2);
        }
    }

    #[tokio::test]
    async fn test_cluster_job_submission() {
        let (mut master, mut worker) = setup_test_cluster().await;

        // Add device to worker
        worker.add_device(create_test_device("device-1"));

        // Register worker and devices
        let node_info = NodeInfo {
            node_id: worker.node_id().to_string(),
            cluster_name: "test-cluster".to_string(),
            role: "worker".to_string(),
            address: "127.0.0.1:8081".to_string(),
            capabilities: worker.capabilities().to_vec(),
            device_count: 1,
            version: crate::CLUSTER_VERSION.to_string(),
        };

        master.handle_node_join(node_info).await.unwrap();
        for ann in worker.device_announcements() {
            master.handle_device_announcement(ann).await.unwrap();
        }

        // Submit job
        let job_id = master
            .submit_job(CommandType::Reset, DeviceSelector::Any, 30)
            .await
            .unwrap();

        assert!(!job_id.is_empty());

        let status = master.status_summary().await;
        assert_eq!(status.running_jobs, 1);
    }

    #[tokio::test]
    async fn test_cluster_heartbeat_flow() {
        let (master, worker) = setup_test_cluster().await;

        // Register worker
        let node_info = NodeInfo {
            node_id: worker.node_id().to_string(),
            cluster_name: "test-cluster".to_string(),
            role: "worker".to_string(),
            address: "127.0.0.1:8081".to_string(),
            capabilities: worker.capabilities().to_vec(),
            device_count: 0,
            version: crate::CLUSTER_VERSION.to_string(),
        };

        master.handle_node_join(node_info).await.unwrap();

        // Send heartbeat
        let heartbeat = Heartbeat {
            node_id: worker.node_id().to_string(),
            timestamp: chrono::Utc::now(),
            device_count: 5,
            active_jobs: 2,
        };

        master.handle_heartbeat(heartbeat).await.unwrap();

        {
            let state = master.state();
            let guard = state.read().await;
            let node = guard.nodes.get(worker.node_id()).unwrap();
            assert_eq!(node.device_count, 5);
            assert_eq!(node.active_jobs, 2);
        }
    }

    #[tokio::test]
    async fn test_cluster_node_expiry_and_cleanup() {
        let config = ClusterConfig {
            cluster_name: "test-cluster".to_string(),
            node_timeout: 1, // 1 second timeout for testing
            ..Default::default()
        };

        let master = MasterNode::new("master-1".to_string(), config);

        // Add a node
        let node_info = NodeInfo {
            node_id: "temp-worker".to_string(),
            cluster_name: "test-cluster".to_string(),
            role: "worker".to_string(),
            address: "127.0.0.1:8081".to_string(),
            capabilities: vec!["flash".to_string()],
            device_count: 0,
            version: crate::CLUSTER_VERSION.to_string(),
        };

        master.handle_node_join(node_info).await.unwrap();

        // Manually expire the node
        {
            let state = master.state();
            let mut guard = state.write().await;
            if let Some(node) = guard.nodes.get_mut("temp-worker") {
                node.last_seen = chrono::Utc::now() - chrono::Duration::seconds(10);
            }
        }

        // Cleanup should remove expired node
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        let count = master.cleanup_expired_nodes().await.unwrap();
        assert_eq!(count, 1);

        {
            let state = master.state();
            let guard = state.read().await;
            assert!(!guard.nodes.contains_key("temp-worker"));
        }
    }

    #[tokio::test]
    async fn test_device_selector_resolution() {
        let (master, mut worker) = setup_test_cluster().await;

        // Add devices with different types
        worker.add_device(LocalDevice {
            id: "esp32-device".to_string(),
            backend: BackendType::USB {
                port: "/dev/ttyUSB0".to_string(),
            },
            board_type: "esp32".to_string(),
            location: "/dev/ttyUSB0".to_string(),
            capabilities: vec!["flash".to_string()],
            logical_name: Some("esp32-dut".to_string()),
            status: crate::cluster::messaging::DeviceStatus::Available,
        });

        worker.add_device(LocalDevice {
            id: "esp32s3-device".to_string(),
            backend: BackendType::USB {
                port: "/dev/ttyUSB1".to_string(),
            },
            board_type: "esp32s3".to_string(),
            location: "/dev/ttyUSB1".to_string(),
            capabilities: vec!["flash".to_string()],
            logical_name: Some("s3-dut".to_string()),
            status: crate::cluster::messaging::DeviceStatus::Available,
        });

        // Register and announce
        let node_info = NodeInfo {
            node_id: worker.node_id().to_string(),
            cluster_name: "test-cluster".to_string(),
            role: "worker".to_string(),
            address: "127.0.0.1:8081".to_string(),
            capabilities: worker.capabilities().to_vec(),
            device_count: 2,
            version: crate::CLUSTER_VERSION.to_string(),
        };

        master.handle_node_join(node_info).await.unwrap();
        for ann in worker.device_announcements() {
            master.handle_device_announcement(ann).await.unwrap();
        }

        // Test different selectors
        let available: Vec<_> = {
            let state_lock = master.state();
            let state = state_lock.read().await;
            state.devices.keys().cloned().collect()
        };

        // Select by type
        let result = master
            .find_device_for_selector(&DeviceSelector::ByType("esp32s3".to_string()), &available)
            .await;
        assert_eq!(result, Some("esp32s3-device".to_string()));

        // Select by name
        let result = master
            .find_device_for_selector(&DeviceSelector::ByName("s3-dut".to_string()), &available)
            .await;
        assert_eq!(result, Some("esp32s3-device".to_string()));

        // Select by specific ID
        let result = master
            .find_device_for_selector(
                &DeviceSelector::Specific("esp32-device".to_string()),
                &available,
            )
            .await;
        assert_eq!(result, Some("esp32-device".to_string()));

        // Select any
        let result = master
            .find_device_for_selector(&DeviceSelector::Any, &available)
            .await;
        assert!(result.is_some());
    }

    #[tokio::test]
    async fn test_job_completion_flow() {
        let (mut master, mut worker) = setup_test_cluster().await;

        // Add device
        worker.add_device(create_test_device("device-1"));

        // Register and announce
        let node_info = NodeInfo {
            node_id: worker.node_id().to_string(),
            cluster_name: "test-cluster".to_string(),
            role: "worker".to_string(),
            address: "127.0.0.1:8081".to_string(),
            capabilities: worker.capabilities().to_vec(),
            device_count: 1,
            version: crate::CLUSTER_VERSION.to_string(),
        };

        master.handle_node_join(node_info).await.unwrap();
        for ann in worker.device_announcements() {
            master.handle_device_announcement(ann).await.unwrap();
        }

        // Submit and assign job
        let job_id = master
            .submit_job(CommandType::Reset, DeviceSelector::Any, 30)
            .await
            .unwrap();

        // Simulate job completion
        let result = JobResult {
            success: true,
            message: "Reset complete".to_string(),
            output: None,
            captured_image: None,
            validation_result: None,
            duration_secs: 0.5,
        };

        master.handle_job_complete(&job_id, result).await.unwrap();

        {
            let state_lock = master.state();
            let state = state_lock.read().await;
            let device = state.devices.get("device-1").unwrap();
            assert!(matches!(device.status, DeviceStatus::Available));
        }
    }
}
