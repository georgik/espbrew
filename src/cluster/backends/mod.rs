//! Device backends for cluster nodes
//!
//! Different connection types for ESP32 devices:
//! - USB: Physical boards via USB serial
//! - QEMU: ESP32 emulator
//! - Wokwi: Circuit simulator

pub mod usb;

use serde::{Deserialize, Serialize};

/// Backend type for device connection
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum BackendType {
    /// Physical USB serial connection
    USB { port: String },
    /// QEMU ESP32 emulator
    QEMU { instance: String },
    /// Wokwi simulator
    Wokwi { project_id: String },
}

impl BackendType {
    /// Get backend identifier string
    pub fn as_str(&self) -> &str {
        match self {
            BackendType::USB { .. } => "usb",
            BackendType::QEMU { .. } => "qemu",
            BackendType::Wokwi { .. } => "wokwi",
        }
    }

    /// Parse backend from string
    pub fn from_str(s: &str) -> Option<Self> {
        let parts: Vec<&str> = s.split(':').collect();
        match parts.first() {
            Some(&"usb") => parts.get(1).map(|port| BackendType::USB { port: port.to_string() }),
            Some(&"qemu") => parts.get(1).map(|inst| BackendType::QEMU { instance: inst.to_string() }),
            Some(&"wokwi") => parts.get(1).map(|pid| BackendType::Wokwi { project_id: pid.to_string() }),
            _ => None,
        }
    }

    /// Get location identifier
    pub fn location(&self) -> String {
        match self {
            BackendType::USB { port } => port.clone(),
            BackendType::QEMU { instance } => instance.clone(),
            BackendType::Wokwi { project_id } => project_id.clone(),
        }
    }
}
