//! Flash configuration data structures

use anyhow::Result;
use espflash::flasher::{FlashSettings, FlashSize};
use espflash::target::{Chip, XtalFrequency};

/// Flash data configuration
#[derive(Debug, Clone)]
pub struct FlashData {
    pub chip: Chip,
    pub xtal_freq: XtalFrequency,
    pub flash_settings: FlashSettings,
    pub min_chip_rev: u16,
}

impl FlashData {
    /// Create FlashData from board information
    pub fn from_board_info(
        chip_type: &str,
        crystal_frequency: &str,
        flash_size: &str,
    ) -> Result<Self> {
        // Parse chip type
        let chip = match chip_type.to_lowercase().as_str() {
            "esp32" => Chip::Esp32,
            "esp32c2" => Chip::Esp32c2,
            "esp32c3" => Chip::Esp32c3,
            "esp32c5" => Chip::Esp32c5,
            "esp32c6" => Chip::Esp32c6,
            "esp32h2" => Chip::Esp32h2,
            "esp32p4" => Chip::Esp32p4,
            "esp32s2" => Chip::Esp32s2,
            "esp32s3" => Chip::Esp32s3,
            _ => {
                println!(
                    "⚠️ Unknown chip type '{}', defaulting to ESP32-S3",
                    chip_type
                );
                Chip::Esp32s3
            }
        };

        // Parse crystal frequency - default to 40MHz if not parseable
        let xtal_freq = match crystal_frequency.parse::<u32>() {
            Ok(40_000_000) | Ok(40) => XtalFrequency::_40Mhz,
            Ok(26_000_000) | Ok(26) => XtalFrequency::_26Mhz,
            Ok(32_000_000) | Ok(32) => XtalFrequency::_32Mhz,
            Ok(48_000_000) | Ok(48) => XtalFrequency::_48Mhz,
            _ => {
                println!(
                    "⚠️ Unknown crystal frequency '{}', defaulting to 40MHz",
                    crystal_frequency
                );
                XtalFrequency::_40Mhz
            }
        };

        // Parse flash size - default to 4MB if not parseable
        let _flash_size = match flash_size.to_lowercase().as_str() {
            s if s.contains("16mb") || s.contains("16") => Some(FlashSize::_16Mb),
            s if s.contains("8mb") || s.contains("8") => Some(FlashSize::_8Mb),
            s if s.contains("4mb") || s.contains("4") => Some(FlashSize::_4Mb),
            s if s.contains("2mb") || s.contains("2") => Some(FlashSize::_2Mb),
            _ => {
                println!("⚠️ Unknown flash size '{}', defaulting to 4MB", flash_size);
                Some(FlashSize::_4Mb)
            }
        };

        // Create flash settings using the default constructor
        let flash_settings = FlashSettings::default();
        // Note: We'll use defaults for now since FlashSettings is non-exhaustive
        // In a full implementation, we would use proper initialization methods

        Ok(FlashData {
            chip,
            xtal_freq,
            flash_settings,
            min_chip_rev: 0,
        })
    }
}
