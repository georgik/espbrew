//! ESPBrew Cluster - Distributed ESP32 device management
//!
//! Nodes discover each other, form clusters, and present unified interface
//! for building, flashing, monitoring, and validating ESP32 firmware.

pub mod backends;
pub mod client;
pub mod discovery;
pub mod flash_executor;
pub mod master;
pub mod messaging;
pub mod network;
pub mod node;
pub mod reservation;
pub mod state;
pub mod worker;

#[cfg(test)]
mod integration_tests;

pub use messaging::*;
pub use state::{ClusterConfig, NodeRole};

/// ESPBrew cluster version
pub const CLUSTER_VERSION: &str = "0.1.0";

/// Default cluster name
pub const DEFAULT_CLUSTER_NAME: &str = "espbrew-default";

/// Default heartbeat interval in seconds
pub const DEFAULT_HEARTBEAT_INTERVAL: u64 = 5;

/// Default node timeout in seconds
pub const DEFAULT_NODE_TIMEOUT: u64 = 30;

/// Default WebSocket port for cluster communication
pub const DEFAULT_CLUSTER_PORT: u16 = 8081;
