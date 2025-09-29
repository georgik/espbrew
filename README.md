## 🍺 ESPBrew - ESP32 Multi-Board Build Manager

A comprehensive ESP32 development tool featuring TUI/CLI build management and a network-based server for remote board management. It automatically discovers board configurations, generates build scripts, provides real-time build monitoring, and offers a web dashboard for ESP32 board detection and flashing.

![ESP32 Multi-Board Support](https://img.shields.io/badge/ESP32-Multi--Board-blue)
![Rust](https://img.shields.io/badge/rust-1.70+-orange.svg)
![License](https://img.shields.io/badge/license-MIT-green.svg)

## ✨ Features

### ESPBrew Server (Remote Flashing & Management)
- **Remote Board Discovery**: Network-based ESP32 board detection and management with hardware-based unique identification
- **Cross-Platform Support**: Detects ESP32 boards on macOS and Linux via USB with enhanced chip detection
- **Web Dashboard**: Beautiful web interface for board monitoring and management
- **Board Configuration Management**: Persistent board type assignments with RON-based configuration
- **Automatic Board Type Discovery**: Auto-discovers board types from `sdkconfig.defaults.*` files
- **Background Enhancement**: Non-blocking native espflash integration for detailed board information
- **Smart Caching**: 1-hour cache for enhanced board information to improve performance
- **Real-time Scanning**: Automatic periodic board discovery every 30 seconds
- **RESTful API**: Complete API for board listing, configuration management, and remote flashing
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
# - Web dashboard for board monitoring
# - RESTful API for remote flashing
# - Automatic device detection every 30 seconds
```

**Server Features:**
- 🔍 **Auto-Discovery**: Detects ESP32-S3/C3/C6/H2 boards automatically
- 🌍 **Web Dashboard**: Beautiful interface at `http://localhost:8080`
- 📡 **RESTful API**: JSON API for board information and flashing
- 🔄 **Real-time Updates**: Automatic board scanning every 30 seconds
- ⚡ **Quick Shutdown**: Clean server shutdown with Ctrl+C (handles hanging connections)
- 📦 **Cross-Platform**: Supports macOS and Linux USB device detection

### Server API Documentation

The ESPBrew Server provides a RESTful API for remote board management:

#### API Endpoints

```bash
# List all connected boards
GET /api/v1/boards

# Get specific board information  
GET /api/v1/boards/{board_id}

# Flash a board (future feature)
POST /api/v1/flash

# Server health check
GET /health
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

### Example Project Structure

```
my-esp-project/
├── CMakeLists.txt
├── main/
├── components/
├── sdkconfig.defaults.esp32_s3_box_3     # ESP32-S3-BOX-3 config
├── sdkconfig.defaults.m5_atom_s3         # M5 Atom S3 config  
├── sdkconfig.defaults.esp32_p4_function_ev # ESP32-P4 config
├── sdkconfig.defaults.m5stack_tab5       # M5Stack Tab5 config
└── sdkconfig.defaults                    # Base config
```

When you run ESPBrew on this project:

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

## 🎮 TUI Interface Guide

### Navigation
- **↑↓ or j/k**: Navigate within focused pane (boards, components, or logs)
- **Tab**: Switch between Board List → Component List → Log Pane
- **Enter**: Show action menu for selected item (board or component)
- **b**: Build all boards
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
  - **Clean**: Clean build files
  - **Purge**: Delete build directory
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

#### Multi-Binary ESP-IDF Flashing (Recommended)

For proper ESP-IDF projects, use multi-binary flashing that includes bootloader, partition table, and application:

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
