//! Server-related data models

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredServer {
    pub name: String,
    pub ip: std::net::IpAddr,
    pub port: u16,
    pub hostname: String,
    pub version: String,
    pub description: String,
    pub board_count: u32,
    pub boards_list: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RemoteActionType {
    Flash,
    Monitor,
}
