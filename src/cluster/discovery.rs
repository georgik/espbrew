//! Cluster discovery via mDNS
//!
//! Extends mDNS service discovery with cluster name support

use crate::cluster::messaging::NodeInfo;
use crate::cluster::state::NodeRole;
use anyhow::Result;
use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo};
use std::time::Duration;

/// mDNS service type for ESPBrew cluster
pub const ESPBREW_SERVICE_TYPE: &str = "_espbrew-cluster._tcp.local.";

/// TXT record keys
pub mod txt_keys {
    pub const CLUSTER: &str = "cluster";
    pub const VERSION: &str = "version";
    pub const ROLE: &str = "role";
    pub const BACKENDS: &str = "backends";
    pub const DEVICE_COUNT: &str = "device_count";
    pub const CAPABILITIES: &str = "capabilities";
}

/// Discover ESPBrew cluster nodes
pub async fn discover_cluster_nodes(
    cluster_name: &str,
    timeout_secs: u64,
) -> Result<Vec<NodeInfo>> {
    let mdns =
        ServiceDaemon::new().map_err(|e| anyhow::anyhow!("Failed to create mDNS daemon: {}", e))?;

    let receiver = mdns
        .browse(ESPBREW_SERVICE_TYPE)
        .map_err(|e| anyhow::anyhow!("Failed to start mDNS browse: {}", e))?;

    let mut nodes = Vec::new();
    let timeout = Duration::from_secs(timeout_secs);
    let start_time = std::time::Instant::now();

    while start_time.elapsed() < timeout {
        let remaining = timeout.saturating_sub(start_time.elapsed());

        match tokio::time::timeout(remaining, receiver.recv_async()).await {
            Ok(Ok(event)) => match event {
                ServiceEvent::ServiceResolved(info) => {
                    if let Some(node) = parse_service_info(info, cluster_name) {
                        log::debug!("Discovered cluster node: {}", node.node_id);
                        nodes.push(node);
                    }
                }
                _ => {}
            },
            Ok(Err(_)) => break,
            Err(_) => break,
        }
    }

    let _ = mdns.stop_browse(ESPBREW_SERVICE_TYPE);
    Ok(nodes)
}

/// Parse mDNS service info into NodeInfo
fn parse_service_info(info: ServiceInfo, expected_cluster: &str) -> Option<NodeInfo> {
    let properties = info.get_properties();

    let cluster = properties.get(txt_keys::CLUSTER)?;
    if cluster.to_string() != expected_cluster {
        return None;
    }

    let role = properties
        .get(txt_keys::ROLE)
        .map(|v| v.to_string())
        .unwrap_or_else(|| "worker".to_string());
    let version = properties
        .get(txt_keys::VERSION)
        .map(|v| v.to_string())
        .unwrap_or_else(|| "unknown".to_string());
    let device_count = properties
        .get(txt_keys::DEVICE_COUNT)
        .and_then(|v| v.to_string().parse().ok())
        .unwrap_or(0);

    let capabilities = properties
        .get(txt_keys::CAPABILITIES)
        .map(|caps| {
            caps.to_string()
                .split(',')
                .map(|s| s.trim().to_string())
                .collect()
        })
        .unwrap_or_default();

    let ip = *info.get_addresses().iter().next()?;

    Some(NodeInfo {
        node_id: info.get_hostname().to_string(),
        cluster_name: cluster.to_string(),
        role,
        address: format!("{}:{}", ip, info.get_port()),
        capabilities,
        device_count,
        version,
    })
}

/// Create mDNS service info for announcement
pub fn create_service_info(
    hostname: &str,
    cluster_name: &str,
    role: NodeRole,
    port: u16,
    device_count: usize,
    capabilities: &[String],
    backends: &[String],
) -> Result<ServiceInfo> {
    // Format hostname properly for mDNS
    let formatted_hostname = if hostname.ends_with(".local.") {
        hostname.to_string()
    } else {
        let base = hostname.trim_end_matches(".local").trim_end_matches(".");
        format!("{}.local.", base)
    };

    let version = crate::CLUSTER_VERSION;
    let capabilities_str = capabilities.join(",");
    let backends_str = backends.join(",");
    let device_count_str = device_count.to_string();

    // Use localhost address for now
    let addresses = vec![std::net::IpAddr::V4(std::net::Ipv4Addr::new(127, 0, 0, 1))];

    let service_info = ServiceInfo::new(
        ESPBREW_SERVICE_TYPE,
        hostname,
        &formatted_hostname,
        &addresses[..],
        port,
        &[
            (txt_keys::CLUSTER, cluster_name),
            (txt_keys::VERSION, version),
            (txt_keys::ROLE, role.as_str()),
            (txt_keys::DEVICE_COUNT, &device_count_str),
            (txt_keys::CAPABILITIES, &capabilities_str),
            (txt_keys::BACKENDS, &backends_str),
        ][..],
    )
    .map_err(|e| anyhow::anyhow!("Failed to create service info: {}", e))?;

    Ok(service_info)
}

/// mDNS announcer for cluster node
pub struct ClusterAnnouncer {
    mdns: ServiceDaemon,
    hostname: String,
    cluster_name: String,
    port: u16,
}

impl ClusterAnnouncer {
    pub fn new(hostname: String, cluster_name: String, port: u16) -> Result<Self> {
        let mdns = ServiceDaemon::new()
            .map_err(|e| anyhow::anyhow!("Failed to create mDNS daemon: {}", e))?;

        Ok(Self {
            mdns,
            hostname,
            cluster_name,
            port,
        })
    }

    pub fn announce(
        &self,
        role: NodeRole,
        device_count: usize,
        capabilities: &[String],
        backends: &[String],
    ) -> Result<()> {
        let service_info = create_service_info(
            &self.hostname,
            &self.cluster_name,
            role,
            self.port,
            device_count,
            capabilities,
            backends,
        )?;

        self.mdns
            .register(service_info)
            .map_err(|e| anyhow::anyhow!("Failed to register mDNS service: {}", e))?;

        log::info!(
            "Announcing cluster node: {} in cluster '{}' as {}",
            self.hostname,
            self.cluster_name,
            role.as_str()
        );

        Ok(())
    }

    pub fn shutdown(self) -> Result<()> {
        let service_name = format!("{}.{}", self.hostname, ESPBREW_SERVICE_TYPE);
        self.mdns
            .unregister(&service_name)
            .map_err(|e| anyhow::anyhow!("Failed to unregister mDNS service: {}", e))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_service_info_master() {
        let result = create_service_info(
            "test-node",
            "test-cluster",
            NodeRole::Master,
            8081,
            4,
            &["flash".to_string(), "monitor".to_string()],
            &["usb".to_string()],
        );

        assert!(result.is_ok());
        let info = result.unwrap();
        assert_eq!(info.get_hostname(), "test-node.local.");
        assert_eq!(info.get_port(), 8081);

        let props = info.get_properties();
        // Properties include key=value format
        let cluster_prop = props.get(txt_keys::CLUSTER).map(|v| v.to_string());
        assert!(cluster_prop.is_some());
        let role_prop = props.get(txt_keys::ROLE).map(|v| v.to_string());
        assert!(role_prop.is_some());
    }

    #[test]
    fn test_create_service_info_hostname_formatting() {
        let result = create_service_info(
            "my-host.local",
            "test-cluster",
            NodeRole::Worker,
            8081,
            0,
            &[],
            &[],
        );

        assert!(result.is_ok());
        let info = result.unwrap();
        assert_eq!(info.get_hostname(), "my-host.local.");
    }

    #[test]
    fn test_create_service_info_hostname_formatting_with_dot() {
        let result = create_service_info(
            "my-host.local.",
            "test-cluster",
            NodeRole::Worker,
            8081,
            0,
            &[],
            &[],
        );

        assert!(result.is_ok());
        let info = result.unwrap();
        assert_eq!(info.get_hostname(), "my-host.local.");
    }

    #[test]
    fn test_txt_keys_constants() {
        assert_eq!(txt_keys::CLUSTER, "cluster");
        assert_eq!(txt_keys::VERSION, "version");
        assert_eq!(txt_keys::ROLE, "role");
        assert_eq!(txt_keys::BACKENDS, "backends");
        assert_eq!(txt_keys::DEVICE_COUNT, "device_count");
        assert_eq!(txt_keys::CAPABILITIES, "capabilities");
    }

    #[test]
    fn test_espbrew_service_type() {
        assert_eq!(ESPBREW_SERVICE_TYPE, "_espbrew-cluster._tcp.local.");
    }
}
