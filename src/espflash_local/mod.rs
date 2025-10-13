//! Local espflash functionality
//!
//! This module contains essential espflash components copied locally
//! to avoid dependency version conflicts and provide better control
//! over the flashing implementation.

pub mod bootloader;
pub mod flash_data;
pub mod idf_bootloader;
pub mod image_format;
pub mod partition_table;

// Re-export commonly used types
pub use bootloader::{default_bootloader, default_partition_table};
pub use flash_data::FlashData;
pub use idf_bootloader::IdfBootloaderFormat;
pub use image_format::{ImageFormat, Segment};
