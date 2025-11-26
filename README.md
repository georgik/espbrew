# 🍺 ESPBrew - ESP32 Multi-Board Development Platform

**ESPBrew** is the most comprehensive ESP32 development platform available, supporting **10 different frameworks and languages** including ESP-IDF, Rust, Arduino, PlatformIO, Python (Micro/Circuit), RTOS (Zephyr/NuttX), TinyGo, and JavaScript (Jaculus). It combines powerful CLI/TUI tools with network-based remote board management, providing automatic project detection, multi-board builds, real-time monitoring, and a modern web dashboard for professional ESP32 development workflows.

## ⚡ **SIMPLIFIED FLASHING - NO ESP-IDF INSTALLATION REQUIRED**

**ESPBrew provides complete independence from ESP-IDF installation for flashing operations:**

🔧 **Streamlined Dependencies:**
- ✅ **No ESP-IDF installation required for flashing** - Flash ESP32 projects without complex setup
- ✅ **No idf.py dependency** - Built-in flashing using native espflash integration
- ✅ **No esptool.py required** - Self-contained multi-partition flashing
- ✅ **Simplified CI/CD workflows** - Reduced dependencies for containerized builds

🛠️ **Comprehensive Flashing Support:**
- ✅ **Multi-partition flashing** - Bootloader + Partition Table + App + Assets
- ✅ **Universal project support** - Rust no_std, ESP-IDF, Arduino, and binary files
- ✅ **Production tested** - Validated on ESP32-S3 hardware with real projects
- ✅ **Full chip support** - ESP32, ESP32-S2/S3, ESP32-C2/C3/C5/C6, ESP32-H2/P4

**This significantly simplifies ESP32 development workflows and deployment processes.**

![ESP32 Multi-Board](https://img.shields.io/badge/ESP32-Multi--Board-blue)
![10 Frameworks](https://img.shields.io/badge/Frameworks-10%20Supported-brightgreen)
![ESP-IDF](https://img.shields.io/badge/ESP--IDF-✓-green)
![Rust](https://img.shields.io/badge/Rust-✓-orange)
![Arduino](https://img.shields.io/badge/Arduino-✓-blue)
![PlatformIO](https://img.shields.io/badge/PlatformIO-✓-orange)
![MicroPython](https://img.shields.io/badge/MicroPython-✓-blue)
![CircuitPython](https://img.shields.io/badge/CircuitPython-✓-purple)
![Zephyr](https://img.shields.io/badge/Zephyr%20RTOS-✓-red)
![NuttX](https://img.shields.io/badge/NuttX%20RTOS-✓-darkred)
![TinyGo](https://img.shields.io/badge/TinyGo-✓-cyan)
![Jaculus](https://img.shields.io/badge/Jaculus%20JS-✓-yellow)
![License](https://img.shields.io/badge/license-MIT-green.svg)
![Production Ready](https://img.shields.io/badge/status-Production%20Ready-brightgreen)

## ✨ Core Features

### 💻 **Multi-Framework ESP32 Support (10 Frameworks)**
- **ESP-IDF Projects**: Traditional C/C++ projects with `sdkconfig.defaults.*` configs
- **Rust no_std**: Full esp-hal, Embassy & embedded frameworks support 🦀
- **Arduino ESP32**: arduino-cli integration with FQBN support 🎨
- **PlatformIO**: Universal IoT platform with multi-environment support 🚀
- **MicroPython**: Python for microcontrollers with mpremote/ampy 🐍
- **CircuitPython**: Python for embedded systems with mass storage support 🔄
- **Zephyr RTOS**: Real-time OS with west build system integration ⚡
- **NuttX RTOS**: POSIX-compliant RTOS with make build system 🏗️
- **TinyGo**: Go for embedded systems targeting ESP32 variants 🏃
- **Jaculus**: JavaScript runtime for ESP32 with jaculus-tools 📱
- **Multi-Board**: Automatic detection and parallel builds
- **Cross-Platform**: macOS, Linux, Windows support

### 🌍 **ESPBrew Server (Remote Management)**
- **Remote Board Discovery**: Network ESP32 detection with MAC-based identification
- **Real-Time Monitoring**: WebSocket serial monitoring with auto-reconnection
- **Remote Flashing**: Multi-binary uploads with bootloader + partition + app
- **Web Dashboard**: Modern interface at `http://localhost:8080`
- **RESTful API**: Complete board management and monitoring APIs
- **Smart Caching**: 5-minute board info caching for performance
- **Session Management**: Automatic cleanup and keep-alive

### 🔧 **Developer Experience**
- **Interactive TUI**: Terminal interface with component management
- **CLI Mode**: Perfect for CI/CD and automation with smart optimization
- **Smart Flash**: Automatic artifact detection - skips rebuilds when possible (~99% time savings)
- **Force Rebuild**: `--force-rebuild` flag for explicit clean builds
- **Live Monitoring**: Real-time build logs and serial output
- **Component Actions**: Clone, move, and manage ESP-IDF components
- **Smart Scripts**: Generated build/flash scripts for each board

## 🚀 Quick Start

### Installation

#### One-Line Install (Recommended)
```bash
# Install espbrew directly from releases
curl -L https://georgik.github.io/espbrew/install.sh | bash
```

#### Homebrew (macOS)
```bash
brew tap georgik/espbrew
brew install espbrew
```

#### From Source
```bash
git clone https://github.com/georgik/espbrew.git && cd espbrew
cargo build --release
```

### Basic Usage

#### TUI Mode (Interactive)
```bash
# Interactive TUI with current directory
espbrew

# Interactive TUI with specific directory  
espbrew /path/to/your/esp32-project
```

#### CLI Mode (Automation)
```bash
# List project boards and components
espbrew --cli

# List connected USB boards (serial ports)
espbrew boards

# Build all boards
espbrew --cli build

# Flash to local board with optimization
espbrew --cli flash --port /dev/ttyUSB0

# Flash with force rebuild
espbrew --cli flash --port /dev/ttyUSB0 --force-rebuild

# Flash to remote board
espbrew --cli remote-flash

# Monitor local ESP32 serial output
espbrew --cli monitor --port /dev/ttyUSB0

# Monitor with timeout (non-blocking for automation)
espbrew --cli monitor --timeout 30

# Monitor with success/failure pattern detection
espbrew --cli monitor --success-pattern "System ready" --failure-pattern "Error:"
```

#### Server Mode (Remote Management)
```bash
# Start ESPBrew Server
cargo run --bin espbrew-server --release

# Access web dashboard
open http://localhost:8080
```

## 📊 Supported Project Types (10 Frameworks)

### 🔧 **ESP-IDF Projects (C/C++)**
```
my-esp-idf-project/
├── CMakeLists.txt
├── main/
├── components/
├── sdkconfig.defaults.esp32s3      # ESP32-S3 config
├── sdkconfig.defaults.esp32c6      # ESP32-C6 config  
└── sdkconfig.defaults               # Base config
```

### 🦀 **Rust no_std Projects**
```
my-rust-project/
├── Cargo.toml
├── .cargo/config.toml               # Or config_*.toml
├── src/main.rs
└── target/xtensa-esp32s3-none-elf/   # Auto-detected chip
```
**Supported frameworks**: esp-hal, Embassy, embedded-hal

**🔥 CRITICAL: ELF-to-Binary Conversion (v0.5.0+)**
ESPBrew automatically converts Rust ELF binaries to proper ESP32 flash images:
- **Problem**: Raw ELF files (with debug symbols/headers) don't work on ESP32
- **Solution**: Automatic conversion using `espflash save-image` during flash
- **Result**: Applications now work identically to standalone `espflash` flashing
- **Performance**: ~45% size reduction (669KB ELF → 363KB binary)
- **Transparency**: Conversion happens automatically - no user action required

### 🎨 **Arduino ESP32 Projects**
```
my-arduino-project/
├── sketch.ino
├── boards.json                      # Multi-board config (optional)
└── build/
```
**Supported boards**: ESP32, ESP32-S2, ESP32-S3, ESP32-C3, ESP32-C6, ESP32-H2, ESP32-P4, M5Stack boards

### 🚀 **PlatformIO Projects**
```
my-platformio-project/
├── platformio.ini                   # Multi-environment config
├── src/
├── lib/
└── [env:esp32s3]                    # Auto-detected environments
```
**Build system**: pio run, pio upload, pio device monitor

### 🐍 **MicroPython Projects**
```
my-micropython-project/
├── main.py                          # Entry point
├── boot.py                          # Boot configuration
├── lib/                             # Libraries
└── requirements.txt                 # Dependencies (optional)
```
**Tools**: mpremote (preferred), ampy (fallback), screen monitoring

### 🔄 **CircuitPython Projects**
```
my-circuitpython-project/
├── code.py                          # Entry point
├── lib/                             # Libraries
└── requirements.txt                 # Dependencies
```
**Upload methods**: Mass storage (CIRCUITPY), circup, mpremote, ampy

### ⚡ **Zephyr RTOS Projects**
```
my-zephyr-project/
├── prj.conf                         # Project configuration
├── CMakeLists.txt                   # Build configuration
├── src/main.c
└── boards/                          # Board definitions (optional)
```
**Build system**: west build, west flash, west monitor

### 🏗️ **NuttX RTOS Projects**
```
my-nuttx-project/
├── .config                          # NuttX configuration
├── Makefile                         # Build system
├── defconfig                        # Default config (optional)
└── hello_main.c                     # Application source
```
**Build system**: make, esptool.py for ESP32 flashing

### 🏃 **TinyGo Projects**
```
my-tinygo-project/
├── go.mod                           # Go module
├── main.go                          # Entry point with "machine" import
└── go.sum                           # Dependencies
```
**Targets**: esp32-coreboard-v2, esp32-s3-usb-otg, esp32-c3-mini, esp32-c6-generic

### 📱 **Jaculus Projects (JavaScript/TypeScript)**
```
my-jaculus-project/
├── jaculus.json                     # Jaculus config (preferred)
├── package.json                     # Or npm-style config
├── index.js                         # Entry point
├── src/                             # Source directory
└── tsconfig.json                    # TypeScript config (optional)
```
**Tools**: jaculus-tools for upload/monitor, supports ESP32/ESP32-S3/ESP32-C3/ESP32-C6

### 🏆 **Framework Coverage Summary**

ESPBrew provides the most comprehensive ESP32 development support available:

| Language/Framework | Build System | Flashing | Monitoring | Multi-Board |
|-------------------|--------------|----------|------------|-------------|
| **C/C++ (ESP-IDF)** | idf.py/cmake | ✓ | ✓ ✓ | ✓ |
| **Rust (no_std)** | cargo | ✓ | ✓ ✓ | ✓ |
| **Arduino** | arduino-cli | ✓ | ✓ ✓ | ✓ |
| **PlatformIO** | pio | ✓ | ✓ ✓ | ✓ |
| **MicroPython** | mpremote/ampy | ✓ | ✓ ✓ | ✓ |
| **CircuitPython** | circup/mass storage | ✓ | ✓ ✓ | ✓ |
| **Zephyr RTOS** | west | ✓ | ✓ ✓ | ✓ |
| **NuttX RTOS** | make | ✓ | ✓ ✓ | ✓ |
| **TinyGo** | tinygo | ✓ | ✓ ✓ | ✓ |
| **Jaculus (JS/TS)** | jaculus-tools | ✓ | ✓ ✓ | ✓ |

**Monitoring**: ✓ Remote monitoring via ESPBrew server, ✓ ✓ Local monitoring with timeout & pattern matching

**Total: 10 frameworks supported** - covering every major ESP32 development approach!

## 📋 TUI Interface Guide

### Navigation
- **↑↓ or j/k**: Navigate boards/components/logs
- **Tab**: Switch between Board List → Component List → Log Pane
- **Enter**: Show action menu for selected item
- **b**: Build all boards
- **m**: Monitor selected board
- **r**: Refresh lists
- **h or ?**: Toggle help
- **q**: Quit

### Board Actions (Press Enter)
- **Build**: Build project for selected board
- **Flash**: Flash all partitions (bootloader + app + data)
- **Monitor**: Flash and start serial monitor
- **Local Monitor**: Monitor local serial output (new!)
- **Remote Flash**: Flash via ESPBrew server
- **Remote Monitor**: Monitor via server WebSocket
- **Clean/Purge**: Clean build files

### Component Actions (Press Enter on components)
- **Move to Components**: Move managed → local
- **Clone from Repository**: Fresh Git clone
- **Remove**: Delete component
- **Open in Editor**: Open in system editor

## 🌐 ESPBrew Server API

### Core Endpoints
```bash
# Board Management  
GET    /api/v1/boards              # List all boards
POST   /api/v1/flash               # Flash board with binaries
POST   /api/v1/reset               # Reset board

# Monitoring
POST   /api/v1/monitor/start       # Start monitoring session
GET    /api/v1/monitor/sessions    # List active sessions
WS     /ws/monitor/{session_id}    # WebSocket for logs

# Board Configuration
GET    /api/v1/board-types         # List available board types
POST   /api/v1/assign-board        # Assign board to type

# System
GET    /health                     # Health check
```

### Example API Response
```json
{
  "boards": [{
    "id": "board_MAC8CBFEAB34E08",
    "port": "/dev/cu.usbmodem1101",
    "chip_type": "ESP32-S3",
    "features": "USB-OTG, WiFi, Bluetooth",
    "device_description": "M5Stack Core S3 - 10:30:15",
    "status": "Available",
    "mac_address": "8C:BF:EA:B3:4E:08",
    "unique_id": "MAC8CBFEAB34E08"
  }],
  "server_info": {
    "version": "0.5.0",
    "hostname": "your-machine.local",
    "total_boards": 1
  }
}
```

## 🏗️ **Architecture Advantages**

### **🎯 Simplified Development Workflow**

ESPBrew's ESP-IDF independence provides significant architectural benefits:

**🔄 Build vs Flash Separation:**
- **Building**: Framework-specific tools (idf.py, cargo, arduino-cli, etc.)
- **Flashing**: Unified espflash-based flashing for all project types
- **Result**: Mix and match frameworks without complex toolchain conflicts

**🚀 CI/CD Optimization:**
```dockerfile
# Dockerfile example - Simplified dependencies for flashing
FROM rust:slim
RUN cargo install espbrew
COPY ./my-rust-project .
RUN cargo build --release  # Build with Rust tools
RUN espbrew flash         # Flash without ESP-IDF dependency
```

**📦 Container Benefits:**
- Smaller container images (avoids 2GB+ ESP-IDF installation)
- Faster container startup times
- Consistent flashing across all environments
- Reduced dependency conflicts between projects

**🔧 Developer Benefits:**
- **Streamlined onboarding** - Reduced setup requirements for new team members
- **Flexible workflows** - Use different frameworks for development and production
- **Remote deployment** - Flash boards over network with minimal dependencies
- **Unified tooling** - Single tool for ESP32 flashing across project types

## 📊 Logging & Configuration

### Logging Levels
ESPBrew uses structured logging for better debugging and monitoring:

```bash
# Standard logging (INFO and above)
espbrew --cli build

# Verbose logging (DEBUG level)
RUST_LOG=debug espbrew --cli build

# Very verbose logging (TRACE level)  
RUST_LOG=trace espbrew --cli build

# Quiet mode (ERROR only)
RUST_LOG=error espbrew --cli build

# Server logging to file
RUST_LOG=info cargo run --bin espbrew-server --release 2>&1 | tee server.log
```

### Log Output Formats
**CLI Mode**: Human-readable logs to stderr
```
2024-01-11 17:03:45 [INFO] espbrew::cli: Starting flash operation on port /dev/ttyUSB0
2024-01-11 17:03:46 [DEBUG] espbrew::utils::espflash: Binary size: 363KB
```

**TUI Mode**: Silent logging to file only (preserves interface)
**Server Mode**: Structured JSON logging for production monitoring

### Configuration Files
- **Server Config**: `~/.config/espbrew/espbrew-boards.ron`
- **Board Assignments**: Persistent MAC-based board mapping
- **Log Files**: `./logs/{operation}.log` for build/flash operations

## 🖥️ Local Serial Monitoring

ESPBrew provides comprehensive local ESP32 serial monitoring with advanced pattern matching and timeout controls, perfect for both development workflows and automated testing.

### Basic Monitoring

```bash
# Simple monitoring with auto port detection
espbrew --cli monitor

# Monitor specific port
espbrew --cli monitor --port /dev/ttyUSB0

# Custom baud rate
espbrew --cli monitor --port /dev/ttyUSB0 --baud-rate 921600
```

### Automation-Ready Monitoring (Timeout & Patterns)

The monitor is designed for **non-blocking automation** with intelligent exit conditions:

```bash
# Timeout-based monitoring (exits after 30 seconds)
espbrew --cli monitor --timeout 30

# Success pattern monitoring (exits when pattern is found)
espbrew --cli monitor --success-pattern "System ready"

# Failure pattern monitoring (exits with error on pattern)
espbrew --cli monitor --failure-pattern "Error|panic|assertion failed"

# Combined monitoring with multiple exit conditions
espbrew --cli monitor \
  --success-pattern "Application started" \
  --failure-pattern "Error|Failed" \
  --timeout 60
```

### Advanced Pattern Matching

Use regular expressions for powerful pattern detection:

```bash
# Regex patterns for complex matching
espbrew --cli monitor --success-pattern "WiFi.*connected"
espbrew --cli monitor --failure-pattern "Boot.*failed|Exception"

# Case-insensitive patterns
espbrew --cli monitor --success-pattern "(?i)system ready"

# Pattern for boot sequence completion
espbrew --cli monitor --success-pattern "main loop|setup complete"
```

### Configuration Options

| Parameter | Description | Default | Use Case |
|-----------|-------------|---------|----------|
| `--port` | Serial port to monitor | Auto-detect | Multiple ESP32 devices |
| `--baud-rate` | Serial communication speed | 115200 | High-speed debugging |
| `--timeout` | Maximum duration in seconds (0 = infinite) | 0 | CI/CD automation |
| `--success-pattern` | Regex pattern for success exit | None | Boot completion detection |
| `--failure-pattern` | Regex pattern for failure exit | None | Error detection |
| `--elf` | ELF file for symbol resolution | None | Address decoding |
| `--log-format` | Log format (serial/defmt) | serial | Structured logging |
| `--reset` | Reset device before monitoring | false | Complete boot capture |
| `--non-interactive` | Disable keyboard input handling | false | Pure automation |

### Real-World Examples

**Development Workflow:**
```bash
# Monitor during development with human-friendly exit
espbrew --cli monitor --success-pattern "WiFi connected"
```

**CI/CD Pipeline:**
```bash
# Automated testing with guaranteed exit
espbrew --cli monitor \
  --success-pattern "All tests passed" \
  --failure-pattern "Test failed|Error" \
  --timeout 120
```

**Production Deployment:**
```bash
# Monitor deployment with rollback conditions
espbrew --cli monitor \
  --success-pattern "Production ready" \
  --failure-pattern "Critical error" \
  --timeout 300
```

**Boot Sequence Validation:**
```bash
# Complete boot sequence monitoring
espbrew --cli monitor \
  --reset \
  --success-pattern "Setup complete" \
  --timeout 45
```

### Features

- **🔄 Auto Port Detection**: Automatically finds ESP32 devices
- **⏱️ Timeout Control**: Prevents infinite monitoring in automation
- **🎯 Pattern Matching**: Regex-based success/failure detection
- **📊 Real-time Display**: Live serial output with formatting
- **🔧 Device Info**: Shows USB device details (VID/PID/Manufacturer)
- **🚀 CI/CD Ready**: Designed for automated workflows
- **💻 Cross-Platform**: Works on macOS, Linux, and Windows

## 🔧 Advanced Usage

### CI/CD Integration
```yaml
# GitHub Actions Example
name: Multi-Board ESP32 Build
on: [push, pull_request]
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: espressif/esp-idf-ci-action@v1
      - name: Install ESPBrew
        run: curl -L https://georgik.github.io/espbrew/install.sh | bash
      - name: Build All Boards
        run: espbrew --cli build
```

### Remote Flashing Examples
```bash
# Multi-binary ESP-IDF flash
curl -X POST http://localhost:8080/api/v1/flash \
  -F "board_id=board_MAC8CBFEAB34E08" \
  -F "binary_count=3" \
  -F "binary_0=@bootloader.bin" \
  -F "binary_0_offset=0x0" \
  -F "binary_1=@partition-table.bin" \
  -F "binary_1_offset=0x8000" \
  -F "binary_2=@app.bin" \
  -F "binary_2_offset=0x10000"

# Rust project remote flash (automatic)
espbrew --cli remote-flash --name "M5Stack Core S3"
```

## 📊 Project Structure

### Generated Files
ESPBrew creates organized project structure:
```
your-project/
├── sdkconfig.defaults.*          # Your board configs
├── build.{board_name}/           # Isolated build dirs
├── logs/                         # Build logs per board
│   ├── esp32s3.log
│   └── esp32c6.log
└── support/                      # Generated scripts
    ├── build-all-idf-build-apps.sh
    ├── build_esp32s3.sh
    └── flash_esp32s3.sh
```

### Configuration Files
- **Server Config**: `~/.config/espbrew/espbrew-boards.ron`
- **Board Assignments**: Persistent MAC-based board mapping
- **Cache**: 5-minute board info caching for performance

## 🔍 Troubleshooting

### Common Issues
**Board Not Detected**
- Check `sdkconfig.defaults.{board_name}` exists
- For building (not flashing): Verify ESP-IDF environment setup
- Note: ESP-IDF is only required for building ESP-IDF projects, not for flashing!
- **Windows**: Use `espbrew boards` to list all detected serial ports
  - Supports ESP32 boards with VID `0x303A`, `0x1001`, `0x10C4` (CP210x), `0x0403` (FTDI), `0x1A86` (CH340), etc.
  - Fixed: ESP32 boards showing "Microsoft" as manufacturer are now correctly detected

**Build Failures**  
- Check logs in `./logs/{board}.log`
- Ensure target dependencies installed
- ESP-IDF projects: Verify ESP-IDF installation and PATH
- Rust projects: Ensure correct target installed (`rustup target add xtensa-esp32s3-none-elf`)
- Rust no_std: ESPBrew automatically converts ELF to binary (v0.5.0+) - no manual conversion needed

**Flashing Issues**
- ESPBrew handles all flashing internally - no ESP-IDF installation required!
- Check USB cable connection and board power
- Verify correct serial port permissions (`sudo usermod -a -G dialout $USER` on Linux)

**Remote Connection Failed**
- Start server: `cargo run --bin espbrew-server --release`
- Check firewall allows port 8080

**Component Actions Failing**
- Ensure Git installed and repository accessible
- Check write permissions for `components/` directory

## 🤝 Contributing

We welcome contributions! ESPBrew maintains high code quality standards with structured logging and zero-warning builds.

### ✨ **Quick Contributor Setup**
```bash
git clone https://github.com/georgik/espbrew.git
cd espbrew
cargo build --release  # Must pass with zero warnings
cargo test              # All tests must pass
```

### 📝 **Critical Guidelines**
- **✅ Production Ready**: Zero compiler warnings required
- **📝 Structured Logging**: Follow logging architecture (see [CONTRIBUTING.md](CONTRIBUTING.md))
- **🚀 TUI Safe**: Never use `println!`/`eprintln!` in TUI components
- **🔧 Shell Commands**: Always use single quotes to avoid syntax issues

### 🎯 **Focus Areas**
- **Framework Extensions**: Enhanced support for existing 10 frameworks
- **New Project Types**: Additional embedded development platforms
- **Enhanced TUI**: More interactive features and better UX
- **Performance**: Build optimization and caching improvements
- **Integration**: IDE plugins and CI/CD workflow enhancements
- **Testing**: Expand test coverage for all frameworks
- **Documentation**: Framework-specific guides and examples

**See [CONTRIBUTING.md](CONTRIBUTING.md) for complete development guidelines.**

## 📜 License

MIT License - see [LICENSE](LICENSE) for details.

## 🚀 Credits

Built with:
- **Ratatui** - Terminal user interfaces
- **Tokio** - Async runtime
- **Warp** - Web server framework
- **Clap** - CLI argument parsing
- **espflash** - ESP32 flashing utilities

---

**🍺 ESPBrew** - The most comprehensive ESP32 development platform supporting 10 frameworks
