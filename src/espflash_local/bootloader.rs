//! ESP-IDF bootloader binaries and functionality
//!
//! This module contains the default ESP-IDF bootloaders for all ESP32 variants
//! copied from the espflash project to enable proper multi-partition flashing.

use anyhow::Result;
use espflash::target::{Chip, XtalFrequency};

// Embedded bootloader binaries for all ESP32 variants and crystal frequencies
const BOOTLOADER_ESP32_26MHZ: &[u8] =
    include_bytes!("resources/bootloaders/esp32_26-bootloader.bin");
const BOOTLOADER_ESP32_40MHZ: &[u8] = include_bytes!("resources/bootloaders/esp32-bootloader.bin");

const BOOTLOADER_ESP32C2_26MHZ: &[u8] =
    include_bytes!("resources/bootloaders/esp32c2_26-bootloader.bin");
const BOOTLOADER_ESP32C2_40MHZ: &[u8] =
    include_bytes!("resources/bootloaders/esp32c2-bootloader.bin");

const BOOTLOADER_ESP32C3: &[u8] = include_bytes!("resources/bootloaders/esp32c3-bootloader.bin");
const BOOTLOADER_ESP32C5: &[u8] = include_bytes!("resources/bootloaders/esp32c5-bootloader.bin");
const BOOTLOADER_ESP32C6: &[u8] = include_bytes!("resources/bootloaders/esp32c6-bootloader.bin");
const BOOTLOADER_ESP32H2: &[u8] = include_bytes!("resources/bootloaders/esp32h2-bootloader.bin");
const BOOTLOADER_ESP32P4: &[u8] = include_bytes!("resources/bootloaders/esp32p4-bootloader.bin");
const BOOTLOADER_ESP32S2: &[u8] = include_bytes!("resources/bootloaders/esp32s2-bootloader.bin");
const BOOTLOADER_ESP32S3: &[u8] = include_bytes!("resources/bootloaders/esp32s3-bootloader.bin");

/// Get the default bootloader for the given chip and crystal frequency
pub fn default_bootloader(chip: Chip, xtal_freq: XtalFrequency) -> Result<&'static [u8]> {
    let error = anyhow::anyhow!(
        "Unsupported crystal frequency {:?} for chip {:?}",
        xtal_freq,
        chip
    );

    match chip {
        Chip::Esp32 => match xtal_freq {
            XtalFrequency::_26Mhz => Ok(BOOTLOADER_ESP32_26MHZ),
            XtalFrequency::_40Mhz => Ok(BOOTLOADER_ESP32_40MHZ),
            _ => Err(error),
        },
        Chip::Esp32c2 => match xtal_freq {
            XtalFrequency::_26Mhz => Ok(BOOTLOADER_ESP32C2_26MHZ),
            XtalFrequency::_40Mhz => Ok(BOOTLOADER_ESP32C2_40MHZ),
            _ => Err(error),
        },
        Chip::Esp32c3 => match xtal_freq {
            XtalFrequency::_40Mhz => Ok(BOOTLOADER_ESP32C3),
            _ => Err(error),
        },
        Chip::Esp32c5 => match xtal_freq {
            XtalFrequency::_40Mhz | XtalFrequency::_48Mhz => Ok(BOOTLOADER_ESP32C5),
            _ => Err(error),
        },
        Chip::Esp32c6 => match xtal_freq {
            XtalFrequency::_40Mhz => Ok(BOOTLOADER_ESP32C6),
            _ => Err(error),
        },
        Chip::Esp32h2 => match xtal_freq {
            XtalFrequency::_32Mhz => Ok(BOOTLOADER_ESP32H2),
            _ => Err(error),
        },
        Chip::Esp32p4 => match xtal_freq {
            XtalFrequency::_40Mhz => Ok(BOOTLOADER_ESP32P4),
            _ => Err(error),
        },
        Chip::Esp32s2 => match xtal_freq {
            XtalFrequency::_40Mhz => Ok(BOOTLOADER_ESP32S2),
            _ => Err(error),
        },
        Chip::Esp32s3 => match xtal_freq {
            XtalFrequency::_40Mhz => Ok(BOOTLOADER_ESP32S3),
            _ => Err(error),
        },
        _ => Err(anyhow::anyhow!("Unsupported chip type: {:?}", chip)),
    }
}

/// Generate a default partition table for the given chip
pub fn default_partition_table(
    chip: Chip,
    flash_size: Option<u32>,
) -> esp_idf_part::PartitionTable {
    use esp_idf_part::{AppType, DataType, Flags, Partition, PartitionTable, SubType, Type};

    const NVS_ADDR: u32 = 0x9000;
    const NVS_SIZE: u32 = 0x6000;
    const PHY_INIT_DATA_ADDR: u32 = 0xf000;
    const PHY_INIT_DATA_SIZE: u32 = 0x1000;

    // Max partition size is 16 MB
    const MAX_PARTITION_SIZE: u32 = 16 * 1000 * 1024;

    let (app_addr, app_size) = match chip {
        Chip::Esp32 => (0x1_0000, 0x3f_0000),
        Chip::Esp32c2 => (0x1_0000, 0x1f_0000),
        Chip::Esp32c3 => (0x1_0000, 0x3f_0000),
        Chip::Esp32c5 => (0x1_0000, 0x3f_0000),
        Chip::Esp32c6 => (0x1_0000, 0x3f_0000),
        Chip::Esp32h2 => (0x1_0000, 0x3f_0000),
        Chip::Esp32p4 => (0x1_0000, 0x3f_0000),
        Chip::Esp32s2 => (0x1_0000, 0x10_0000),
        Chip::Esp32s3 => (0x1_0000, 0x10_0000),
        _ => (0x1_0000, 0x3f_0000), // Default values for unknown chip types
    };

    PartitionTable::new(vec![
        Partition::new(
            String::from("nvs"),
            Type::Data,
            SubType::Data(DataType::Nvs),
            NVS_ADDR,
            NVS_SIZE,
            Flags::empty(),
        ),
        Partition::new(
            String::from("phy_init"),
            Type::Data,
            SubType::Data(DataType::Phy),
            PHY_INIT_DATA_ADDR,
            PHY_INIT_DATA_SIZE,
            Flags::empty(),
        ),
        Partition::new(
            String::from("factory"),
            Type::App,
            SubType::App(AppType::Factory),
            app_addr,
            core::cmp::min(
                flash_size.map_or(app_size, |size| size - app_addr),
                MAX_PARTITION_SIZE,
            ),
            Flags::empty(),
        ),
    ])
}
