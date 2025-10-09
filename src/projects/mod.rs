//! Project management and build system integrations
//!
//! This module provides support for various ESP32 development frameworks
//! including ESP-IDF, Arduino, Rust no_std, and many others.

pub mod config;
pub mod handlers;
pub mod registry;

// Re-export the new types
pub use crate::models::ProjectType;
pub use registry::{ProjectHandler, ProjectRegistry};
