//! Cluster node runner
//!
//! Manages the lifecycle of cluster nodes (master/worker)

use crate::cluster::backends::usb::detect_usb_devices;
use crate::cluster::discovery::ClusterAnnouncer;
use crate::cluster::master::MasterNode;
use crate::cluster::messaging::*;
use crate::cluster::state::{ClusterConfig, NodeRole};
use crate::cluster::worker::{LocalDevice, WorkerNode};
use anyhow::{Context, Result};
use log::info;
use std::sync::Arc;
use tokio::task::JoinHandle;

/// Cluster node instance
pub struct ClusterNode {
    /// Node configuration
    config: ClusterConfig,
    /// Master node (if this is a master) - wrapped in Arc for sharing with HTTP server
    master: Option<Arc<MasterNode>>,
    /// Worker node (if this is a worker)
    worker: Option<WorkerNode>,
    /// mDNS announcer
    announcer: Option<ClusterAnnouncer>,
    /// HTTP server handle
    server_handle: Option<JoinHandle<()>>,
    /// Device scanner handle (for continuous USB device polling)
    scanner_handle: Option<JoinHandle<()>>,
    /// Cancellation token
    cancel_token: Arc<std::sync::atomic::AtomicBool>,
}

impl ClusterNode {
    /// Create a new cluster node
    pub fn new(config: ClusterConfig) -> Self {
        Self {
            config,
            master: None,
            worker: None,
            announcer: None,
            server_handle: None,
            scanner_handle: None,
            cancel_token: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }

    /// Start the cluster node
    pub async fn start(&mut self) -> Result<()> {
        let hostname = hostname::get()
            .unwrap_or_else(|_| "espbrew".into())
            .to_string_lossy()
            .to_string();

        let node_id = format!("{}@{}", self.config.role.as_str(), hostname);

        // Start based on role
        match self.config.role {
            NodeRole::Master | NodeRole::Auto => {
                self.start_master(&node_id, &hostname)?;
                // Register local devices (async, so no runtime conflict)
                self.register_master_local_devices(&node_id).await?;
            }
            NodeRole::Worker => {
                self.start_worker(&node_id, &hostname)?;
            }
        }

        // Start mDNS announcement
        self.start_announcement(&hostname).await?;

        // Start HTTP API server
        self.start_http_server().await?;

        // Start device watcher for continuous USB device monitoring
        self.start_device_watcher(&node_id);

        info!("Cluster node started: {}", node_id);
        info!("Cluster: {}", self.config.cluster_name);
        info!("Role: {:?}", self.config.role);

        Ok(())
    }

    /// Start as master node
    fn start_master(&mut self, node_id: &str, _hostname: &str) -> Result<()> {
        info!("Starting cluster as master");

        let master = Arc::new(MasterNode::new(node_id.to_string(), self.config.clone()));
        self.master = Some(master);
        Ok(())
    }

    /// Register local devices with master (must be called from async context)
    async fn register_master_local_devices(&self, node_id: &str) -> Result<()> {
        if let Some(ref master) = self.master {
            match detect_usb_devices() {
                Ok(devices) => {
                    info!("Auto-detected {} USB devices on master", devices.len());
                    let announcements: Vec<DeviceAnnouncement> = devices
                        .into_iter()
                        .map(|d| DeviceAnnouncement {
                            device_id: d.id.clone(),
                            node_id: node_id.to_string(),
                            backend: d.backend,
                            board_type: d.board_type,
                            capabilities: d.capabilities,
                            location: d.location,
                            logical_name: d.logical_name,
                        })
                        .collect();

                    let count = master.register_local_devices(announcements).await?;
                    info!("Master node registered with {} local devices", count);
                }
                Err(e) => {
                    info!("Failed to auto-detect USB devices on master: {}", e);
                }
            }
        }
        Ok(())
    }

    /// Start as worker node
    fn start_worker(&mut self, node_id: &str, _hostname: &str) -> Result<()> {
        info!("Starting cluster as worker");

        let mut worker = WorkerNode::new(node_id.to_string(), self.config.clone());

        // Auto-detect local USB devices
        match detect_usb_devices() {
            Ok(devices) => {
                info!("Auto-detected {} USB devices", devices.len());
                for device in devices {
                    info!("  Found device: {} ({})", device.id, device.board_type);
                    worker.add_device(device);
                }
            }
            Err(e) => {
                info!("Failed to auto-detect USB devices: {}", e);
            }
        }

        self.worker = Some(worker);

        Ok(())
    }

    /// Start mDNS announcement
    async fn start_announcement(&mut self, hostname: &str) -> Result<()> {
        let announcer =
            ClusterAnnouncer::new(hostname.to_string(), self.config.cluster_name.clone(), 8081)
                .context("Failed to create mDNS announcer")?;

        // Get device count from worker or master
        let device_count = if let Some(ref worker) = self.worker {
            worker.device_count()
        } else if let Some(ref master) = self.master {
            // Get device count from master's cluster state
            let state = master.state();
            let guard = state.read().await;
            guard.devices.len()
        } else {
            0
        };

        let capabilities = self
            .worker
            .as_ref()
            .map(|w| w.capabilities().to_vec())
            .unwrap_or_else(|| vec!["flash".to_string(), "monitor".to_string()]);
        let backends = vec!["usb".to_string()];

        info!(
            "Announcing to cluster: {} devices, capabilities: {}",
            device_count,
            capabilities.join(", ")
        );

        announcer
            .announce(self.config.role, device_count, &capabilities, &backends)
            .context("Failed to announce cluster node")?;

        self.announcer = Some(announcer);
        Ok(())
    }

    /// Start HTTP API server
    async fn start_http_server(&mut self) -> Result<()> {
        // Create server state with cluster master if available
        if let Some(master) = &self.master {
            // Extract host from bind_address, replacing 0.0.0.0 with actual hostname
            let bind_host = self
                .config
                .bind_address
                .split(':')
                .next()
                .unwrap_or("0.0.0.0");

            let display_host = if bind_host == "0.0.0.0" {
                hostname::get()
                    .unwrap_or_else(|_| "localhost".into())
                    .to_string_lossy()
                    .to_string()
            } else {
                bind_host.to_string()
            };

            info!("Cluster HTTP API available at http://{}:8081", display_host);

            // Parse bind address to SocketAddr
            use std::str::FromStr;
            let bind_addr = std::net::SocketAddr::from_str(&self.config.bind_address)
                .with_context(|| format!("Invalid bind address: {}", self.config.bind_address))?;

            // Create routes and start the actual HTTP server
            use crate::cluster::network;
            let routes = network::create_cluster_routes(self.config.clone(), Arc::clone(master));
            let cancel_token = self.cancel_token.clone();

            let handle = tokio::spawn(async move {
                let (_, server) =
                    warp::serve(routes).bind_with_graceful_shutdown(bind_addr, async move {
                        // Wait for cancel signal
                        while !cancel_token.load(std::sync::atomic::Ordering::Relaxed) {
                            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                        }
                    });
                server.await;
            });

            self.server_handle = Some(handle);
            Ok(())
        } else {
            info!("Worker mode - HTTP API not yet implemented");
            Ok(())
        }
    }

    /// Start device watcher for continuous USB device monitoring
    fn start_device_watcher(&mut self, node_id: &str) {
        use crate::cluster::device_watcher;

        let (mut rx, _shutdown) = device_watcher::spawn_device_watcher();
        let master = self.master.clone();
        let node_id = node_id.to_string();

        self.scanner_handle = Some(tokio::spawn(async move {
            use crate::cluster::backends::usb::detect_usb_devices;
            use crate::cluster::messaging::DeviceAnnouncement;

            while let Some(event) = rx.recv().await {
                match event {
                    device_watcher::DeviceEvent::Added(port) => {
                        info!("Device detected: {}", port);
                        if let Some(ref master) = master {
                            // Re-scan to get full device info
                            if let Ok(devices) = detect_usb_devices() {
                                for device in devices {
                                    if device.location == port {
                                        let ann = DeviceAnnouncement {
                                            device_id: device.id.clone(),
                                            node_id: node_id.clone(),
                                            backend: device.backend,
                                            board_type: device.board_type,
                                            capabilities: device.capabilities,
                                            location: device.location.clone(),
                                            logical_name: device.logical_name,
                                        };
                                        let _ = master.register_local_devices(vec![ann]).await;
                                        info!("Registered new device: {}", port);
                                    }
                                }
                            }
                        }
                    }
                    device_watcher::DeviceEvent::Removed(port) => {
                        info!("Device removed: {}", port);
                        if let Some(ref master) = master {
                            // Remove device from cluster state
                            let state = master.state();
                            let mut state_guard = state.write().await;
                            state_guard.devices.retain(|id, dev| {
                                let keep = dev.location != port;
                                if !keep {
                                    info!("Removed device {} from cluster state", id);
                                }
                                keep
                            });
                        }
                    }
                }
            }
        }));
    }

    /// Run the cluster node (blocking)
    pub async fn run(&mut self) -> Result<()> {
        self.start().await?;

        // Run heartbeat loop
        while !self.cancel_token.load(std::sync::atomic::Ordering::Relaxed) {
            tokio::time::sleep(tokio::time::Duration::from_secs(
                self.config.heartbeat_interval,
            ))
            .await;

            // Send heartbeat if worker
            if let Some(ref worker) = self.worker {
                // TODO: Send heartbeat to master
                let _ = worker;
            }

            // Cleanup expired nodes if master
            if let Some(ref master) = self.master {
                let _ = master.cleanup_expired_nodes().await;
            }
        }

        Ok(())
    }

    /// Stop the cluster node
    pub async fn stop(&mut self) -> Result<()> {
        info!("Stopping cluster node");

        self.cancel_token
            .store(true, std::sync::atomic::Ordering::Relaxed);

        // Shutdown mDNS announcer
        if let Some(announcer) = self.announcer.take() {
            announcer.shutdown()?;
        }

        // Shutdown HTTP server
        if let Some(handle) = self.server_handle.take() {
            handle.abort();
        }

        // Shutdown device scanner
        if let Some(handle) = self.scanner_handle.take() {
            handle.abort();
        }

        info!("Cluster node stopped");
        Ok(())
    }

    /// Add local device (worker mode)
    pub fn add_device(&mut self, device: LocalDevice) -> Result<()> {
        if let Some(ref mut worker) = self.worker {
            worker.add_device(device);
            Ok(())
        } else {
            Err(anyhow::anyhow!("Cannot add device - not in worker mode"))
        }
    }

    /// Get cluster status (master mode)
    pub async fn status(&self) -> Result<Option<crate::cluster::master::ClusterStatus>> {
        if let Some(ref master) = self.master {
            Ok(Some(master.status_summary().await))
        } else {
            Ok(None)
        }
    }
}

/// Builder for creating cluster nodes
pub struct ClusterNodeBuilder {
    cluster_name: String,
    role: NodeRole,
    bind_address: String,
}

impl Default for ClusterNodeBuilder {
    fn default() -> Self {
        Self {
            cluster_name: crate::cluster::DEFAULT_CLUSTER_NAME.to_string(),
            role: NodeRole::Auto,
            bind_address: "0.0.0.0:8081".to_string(),
        }
    }
}

impl ClusterNodeBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cluster_name(mut self, name: String) -> Self {
        self.cluster_name = name;
        self
    }

    pub fn role(mut self, role: NodeRole) -> Self {
        self.role = role;
        self
    }

    pub fn bind_address(mut self, addr: String) -> Self {
        self.bind_address = addr;
        self
    }

    pub fn build(self) -> ClusterNode {
        let config = ClusterConfig {
            cluster_name: self.cluster_name,
            role: self.role,
            bind_address: self.bind_address,
            heartbeat_interval: crate::cluster::DEFAULT_HEARTBEAT_INTERVAL,
            node_timeout: crate::cluster::DEFAULT_NODE_TIMEOUT,
        };

        ClusterNode::new(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cluster::backends::BackendType;

    #[test]
    fn test_builder_defaults() {
        let node = ClusterNodeBuilder::new().build();

        assert_eq!(
            node.config.cluster_name,
            crate::cluster::DEFAULT_CLUSTER_NAME
        );
        assert_eq!(node.config.role, NodeRole::Auto);
        assert_eq!(node.config.bind_address, "0.0.0.0:8081");
    }

    #[test]
    fn test_builder_custom() {
        let node = ClusterNodeBuilder::new()
            .cluster_name("test-cluster".to_string())
            .role(NodeRole::Master)
            .bind_address("127.0.0.1:9090".to_string())
            .build();

        assert_eq!(node.config.cluster_name, "test-cluster");
        assert_eq!(node.config.role, NodeRole::Master);
        assert_eq!(node.config.bind_address, "127.0.0.1:9090");
    }

    #[test]
    fn test_add_device_worker() {
        let mut node = ClusterNodeBuilder::new().role(NodeRole::Worker).build();

        let device = LocalDevice {
            id: "test-device".to_string(),
            backend: BackendType::USB {
                port: "/dev/ttyUSB0".to_string(),
            },
            board_type: "esp32s3".to_string(),
            location: "/dev/ttyUSB0".to_string(),
            capabilities: vec!["flash".to_string()],
            logical_name: None,
            status: DeviceStatus::Available,
        };

        // Before starting, worker is None, so add_device should fail
        assert!(node.add_device(device).is_err());
    }

    #[tokio::test]
    async fn test_cluster_status_master() {
        let node = ClusterNodeBuilder::new().role(NodeRole::Master).build();

        // Before starting, status should be None
        assert!(node.status().await.unwrap().is_none());
    }
}
