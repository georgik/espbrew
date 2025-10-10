//! Partition table handling
//!
//! This module provides partition table functionality using the esp-idf-part crate
//! for proper ESP-IDF compatible multi-partition flashing.

pub use esp_idf_part::*;

// Re-export esp-idf-part types for convenience
pub type PartitionTable = esp_idf_part::PartitionTable;
pub type Partition = esp_idf_part::Partition;
pub type Type = esp_idf_part::Type;
pub type SubType = esp_idf_part::SubType;
pub type AppType = esp_idf_part::AppType;
pub type DataType = esp_idf_part::DataType;
pub type Flags = esp_idf_part::Flags;
