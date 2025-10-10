//! ESP-IDF bootloader format implementation with multi-partition support
//!
//! This implementation provides full ESP-IDF compatible multi-partition flashing
//! including bootloader, partition table, and application segments.

use crate::espflash_local::{default_bootloader, default_partition_table, image_format::Segment};
use anyhow::Result;
use espflash::target::{Chip, XtalFrequency};
use std::borrow::Cow;
use std::collections::HashMap;

/// ESP-IDF bootloader format with multi-partition support
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdfBootloaderFormat<'a> {
    /// Boot address for bootloader
    boot_addr: u32,
    /// Bootloader binary
    bootloader: Cow<'a, [u8]>,
    /// Partition table
    partition_table: esp_idf_part::PartitionTable,
    /// Application segment (main firmware)
    flash_segment: Segment<'a>,
    /// Application size
    app_size: u32,
    /// Partition table offset (usually 0x8000)
    partition_table_offset: u32,
}

impl<'a> IdfBootloaderFormat<'a> {
    /// Create a new ESP-IDF bootloader format for the given chip and application data
    pub fn new(
        chip: Chip,
        xtal_freq: XtalFrequency,
        flash_size: Option<u32>,
        app_data: &'a [u8],
        app_offset: u32,
    ) -> Result<Self> {
        // Get default bootloader for this chip
        let bootloader = default_bootloader(chip, xtal_freq)?.to_vec();

        // Create default partition table
        let partition_table = default_partition_table(chip, flash_size);

        // Boot address depends on chip
        let boot_addr = match chip {
            Chip::Esp32 => 0x1000,
            _ => 0x0,
        };

        // Standard partition table offset
        let partition_table_offset = 0x8000;

        // Create application segment
        let flash_segment = Segment {
            addr: app_offset,
            data: Cow::Borrowed(app_data),
        };

        let app_size = app_data.len() as u32;

        Ok(IdfBootloaderFormat {
            boot_addr,
            bootloader: Cow::Owned(bootloader),
            partition_table,
            flash_segment,
            app_size,
            partition_table_offset,
        })
    }

    /// Create a new ESP-IDF bootloader format for the given chip and owned application data
    pub fn new_from_owned(
        chip: Chip,
        xtal_freq: XtalFrequency,
        flash_size: Option<u32>,
        app_data: Vec<u8>,
        app_offset: u32,
    ) -> Result<IdfBootloaderFormat<'static>> {
        // Get default bootloader for this chip
        let bootloader = default_bootloader(chip, xtal_freq)?.to_vec();

        // Create default partition table
        let partition_table = default_partition_table(chip, flash_size);

        // Boot address depends on chip
        let boot_addr = match chip {
            Chip::Esp32 => 0x1000,
            _ => 0x0,
        };

        // Standard partition table offset
        let partition_table_offset = 0x8000;

        // Create application segment with owned data
        let app_size = app_data.len() as u32;
        let flash_segment = Segment {
            addr: app_offset,
            data: Cow::Owned(app_data),
        };

        Ok(IdfBootloaderFormat {
            boot_addr,
            bootloader: Cow::Owned(bootloader),
            partition_table,
            flash_segment,
            app_size,
            partition_table_offset,
        })
    }

    /// Create from flash data map (offset -> binary data) - for backward compatibility
    pub fn from_flash_data(
        flash_data: HashMap<u32, Vec<u8>>,
    ) -> Result<IdfBootloaderFormat<'static>> {
        // For now, just take the first binary as application
        let mut sorted_entries: Vec<_> = flash_data.into_iter().collect();
        sorted_entries.sort_by_key(|(offset, _)| *offset);

        if let Some((offset, data)) = sorted_entries.into_iter().next() {
            // Default to ESP32-S3 40MHz for backward compatibility
            Self::new_from_owned(
                Chip::Esp32s3,
                XtalFrequency::_40Mhz,
                Some(4 * 1024 * 1024), // 4MB default
                data,
                offset,
            )
        } else {
            Err(anyhow::anyhow!("No flash data provided"))
        }
    }

    /// Returns all flash segments (bootloader + partition table + application)
    pub fn flash_segments(self) -> Vec<Segment<'a>> {
        use std::iter::once;

        let bootloader_segment = Segment {
            addr: self.boot_addr,
            data: self.bootloader,
        };

        let partition_table_data = self
            .partition_table
            .to_bin()
            .unwrap_or_else(|_| vec![0xFF; 0xC00]); // Default partition table size
        let partition_table_segment = Segment {
            addr: self.partition_table_offset,
            data: Cow::Owned(partition_table_data),
        };

        once(bootloader_segment)
            .chain(once(partition_table_segment))
            .chain(once(self.flash_segment))
            .collect()
    }

    /// Returns OTA segments (just the application for OTA updates)
    pub fn ota_segments(self) -> Vec<Segment<'a>> {
        vec![self.flash_segment]
    }

    /// Returns metadata about the application image
    pub fn metadata(&self) -> HashMap<&str, String> {
        HashMap::from([
            ("app_size", self.app_size.to_string()),
            ("bootloader_addr", format!("0x{:x}", self.boot_addr)),
            (
                "partition_table_addr",
                format!("0x{:x}", self.partition_table_offset),
            ),
            ("app_addr", format!("0x{:x}", self.flash_segment.addr)),
        ])
    }
}
