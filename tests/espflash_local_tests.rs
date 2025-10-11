use espbrew::espflash_local::{default_bootloader, default_partition_table};
use espflash::target::{Chip, XtalFrequency};

#[test]
fn test_default_bootloader_esp32() {
    // Test ESP32 with both supported frequencies
    let bootloader_26 = default_bootloader(Chip::Esp32, XtalFrequency::_26Mhz).unwrap();
    let bootloader_40 = default_bootloader(Chip::Esp32, XtalFrequency::_40Mhz).unwrap();

    assert!(
        bootloader_26.len() > 0,
        "ESP32 26MHz bootloader should not be empty"
    );
    assert!(
        bootloader_40.len() > 0,
        "ESP32 40MHz bootloader should not be empty"
    );
    assert_ne!(
        bootloader_26, bootloader_40,
        "Different frequencies should have different bootloaders"
    );
}

#[test]
fn test_default_bootloader_esp32c2() {
    // Test ESP32-C2 with both supported frequencies
    let bootloader_26 = default_bootloader(Chip::Esp32c2, XtalFrequency::_26Mhz).unwrap();
    let bootloader_40 = default_bootloader(Chip::Esp32c2, XtalFrequency::_40Mhz).unwrap();

    assert!(
        bootloader_26.len() > 0,
        "ESP32-C2 26MHz bootloader should not be empty"
    );
    assert!(
        bootloader_40.len() > 0,
        "ESP32-C2 40MHz bootloader should not be empty"
    );
    assert_ne!(
        bootloader_26, bootloader_40,
        "Different frequencies should have different bootloaders"
    );
}

#[test]
fn test_default_bootloader_esp32c3() {
    // Test ESP32-C3 with supported frequency
    let bootloader_40 = default_bootloader(Chip::Esp32c3, XtalFrequency::_40Mhz).unwrap();
    assert!(
        bootloader_40.len() > 0,
        "ESP32-C3 40MHz bootloader should not be empty"
    );

    // Test unsupported frequency
    let result = default_bootloader(Chip::Esp32c3, XtalFrequency::_26Mhz);
    assert!(result.is_err(), "ESP32-C3 26MHz should be unsupported");
}

#[test]
fn test_default_bootloader_esp32c5() {
    // Test ESP32-C5 with both supported frequencies
    let bootloader_40 = default_bootloader(Chip::Esp32c5, XtalFrequency::_40Mhz).unwrap();
    let bootloader_48 = default_bootloader(Chip::Esp32c5, XtalFrequency::_48Mhz).unwrap();

    assert!(
        bootloader_40.len() > 0,
        "ESP32-C5 40MHz bootloader should not be empty"
    );
    assert!(
        bootloader_48.len() > 0,
        "ESP32-C5 48MHz bootloader should not be empty"
    );
    assert_eq!(
        bootloader_40, bootloader_48,
        "ESP32-C5 should use same bootloader for 40MHz and 48MHz"
    );
}

#[test]
fn test_default_bootloader_esp32c6() {
    let bootloader_40 = default_bootloader(Chip::Esp32c6, XtalFrequency::_40Mhz).unwrap();
    assert!(
        bootloader_40.len() > 0,
        "ESP32-C6 40MHz bootloader should not be empty"
    );

    // Test unsupported frequency
    let result = default_bootloader(Chip::Esp32c6, XtalFrequency::_26Mhz);
    assert!(result.is_err(), "ESP32-C6 26MHz should be unsupported");
}

#[test]
fn test_default_bootloader_esp32h2() {
    let bootloader_32 = default_bootloader(Chip::Esp32h2, XtalFrequency::_32Mhz).unwrap();
    assert!(
        bootloader_32.len() > 0,
        "ESP32-H2 32MHz bootloader should not be empty"
    );

    // Test unsupported frequency
    let result = default_bootloader(Chip::Esp32h2, XtalFrequency::_40Mhz);
    assert!(result.is_err(), "ESP32-H2 40MHz should be unsupported");
}

#[test]
fn test_default_bootloader_esp32p4() {
    let bootloader_40 = default_bootloader(Chip::Esp32p4, XtalFrequency::_40Mhz).unwrap();
    assert!(
        bootloader_40.len() > 0,
        "ESP32-P4 40MHz bootloader should not be empty"
    );
}

#[test]
fn test_default_bootloader_esp32s2() {
    let bootloader_40 = default_bootloader(Chip::Esp32s2, XtalFrequency::_40Mhz).unwrap();
    assert!(
        bootloader_40.len() > 0,
        "ESP32-S2 40MHz bootloader should not be empty"
    );
}

#[test]
fn test_default_bootloader_esp32s3() {
    let bootloader_40 = default_bootloader(Chip::Esp32s3, XtalFrequency::_40Mhz).unwrap();
    assert!(
        bootloader_40.len() > 0,
        "ESP32-S3 40MHz bootloader should not be empty"
    );
}

#[test]
fn test_default_partition_table_structure() {
    let table = default_partition_table(Chip::Esp32, Some(0x400000)); // 4MB flash
    let partitions = table.partitions();

    assert_eq!(
        partitions.len(),
        3,
        "Default partition table should have exactly 3 partitions"
    );

    // Check partition names
    assert_eq!(partitions[0].name(), "nvs", "First partition should be NVS");
    assert_eq!(
        partitions[1].name(),
        "phy_init",
        "Second partition should be PHY init data"
    );
    assert_eq!(
        partitions[2].name(),
        "factory",
        "Third partition should be factory app"
    );
}

#[test]
fn test_default_partition_table_addresses() {
    let table = default_partition_table(Chip::Esp32, Some(0x400000)); // 4MB flash
    let partitions = table.partitions();

    // Check addresses
    assert_eq!(
        partitions[0].offset(),
        0x9000,
        "NVS partition should start at 0x9000"
    );
    assert_eq!(
        partitions[1].offset(),
        0xf000,
        "PHY init partition should start at 0xf000"
    );
    assert_eq!(
        partitions[2].offset(),
        0x10000,
        "Factory app partition should start at 0x10000"
    );

    // Check sizes
    assert_eq!(partitions[0].size(), 0x6000, "NVS partition should be 24KB");
    assert_eq!(
        partitions[1].size(),
        0x1000,
        "PHY init partition should be 4KB"
    );
    assert!(
        partitions[2].size() > 0,
        "Factory app partition should have positive size"
    );
}

#[test]
fn test_default_partition_table_flash_size_scaling() {
    let table_2mb = default_partition_table(Chip::Esp32, Some(0x200000)); // 2MB flash
    let table_4mb = default_partition_table(Chip::Esp32, Some(0x400000)); // 4MB flash

    let app_partition_2mb = &table_2mb.partitions()[2];
    let app_partition_4mb = &table_4mb.partitions()[2];

    // 4MB flash should have larger app partition
    assert!(
        app_partition_4mb.size() > app_partition_2mb.size(),
        "Larger flash should result in larger app partition"
    );

    // Both should have same offset for app partition
    assert_eq!(
        app_partition_2mb.offset(),
        app_partition_4mb.offset(),
        "App partition offset should be consistent across flash sizes"
    );
}

#[test]
fn test_default_partition_table_chip_variants() {
    // Test different chip variants to ensure they all work
    let chips = vec![
        Chip::Esp32,
        Chip::Esp32c2,
        Chip::Esp32c3,
        Chip::Esp32c5,
        Chip::Esp32c6,
        Chip::Esp32h2,
        Chip::Esp32p4,
        Chip::Esp32s2,
        Chip::Esp32s3,
    ];

    for chip in chips {
        let table = default_partition_table(chip, Some(0x400000));
        let partitions = table.partitions();

        assert_eq!(
            partitions.len(),
            3,
            "All chips should have 3 default partitions"
        );
        assert_eq!(
            partitions[0].name(),
            "nvs",
            "All chips should have NVS partition"
        );
        assert_eq!(
            partitions[1].name(),
            "phy_init",
            "All chips should have PHY init partition"
        );
        assert_eq!(
            partitions[2].name(),
            "factory",
            "All chips should have factory app partition"
        );
    }
}

#[test]
fn test_default_partition_table_no_flash_size() {
    // Test with None flash size (should use chip defaults)
    let table_esp32 = default_partition_table(Chip::Esp32, None);
    let table_esp32s2 = default_partition_table(Chip::Esp32s2, None);

    let app_partition_esp32 = &table_esp32.partitions()[2];
    let app_partition_esp32s2 = &table_esp32s2.partitions()[2];

    // ESP32-S2 should have smaller default app partition than ESP32
    assert!(
        app_partition_esp32.size() > app_partition_esp32s2.size(),
        "ESP32 should have larger default app partition than ESP32-S2"
    );
}

#[test]
fn test_default_partition_table_types() {
    use esp_idf_part::{AppType, DataType, SubType, Type};

    let table = default_partition_table(Chip::Esp32, Some(0x400000));
    let partitions = table.partitions();

    // Check partition types
    assert_eq!(partitions[0].ty(), Type::Data, "NVS should be Data type");
    assert_eq!(
        partitions[1].ty(),
        Type::Data,
        "PHY init should be Data type"
    );
    assert_eq!(partitions[2].ty(), Type::App, "Factory should be App type");

    // Check subtypes
    assert_eq!(
        partitions[0].subtype(),
        SubType::Data(DataType::Nvs),
        "NVS should have NVS subtype"
    );
    assert_eq!(
        partitions[1].subtype(),
        SubType::Data(DataType::Phy),
        "PHY init should have PHY subtype"
    );
    assert_eq!(
        partitions[2].subtype(),
        SubType::App(AppType::Factory),
        "Factory should have Factory subtype"
    );
}

#[test]
fn test_default_partition_table_max_size_limit() {
    // Test with very large flash size to ensure max partition size is respected
    let table = default_partition_table(Chip::Esp32, Some(32 * 1024 * 1024)); // 32MB flash
    let partitions = table.partitions();

    let app_partition = &partitions[2];

    // App partition should be limited to MAX_PARTITION_SIZE (16MB)
    assert!(
        app_partition.size() <= 16 * 1000 * 1024,
        "App partition should not exceed 16MB limit even with large flash"
    );
}

#[test]
fn test_bootloader_binaries_are_different_across_chips() {
    // Test that different chips have different bootloaders
    let esp32_bootloader = default_bootloader(Chip::Esp32, XtalFrequency::_40Mhz).unwrap();
    let esp32s3_bootloader = default_bootloader(Chip::Esp32s3, XtalFrequency::_40Mhz).unwrap();
    let esp32c3_bootloader = default_bootloader(Chip::Esp32c3, XtalFrequency::_40Mhz).unwrap();

    assert_ne!(
        esp32_bootloader, esp32s3_bootloader,
        "ESP32 and ESP32-S3 should have different bootloaders"
    );
    assert_ne!(
        esp32_bootloader, esp32c3_bootloader,
        "ESP32 and ESP32-C3 should have different bootloaders"
    );
    assert_ne!(
        esp32s3_bootloader, esp32c3_bootloader,
        "ESP32-S3 and ESP32-C3 should have different bootloaders"
    );
}

#[test]
fn test_bootloader_unsupported_frequencies() {
    // Test that unsupported frequency/chip combinations fail appropriately
    let unsupported_combinations = vec![
        (Chip::Esp32c3, XtalFrequency::_26Mhz),
        (Chip::Esp32c6, XtalFrequency::_26Mhz),
        (Chip::Esp32h2, XtalFrequency::_40Mhz),
        (Chip::Esp32s2, XtalFrequency::_26Mhz),
        (Chip::Esp32s3, XtalFrequency::_26Mhz),
    ];

    for (chip, freq) in unsupported_combinations {
        let result = default_bootloader(chip, freq);
        assert!(
            result.is_err(),
            "Unsupported combination {:?} + {:?} should return an error",
            chip,
            freq
        );
    }
}

#[test]
fn test_partition_table_consistency() {
    // Test that partition table is consistent across multiple calls
    let table1 = default_partition_table(Chip::Esp32, Some(0x400000));
    let table2 = default_partition_table(Chip::Esp32, Some(0x400000));

    let partitions1 = table1.partitions();
    let partitions2 = table2.partitions();

    assert_eq!(
        partitions1.len(),
        partitions2.len(),
        "Partition count should be consistent"
    );

    for (i, (p1, p2)) in partitions1.iter().zip(partitions2.iter()).enumerate() {
        assert_eq!(
            p1.name(),
            p2.name(),
            "Partition {} name should be consistent",
            i
        );
        assert_eq!(
            p1.offset(),
            p2.offset(),
            "Partition {} offset should be consistent",
            i
        );
        assert_eq!(
            p1.size(),
            p2.size(),
            "Partition {} size should be consistent",
            i
        );
        assert_eq!(
            p1.ty(),
            p2.ty(),
            "Partition {} type should be consistent",
            i
        );
        assert_eq!(
            p1.subtype(),
            p2.subtype(),
            "Partition {} subtype should be consistent",
            i
        );
    }
}
