## 🍺 ESPBrew - ESP32 Multi-Board Build Manager

A comprehensive ESP32 development tool featuring TUI/CLI build management and a network-based server for remote board management. It automatically discovers board configurations, generates build scripts, provides real-time build monitoring, and offers a web dashboard for ESP32 board detection and flashing.

![ESP32 Multi-Board Support](https://img.shields.io/badge/ESP32-Multi--Board-blue)
![Rust](https://img.shields.io/badge/rust-1.70+-orange.svg)
![License](https://img.shields.io/badge/license-MIT-green.svg)

## ✨ Features

### 🦀 Rust no_std Embedded Projects Support (⭐ NEW)
- **Native Rust Support**: Full support for Rust no_std embedded projects with Cargo.toml detection
- **Automatic Chip Detection**: Detects ESP32 chip type (esp32s3, esp32c6, esp32p4, etc.) from target directory paths
- **Multi-Component Firmware**: Advanced firmware extraction using espflash crate for bootloader, partition table, and application
- **Remote Flashing**: Complete remote flashing solution with multipart upload to ESPBrew server
- **Framework Compatibility**: Works with esp-hal, Embassy, embedded-hal, and other Rust embedded frameworks
- **Release Builds**: Automatically enforces `--release` builds for optimal embedded performance
- **Smart Fallbacks**: Robust error handling with fallback from individual components to merged firmware images
- **Production Ready**: Handles complex firmware layouts and large binaries (4MB+) with progress monitoring

### ESPBrew Server (Remote Board Management)
- **Remote Board Discovery**: Network-based ESP32 board detection and management with hardware-based unique identification
- **Remote Serial Monitoring**: Real-time WebSocket-based serial monitoring with automatic reconnection
- **Remote Flashing**: Multi-binary ESP-IDF flashing via REST API with bootloader, partition table, and application support
- **Cross-Platform Support**: Detects ESP32 boards on macOS and Linux via USB with enhanced chip detection
- **Web Dashboard**: Beautiful web interface for board monitoring, flashing, and real-time log streaming
- **Board Configuration Management**: Persistent board type assignments with RON-based configuration
- **Reset Integration**: Remote board reset capability during monitoring to capture complete boot sequences
- **Session Management**: Automatic monitoring session tracking with keep-alive and cleanup functionality
- **Automatic Board Type Discovery**: Auto-discovers board types from `sdkconfig.defaults.*` files
- **Background Enhancement**: Non-blocking native espflash integration for detailed board information
- **Smart Caching**: 1-hour cache for enhanced board information to improve performance
- **Real-time Scanning**: Automatic periodic board discovery every 30 seconds
- **RESTful API**: Complete API for board listing, configuration management, remote flashing, and monitoring
- **Quick Shutdown**: Graceful server shutdown with Ctrl+C (handles hanging connections)
- **Device Detection**: Supports `/dev/cu.usbmodem*`, `/dev/tty.usbmodem*` (macOS) and `/dev/ttyUSB*`, `/dev/ttyACM*` (Linux)

## 🆕 Board Configuration and Management Features

### Persistent Board Configuration

- ESPBrew now supports **persistent board configuration management**, saving board types and assignments to a human-readable RON file at:
  
  ```
  ~/.config/espbrew/espbrew-boards.ron
  ```

- This file includes:
  - All discovered board types (parsed from `sdkconfig.defaults.*` files in the snow directory)
  - Board assignments that map physical boards (using unique hardware IDs) to board types
  - Server configuration overrides and versioning info

- This enables consistent mapping of physical boards to their project roles, surviving restarts and hardware reconnections.

### Automatic Board Type Discovery

- Upon startup, ESPBrew **automatically discovers all board types** by scanning the `../snow` directory for `sdkconfig.defaults.*` files.

- Board IDs and names are generated from file names, and chip types are inferred (e.g., `esp32_s3_eye` → `esp32s3` chip type).

- If the snow directory is missing, ESPBrew falls back to default minimal board types such as generic ESP32, ESP32-S3, and ESP32-C6.

### Board Assignment System

- Using unique board hardware IDs (e.g., MAC addresses), users can **assign physical boards to specific board types**.

- Assignments allow assigning **logical names** to boards, improving readability and tracking.

- The server applies these assignments dynamically, showing board type info and logical names in the API and UI.

- Assignments are timestamped and stored persistently.

### Board Management RESTful API

The ESPBrew Server exposes new API endpoints for managing boards and their assignments:

| Endpoint                                | Method | Description                              |
|---------------------------------------|--------|--------------------------------------|
| `/api/v1/board-types`                  | GET    | List all available board types        |
| `/api/v1/assign-board`                 | POST   | Assign a physical board to a board type (with optional logical name) |
| `/api/v1/assign-board/{unique_id}`    | DELETE | Remove a board assignment by unique ID |

#### Example Assign Board Request

```json
{
  "board_unique_id": "MAC8CBFEAB34E08",
  "board_type_id": "esp32_c6_devkit",
  "logical_name": "ESP32-C6 DevKit Board"
}
```

### Enhanced Board Information Integration

- The enhanced board info caching system now **integrates with the assignment data**, applying user-defined board types and logical names alongside hardware detection results.

- This enables consistent, rich board info presentation in APIs and UI, facilitating large-scale multi-board setups.

### How to Use These Features

1. Start the ESPBrew Server as usual:

   ```bash
   cargo run --bin espbrew-server --release
   ```

2. The server will auto-discover board types and existing assignments (or create a new config file if none exist).

3. Query available board types:

   ```bash
   curl http://localhost:8080/api/v1/board-types
   ```

4. Assign a physical board to a board type:

   ```bash
   curl -X POST http://localhost:8080/api/v1/assign-board \
        -H "Content-Type: application/json" \
        -d '{
          "board_unique_id": "MACXXXXXXXXXXXX",
          "board_type_id": "esp32_c6_devkit",
          "logical_name": "My ESP32-C6 Board"
        }'
   ```

5. Unassign a board if needed:

   ```bash
   curl -X DELETE http://localhost:8080/api/v1/assign-board/MACXXXXXXXXXXXX
   ```

6. Check boards and their assignments via the existing `/api/v1/boards` endpoint. Assigned board type info and logical names will appear.

### Configuration File Location

- The persistent configuration is stored in:

  ```
  ~/.config/espbrew/espbrew-boards.ron
  ```

- Feel free to inspect or manually edit this file to manage board types and assignments directly.

This new system enables robust and persistent multi-board management, ideal for test farms, CI/CD setups, and team collaborations involving diverse ESP32 hardware.

### Multi-Board Management
- Auto-Discovery: Automatically finds all `sdkconfig.defaults.*` configurations
- Board-Specific Builds: Each board gets its own build directory (`build.{board_name}`)
- Target Detection: Automatically detects ESP32 target (S3, P4, C6, C3) from config files
- Flexible Build Strategies: Choose between sequential (safe, default) and parallel (faster) builds
- Conflict-Free Configuration: Uses `-D SDKCONFIG` parameter to prevent sdkconfig conflicts
- Component Manager Awareness: Sequential builds avoid ESP-IDF Component Manager lock conflicts

### Component Management
- Component Discovery: Automatically finds components in `components/` and `managed_components/` directories
- Visual Indicators: Distinguishes between local (🔧) and managed (📦) components
- Component Actions: Move, clone from repository, remove, or open in editor
- Smart Repository Cloning: Automatically converts `git://` URLs to `https://` for compatibility
- Manifest Parsing: Reads `idf_component.yml` files to extract repository information

### Dual Interface
- Interactive TUI: Terminal interface with real-time monitoring and component management
- CLI Mode: List components and boards, or build all boards
- Live Logs: Real-time build output streaming
- Build Status: Color-coded status indicators (⏳ Pending, ⚙️ Building, ✅ Success, ❌ Failed)

### Script Generation
- Build Scripts: Generates `build_{board}.sh` scripts
- Flash Scripts: Creates `flash_{board}.sh` scripts for deployment
- Automatic Cleanup: Handles target switching and dependency management
- Cross-Platform: Works on macOS, Linux, and Windows

### Logging
- Individual Logs: Each board gets its own log file in `./logs/`
- Build Artifacts: All outputs preserved for debugging
- Progress Tracking: Real-time progress indicators
- Error Reporting: Clear error messages and exit codes

## 📊 Supported Project Types

ESPBrew supports multiple ESP32 development frameworks and project types:

### 🔧 ESP-IDF Projects (Traditional C/C++)
- **Standard ESP-IDF**: Full support for ESP-IDF v4.4+ projects
- **Multi-Board Configuration**: Uses `sdkconfig.defaults.*` files for board-specific builds
- **Component Management**: Local and managed components with ESP Component Registry
- **Advanced Build Strategies**: Professional idf-build-apps integration
- **Complete Toolchain**: Build, flash, monitor, and remote operations

### 🦀 Rust no_std Embedded Projects (⭐ NEW)
- **Cargo.toml Detection**: Automatic project recognition for Rust embedded projects
- **ESP Rust Ecosystem**: Full support for esp-hal, esp-backtrace, esp-println, Embassy
- **Target Auto-Detection**: Detects chip type from Cargo target directory (xtensa-esp32s3-none-elf, riscv32imc-esp-espidf, etc.)
- **Advanced Flashing**: Multi-component firmware extraction and remote flashing capabilities
- **Embedded Optimizations**: Automatic `--release` builds and embedded-specific optimizations
- **Framework Agnostic**: Works with any Rust no_std framework targeting ESP32 chips
- **Multiconfig Support (⭐ NEW)**: Automatic detection of `.cargo/config_*.toml` files for multi-board projects
- **Multitarget Support (⭐ NEW)**: Cargo alias parsing for multiple targets in single `.cargo/config.toml`
- **Intelligent Board Discovery**: Supports single-target, multiconfig, and multitarget project patterns

### 🎨 Arduino ESP32 Projects (⭐ NEW)
- **Arduino IDE Compatibility**: Full support for Arduino IDE .ino sketch files
- **arduino-cli Integration**: Uses professional arduino-cli for building and flashing
- **Multi-Board Configuration**: Support for multiple ESP32 boards via `boards.json` configuration
- **FQBN Support**: Full Qualified Board Name (FQBN) configuration for precise board targeting
- **Build Artifact Management**: Automatic detection of bootloader.bin, partitions.bin, and .ino.bin files
- **Remote Arduino Flashing**: Upload Arduino sketches to remote ESP32 boards via ESPBrew server
- **Serial Monitor**: Built-in serial monitoring with configurable baud rates
- **ESP32 Focus**: Optimized for ESP32, ESP32-S2, ESP32-S3, ESP32-C3, ESP32-C6, ESP32-H2, ESP32-P4

**Rust Project Detection Criteria:**
ESPBrew automatically detects Rust no_std projects by scanning for:
- `Cargo.toml` file presence
- ESP-specific dependencies: `esp-hal`, `esp-backtrace`, `esp-println`, `embedded-hal`
- `no_std` attribute combined with ESP32-related content
- `.cargo/config.toml` with ESP32 target configurations

**Arduino Project Detection Criteria:**
ESPBrew automatically detects Arduino projects by scanning for:
- `.ino` Arduino sketch files in the project directory
- `boards.json` configuration file with Arduino project type
- arduino-cli compatibility and ESP32 board configurations

**Supported Rust ESP32 Targets:**
- `xtensa-esp32-none-elf` (ESP32)
- `xtensa-esp32s2-none-elf` (ESP32-S2)
- `xtensa-esp32s3-none-elf` (ESP32-S3)
- `riscv32imc-esp-espidf` (ESP32-C3, ESP32-C6)
- `riscv32imac-esp-espidf` (ESP32-P4)

## 🚀 Quick Start

### Installation

#### One-Line Install (Recommended)
```bash
# Install espbrew directly from releases
curl -L https://georgik.github.io/espbrew/install.sh | bash
```

#### Homebrew (macOS Apple Silicon)
```bash
# Add tap first, then install (recommended)
brew tap georgik/espbrew
brew install espbrew
```

> **Note**: The Homebrew formula is maintained in a separate repository: [georgik/homebrew-espbrew](https://github.com/georgik/homebrew-espbrew)

#### Manual Installation
```bash
# Review the script first (optional)
curl -L https://georgik.github.io/espbrew/install.sh -o install.sh
chmod +x install.sh
./install.sh

# Or build from source
git clone https://github.com/georgik/espbrew.git && cd espbrew
cargo build --release

# Or install directly from crates.io (when published)
cargo install espbrew
```

#### Custom Installation Directory
```bash
# Install to custom directory (e.g., /usr/local/bin)
export INSTALL_DIR=/usr/local/bin
curl -L https://georgik.github.io/espbrew/install.sh | bash
```

#### Supported Platforms
- **macOS** (Apple Silicon) - TUI/CLI + Server with USB device detection
- **Linux** (x86_64) - TUI/CLI + Server with USB device detection  
- **Windows** (x86_64) - TUI/CLI (Server support coming soon)

### Basic Usage

#### TUI & CLI Modes

```bash
# Interactive TUI mode (default) - uses current directory
# TUI includes component management. Press 'b' to build, Tab to switch panes.
espbrew

# Interactive TUI mode with specific directory
espbrew /path/to/your/esp-idf-project

# CLI mode - list boards and components (default)
espbrew --cli-only
espbrew --cli-only list

# CLI mode - build all boards
espbrew --cli-only build

# CLI mode with specific directory
espbrew --cli-only /path/to/your/esp-idf-project
espbrew --cli-only /path/to/your/esp-idf-project build

# Help and options
espbrew --help
```

#### ESPBrew Server Mode

```bash
# Start ESPBrew Server for remote board management
cargo run --bin espbrew-server

# Server will start on http://0.0.0.0:8080
# 🌍 Web Dashboard: http://localhost:8080
# 📡 API Endpoint: http://localhost:8080/api/v1/boards
# ❤️ Health Check: http://localhost:8080/health

# The server provides:
# - Real-time ESP32 board discovery (macOS & Linux)
# - Web dashboard for board monitoring and real-time log streaming
# - RESTful API for remote flashing and serial monitoring
# - WebSocket-based remote monitoring with automatic reconnection
# - Automatic device detection every 30 seconds
```

#### Rust no_std Projects (⭐ NEW)

```bash
# ESPBrew now fully supports Rust no_std embedded projects!
# Works with esp-hal, Embassy, and other embedded Rust frameworks

# Use ESPBrew with Rust no_std project (auto-detects Cargo.toml)
espbrew /path/to/your/rust-embedded-project

# Remote flash Rust no_std project to ESP32 board
espbrew --cli-only remote-flash

# Build Rust project (automatically uses --release as per embedded best practices)
espbrew --cli-only build

# Supported Rust embedded dependencies:
# - esp-hal (ESP HAL for no_std)
# - esp-backtrace, esp-println
# - embedded-hal and ecosystem crates
# - Embassy async runtime
# - Any no_std + ESP32 combination

# Features:
# 🦀 Native Rust project support with Cargo.toml detection
# 🎯 Automatic chip detection from target directory (esp32s3, esp32c6, esp32p4, etc.)
# 📦 Multi-component firmware extraction using espflash crate
# 🚀 Advanced remote flashing with bootloader + partition table + app
# ⚡ Optimized release builds (--release flag enforced for embedded targets)
# 🛠️ Multiconfig & Multitarget support for complex multi-board projects
```

#### Arduino ESP32 Projects (⭐ NEW)

```bash
# ESPBrew now fully supports Arduino ESP32 projects!
# Professional arduino-cli integration with multi-board support

# Use ESPBrew with Arduino project (auto-detects .ino files)
espbrew /path/to/your/arduino-project

# Remote flash Arduino project to ESP32 board  
espbrew --cli-only remote-flash

# Build Arduino project using arduino-cli
espbrew --cli-only build

# Supported Arduino ESP32 boards:
# - ESP32, ESP32-S2, ESP32-S3, ESP32-C3, ESP32-C6, ESP32-H2, ESP32-P4
# - M5Stack boards (Core, Core2, StickC, etc.)
# - Custom arduino-cli compatible ESP32 board packages

# Features:
# 🎨 Arduino IDE .ino file compatibility
# 🛠️ Professional arduino-cli build system integration
# 📦 Multi-board configuration via boards.json
# 🚀 Remote Arduino flashing with complete firmware upload
# 📺 Serial monitor with configurable baud rates
# ⚡ FQBN (Fully Qualified Board Name) support for precise targeting
```

#### Remote Monitoring Quick Start

```bash
# Start server (in one terminal)
cargo run --bin espbrew-server --release

# Monitor remote board with boot log capture (in another terminal)
espbrew --cli-only remote-monitor --name "M5Stack Core S3" --reset

# Or use TUI with remote monitoring
espbrew --server-url http://localhost:8080 .
# Then press 'm' to open monitor modal or use "Remote Monitor" action
```

**Server Features:**
- 🔍 **Auto-Discovery**: Detects ESP32-S3/C3/C6/H2 boards automatically
- 📺 **Remote Monitoring**: Real-time WebSocket serial monitoring with auto-reconnection
- 🌍 **Web Dashboard**: Beautiful interface with live log streaming at `http://localhost:8080`
- 📡 **RESTful API**: JSON API for board information, flashing, and monitoring
- 🔄 **Real-time Updates**: Automatic board scanning every 30 seconds
- ⚡ **Quick Shutdown**: Clean server shutdown with Ctrl+C (handles hanging connections)
- 🔧 **Session Management**: Automatic monitoring session cleanup and keep-alive
- 📦 **Cross-Platform**: Supports macOS and Linux USB device detection

### Server API Documentation

The ESPBrew Server provides a RESTful API for remote board management:

#### API Endpoints

```bash
# Board Management
GET    /api/v1/boards              - List all connected boards
GET    /api/v1/boards/{board_id}   - Get specific board information
POST   /api/v1/flash               - Flash a board with binaries
POST   /api/v1/reset               - Reset a board

# Remote Monitoring
POST   /api/v1/monitor/start       - Start monitoring a board
POST   /api/v1/monitor/stop        - Stop monitoring session
POST   /api/v1/monitor/keepalive   - Keep monitoring session alive
GET    /api/v1/monitor/sessions    - List active monitoring sessions
WS     /ws/monitor/{session_id}    - WebSocket for receiving logs

# Board Configuration
GET    /api/v1/board-types         - List available board types
POST   /api/v1/assign-board        - Assign board to board type
DELETE /api/v1/assign-board/{id}   - Remove board assignment

# System
GET    /health                     - Server health check
```

#### Example API Response

```json
{
  "boards": [
    {
      "id": "board__dev_tty_usbmodem1101",
      "port": "/dev/tty.usbmodem1101",
      "chip_type": "ESP32-S3/C3/C6/H2",
      "features": "USB-OTG, WiFi, Bluetooth",
      "device_description": "Espressif - USB JTAG/serial debug unit",
      "status": "Available",
      "last_updated": "2025-09-25T14:07:21.945919+02:00"
    }
  ],
  "server_info": {
    "version": "0.1.0",
    "hostname": "your-machine.local",
    "last_scan": "2025-09-25T14:07:21.945924+02:00",
    "total_boards": 1
  }
}
```

#### Supported Device Paths

**macOS:**
- `/dev/cu.usbmodem*` - USB modem devices (ESP32-S3/C3/C6/H2)
- `/dev/cu.usbserial*` - USB serial devices (ESP32/ESP8266)
- `/dev/tty.usbmodem*` - TTY USB modem devices
- `/dev/tty.usbserial*` - TTY USB serial devices

**Linux:**
- `/dev/ttyUSB*` - Most ESP32 boards with CP210x/FTDI chips
- `/dev/ttyACM*` - ESP32-S3/C3/C6/H2 with native USB support

### Example Project Structures

#### ESP-IDF Project (Traditional C/C++)

```
my-esp-idf-project/
├── CMakeLists.txt
├── main/
├── components/
├── sdkconfig.defaults.esp32_s3_box_3     # ESP32-S3-BOX-3 config
├── sdkconfig.defaults.m5_atom_s3         # M5 Atom S3 config  
├── sdkconfig.defaults.esp32_p4_function_ev # ESP32-P4 config
├── sdkconfig.defaults.m5stack_tab5       # M5Stack Tab5 config
└── sdkconfig.defaults                    # Base config
```

When you run ESPBrew on this ESP-IDF project:

```bash
espbrew .
```

ESPBrew will:
1. 🔍 **Discover** 4 board configurations
2. 📝 **Generate** 8 scripts (`build_*.sh` + `flash_*.sh`) in `./support/`
3. 📁 **Create** `./logs/` for build logs
4. 🏗️ **Build** all boards with separate build directories:
   - `build.esp32_s3_box_3/`
   - `build.m5_atom_s3/`
   - `build.esp32_p4_function_ev/`
   - `build.m5stack_tab5/`

#### Rust no_std Project ⭐ NEW

ESPBrew supports **three different patterns** for managing Rust no_std embedded projects with multiple ESP32 targets:

**1. Single Target Project (Basic):**
```
my-rust-embedded-project/
├── Cargo.toml                           # Main project manifest
├── .cargo/
│   └── config.toml                      # Single target configuration
├── src/
│   └── main.rs                          # Main application code
├── target/                              # Cargo build output directory
│   └── xtensa-esp32s3-none-elf/        # ESP32-S3 target builds
│       └── release/
│           └── my-project.elf          # Release ELF binary
├── memory.x                             # Memory layout (if using cortex-m-rt)
└── sdkconfig.defaults                   # ESP-IDF SDK configuration (optional)
```

**2. Multiconfig Pattern (Advanced - ⭐ NEW):**
```
my-rust-multiconfig-project/
├── Cargo.toml                           # Main project manifest
├── .cargo/
│   ├── config.toml                      # Base configuration
│   ├── config_esp32s3.toml             # ESP32-S3 specific config
│   ├── config_esp32c6.toml             # ESP32-C6 specific config
│   └── config_esp32p4.toml             # ESP32-P4 specific config
├── src/
│   └── main.rs                          # Main application code
├── target/                              # Cargo build output directory
│   ├── xtensa-esp32s3-none-elf/        # ESP32-S3 target builds
│   ├── riscv32imc-esp-espidf/          # ESP32-C6 target builds
│   └── riscv32imac-esp-espidf/         # ESP32-P4 target builds
├── memory.x                             # Memory layout (if using cortex-m-rt)
└── sdkconfig.defaults                   # ESP-IDF SDK configuration (optional)
```

**3. Multitarget Pattern (Advanced - ⭐ NEW):**
```
my-rust-multitarget-project/
├── Cargo.toml                           # Main project manifest
├── .cargo/
│   └── config.toml                      # All targets with cargo aliases
├── src/
│   └── main.rs                          # Main application code
├── target/                              # Cargo build output directory
│   ├── xtensa-esp32s3-none-elf/        # ESP32-S3 target builds
│   ├── riscv32imc-esp-espidf/          # ESP32-C6 target builds
│   └── riscv32imac-esp-espidf/         # ESP32-P4 target builds
├── memory.x                             # Memory layout (if using cortex-m-rt)
└── sdkconfig.defaults                   # ESP-IDF SDK configuration (optional)
```

**Example Cargo.toml for Rust no_std ESP32:**

```toml
[package]
name = "esp32-conways-game-of-life-rs"
version = "0.1.0"
edition = "2021"

[dependencies]
esp-hal = "1.0.0-rc.0"
esp-backtrace = { version = "0.14", features = ["esp32s3", "exception-handler", "panic-handler", "println"] }
esp-println = { version = "0.11", features = ["esp32s3"] }
embedded-hal = "1.0"

[[bin]]
name = "esp32-conways-game-of-life-rs"
path = "src/main.rs"
```

##### Configuration Examples

**Single Target .cargo/config.toml:**
```toml
[build]
target = "xtensa-esp32s3-none-elf"

[target.xtensa-esp32s3-none-elf]
runner = "espflash flash --monitor"

[env]
ESP_LOG_LEVEL = "info"
```

**Multiconfig Pattern - .cargo/config_esp32s3.toml:**
```toml
[build]
target = "xtensa-esp32s3-none-elf"

[target.xtensa-esp32s3-none-elf]
runner = "espflash flash --monitor"

[env]
ESP_CONFIG_CHIP = "esp32s3"
ESP_LOG_LEVEL = "info"
```

**Multiconfig Pattern - .cargo/config_esp32c6.toml:**
```toml
[build]
target = "riscv32imc-esp-espidf"

[target.riscv32imc-esp-espidf]
runner = "espflash flash --monitor"

[env]
ESP_CONFIG_CHIP = "esp32c6"
ESP_LOG_LEVEL = "debug"
```

**Multitarget Pattern - .cargo/config.toml with Aliases:**
```toml
[build]
target = "xtensa-esp32s3-none-elf"  # Default target

[target.xtensa-esp32s3-none-elf]
runner = "espflash flash --monitor"

[target.riscv32imc-esp-espidf]
runner = "espflash flash --monitor"

[target.riscv32imac-esp-espidf]
runner = "espflash flash --monitor"

# Cargo aliases for different targets
[alias]
esp32s3 = ["build", "--release", "--target", "xtensa-esp32s3-none-elf"]
esp32c6 = ["build", "--release", "--target", "riscv32imc-esp-espidf", "--features", "esp32c6"]
esp32p4 = ["build", "--release", "--target", "riscv32imac-esp-espidf", "--features", "esp32p4"]

[env]
ESP_LOG_LEVEL = "info"
```

##### ESPBrew Automatic Detection & Multi-Board Support ⭐ NEW

ESPBrew **automatically detects** and supports all three Rust project patterns with intelligent board discovery:

**For Multiconfig Projects (.cargo/config_*.toml files):**
```bash
espbrew .
```

ESPBrew will:
1. 🔍 **Discover** all `.cargo/config_*.toml` files (e.g., `config_esp32s3.toml`, `config_esp32c6.toml`)
2. 🎯 **Extract** target architecture from each config file's `[build].target` section
3. 📝 **Parse** environment variables like `ESP_CONFIG_CHIP` for chip identification
4. 📝 **Create** board configurations automatically (e.g., `esp32s3`, `esp32c6`, `esp32p4`)
5. 🔨 **Build** each board with: `cargo build --release --config .cargo/config_esp32s3.toml`

**For Multitarget Projects (cargo aliases in .cargo/config.toml):**
```bash
espbrew .
```

ESPBrew will:
1. 🔍 **Scan** `.cargo/config.toml` for `[alias]` section
2. 🎯 **Parse** alias commands to extract targets and features
3. 📝 **Detect** chip types from target names (e.g., `xtensa-esp32s3-none-elf` → `esp32s3`)
4. 📝 **Create** board configurations from each alias (e.g., `esp32s3`, `esp32c6`, `esp32p4`)
5. 🔨 **Build** each board using the alias command: `cargo esp32s3` (which runs `cargo build --release --target xtensa-esp32s3-none-elf`)

**For Single Target Projects (.cargo/config.toml):**
```bash
espbrew .
```

ESPBrew will:
1. 🦀 **Auto-detect** Rust no_std project from Cargo.toml
2. 🔍 **Find** target chip from `.cargo/config.toml` or existing target directory
3. 🔨 **Build** with `cargo build --release` (embedded optimization)
4. 📦 **Extract** bootloader, partition table, and application from ELF
5. 🚀 **Support** remote flashing with multi-component firmware upload

##### Example: Multi-Board Rust Project Output

**CLI Mode for Multiconfig Rust Project:**
```bash
espbrew --cli-only /path/to/rust-multiconfig-project

🍺 ESPBrew CLI Mode - Rust no_std Project Information
🦀 Project Type: Rust no_std (multiconfig pattern)
📂 Project Directory: /path/to/rust-multiconfig-project
Found 3 Rust boards:
  - esp32s3 (config: .cargo/config_esp32s3.toml, target: xtensa-esp32s3-none-elf)
  - esp32c6 (config: .cargo/config_esp32c6.toml, target: riscv32imc-esp-espidf)
  - esp32p4 (config: .cargo/config_esp32p4.toml, target: riscv32imac-esp-espidf)

Build commands:
  - cargo build --release --config .cargo/config_esp32s3.toml
  - cargo build --release --config .cargo/config_esp32c6.toml  
  - cargo build --release --config .cargo/config_esp32p4.toml

Use 'espbrew --cli-only build' to build all Rust boards.
```

**CLI Mode for Multitarget Rust Project:**
```bash
espbrew --cli-only /path/to/rust-multitarget-project

🍺 ESPBrew CLI Mode - Rust no_std Project Information
🦀 Project Type: Rust no_std (multitarget pattern)
📂 Project Directory: /path/to/rust-multitarget-project
Found 3 Rust boards:
  - esp32s3 (alias: esp32s3, target: xtensa-esp32s3-none-elf)
  - esp32c6 (alias: esp32c6, target: riscv32imc-esp-espidf, features: esp32c6)
  - esp32p4 (alias: esp32p4, target: riscv32imac-esp-espidf, features: esp32p4)

Build commands:
  - cargo esp32s3
  - cargo esp32c6
  - cargo esp32p4

Use 'espbrew --cli-only build' to build all Rust boards.
```

##### Real-World Examples

**ESP-HAL Multiconfig Example:**
The [esp-hal-multiconfig-example](https://github.com/georgik/esp-hal-multiconfig-example) repository demonstrates both multiconfig and multitarget patterns:

```bash
# Clone the example repository to see these patterns in action
git clone https://github.com/georgik/esp-hal-multiconfig-example.git
cd esp-hal-multiconfig-example

# Try ESPBrew on the multiconfig example
espbrew multiconfig/

# Try ESPBrew on the multitarget example  
espbrew multitarget/
```

This repository showcases real-world usage of these patterns with esp-hal 1.0.0-rc.0 and provides working examples you can build and flash.

**Supported ESP32 Targets for Multiconfig/Multitarget:**
ESPBrew automatically detects and supports all ESP32 chip variants:
- **ESP32**: `xtensa-esp32-none-elf`
- **ESP32-S2**: `xtensa-esp32s2-none-elf`
- **ESP32-S3**: `xtensa-esp32s3-none-elf` 
- **ESP32-C3**: `riscv32imc-esp-espidf`
- **ESP32-C6**: `riscv32imc-esp-espidf`
- **ESP32-H2**: `riscv32imc-esp-espidf`
- **ESP32-P4**: `riscv32imac-esp-espidf`

**Remote Flash Example for Rust:**

```bash
# Flash the Rust project to a remote ESP32-S3 board
espbrew --cli-only remote-flash --name "M5Stack Core S3"

# ESPBrew automatically:
# 1. Finds target/xtensa-esp32s3-none-elf/release/my-project.elf
# 2. Detects chip type: esp32s3
# 3. Extracts firmware components using espflash
# 4. Uploads all components to ESPBrew server
# 5. Server flashes complete firmware remotely
```

#### Arduino ESP32 Project ⭐ NEW

ESPBrew provides full Arduino IDE compatibility with professional arduino-cli integration for ESP32 development.

**Single Board Arduino Project:**
```
my-arduino-project/
├── my-arduino-project.ino              # Main Arduino sketch
├── config.h                             # Optional configuration header
├── libraries/                           # Local libraries (optional)
│   └── MyCustomLib/
│       ├── MyCustomLib.cpp
│       └── MyCustomLib.h
└── build/                               # Build output directory
    ├── bootloader.bin                   # ESP32 bootloader
    ├── partitions.bin                   # Partition table
    ├── my-arduino-project.ino.bin       # Application binary
    └── my-arduino-project.ino.elf       # ELF debug symbols
```

**Multi-Board Arduino Project:**
```
my-multi-board-arduino-project/
├── my-project.ino                       # Main Arduino sketch
├── boards.json                          # Multi-board configuration
├── libraries/                           # Local libraries
└── build/                               # Build output directory
```

##### Arduino Configuration Examples

**Single Board (Auto-Detection):**
For simple projects, ESPBrew automatically creates a default ESP32-C6 configuration. No configuration file needed!

**Multi-Board boards.json:**
```json
{
  "project_type": "arduino",
  "description": "Multi-board Arduino ESP32 project",
  "boards": [
    {
      "name": "esp32c6",
      "fqbn": "esp32:esp32:esp32c6",
      "description": "ESP32-C6 Development Board",
      "target": "ESP32-C6",
      "build_properties": {
        "build.flash_freq": "80m",
        "build.flash_size": "4MB"
      }
    },
    {
      "name": "esp32s3",
      "fqbn": "esp32:esp32:esp32s3",
      "description": "ESP32-S3 Development Board",
      "target": "ESP32-S3",
      "build_properties": {
        "build.flash_freq": "80m",
        "build.flash_size": "16MB",
        "build.psram_type": "qio_qspi"
      }
    },
    {
      "name": "m5stack",
      "fqbn": "m5stack:esp32:m5stack-core2",
      "description": "M5Stack Core2",
      "target": "M5Stack-Core2",
      "build_properties": {
        "build.flash_freq": "80m",
        "build.flash_size": "16MB"
      }
    }
  ],
  "libraries": [
    "WiFi",
    "ArduinoJson",
    "M5Core2"
  ],
  "build_settings": {
    "compiler.warning_level": "all",
    "build.extra_flags": "-DDEBUG_LEVEL=4"
  }
}
```

##### ESPBrew Arduino Integration ⭐ NEW

When you run ESPBrew on an Arduino project:

```bash
espbrew .
```

ESPBrew will:
1. 🎨 **Auto-detect** Arduino project from .ino files
2. 🔍 **Parse** boards.json for multi-board configurations
3. 🛠️ **Use arduino-cli** for professional building and flashing
4. 📦 **Extract** bootloader, partition table, and application binaries
5. 🚀 **Support** remote flashing to ESP32 boards via ESPBrew server

##### Example: Arduino Multi-Board Project Output

**CLI Mode for Arduino Project:**
```bash
espbrew --cli-only /path/to/my-arduino-project

🍺 ESPBrew CLI Mode - Arduino Project Information
🎨 Project Type: Arduino ESP32
📂 Project Directory: /path/to/my-arduino-project
Found 3 Arduino boards:
  - esp32c6 (FQBN: esp32:esp32:esp32c6, target: ESP32-C6)
  - esp32s3 (FQBN: esp32:esp32:esp32s3, target: ESP32-S3)
  - m5stack (FQBN: m5stack:esp32:m5stack-core2, target: M5Stack-Core2)

Build commands:
  - arduino-cli compile --fqbn esp32:esp32:esp32c6 --build-path build my-project.ino
  - arduino-cli compile --fqbn esp32:esp32:esp32s3 --build-path build my-project.ino
  - arduino-cli compile --fqbn m5stack:esp32:m5stack-core2 --build-path build my-project.ino

Use 'espbrew --cli-only build' to build all Arduino boards.
Use 'espbrew' (without --cli-only) to launch the TUI for Arduino development.
```

**Supported Arduino ESP32 Boards:**
ESPBrew supports all arduino-cli compatible ESP32 boards:
- **ESP32**: `esp32:esp32:esp32`
- **ESP32-S2**: `esp32:esp32:esp32s2`
- **ESP32-S3**: `esp32:esp32:esp32s3`
- **ESP32-C3**: `esp32:esp32:esp32c3`
- **ESP32-C6**: `esp32:esp32:esp32c6`
- **ESP32-H2**: `esp32:esp32:esp32h2`
- **ESP32-P4**: `esp32:esp32:esp32p4`
- **M5Stack Boards**: `m5stack:esp32:*`
- **Custom Board Packages**: Any arduino-cli compatible ESP32 board package

**Arduino Remote Flash Example:**

```bash
# Flash Arduino project to a remote ESP32-C6 board
espbrew --cli-only remote-flash --name "ESP32-C6 DevKit"

# ESPBrew automatically:
# 1. Builds the .ino sketch using arduino-cli
# 2. Finds build/my-project.ino.bin and supporting files
# 3. Extracts bootloader, partition table, and application
# 4. Uploads all components to ESPBrew server
# 5. Server flashes complete Arduino firmware remotely
```

**Arduino Prerequisites:**
```bash
# Install arduino-cli (required for Arduino support)
curl -fsSL https://raw.githubusercontent.com/arduino/arduino-cli/master/install.sh | sh

# Update core index
arduino-cli core update-index

# Install ESP32 Arduino core
arduino-cli core install esp32:esp32

# Install M5Stack core (optional)
arduino-cli core install m5stack:esp32 --additional-urls https://m5stack.oss-cn-shenzhen.aliyuncs.com/resource/arduino/package_m5stack_index.json

# Install required libraries
arduino-cli lib install "WiFi" "ArduinoJson"
```

## 🎮 TUI Interface Guide

### Navigation
- **↑↓ or j/k**: Navigate within focused pane (boards, components, or logs)
- **Tab**: Switch between Board List → Component List → Log Pane
- **Enter**: Show action menu for selected item (board or component)
- **b**: Build all boards
- **m/M**: Open monitor modal for selected board
- **r**: Refresh board and component lists
- **h or ?**: Toggle help
- **q**: Quit

### Component Management
- **Focus Component List**: Use Tab to navigate to the component pane
- **Select Component**: Use ↑↓ to select a component
- **Component Actions**: Press Enter to open the action menu with options:
  - **Move to Components**: Move managed component to local components
  - **Clone from Repository**: Clone component from Git repository to components
    - **Wrapper Component Support**: Automatically handles wrapper components (e.g., `georgik__sdl`) by cloning with `--recursive --shallow-submodules` and extracting the correct subdirectory
  - **Remove**: Delete component directory
  - **Open in Editor**: Open component in system editor
- **Board Actions**: Press Enter in Board List to access board actions:
  - **Build**: Build the project for the selected board
  - **Generate Binary**: Create single binary file for distribution
  - **Flash**: Flash all partitions (bootloader, app, data)
  - **Flash App Only**: Flash only the application partition (faster)
  - **Monitor**: Flash and start serial monitor
  - **Remote Flash**: Flash board via ESPBrew server API
  - **Remote Monitor**: Monitor board logs via ESPBrew server with real-time WebSocket streaming
  - **Clean**: Clean build files
  - **Purge**: Delete build directory
- **Monitor Modal Controls** (when monitoring is active):
  - **↑/↓**: Scroll logs up/down
  - **PgUp/PgDn**: Scroll logs by page
  - **A**: Toggle auto-scroll on/off
  - **Ctrl+R**: Reset the board
  - **Ctrl+C**: Clear all logs
  - **ESC**: Close monitor modal
- **Visual Indicators**:
  - 📦 **Managed Component** (in `managed_components/`)
  - 🔧 **Local Component** (in `components/`)

### Interface Layout

```
┌─ 🍺 ESP Boards [FOCUSED] ─────┬─ Board Details ─────────────────┐
│ ⏳ esp32_s3_box_3         │ Board: esp32_s3_box_3           │
│ ⚙️  m5_atom_s3             │ Status: ⚙️  Building            │
│ ✅ esp32_p4_function_ev    │ Config: sdkconfig.defaults.*    │
├─ 🧩 Components ────────────┤ Build Dir: build.m5_atom_s3     │
│ 📦 esp32_camera (managed)   ┼─ Build Log ─────────────────┤
│ 🔧 my_component (local)     │ [CMake] Configuring done        │
│ 📦 georgik__sdl (managed)   │ [CMake] Generating done          │
└───────────────────────────┤ [Build] Building ESP-IDF app     │
                            │ [Build] Compiling main.c         │
                            └─────────────────────────────────┘
```

**Three-Pane Layout:**
- **Left Panel (Top)**: ESP board list with build statuses
- **Left Panel (Bottom)**: Component list with managed/local indicators
- **Right Panel**: Board details and live build logs
## 🛠️ CLI Mode

Perfect for CI/CD pipelines, automated builds, and component inspection:

```bash
# List boards and components (default CLI behavior)
espbrew --cli-only
espbrew --cli-only list

# Build all boards (default: professional idf-build-apps, zero conflicts)
espbrew --cli-only build
espbrew --cli-only --build-strategy idf-build-apps build

# Alternative strategies:
espbrew --cli-only --build-strategy sequential build    # Safe, slower
espbrew --cli-only --build-strategy parallel build     # Faster, may have conflicts

# Direct script execution (same as default idf-build-apps strategy)
./support/build-all-idf-build-apps.sh

# Work with specific project directory
espbrew --cli-only ./my-project
espbrew --cli-only ./my-project build
```

### List Mode Example Output:
```
🍺 ESPBrew CLI Mode - Project Information
Found 4 boards:
  - esp32_s3_box_3 (sdkconfig.defaults.esp32_s3_box_3)
  - m5_atom_s3 (sdkconfig.defaults.m5_atom_s3)
  - esp32_p4_function_ev (sdkconfig.defaults.esp32_p4_function_ev)
  - m5stack_tab5 (sdkconfig.defaults.m5stack_tab5)

Found 8 components:
  - OpenTyrian (./components/OpenTyrian) [local]
  - esp32_camera (./managed_components/esp32_camera) [managed]
  - georgik__sdl (./managed_components/georgik__sdl) [managed]
  - my_custom_lib (./components/my_custom_lib) [local]

Use 'espbrew --cli-only build' to start building all boards.
Use 'espbrew' (without --cli-only) to launch the TUI for component management.
```

### Build Mode Example Output:
```
🍺 ESPBrew CLI Mode - Building all boards...
Found 4 boards and 8 components...

🔄 Starting builds for all boards...

🔨 [esp32_s3_box_3] Executing action: set-target
🔨 [m5_atom_s3] Configuring project...
✅ [esp32_s3_box_3] Build completed successfully! (1/4 done)
✅ [m5_atom_s3] Build completed successfully! (2/4 done)
❌ [m5stack_tab5] Build failed! (3/4 done)
✅ [esp32_p4_function_ev] Build completed successfully! (4/4 done)

🍺 ESPBrew CLI Build Summary:
  Total boards: 4
  ✅ Succeeded: 3  
  ❌ Failed: 1

Build logs saved in ./logs/
Flash scripts available in ./support/
⚠️  Some builds failed. Check the logs for details.
```

## 📁 Generated Files

### Professional Multi-Board Build Script (`./support/build-all-idf-build-apps.sh`)

ESPBrew generates a professional-grade build script that leverages **ESP-IDF's official [`idf-build-apps`](https://github.com/espressif/idf-build-apps) tool** - Espressif's professional multi-board build solution used in production CI/CD pipelines:

```bash
#!/bin/bash
# ESPBrew generated idf-build-apps script
# This script uses the professional ESP-IDF idf-build-apps tool for efficient multi-board building.
# It automatically handles component manager conflicts and provides advanced build features.

echo "🍺 ESPBrew: Building all boards using idf-build-apps (professional ESP-IDF multi-build tool)"
echo "Project: /path/to/project"
echo "Detected 3 boards: esp32_s3_box_3, esp32_p4_function_ev, esp32_c6_devkit"
echo "Targets: esp32s3 esp32p4 esp32c6"

# Auto-install idf-build-apps if not available
if ! command -v idf-build-apps &> /dev/null; then
    pip install idf-build-apps
fi

# Build all applications using idf-build-apps
idf-build-apps build \
    --paths . \
    --target esp32s3 esp32p4 esp32c6 \
    --config-rules "sdkconfig.defaults.*" \
    --build-dir "build.@w" \
    --build-log-filename "build.log" \
    --keep-going \
    --recursive

echo "🎉 All boards built successfully using idf-build-apps!"
echo "Build directories: build.esp32_s3_box_3, build.esp32_p4_function_ev, build.esp32_c6_devkit"
```

**Key Features (powered by [idf-build-apps](https://github.com/espressif/idf-build-apps)):**
- ✅ **Zero Component Manager Conflicts**: Official Espressif tool with proper isolation
- ✅ **Intelligent Parallel Builds**: Production-grade job distribution and resource management
- ✅ **Auto-Discovery**: Smart detection of all `sdkconfig.defaults.*` configurations
- ✅ **Build Directory Isolation**: Each board gets its own `build.{board_name}` directory
- ✅ **Auto-Installation**: Automatically installs `idf-build-apps` via pip if not available
- ✅ **Comprehensive Logging**: Individual build logs per board with detailed timing
- ✅ **CI/CD Ready**: Professional error handling and exit codes used by ESP-IDF team
- ✅ **Industry Standard**: Same tool used by Espressif for ESP-IDF testing and CI/CD

### Individual Board Scripts (`./support/build_*.sh`)
```bash
#!/bin/bash
# ESPBrew generated build script for esp32_s3_box_3

set -e

echo "🍺 ESPBrew: Building esp32_s3_box_3 board..."
echo "Project: /path/to/project"
echo "Config: sdkconfig.defaults.esp32_s3_box_3"
echo "Build dir: build.esp32_s3_box_3"

cd "/path/to/project"

# Set target based on board configuration
BOARD_CONFIG="sdkconfig.defaults.esp32_s3_box_3"
if grep -q "esp32p4" "$BOARD_CONFIG"; then
    TARGET="esp32p4"
elif grep -q "esp32c6" "$BOARD_CONFIG"; then
    TARGET="esp32c6"
elif grep -q "esp32c3" "$BOARD_CONFIG"; then
    TARGET="esp32c3"
else
    TARGET="esp32s3"
fi

echo "Target: $TARGET"

# Build with board-specific configuration
# Use board-specific sdkconfig file to avoid conflicts when building multiple boards in parallel
SDKCONFIG_FILE="build.esp32_s3_box_3/sdkconfig"

# Set target and build with board-specific defaults and sdkconfig
SDKCONFIG_DEFAULTS="sdkconfig.defaults.esp32_s3_box_3" idf.py -D SDKCONFIG="$SDKCONFIG_FILE" -B "build.esp32_s3_box_3" set-target $TARGET
SDKCONFIG_DEFAULTS="sdkconfig.defaults.esp32_s3_box_3" idf.py -D SDKCONFIG="$SDKCONFIG_FILE" -B "build.esp32_s3_box_3" build

echo "✅ Build completed for esp32_s3_box_3"
```

### Flash Scripts (`./support/flash_*.sh`)
```bash
#!/bin/bash
# ESPBrew generated flash script for esp32_s3_box_3

set -e

echo "🔥 ESPBrew: Flashing esp32_s3_box_3 board..."
echo "Build dir: build.esp32_s3_box_3"

cd "/path/to/project"

if [ ! -d "build.esp32_s3_box_3" ]; then
    echo "❌ Build directory does not exist. Please build first."
    exit 1
fi

# Flash the board
idf.py -B "build.esp32_s3_box_3" flash monitor

echo "🔥 Flash completed for esp32_s3_box_3"
```

### Remote Flashing via ESPBrew Server

ESPBrew supports **remote flashing** through its server API, allowing you to flash ESP32 boards connected to remote machines. This is particularly useful for CI/CD pipelines, distributed development, and board farms.

#### Rust no_std Remote Flashing (Advanced)

**⭐ NEW: Complete Rust no_std Multi-Component Remote Flashing**

ESPBrew now provides comprehensive remote flashing support for Rust no_std embedded projects with automatic multi-component extraction and upload. This feature uses the powerful espflash crate to handle complex ELF files and extract all necessary firmware components.

**Key Features:**
- 🔧 **Automatic Chip Detection**: Detects ESP32 chip type from ELF file path
- 📦 **Multi-Component Extraction**: Extracts bootloader, partition table, and application from ELF
- 🎯 **Smart Fallback**: Falls back to merged firmware image if individual component extraction fails
- 🚀 **Robust Upload**: Sends complete multipart payload to server's multi-binary flash API
- 📊 **Progress Monitoring**: Real-time feedback with detailed logging and timing information
- ⚡ **Production Ready**: Handles large firmware images (4MB+) and complex flash layouts

**How It Works:**
1. **Build Detection**: Automatically finds the release ELF binary from `target/{chip}/release/{project_name}`
2. **Chip Recognition**: Extracts ESP32 chip type (esp32s3, esp32c6, esp32p4, etc.) from the target directory path
3. **Component Extraction**: Uses `espflash save-image` to extract individual components or merged flash image
4. **Metadata Generation**: Creates comprehensive multipart form with all necessary flash parameters
5. **Server Upload**: Transmits to ESPBrew server's `/api/v1/flash` multi-binary endpoint
6. **Remote Flash**: Server flashes the complete firmware using the espflash crate or CLI tools

**CLI Usage:**
```bash
# Flash Rust no_std project to remote board (auto-detects ELF and chip)
espbrew --cli-only remote-flash

# Flash specific board by name
espbrew --cli-only remote-flash --name "M5Stack Core S3"

# Flash with specific server URL
espbrew --cli-only --server-url http://192.168.1.100:8080 remote-flash

# Flash with automatic reset after upload
espbrew --cli-only remote-flash --reset
```

**Example Output:**
```
🔥 ESPBrew Remote Flash Mode - Rust no_std Project
📁 Project Directory: /home/user/esp32-conways-game-of-life-rs
🎯 Found ELF: target/xtensa-esp32s3-none-elf/release/esp32-conways-game-of-life-rs
🔍 Detected Chip: esp32s3 (from path: xtensa-esp32s3-none-elf)
🔧 Extracting firmware components using espflash...
📦 Attempting individual component extraction...
💾 Bootloader: 27,312 bytes at 0x0
📋 Partition Table: 3,072 bytes at 0x8000
🚀 Application: 1,398,832 bytes at 0x10000
📤 Uploading 3 components to server (1.4 MB total)...
✅ Remote flash successful! Duration: 8.9s
🎉 ESP32-S3 firmware flashed successfully via ESPBrew server
```

**Advanced Multi-Component Processing:**

The Rust remote flashing implementation automatically handles complex firmware extraction:

```bash
# Individual Component Extraction (Primary Method)
# Extracts bootloader, partition table, and application separately
espflash save-image --chip esp32s3 --merge \
  target/xtensa-esp32s3-none-elf/release/project.elf \
  extracted_components/

# Merged Firmware Extraction (Fallback Method)  
# Creates single merged flash image if component extraction fails
espflash save-image --chip esp32s3 \
  target/xtensa-esp32s3-none-elf/release/project.elf \
  merged_firmware.bin
```

**Server-Side Processing:**

The ESPBrew server receives the multipart payload and processes it through the multi-binary flash handler:

```json
{
  "board_id": "board__dev_ttyACM0",
  "flash_mode": "dio",
  "flash_freq": "80m", 
  "flash_size": "16MB",
  "binary_count": 3,
  "components": [
    {
      "name": "bootloader",
      "offset": "0x0",
      "size": 27312,
      "filename": "bootloader.bin"
    },
    {
      "name": "partition_table",
      "offset": "0x8000", 
      "size": 3072,
      "filename": "partition-table.bin"
    },
    {
      "name": "application",
      "offset": "0x10000",
      "size": 1398832,
      "filename": "esp32-conways-game-of-life-rs.bin"
    }
  ]
}
```

**Error Handling & Fallbacks:**

The implementation includes comprehensive error handling:

1. **Build Detection Failure**: Clear error if no ELF binary found
2. **Chip Detection Failure**: Fallback to esp32s3 with warning
3. **Component Extraction Failure**: Automatic fallback to merged firmware approach
4. **Network Errors**: Detailed error messages with troubleshooting tips
5. **Server Errors**: Proper error propagation with server response details

**Troubleshooting:**

**Build Not Found:**
```
❌ No ELF binary found. Please build first with: cargo build --release
```
*Solution*: Ensure Rust project is built with `cargo build --release`

**Chip Detection Issues:**
```
⚠️ Could not detect chip from path, defaulting to esp32s3
```
*Note*: This is usually fine, but verify the target matches your board

**Component Extraction Failure:**
```
⚠️ Individual component extraction failed, trying merged firmware...
📦 Fallback: Creating merged firmware image
```
*Note*: This is normal fallback behavior and usually works fine

**Server Connection Issues:**
```
❌ Failed to connect to ESPBrew server at http://localhost:8080
```
*Solution*: Start server with `cargo run --bin espbrew-server --release`

#### Multi-Binary ESP-IDF Flashing (Traditional)

For standard ESP-IDF projects, use multi-binary flashing that includes bootloader, partition table, and application:

```bash
# Flash M5Stack Core S3 with complete ESP-IDF build
curl -X POST http://localhost:8080/api/v1/flash \
  -F "board_id=board__dev_ttyACM0" \
  -F "flash_mode=dio" \
  -F "flash_freq=80m" \
  -F "flash_size=16MB" \
  -F "binary_count=3" \
  -F "binary_0=@build.m5stack_core_s3/bootloader/bootloader.bin" \
  -F "binary_0_offset=0x0" \
  -F "binary_0_name=bootloader" \
  -F "binary_0_filename=bootloader.bin" \
  -F "binary_1=@build.m5stack_core_s3/partition_table/partition-table.bin" \
  -F "binary_1_offset=0x8000" \
  -F "binary_1_name=partition_table" \
  -F "binary_1_filename=partition-table.bin" \
  -F "binary_2=@build.m5stack_core_s3/snow.bin" \
  -F "binary_2_offset=0x10000" \
  -F "binary_2_name=application" \
  -F "binary_2_filename=snow.bin"
```

**Response:**
```json
{
  "success": true,
  "message": "Successfully flashed board__dev_ttyACM0 (21280 bytes)",
  "duration_ms": 8875
}
```

#### Single Binary Flashing (Legacy)

For simple projects or custom binaries:

```bash
# Flash single binary (legacy method)
curl -X POST http://localhost:8080/api/v1/flash \
  -F "board_id=board__dev_ttyACM0" \
  -F "binary_file=@build/my_app.bin"
```

#### ESPBrew CLI Remote Flash

```bash
# Auto-detect boards and flash via CLI
espbrew --cli-only flash

# Flash specific binary
espbrew --cli-only flash --binary build/my_app.bin

# Target specific board by MAC address
espbrew --cli-only --board-mac AA:BB:CC:DD:EE:FF flash
```

#### Remote Flash vs Local Flash

**Local Flash (Direct):**
- Uses `idf.py flash` directly on the local machine
- Requires ESP-IDF environment and board connection
- Generated scripts: `./support/flash_*.sh`

**Remote Flash (ESPBrew Server API):**
- Sends binaries over HTTP to ESPBrew server
- Server handles the actual flashing via `esptool`
- Useful for distributed development and CI/CD
- No local ESP-IDF environment required on client

**Multi-Binary Benefits:**
- ✅ **Complete Firmware**: Flashes bootloader, partition table, and application
- ✅ **Proper Configuration**: Uses correct flash mode, frequency, and size
- ✅ **ESP-IDF Compatible**: Matches `idf.py flash` behavior exactly
- ✅ **Reliable**: Ensures board boots correctly with all components

### Remote Monitoring via ESPBrew Server

ESPBrew provides **real-time serial monitoring** of ESP32 boards connected to remote machines through WebSocket connections. This enables remote debugging, log analysis, and boot sequence monitoring from anywhere on the network.

#### Key Features

- **🔴 Real-time Log Streaming**: WebSocket-based live serial output
- **🔄 Automatic Reconnection**: Captures complete boot sequences after board resets
- **📱 Multi-Interface Support**: Available in CLI, TUI, and Web Dashboard
- **⚡ Reset Integration**: Send reset commands during monitoring to capture boot logs
- **🔧 Session Management**: Automatic session tracking with keep-alive functionality
- **🌐 Network-Based**: Monitor boards on remote servers over HTTP/WebSocket

#### CLI Remote Monitoring

```bash
# Monitor by board name with auto-reset for boot log capture
espbrew --cli-only remote-monitor --name "M5Stack Core S3" --reset

# Monitor by MAC address
espbrew --cli-only remote-monitor --mac AB:CD:EF:12:34:56

# Monitor with custom baud rate
espbrew --cli-only remote-monitor --name "M5Stack Core S3" --baud-rate 921600

# List available boards
espbrew --cli-only remote-monitor
```

**CLI Output Example:**
```
📺 ESPBrew Remote Monitor Mode - API Monitoring
🔍 Connecting to ESPBrew server: http://localhost:8080
📊 Found 1 board(s) on server
🎯 Targeting board with name: M5Stack Core S3
✅ Selected board: M5Stack Core S3 (AB:CD:EF:12:34:56)
📺 Starting remote monitoring session...
✅ Remote monitoring started: Monitoring session started successfully
🔗 Session ID: a36970c6-2c02-4d12-a6ae-a43f08c5259d
🔌 Connecting to WebSocket...
✅ WebSocket connected! Streaming logs from M5Stack Core S3...
🔄 Resetting board to capture complete boot sequence...
💡 Press Ctrl+C to stop monitoring
────────────────────────────────────────────────────────────
I (0) cpu_start: Starting scheduler.
I (10) main_task: Started on CPU0
I (20) main_task: Calling app_main()
Apple at (15,1): frame 0->1 (max=3)
🔄 WebSocket disconnected during board reset (expected behavior)
🔄 Attempting to reconnect to continue monitoring boot sequence...
🔄 Disconnected after reset - attempting reconnect to capture boot logs...
🔗 Found new session after reset: 41901e0c-50c4-4c46-a4f8-91df0d1357f4
🔄 Reconnection attempt 2/3
✅ WebSocket connected! Streaming logs from M5Stack Core S3...
Apple at (15,4): frame 1->2 (max=3)
```

#### TUI Remote Monitoring

The TUI provides an integrated monitoring modal with full keyboard controls:

**Accessing Remote Monitor:**
1. **Launch TUI**: `espbrew --server-url http://localhost:8080`
2. **Select Board**: Navigate to any board with arrow keys
3. **Open Menu**: Press `Enter` to show action menu
4. **Remote Monitor**: Select "Remote Monitor" and press `Enter`
5. **Select Remote Board**: Choose target board from server and press `Enter`
6. **Monitor Modal Opens**: Full-featured monitoring interface

**Alternative Quick Access:**
- Press `'m'` or `'M'` directly in TUI to open monitor modal for local board

**Monitor Modal Controls:**
```
┌─ 📺 Serial Monitor - board__dev_ttyACM0 🟢 Connected ─────────────┐
│ [LOG CONTENT]                                                     │
│ I (0) cpu_start: Starting scheduler.                              │
│ I (10) main_task: Started on CPU0                                 │
│ Apple at (15,1): frame 0->1 (max=3)                              │
│ Apple at (15,4): frame 1->2 (max=3)                              │
│                                                                   │
│ [↑↓] Scroll [PgUp/PgDn] Page [A] Auto-scroll [Ctrl+R] Reset      │
│ [Ctrl+C] Clear [ESC] Close                                        │
└───────────────────────────────────────────────────────────────────┘
```

**Keyboard Shortcuts:**
- **`↑`/`↓`**: Scroll up/down one line
- **`PgUp`/`PgDn`**: Scroll up/down one page  
- **`A`**: Toggle auto-scroll on/off
- **`Ctrl+R`**: Reset the board
- **`Ctrl+C`**: Clear all logs
- **`ESC`**: Close monitoring modal

#### Web Dashboard Remote Monitoring

The web interface at `http://localhost:8080` provides:
- **Board List**: Visual board status and selection
- **Real-time Logs**: WebSocket-powered log streaming
- **Reset Controls**: One-click board reset functionality
- **Session Management**: Monitor multiple boards simultaneously

#### Advanced Monitoring Features

**Automatic Boot Log Capture:**
```bash
# The --reset flag ensures complete boot sequence capture
espbrew --cli-only remote-monitor --name "M5Stack Core S3" --reset
```

**Process:**
1. **Start Monitoring**: Establishes WebSocket connection first
2. **Send Reset**: Resets the board after monitoring is active
3. **Capture Boot**: Records bootloader, kernel, and application startup
4. **Auto-Reconnect**: Handles WebSocket disconnection during reset
5. **Continue Monitoring**: Seamless transition to runtime logs

**Session Management:**
- **Keep-Alive**: Automatic 60-second keep-alive messages
- **Auto-Cleanup**: Server removes inactive sessions after 2 minutes
- **Session Discovery**: Client can find and reconnect to new sessions
- **Concurrent Sessions**: Multiple clients can monitor different boards

#### Remote Monitoring API

ESPBrew exposes REST and WebSocket APIs for remote monitoring:

```bash
# Start monitoring session
curl -X POST http://localhost:8080/api/v1/monitor/start \
  -H "Content-Type: application/json" \
  -d '{
    "board_id": "board__dev_ttyACM0",
    "baud_rate": 115200
  }'

# Response: 
# {
#   "success": true,
#   "message": "Monitoring session started successfully",
#   "session_id": "f47ac10b-58cc-4372-a567-0e02b2c3d479",
#   "websocket_url": "/ws/monitor/f47ac10b-58cc-4372-a567-0e02b2c3d479"
# }

# WebSocket connection for real-time logs
ws://localhost:8080/ws/monitor/f47ac10b-58cc-4372-a567-0e02b2c3d479

# Reset board during monitoring
curl -X POST http://localhost:8080/api/v1/reset \
  -H "Content-Type: application/json" \
  -d '{"board_id": "board__dev_ttyACM0"}'

# List active monitoring sessions
curl http://localhost:8080/api/v1/monitor/sessions

# Stop monitoring session
curl -X POST http://localhost:8080/api/v1/monitor/stop \
  -H "Content-Type: application/json" \
  -d '{"session_id": "f47ac10b-58cc-4372-a567-0e02b2c3d479"}'
```

**WebSocket Message Format:**
```json
{
  "type": "log",
  "session_id": "session-uuid",
  "content": "ESP32 log line content",
  "timestamp": "2024-01-15T10:30:00Z"
}
```

#### Troubleshooting Remote Monitoring

**Common Issues:**

**Connection Failed:**
```
❌ Remote Monitor Connection Failed: Connection refused
```
*Solution*: Ensure ESPBrew server is running: `cargo run --bin espbrew-server --release`

**No Logs Appearing:**
```
🔌 WebSocket connected successfully  
💓 Session keep-alive sent
```
*Solution*: Check if ESP32 board is outputting serial data at the correct baud rate

**Reset Disconnection:**
```
🔄 WebSocket disconnected during board reset (expected behavior)
🔄 Attempting to reconnect to capture boot logs...
```
*Status*: Normal behavior - ESPBrew automatically reconnects after reset

**Firewall Issues:**
```bash
# Open port for remote access
sudo ufw allow 8080/tcp

# Start server on all interfaces
cargo run --bin espbrew-server --release -- --bind 0.0.0.0 --port 8080
```

#### Use Cases

- **🏗️ Development**: Monitor ESP32 boards during development without physical access
- **🔧 Debugging**: Capture complete boot sequences and crash logs remotely  
- **🏭 Production**: Monitor deployed ESP32 devices in test farms or production
- **👥 Team Collaboration**: Share board access across development teams
- **🤖 CI/CD**: Automated monitoring and log analysis in continuous integration
- **📊 Quality Assurance**: Remote testing and validation of ESP32 applications

## 🎯 Supported Board Patterns

ESPBrew automatically detects ESP32 targets from config files:

| Pattern in Config | Detected Target | Example Board |
|-------------------|----------------|---------------|
| `esp32p4` or `CONFIG_IDF_TARGET="esp32p4"` | `esp32p4` | M5Stack Tab5, ESP32-P4-Function-EV |
| `esp32c6` or `CONFIG_IDF_TARGET="esp32c6"` | `esp32c6` | ESP32-C6-DevKit |
| `esp32c3` or `CONFIG_IDF_TARGET="esp32c3"` | `esp32c3` | ESP32-C3-LCDKit |
| Default | `esp32s3` | M5 Atom S3, ESP32-S3-BOX-3, M5Stack CoreS3 |

## 🧩 Component Management

ESPBrew provides powerful component management capabilities for ESP-IDF projects:

### Component Types

- **🔧 Local Components** (in `components/` directory)
  - User-created or modified components
  - Full control over source code
  - Version controlled with your project

- **📦 Managed Components** (in `managed_components/` directory)
  - Components managed by ESP Component Registry
  - Installed via `idf.py add-dependency`
  - Include manifest files (`idf_component.yml`)

### Component Actions

#### Move to Components
Moves a managed component to the local components directory:
- **Use Case**: When you need to modify a managed component
- **Result**: Component becomes local and editable
- **Location**: `managed_components/component` → `components/component`

#### Clone from Repository  
Clones a component from its Git repository:
- **Use Case**: Get the latest source code from the official repository
- **Requirements**: Component must have `idf_component.yml` with repository URL
- **Smart URL Handling**: Automatically converts `git://` to `https://` URLs
- **Process**: 
  1. Reads `idf_component.yml` manifest file
  2. Extracts repository URL (`repository`, `git`, or `url` fields)
  3. Clones the repository to `components/component`
  4. Removes the original managed component
- **Result**: Fresh Git repository clone in components directory

##### Wrapper Component Support

ESPBrew automatically detects and handles **wrapper components** that contain multiple sub-components. These are repositories that don't directly contain an ESP-IDF component, but instead contain subdirectories with the actual components.

**Example**: The `georgik__sdl` component is a wrapper containing an `sdl/` subdirectory with the actual ESP-IDF component.

**Automatic Wrapper Handling**:
1. **Detection**: ESPBrew identifies wrapper components by name patterns (e.g., `georgik__sdl`)
2. **Recursive Clone**: Uses `git clone --recursive --shallow-submodules` to include all submodules
3. **Subdirectory Extraction**: Automatically finds and extracts the correct subdirectory
4. **Placement**: Moves only the component subdirectory to `components/` with the proper name
5. **Cleanup**: Removes the temporary wrapper repository clone

**Supported Wrapper Components**:
- `georgik__sdl` → extracts `sdl/` subdirectory
- Additional wrapper components can be added as needed

**Process Flow**:
```
Wrapper Repository:
georgik__sdl/
├── sdl/              ← Target subdirectory
│   ├── CMakeLists.txt
│   ├── src/
│   └── include/
├── other_component/
└── README.md

Result in components/:
components/georgik__sdl/  ← Renamed from sdl/
├── CMakeLists.txt
├── src/
└── include/
```

#### Remove Component
Deletes a component directory entirely:
- **Use Case**: Remove unused components
- **Warning**: This permanently deletes the component

#### Open in Editor
Opens the component directory in your system's default editor:
- **macOS**: Uses `open` command
- **Linux**: Uses `xdg-open` command
- **Windows**: Uses `explorer` command

### Example Workflow

1. **Discover Components**: Launch ESPBrew to see all components
2. **Select Target**: Navigate to a managed component (e.g., `georgik__sdl`)
3. **Choose Action**: Press Enter to see available actions
4. **Clone Repository**: Select "Clone from Repository" to get latest source
5. **Result**: Fresh Git repository in `components/georgik__sdl/`

### Manifest File Support

ESPBrew reads `idf_component.yml` files to extract repository information:

```yaml
version: "2.0.11"
repository: git://github.com/georgik/esp-idf-component-SDL.git
description: "ESP32 SDL wrapper component"
license: "Zlib"
```

Supported repository fields (in order of preference):
1. `repository`: Primary repository URL field
2. `git`: Alternative Git URL field  
3. `url`: Fallback URL field

## 🔧 Advanced Usage

### Integration with IDEs

**VS Code Task (`tasks.json`)**:
```json
{
    "version": "2.0.0",
    "tasks": [
        {
            "label": "ESPBrew: Build All Boards",
            "type": "shell",
            "command": "espbrew",
            "args": ["--cli-only"],
            "group": "build",
            "presentation": {
                "echo": true,
                "reveal": "always"
            }
        }
    ]
}
```

### CI/CD Integration

**GitHub Actions**:
```yaml
name: Multi-Board ESP32 Build

on: [push, pull_request]

jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      
      - name: Setup ESP-IDF
        uses: espressif/esp-idf-ci-action@v1
        
      - name: Install ESPBrew
        run: cargo install espbrew
        
      - name: Build All Boards
        run: espbrew --cli-only
        
      - name: Upload Build Artifacts
        uses: actions/upload-artifact@v3
        with:
          name: build-artifacts
          path: |
            build.*/
            logs/
```

### Binary Generation for Distribution

ESPBrew can generate single binary files for easy distribution and deployment:

**Using the TUI**:
1. Build your project first
2. Select a board and press Enter
3. Choose "Generate Binary" from the action menu
4. ESPBrew creates `{board_name}-{target}.bin` in your project root

**What it does**:
- Uses `esptool.py merge_bin` to combine all flash components
- Automatically detects target chip (esp32s3, esp32p4, esp32c6, etc.)
- Creates a single binary file that can be flashed with:
  ```bash
  esptool.py --chip esp32s3 write_flash 0x0 my_board-esp32s3.bin
  ```

**Use Cases**:
- **Distribution**: Send a single file instead of multiple components
- **Production**: Simplified manufacturing and deployment process
- **ESP Web Tools**: Compatible with browser-based flashing tools
- **Custom Flashers**: Easy integration with custom flashing solutions

**Generated Files**:
```
your-project/
├── esp32_s3_box_3-esp32s3.bin     # Complete binary for ESP32-S3-BOX-3
├── m5stack_tab5-esp32p4.bin        # Complete binary for M5Stack Tab5
└── esp32_p4_function_ev-esp32p4.bin # Complete binary for ESP32-P4-Function-EV
```

## 🔄 Multi-Board Build Strategies

ESPBrew provides three build strategies to handle multiple boards, each with different trade-offs:

### Professional idf-build-apps (Default, Recommended)

**About [`idf-build-apps`](https://github.com/espressif/idf-build-apps):**  
The official ESP-IDF multi-application build tool developed by Espressif. This is the same production-grade tool used by the ESP-IDF team for continuous integration, testing, and release processes.

**Advantages:**
- ✅ **Zero Component Manager Conflicts**: Official Espressif tool with proper build isolation
- ✅ **Intelligent Parallel Builds**: Production-grade job distribution and resource management
- ✅ **Industry Standard**: Same tool used by Espressif for ESP-IDF CI/CD and testing
- ✅ **Auto-Installation**: Automatically installs `idf-build-apps` via pip if not available
- ✅ **Rich Output**: Detailed build status, timing, and comprehensive error reporting
- ✅ **Build Log Integration**: Professional logging stored in build directories (`build.{board}/build.log`)
- ✅ **Advanced Features**: Support for build matrix, custom rules, and complex project structures

**Use When:**
- Any multi-board ESP-IDF project (recommended for all use cases)
- Production builds and CI/CD pipelines
- Projects with managed components
- **This is the default and recommended strategy**

### Sequential Builds (Legacy, Safe)

**Advantages:**
- **Component Manager Safe**: Avoids ESP-IDF Component Manager lock conflicts
- **Resource Efficient**: Full CPU/memory allocation per build
- **Clear Output**: Easy to follow build progress and identify issues
- **Reliable**: No race conditions or locking issues
- **Better for CI/CD**: Predictable resource usage and clearer error reporting

**Use When:**
- Project uses managed components (`managed_components/` directory)
- Building on resource-constrained systems
- Need reliable, predictable builds
- **This is the default and recommended strategy**

### Parallel Builds (Optional)

**Advantages:**
- **Speed**: All boards build simultaneously
- **Good for Simple Projects**: Works well with projects using only local components

**Limitations:**
- **Component Manager Conflicts**: ESP-IDF Component Manager cannot isolate `managed_components/` directory, causing lock conflicts
- **Resource Contention**: Multiple builds compete for CPU/memory/disk I/O
- **Complex Output**: Interleaved log output can be harder to debug

**Use When:**
- Project has no managed components
- System has abundant resources
- Speed is more important than reliability

### Configuration Isolation

Both build strategies use board-specific `sdkconfig` files to avoid configuration conflicts:

- **Traditional ESP-IDF**: All builds share a single `sdkconfig` file in the project root
- **ESPBrew Approach**: Each board uses its own `sdkconfig` file in its build directory

```bash
# Example: Each board gets its own sdkconfig
# Board 1: build.esp32_s3_box_3/sdkconfig
# Board 2: build.esp32_p4_function_ev/sdkconfig
SDKCONFIG_DEFAULTS="sdkconfig.defaults.esp32_s3_box_3" \
idf.py -D SDKCONFIG="build.esp32_s3_box_3/sdkconfig" \
       -B "build.esp32_s3_box_3" build
```

### Choosing a Build Strategy

```bash
# Professional idf-build-apps (default, recommended)
espbrew --build-strategy idf-build-apps
espbrew  # Same as above (default)

# Legacy strategies:
espbrew --build-strategy sequential  # Safe, slower
espbrew --build-strategy parallel    # Fast, may have conflicts
```

## 📊 Project Structure

ESPBrew creates the following structure:

```
your-project/
├── sdkconfig.defaults.*          # Your board configs
├── components/                   # Local components (user-managed)
│   ├── my_custom_lib/
│   └── cloned_component/         # Components cloned from repositories
├── managed_components/           # ESP Component Registry components
│   ├── georgik__sdl/
│   │   └── idf_component.yml     # Component manifest with repository info
│   └── esp32_camera/
├── build.{board_name}/           # Generated build dirs
├── logs/                         # Generated by ESPBrew
│   ├── esp32_s3_box_3.log
│   ├── m5_atom_s3.log
│   └── ...
└── support/                      # Generated by ESPBrew
    ├── build_esp32_s3_box_3.sh
    ├── flash_esp32_s3_box_3.sh
    ├── build_m5_atom_s3.sh
    ├── flash_m5_atom_s3.sh
    └── ...
```

## 🐛 Troubleshooting

### Common Issues

**Board Not Detected**:
- Ensure your config file follows the pattern `sdkconfig.defaults.{board_name}`
- Check that the config file contains `CONFIG_IDF_TARGET="..."` or target-specific content

**Build Failures**:
- Check individual log files in `./logs/` directory
- Verify ESP-IDF environment is properly set up
- Ensure all dependencies are installed for the target platform

**Permission Issues (macOS/Linux)**:
- Generated scripts are automatically made executable
- If needed: `chmod +x support/*.sh`

**Component Not Showing Actions**:
- "Clone from Repository" only appears for managed components with `idf_component.yml`
- "Move to Components" only appears for managed components
- Ensure component directories exist and are readable

**Git Clone Failures**:
- Check internet connectivity and repository accessibility
- ESPBrew automatically converts `git://` URLs to `https://` for better compatibility
- Verify the repository URL in `idf_component.yml` is correct

**Component Action Failures**:
- Ensure sufficient disk space for cloning repositories
- Check write permissions for `components/` directory
- Verify Git is installed and accessible in PATH

### Debug Mode

For detailed debugging, check the log files:

```bash
# View build log for specific board
tail -f logs/esp32_s3_box_3.log

# View all recent activity
ls -la logs/
```

## 🤝 Contributing

We welcome contributions! Areas for improvement:

- **More Board Support**: Add support for additional ESP32 variants
- **Enhanced Component Management**: Additional component actions and integrations
- **Enhanced TUI**: More interactive features and better error handling
- **Performance**: Optimize build parallelization and component operations
- **Integration**: More IDE, CI/CD, and component registry integrations

## 📄 License

MIT License - see LICENSE file for details.

## 🙏 Credits

- **Ratatui**: Terminal user interfaces for the interactive TUI
- **Tokio**: Async runtime for concurrent builds and operations
- **ESP-IDF**: Espressif IoT Development Framework
- **Clap**: Command line argument parsing with subcommands
- **serde_yaml**: YAML parsing for component manifests

---

🍺 ESPBrew - Simplifying ESP32 multi-board development
