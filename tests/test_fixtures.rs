//! Test fixtures and mock data for espbrew testing
//!
//! This module provides comprehensive test fixtures for creating mock ESP projects,
//! configuration files, device responses, and temporary test environments.

use serde_json::json;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use tempfile::TempDir;

/// Test fixture for creating various types of ESP projects
pub struct ProjectFixtures;

impl ProjectFixtures {
    /// Create a comprehensive Rust no_std ESP32 project
    pub fn create_rust_nostd_project(temp_dir: &Path, chip: &str) -> std::io::Result<()> {
        // Create directory structure
        fs::create_dir_all(temp_dir.join("src"))?;
        fs::create_dir_all(temp_dir.join(".cargo"))?;
        fs::create_dir_all(temp_dir.join("cfg"))?;

        // Create Cargo.toml with realistic ESP32 dependencies
        let cargo_toml = format!(
            r#"[package]
name = "esp32-test-project"
version = "0.1.0"
edition = "2021"
license = "MIT OR Apache-2.0"

[dependencies]
esp-hal = {{ version = "1.0.0-rc.0", features = ["{}"] }}
esp-backtrace = {{ version = "0.14.0", features = ["esp32{}", "exception-handler", "panic-handler", "println"] }}
esp-println = {{ version = "0.12.0", features = ["esp32{}"] }}
embedded-hal = "1.0.0"

[profile.dev]
# Rust debug is too slow.
# For debug builds always builds with some optimization
opt-level = "s"

[profile.release]
codegen-units = 1 # LLVM can perform better optimizations using a single thread
debug = 2
debug-assertions = false
incremental = false
lto = 'fat'
opt-level = 's'
overflow-checks = false
"#,
            chip, chip, chip
        );
        fs::write(temp_dir.join("Cargo.toml"), cargo_toml)?;

        // Create .cargo/config.toml with appropriate target
        let target = match chip {
            "s2" => "xtensa-esp32s2-none-elf",
            "s3" => "xtensa-esp32s3-none-elf",
            "c2" | "c3" | "c6" | "h2" => "riscv32imac-unknown-none-elf",
            _ => "xtensa-esp32-none-elf",
        };

        let cargo_config = format!(
            r#"[build]
target = "{}"

[target.{}]
runner = "espflash flash --monitor"

[env]
# Note: this variable is not used by the xtensa-esp32-none-elf target
DEFMT_LOG = "trace"
"#,
            target, target
        );
        fs::write(temp_dir.join(".cargo/config.toml"), cargo_config)?;

        // Create rust-toolchain.toml
        let rust_toolchain = r#"[toolchain]
channel = "nightly-2024-10-01"
"#;
        fs::write(temp_dir.join("rust-toolchain.toml"), rust_toolchain)?;

        // Create realistic main.rs
        let main_rs = format!(
            r#"#![no_std]
#![no_main]

use esp_backtrace as _;
use esp_hal::{{
    clock::ClockControl,
    delay::Delay,
    gpio::{{Io, Level, Output}},
    peripherals::Peripherals,
    prelude::*,
    system::SystemControl,
    timer::timg::TimerGroup,
}};
use esp_println::println;

#[entry]
fn main() -> ! {{
    let peripherals = Peripherals::take();
    let system = SystemControl::new(peripherals.SYSTEM);
    
    let clocks = ClockControl::max(system.clock_control).freeze();
    let delay = Delay::new(&clocks);
    
    esp_println::logger::init_logger_from_env();
    
    let io = Io::new(peripherals.GPIO, peripherals.IO_MUX);
    
    // This is the built-in LED on many ESP32{} boards
    let mut led = Output::new(io.pins.gpio2, Level::Low);
    
    let timer_group0 = TimerGroup::new(peripherals.TIMG0, &clocks, None);
    let mut wdt0 = timer_group0.wdt;
    wdt0.disable();
    let timer_group1 = TimerGroup::new(peripherals.TIMG1, &clocks, None);
    let mut wdt1 = timer_group1.wdt;
    wdt1.disable();
    
    println!("ESP32{} Test Project Started!");
    
    loop {{
        led.set_high();
        delay.delay_millis(500u32);
        led.set_low();
        delay.delay_millis(500u32);
        println!("LED blink!");
    }}
}}
"#,
            chip, chip
        );
        fs::write(temp_dir.join("src/main.rs"), main_rs)?;

        // Create build.rs if needed for some chips
        if matches!(chip, "s3" | "c6") {
            let build_rs = r#"fn main() {
    // This is required for ESP32-S3 and ESP32-C6 builds
    println!("cargo:rustc-link-arg-bins=-Tlinkall.x");
    println!("cargo:rustc-link-arg-bins=-Trom_functions.x");
}
"#;
            fs::write(temp_dir.join("build.rs"), build_rs)?;
        }

        Ok(())
    }

    /// Create an ESP-IDF project
    pub fn create_esp_idf_project(temp_dir: &Path, chip: &str) -> std::io::Result<()> {
        // Create directory structure
        fs::create_dir_all(temp_dir.join("main"))?;
        fs::create_dir_all(temp_dir.join("components"))?;

        // Create CMakeLists.txt
        let cmake_main = r#"cmake_minimum_required(VERSION 3.16)
include($ENV{IDF_PATH}/tools/cmake/project.cmake)
project(esp32-test-project)
"#;
        fs::write(temp_dir.join("CMakeLists.txt"), cmake_main)?;

        // Create main/CMakeLists.txt
        let cmake_main_component = r#"idf_component_register(SRCS "main.c"
                    INCLUDE_DIRS ".")
"#;
        fs::write(temp_dir.join("main/CMakeLists.txt"), cmake_main_component)?;

        // Create main/main.c
        let main_c = format!(
            r#"#include <stdio.h>
#include "freertos/FreeRTOS.h"
#include "freertos/task.h"
#include "driver/gpio.h"
#include "esp_log.h"

static const char *TAG = "ESP32{}_TEST";

#define BLINK_GPIO 2

void app_main(void)
{{
    ESP_LOGI(TAG, "ESP32{} Test Project Started!");
    
    gpio_reset_pin(BLINK_GPIO);
    gpio_set_direction(BLINK_GPIO, GPIO_MODE_OUTPUT);
    
    while(1) {{
        gpio_set_level(BLINK_GPIO, 1);
        vTaskDelay(500 / portTICK_PERIOD_MS);
        gpio_set_level(BLINK_GPIO, 0);
        vTaskDelay(500 / portTICK_PERIOD_MS);
        ESP_LOGI(TAG, "LED blink!");
    }}
}}
"#,
            chip.to_uppercase(),
            chip.to_uppercase()
        );
        fs::write(temp_dir.join("main/main.c"), main_c)?;

        // Create sdkconfig with chip-specific settings
        let sdkconfig = format!(
            r#"CONFIG_IDF_TARGET_ESP32{}=y
CONFIG_IDF_TARGET="esp32{}"
CONFIG_ESPTOOLPY_FLASHMODE_DIO=y
CONFIG_ESPTOOLPY_FLASHFREQ_80M=y
CONFIG_ESPTOOLPY_FLASHSIZE_4MB=y
CONFIG_BOOTLOADER_LOG_LEVEL_INFO=y
CONFIG_LOG_DEFAULT_LEVEL_INFO=y
CONFIG_FREERTOS_HZ=1000
"#,
            chip.to_uppercase(),
            chip
        );
        fs::write(temp_dir.join("sdkconfig"), sdkconfig.clone())?;

        // Create sdkconfig.defaults file for board discovery
        let board_name = if chip.is_empty() {
            "esp32"
        } else {
            &format!("esp32{}", chip)
        };
        fs::write(
            temp_dir.join(format!("sdkconfig.defaults.{}", board_name)),
            sdkconfig,
        )?;

        Ok(())
    }

    /// Create an Arduino project
    pub fn create_arduino_project(temp_dir: &Path, project_name: &str) -> std::io::Result<()> {
        // Create Arduino sketch file
        let ino_content = format!(
            r#"/*
 * {} - ESP32 Arduino Project
 * Generated for testing purposes
 */

#include <WiFi.h>

// Pin definitions
const int LED_PIN = 2;
const int BUTTON_PIN = 0;

void setup() {{
  Serial.begin(115200);
  
  pinMode(LED_PIN, OUTPUT);
  pinMode(BUTTON_PIN, INPUT_PULLUP);
  
  Serial.println("{} started!");
  Serial.print("Chip Model: ");
  Serial.println(ESP.getChipModel());
  Serial.print("CPU Frequency: ");
  Serial.print(ESP.getCpuFreqMHz());
  Serial.println(" MHz");
}}

void loop() {{
  static unsigned long lastBlink = 0;
  static bool ledState = false;
  
  if (millis() - lastBlink >= 1000) {{
    ledState = !ledState;
    digitalWrite(LED_PIN, ledState);
    Serial.println(ledState ? "LED ON" : "LED OFF");
    lastBlink = millis();
  }}
  
  // Check button press
  if (digitalRead(BUTTON_PIN) == LOW) {{
    Serial.println("Button pressed!");
    delay(200); // Simple debounce
  }}
  
  delay(50);
}}
"#,
            project_name, project_name
        );

        let sketch_filename = format!("{}.ino", project_name.replace("-", "_"));
        fs::write(temp_dir.join(sketch_filename), ino_content)?;

        // Create boards.json for ESP32 boards
        let boards_json = json!({
            "project_type": "arduino",
            "boards": [
                {
                    "name": "ESP32",
                    "target": "esp32",
                    "fqbn": "esp32:esp32:esp32",
                    "description": "ESP32 Development Board"
                },
                {
                    "name": "ESP32-S2",
                    "target": "esp32s2",
                    "fqbn": "esp32:esp32:esp32s2",
                    "description": "ESP32-S2 Development Board"
                },
                {
                    "name": "ESP32-S3",
                    "target": "esp32s3",
                    "fqbn": "esp32:esp32:esp32s3",
                    "description": "ESP32-S3 Development Board"
                },
                {
                    "name": "ESP32-C3",
                    "target": "esp32c3",
                    "fqbn": "esp32:esp32:esp32c3",
                    "description": "ESP32-C3 Development Board"
                }
            ]
        });
        fs::write(
            temp_dir.join("boards.json"),
            serde_json::to_string_pretty(&boards_json)?,
        )?;

        // Create Arduino sketch configuration
        let sketch_json = json!({
            "cpu": {
                "fqbn": "esp32:esp32:esp32",
                "name": "ESP32 Dev Module",
                "type": "ESP32"
            },
            "secrets": [],
            "included_libs": []
        });
        fs::write(
            temp_dir.join("sketch.json"),
            serde_json::to_string_pretty(&sketch_json)?,
        )?;

        Ok(())
    }

    /// Create a MicroPython project
    pub fn create_micropython_project(temp_dir: &Path) -> std::io::Result<()> {
        // Create main.py
        let main_py = r#""""
MicroPython ESP32 Test Project
"""
import machine
import time
import network

# Configuration
LED_PIN = 2
BUTTON_PIN = 0

def setup():
    """Initialize hardware"""
    global led, button
    led = machine.Pin(LED_PIN, machine.Pin.OUT)
    button = machine.Pin(BUTTON_PIN, machine.Pin.IN, machine.Pin.PULL_UP)
    
    print("MicroPython ESP32 Test Project started!")
    print(f"Frequency: {machine.freq()} Hz")
    
    # Show network interfaces
    wlan = network.WLAN(network.STA_IF)
    print(f"MAC Address: {':'.join(['%02x' % b for b in wlan.config('mac')])}")

def blink_led():
    """Blink LED function"""
    led.value(1)
    time.sleep_ms(500)
    led.value(0)
    time.sleep_ms(500)
    print("LED blink!")

def check_button():
    """Check button press"""
    if not button.value():
        print("Button pressed!")
        time.sleep_ms(200)  # Simple debounce

def main_loop():
    """Main application loop"""
    setup()
    
    while True:
        blink_led()
        check_button()

if __name__ == "__main__":
    main_loop()
"#;
        fs::write(temp_dir.join("main.py"), main_py)?;

        // Create boot.py
        let boot_py = r#"# boot.py -- run on boot-up
import esp
import network
import gc

# Disable ESP32 vendor OS debugging
esp.osdebug(None)

# Enable automatic garbage collection
gc.collect()

print("Boot sequence completed")
"#;
        fs::write(temp_dir.join("boot.py"), boot_py)?;

        // Create requirements.txt
        let requirements = r#"# MicroPython ESP32 Requirements
# This file lists the MicroPython libraries required for this project

# Core ESP32 modules (built-in)
# - machine
# - network
# - time
# - gc

# Optional external libraries can be listed here
# micropython-lib packages can be installed with mip or upip
"#;
        fs::write(temp_dir.join("requirements.txt"), requirements)?;

        Ok(())
    }

    /// Create a PlatformIO project
    #[allow(dead_code)]
    pub fn create_platformio_project(
        temp_dir: &Path,
        framework: &str,
        board: &str,
    ) -> std::io::Result<()> {
        // Create directory structure
        fs::create_dir_all(temp_dir.join("src"))?;
        fs::create_dir_all(temp_dir.join("lib"))?;
        fs::create_dir_all(temp_dir.join("test"))?;
        fs::create_dir_all(temp_dir.join("include"))?;

        // Create platformio.ini
        let platformio_ini = format!(
            r#"[env:{}]
platform = espressif32
board = {}
framework = {}
monitor_speed = 115200
upload_speed = 921600

; Build options
build_flags =
    -D CORE_DEBUG_LEVEL=0
    -D ARDUINO_USB_CDC_ON_BOOT=0

; Library dependencies
lib_deps =

; Monitor options
monitor_port = /dev/cu.usbmodem*
monitor_filters = esp32_exception_decoder
"#,
            board, board, framework
        );
        fs::write(temp_dir.join("platformio.ini"), platformio_ini)?;

        // Create main source file based on framework
        let main_content = match framework {
            "arduino" => {
                r#"#include <Arduino.h>

#define LED_PIN 2
#define BUTTON_PIN 0

void setup() {
  Serial.begin(115200);
  
  pinMode(LED_PIN, OUTPUT);
  pinMode(BUTTON_PIN, INPUT_PULLUP);
  
  Serial.println("PlatformIO Arduino ESP32 Project started!");
}

void loop() {
  static unsigned long lastBlink = 0;
  static bool ledState = false;
  
  if (millis() - lastBlink >= 1000) {
    ledState = !ledState;
    digitalWrite(LED_PIN, ledState);
    Serial.println(ledState ? "LED ON" : "LED OFF");
    lastBlink = millis();
  }
  
  if (digitalRead(BUTTON_PIN) == LOW) {
    Serial.println("Button pressed!");
    delay(200);
  }
  
  delay(50);
}
"#
            }
            "espidf" => {
                r#"#include <stdio.h>
#include "freertos/FreeRTOS.h"
#include "freertos/task.h"
#include "driver/gpio.h"
#include "esp_log.h"

static const char *TAG = "PLATFORMIO_TEST";

#define BLINK_GPIO 2

extern "C" void app_main(void)
{
    ESP_LOGI(TAG, "PlatformIO ESP-IDF Project started!");
    
    gpio_reset_pin(BLINK_GPIO);
    gpio_set_direction(BLINK_GPIO, GPIO_MODE_OUTPUT);
    
    while(1) {
        gpio_set_level(BLINK_GPIO, 1);
        vTaskDelay(500 / portTICK_PERIOD_MS);
        gpio_set_level(BLINK_GPIO, 0);
        vTaskDelay(500 / portTICK_PERIOD_MS);
        ESP_LOGI(TAG, "LED blink!");
    }
}
"#
            }
            _ => {
                r#"// Unknown framework - generic C++ template
#include <stdio.h>

int main() {
    printf("PlatformIO Generic Project started!\n");
    return 0;
}
"#
            }
        };

        let extension = match framework {
            "arduino" | "espidf" => "cpp",
            _ => "c",
        };

        fs::write(
            temp_dir.join(format!("src/main.{}", extension)),
            main_content,
        )?;

        // Create a simple library example
        fs::create_dir_all(temp_dir.join("lib/SimpleLib/src"))?;
        let lib_header = r#"#ifndef SIMPLE_LIB_H
#define SIMPLE_LIB_H

class SimpleLib {
public:
    void begin();
    void blink(int pin);
private:
    bool initialized;
};

#endif
"#;
        fs::write(temp_dir.join("lib/SimpleLib/src/SimpleLib.h"), lib_header)?;

        let lib_source = r#"#include "SimpleLib.h"
#include <Arduino.h>

void SimpleLib::begin() {
    initialized = true;
}

void SimpleLib::blink(int pin) {
    if (!initialized) return;
    
    digitalWrite(pin, HIGH);
    delay(100);
    digitalWrite(pin, LOW);
}
"#;
        fs::write(temp_dir.join("lib/SimpleLib/src/SimpleLib.cpp"), lib_source)?;

        // Create library.json
        let library_json = json!({
            "name": "SimpleLib",
            "version": "1.0.0",
            "description": "Simple library for testing",
            "authors": {
                "name": "Test Author"
            },
            "frameworks": [framework],
            "platforms": ["espressif32"]
        });
        fs::write(
            temp_dir.join("lib/SimpleLib/library.json"),
            serde_json::to_string_pretty(&library_json)?,
        )?;

        Ok(())
    }
}

/// Mock device responses for testing flash and monitor operations
pub struct MockDeviceResponses;

impl MockDeviceResponses {
    /// Create mock serial device responses
    pub fn get_esp32_bootloader_response() -> Vec<u8> {
        let response = r#"ESP-ROM:esp32s3-20210327
Build:Mar 27 2021
rst:0x1 (POWERON),boot:0x8 (SPI_FAST_FLASH_BOOT)
SPIWP:0xee
mode:DIO, clock div:1
load:0x3fce3810,len:0x178c
load:0x403c9700,len:0x4
load:0x403c9704,len:0xc40
load:0x403cc700,len:0x2da0
entry 0x403c9914
"#;
        response.as_bytes().to_vec()
    }

    /// Create mock application output
    pub fn get_esp32_application_output() -> Vec<u8> {
        let output = r#"I (29) boot: ESP-IDF v5.1.2-dirty 2nd stage bootloader
I (29) boot: compile time 14:46:43
I (29) boot: Multicore bootloader
I (33) boot: chip revision: v0.1
I (36) boot.esp32s3: Boot SPI Speed : 80MHz
I (41) boot.esp32s3: SPI Mode       : DIO
I (46) boot.esp32s3: SPI Flash Size : 8MB
I (51) boot: Enabling RNG early entropy source...
I (56) boot: Partition Table:
I (60) boot: ## Label            Usage          Type ST Offset   Length
I (67) boot:  0 nvs              WiFi data        01 02 00009000 00006000
I (75) boot:  1 phy_init         RF data          01 01 0000f000 00001000
I (82) boot:  2 factory          factory app      00 00 00010000 00100000
I (90) boot: End of partition table
I (94) esp_image: segment 0: paddr=00010020 vaddr=3c020020 size=08e28h ( 36392) map
I (108) esp_image: segment 1: paddr=00018e50 vaddr=3fc91800 size=02634h (  9780) load
Hello ESP32-S3!
LED blink!
LED blink!
"#;
        output.as_bytes().to_vec()
    }

    /// Create mock flash operation response
    pub fn get_flash_success_response() -> HashMap<String, String> {
        let mut responses = HashMap::new();
        responses.insert("connecting".to_string(), "Connecting...".to_string());
        responses.insert(
            "chip_detect".to_string(),
            "Chip is ESP32-S3 (revision v0.1)".to_string(),
        );
        responses.insert(
            "flash_begin".to_string(),
            "Configuring flash size...".to_string(),
        );
        responses.insert(
            "flash_progress".to_string(),
            "Writing at 0x00010000... (100%)".to_string(),
        );
        responses.insert(
            "flash_complete".to_string(),
            "Hash of data verified.".to_string(),
        );
        responses.insert(
            "reset".to_string(),
            "Hard resetting via RTS pin...".to_string(),
        );
        responses
    }

    /// Create mock error responses
    #[allow(dead_code)]
    pub fn get_device_not_found_error() -> String {
        "Error: Could not open /dev/cu.usbmodem14101, the port doesn't exist".to_string()
    }

    #[allow(dead_code)]
    pub fn get_connection_failed_error() -> String {
        "Error: Failed to connect to ESP32: Timed out waiting for packet header".to_string()
    }

    #[allow(dead_code)]
    pub fn get_flash_failed_error() -> String {
        "Error: Flash operation failed at address 0x10000".to_string()
    }
}

/// Configuration file fixtures
pub struct ConfigFixtures;

impl ConfigFixtures {
    /// Create various espbrew.toml configurations
    pub fn create_espbrew_config(
        temp_dir: &Path,
        project_type: &str,
        target: &str,
    ) -> std::io::Result<()> {
        let config_content = match project_type {
            "rust_nostd" => format!(
                r#"[project]
name = "test-rust-project"
type = "rust_nostd"
target = "{}"
version = "1.0.0"
description = "Test Rust ESP32 project"

[build]
strategy = "sequential"
release = true
features = []
toolchain = "nightly-2024-10-01"

[flash]
monitor = true
reset = true
erase_flash = false
baudrate = 115200

[logging]
level = "info"
color = true
timestamp = true
"#,
                target
            ),

            "esp_idf" => format!(
                r#"[project]
name = "test-espidf-project"
type = "esp_idf"
target = "{}"
version = "1.0.0"
description = "Test ESP-IDF project"

[build]
strategy = "idf_build_apps"
release = true
cmake_args = []
sdkconfig_defaults = []

[flash]
monitor = true
reset = true
erase_flash = false
baudrate = 115200
partition_table = "default"

[idf]
version = "v5.1.2"
components = []
"#,
                target
            ),

            "arduino" => format!(
                r#"[project]
name = "test-arduino-project"
type = "arduino"
target = "{}"
version = "1.0.0"
description = "Test Arduino ESP32 project"

[build]
strategy = "sequential"
release = false
board = "esp32dev"
libraries = []

[flash]
monitor = true
reset = true
baudrate = 115200
upload_speed = 921600

[arduino]
ide_version = "2.0.0"
core_version = "2.0.11"
"#,
                target
            ),

            _ => format!(
                r#"[project]
name = "test-generic-project"
type = "{}"
target = "{}"
version = "1.0.0"
description = "Test generic ESP32 project"

[build]
strategy = "sequential"

[flash]
monitor = true
reset = true
"#,
                project_type, target
            ),
        };

        fs::write(temp_dir.join("espbrew.toml"), config_content)?;
        Ok(())
    }

    /// Create invalid configuration for error testing
    pub fn create_invalid_config(temp_dir: &Path, error_type: &str) -> std::io::Result<()> {
        let invalid_content = match error_type {
            "syntax_error" => {
                r#"[project
name = "invalid-syntax"
type = "rust_nostd"
"#
            }
            "missing_required" => {
                r#"[project]
description = "Missing required fields"
"#
            }
            "invalid_type" => {
                r#"[project]
name = "invalid-type-project"
type = "non_existent_type"
target = "esp32"
"#
            }
            "invalid_target" => {
                r#"[project]
name = "invalid-target-project"
type = "rust_nostd"
target = "non_existent_chip"
"#
            }
            _ => "completely invalid content that is not TOML at all!",
        };

        fs::write(temp_dir.join("espbrew.toml"), invalid_content)?;
        Ok(())
    }
}

/// Helper functions for creating test environments
pub struct TestEnvironment;

impl TestEnvironment {
    /// Create a complete test workspace with multiple projects
    pub fn create_test_workspace() -> std::io::Result<TempDir> {
        let temp_workspace = TempDir::new()?;
        let workspace_path = temp_workspace.path();

        // Create multiple test projects
        let rust_project = workspace_path.join("rust-esp32s3-project");
        fs::create_dir_all(&rust_project)?;
        ProjectFixtures::create_rust_nostd_project(&rust_project, "s3")?;
        ConfigFixtures::create_espbrew_config(&rust_project, "rust_nostd", "esp32s3")?;

        let arduino_project = workspace_path.join("arduino-esp32-project");
        fs::create_dir_all(&arduino_project)?;
        ProjectFixtures::create_arduino_project(&arduino_project, "arduino-esp32-project")?;
        ConfigFixtures::create_espbrew_config(&arduino_project, "arduino", "esp32")?;

        let idf_project = workspace_path.join("esp-idf-project");
        fs::create_dir_all(&idf_project)?;
        ProjectFixtures::create_esp_idf_project(&idf_project, "")?;
        ConfigFixtures::create_espbrew_config(&idf_project, "esp_idf", "esp32")?;

        let micropython_project = workspace_path.join("micropython-project");
        fs::create_dir_all(&micropython_project)?;
        ProjectFixtures::create_micropython_project(&micropython_project)?;
        ConfigFixtures::create_espbrew_config(&micropython_project, "micropython", "esp32")?;

        // Create an invalid project for testing error handling
        let invalid_project = workspace_path.join("invalid-project");
        fs::create_dir_all(&invalid_project)?;
        fs::write(
            invalid_project.join("random.txt"),
            "This is not a valid ESP project",
        )?;
        ConfigFixtures::create_invalid_config(&invalid_project, "syntax_error")?;

        Ok(temp_workspace)
    }

    /// Create a project with specific characteristics for targeted testing
    #[allow(dead_code)]
    pub fn create_targeted_test_project(
        project_type: &str,
        chip: &str,
        with_errors: bool,
    ) -> std::io::Result<TempDir> {
        let temp_dir = TempDir::new()?;
        let project_path = temp_dir.path();

        // Create the project based on type
        match project_type {
            "rust_nostd" => {
                ProjectFixtures::create_rust_nostd_project(project_path, chip)?;
                if with_errors {
                    // Introduce syntax errors for testing
                    let bad_main = r#"#![no_std]
#![no_main]

// This will cause compilation errors
use non_existent_crate::*;
invalid_syntax_here
"#;
                    fs::write(project_path.join("src/main.rs"), bad_main)?;
                }
            }
            "arduino" => {
                ProjectFixtures::create_arduino_project(project_path, "test-project")?;
                if with_errors {
                    // Create invalid Arduino sketch
                    let bad_sketch = r#"// Invalid Arduino sketch
#include <NonExistentLibrary.h>

void setup() {
  invalid_function_call();
}

// Missing loop() function
"#;
                    fs::write(project_path.join("test_project.ino"), bad_sketch)?;
                }
            }
            "esp_idf" => {
                ProjectFixtures::create_esp_idf_project(project_path, chip)?;
                if with_errors {
                    // Create invalid CMakeLists.txt
                    let bad_cmake = r#"# Invalid CMakeLists.txt
cmake_minimum_required(VERSION 99.99)  # Impossible version
invalid_cmake_command()
"#;
                    fs::write(project_path.join("CMakeLists.txt"), bad_cmake)?;
                }
            }
            _ => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    format!("Unsupported project type: {}", project_type),
                ));
            }
        }

        // Always create espbrew config
        if with_errors {
            ConfigFixtures::create_invalid_config(project_path, "syntax_error")?;
        } else {
            ConfigFixtures::create_espbrew_config(
                project_path,
                project_type,
                &format!("esp32{}", chip),
            )?;
        }

        Ok(temp_dir)
    }

    /// Validate that a test project has the expected structure
    pub fn validate_project_structure(project_path: &Path, project_type: &str) -> bool {
        match project_type {
            "rust_nostd" => {
                project_path.join("Cargo.toml").exists()
                    && project_path.join(".cargo/config.toml").exists()
                    && project_path.join("src/main.rs").exists()
                    && project_path.join("espbrew.toml").exists()
            }
            "arduino" => {
                let has_ino_file = project_path
                    .read_dir()
                    .map(|mut entries| {
                        entries.any(|entry| {
                            entry
                                .map(|e| e.path().extension().map_or(false, |ext| ext == "ino"))
                                .unwrap_or(false)
                        })
                    })
                    .unwrap_or(false);

                has_ino_file
                    && project_path.join("boards.json").exists()
                    && project_path.join("espbrew.toml").exists()
            }
            "esp_idf" => {
                project_path.join("CMakeLists.txt").exists()
                    && project_path.join("main/CMakeLists.txt").exists()
                    && project_path.join("main/main.c").exists()
                    && project_path.join("espbrew.toml").exists()
            }
            "micropython" => {
                project_path.join("main.py").exists()
                    && project_path.join("boot.py").exists()
                    && project_path.join("espbrew.toml").exists()
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod fixture_tests {
    use super::*;

    #[test]
    fn test_rust_project_fixture() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let path = temp_dir.path();

        ProjectFixtures::create_rust_nostd_project(path, "s3").expect("Failed to create project");

        assert!(path.join("Cargo.toml").exists());
        assert!(path.join(".cargo/config.toml").exists());
        assert!(path.join("src/main.rs").exists());
        assert!(path.join("rust-toolchain.toml").exists());

        // Verify content contains expected elements
        let cargo_content = fs::read_to_string(path.join("Cargo.toml")).unwrap();
        assert!(cargo_content.contains("esp-hal"));
        assert!(cargo_content.contains("esp32s3"));
    }

    #[test]
    fn test_arduino_project_fixture() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let path = temp_dir.path();

        ProjectFixtures::create_arduino_project(path, "test-project")
            .expect("Failed to create project");

        assert!(path.join("test_project.ino").exists());
        assert!(path.join("boards.json").exists());
        assert!(path.join("sketch.json").exists());
    }

    #[test]
    fn test_complete_workspace() {
        let workspace =
            TestEnvironment::create_test_workspace().expect("Failed to create test workspace");
        let workspace_path = workspace.path();

        // Verify all projects were created
        assert!(workspace_path.join("rust-esp32s3-project").exists());
        assert!(workspace_path.join("arduino-esp32-project").exists());
        assert!(workspace_path.join("esp-idf-project").exists());
        assert!(workspace_path.join("micropython-project").exists());
        assert!(workspace_path.join("invalid-project").exists());

        // Verify project structure validation works
        assert!(TestEnvironment::validate_project_structure(
            &workspace_path.join("rust-esp32s3-project"),
            "rust_nostd"
        ));
        assert!(TestEnvironment::validate_project_structure(
            &workspace_path.join("arduino-esp32-project"),
            "arduino"
        ));
        assert!(!TestEnvironment::validate_project_structure(
            &workspace_path.join("invalid-project"),
            "rust_nostd"
        ));
    }

    #[test]
    fn test_mock_device_responses() {
        let bootloader_response = MockDeviceResponses::get_esp32_bootloader_response();
        assert!(!bootloader_response.is_empty());

        let app_output = MockDeviceResponses::get_esp32_application_output();
        let app_string = String::from_utf8_lossy(&app_output);
        assert!(app_string.contains("ESP-IDF"));
        assert!(app_string.contains("LED blink"));

        let flash_responses = MockDeviceResponses::get_flash_success_response();
        assert!(flash_responses.contains_key("connecting"));
        assert!(flash_responses.contains_key("flash_complete"));
    }

    #[test]
    fn test_config_fixtures() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let path = temp_dir.path();

        ConfigFixtures::create_espbrew_config(path, "rust_nostd", "esp32s3")
            .expect("Failed to create config");

        let config_path = path.join("espbrew.toml");
        assert!(config_path.exists());

        let content = fs::read_to_string(config_path).unwrap();
        assert!(content.contains("rust_nostd"));
        assert!(content.contains("esp32s3"));
        assert!(content.contains("[build]"));
        assert!(content.contains("[flash]"));
    }
}
