//! Remote operations for interacting with ESPBrew servers
//!
//! This module provides client-side functionality for discovering and
//! interacting with remote ESPBrew servers.

pub mod client;
pub mod discovery;
pub mod websocket_client;

pub use discovery::*;
