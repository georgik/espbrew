# ESPBrew - ESP32 Multi-Framework Build Tool

ESPBrew is a command-line tool that supports building and flashing ESP32 projects across multiple development frameworks. It provides both a terminal user interface (TUI) and CLI mode for automating builds, along with optional remote board management capabilities.

## Supported Frameworks

ESPBrew currently supports 10 ESP32 development frameworks:

- **ESP-IDF** (C/C++) - Traditional ESP32 development
- **Rust no_std** - Embedded Rust with esp-hal/Embassy
- **Arduino** - Arduino framework with arduino-cli
- **PlatformIO** - Cross-platform IoT development
- **MicroPython** - Python for microcontrollers
- **CircuitPython** - Python for embedded systems
- **Zephyr RTOS** - Real-time operating system
- **NuttX RTOS** - POSIX-compliant RTOS
- **TinyGo** - Go for embedded systems
- **Jaculus** - JavaScript runtime for ESP32

## Flashing without ESP-IDF

For most operations, ESPBrew uses `espflash` for flashing, which means you don't need to install ESP-IDF unless you're building ESP-IDF projects:

- **Flashing**: Uses espflash for all project types
- **Building**: Framework-specific tools (idf.py, cargo, arduino-cli, etc.)
- **ESP-IDF projects**: Still require ESP-IDF for building, but flashing works without it

**Supported chips**: ESP32, ESP32-S2, ESP32-S3, ESP32-C2, ESP32-C3, ESP32-C5, ESP32-C6, ESP32-H2, ESP32-P4

**Platforms**: macOS, Linux, Windows

**License**: MIT

## Features

### Multi-Framework Support
- **ESP-IDF**: Traditional C/C++ development with sdkconfig files
- **Rust no_std**: Embedded Rust with esp-hal and Embassy
- **Arduino**: Arduino framework with arduino-cli integration
- **PlatformIO**: Cross-platform development with multi-environment support
- **MicroPython**: Python with mpremote/ampy tools
- **CircuitPython**: Python with mass storage and circup support
- **Zephyr RTOS**: Real-time OS with west build system
- **NuttX RTOS**: POSIX-compliant RTOS with make build system
- **TinyGo**: Go language for embedded systems
- **Jaculus**: JavaScript/TypeScript runtime for ESP32

### User Interfaces
- **TUI Mode**: Interactive terminal interface for development
- **CLI Mode**: Command-line interface suitable for CI/CD automation
- **Server Mode**: Optional web dashboard for remote board management

### Build & Flash Management
- **Multi-board support**: Automatic board detection and configuration
- **Smart artifact detection**: Skips rebuilds when binaries are up to date
- **Serial monitoring**: Local and remote monitoring with pattern matching
- **Remote operations**: Network-based flashing and monitoring (requires server)

## Installation

### Install from Releases
```bash
curl -L https://georgik.github.io/espbrew/install.sh | bash
```

### Homebrew (macOS)
```bash
brew tap georgik/espbrew
brew install espbrew
```

### Build from Source
```bash
git clone https://github.com/georgik/espbrew.git
cd espbrew
cargo build --release
```

## Quick Start

### TUI Mode (Interactive)
```bash
# Interactive TUI with current directory
espbrew

# Interactive TUI with specific directory
espbrew /path/to/your/esp32-project
```

### CLI Mode (Automation)
```bash
# List project boards and components
espbrew --cli

# List connected USB boards (serial ports)
espbrew boards

# Build all boards
espbrew --cli build

# Flash to local board
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

# Monitor remote ESP32 with pattern matching
espbrew --cli remote-monitor --timeout 60 --success-pattern "WiFi.*connected" --failure-pattern "Error|failed" --reset
```

### Server Mode (Remote Management)
```bash
# Start ESPBrew Server
cargo run --bin espbrew-server --release

# Access web dashboard
open http://localhost:8080
```

## Project Types

### ESP-IDF Projects (C/C++)
```
my-esp-idf-project/
├── CMakeLists.txt
├── main/
├── components/
├── sdkconfig.defaults.esp32s3      # ESP32-S3 config
├── sdkconfig.defaults.esp32c6      # ESP32-C6 config
└── sdkconfig.defaults               # Base config
```

### Rust no_std Projects
```
my-rust-project/
├── Cargo.toml
├── .cargo/config.toml               # Or config_*.toml
├── src/main.rs
└── target/xtensa-esp32s3-none-elf/   # Auto-detected chip
```
**Supported frameworks**: esp-hal, Embassy, embedded-hal

**ELF-to-Binary Conversion**: ESPBrew automatically converts Rust ELF binaries to ESP32 flash images using `espflash save-image` during flashing.

### Arduino ESP32 Projects
```
my-arduino-project/
├── sketch.ino
├── boards.json                      # Multi-board config (optional)
└── build/
```
**Supported boards**: ESP32, ESP32-S2, ESP32-S3, ESP32-C3, ESP32-C6, ESP32-H2, ESP32-P4, M5Stack boards

### PlatformIO Projects
```
my-platformio-project/
├── platformio.ini                   # Multi-environment config
├── src/
├── lib/
└── [env:esp32s3]                    # Auto-detected environments
```
**Build system**: pio run, pio upload, pio device monitor

### MicroPython Projects
```
my-micropython-project/
├── main.py                          # Entry point
├── boot.py                          # Boot configuration
├── lib/                             # Libraries
└── requirements.txt                 # Dependencies (optional)
```
**Tools**: mpremote (preferred), ampy (fallback), screen monitoring

### CircuitPython Projects
```
my-circuitpython-project/
├── code.py                          # Entry point
├── lib/                             # Libraries
└── requirements.txt                 # Dependencies
```
**Upload methods**: Mass storage (CIRCUITPY), circup, mpremote, ampy

### Zephyr RTOS Projects
```
my-zephyr-project/
├── prj.conf                         # Project configuration
├── CMakeLists.txt                   # Build configuration
├── src/main.c
└── boards/                          # Board definitions (optional)
```
**Build system**: west build, west flash, west monitor

### NuttX RTOS Projects
```
my-nuttx-project/
├── .config                          # NuttX configuration
├── Makefile                         # Build system
├── defconfig                        # Default config (optional)
└── hello_main.c                     # Application source
```
**Build system**: make, esptool.py for ESP32 flashing

### TinyGo Projects
```
my-tinygo-project/
├── go.mod                           # Go module
├── main.go                          # Entry point with "machine" import
└── go.sum                           # Dependencies
```
**Targets**: esp32-coreboard-v2, esp32-s3-usb-otg, esp32-c3-mini, esp32-c6-generic

### Jaculus Projects (JavaScript/TypeScript)
```
my-jaculus-project/
├── jaculus.json                     # Jaculus config (preferred)
├── package.json                     # Or npm-style config
├── index.js                         # Entry point
├── src/                             # Source directory
└── tsconfig.json                    # TypeScript config (optional)
```
**Tools**: jaculus-tools for upload/monitor, supports ESP32/ESP32-S3/ESP32-C3/ESP32-C6

## Framework Support Matrix

| Language/Framework | Build System | Flashing | Local Monitoring | Remote Monitoring | Multi-Board |
|-------------------|--------------|----------|----------------|------------------|-------------|
| **C/C++ (ESP-IDF)** | idf.py/cmake | ✓ | ✓ | ✓ | ✓ |
| **Rust (no_std)** | cargo | ✓ | ✓ | ✓ | ✓ |
| **Arduino** | arduino-cli | ✓ | ✓ | ✓ | ✓ |
| **PlatformIO** | pio | ✓ | ✓ | ✓ | ✓ |
| **MicroPython** | mpremote/ampy | ✓ | ✓ | ✓ | ✓ |
| **CircuitPython** | circup/mass storage | ✓ | ✓ | ✓ | ✓ |
| **Zephyr RTOS** | west | ✓ | ✓ | ✓ | ✓ |
| **NuttX RTOS** | make | ✓ | ✓ | ✓ | ✓ |
| **TinyGo** | tinygo | ✓ | ✓ | ✓ | ✓ |
| **Jaculus (JS/TS)** | jaculus-tools | ✓ | ✓ | ✓ | ✓ |

**Monitoring Features**:
- Timeout control and pattern matching
- ANSI color preservation
- CI/CD automation support
- Remote streaming via WebSocket

## TUI Interface

### Navigation Controls
- **↑↓ or j/k**: Navigate boards/components/logs
- **Tab**: Switch between Board List → Component List → Log Pane
- **Enter**: Show action menu for selected item
- **b**: Build all boards
- **m**: Monitor selected board
- **r**: Refresh lists
- **h or ?**: Toggle help
- **q**: Quit

### Board Actions
- **Build**: Build project for selected board
- **Flash**: Flash all partitions (bootloader + app + data)
- **Monitor**: Flash and start serial monitor
- **Local Monitor**: Monitor local serial output
- **Remote Flash**: Flash via ESPBrew server
- **Remote Monitor**: Monitor via server WebSocket
- **Clean/Purge**: Clean build files

### Component Actions
- **Move to Components**: Move managed → local
- **Clone from Repository**: Fresh Git clone
- **Remove**: Delete component
- **Open in Editor**: Open in system editor

## Server API

### Endpoints
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

### API Response Example
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
    "version": "0.7.0",
    "hostname": "your-machine.local",
    "total_boards": 1
  }
}
```

## Architecture

### Build vs Flash Separation
- **Building**: Uses framework-specific tools (idf.py, cargo, arduino-cli, etc.)
- **Flashing**: Unified espflash-based flashing for all project types
- **Result**: Consistent flashing workflow across different frameworks

### CI/CD Benefits
```dockerfile
# Dockerfile example
FROM rust:slim
RUN cargo install espbrew
COPY ./my-rust-project .
RUN cargo build --release
RUN espbrew flash
```

This approach provides:
- Smaller container images (avoids full ESP-IDF installation)
- Faster container startup times
- Consistent flashing across environments
- Reduced dependency conflicts

## Logging & Configuration

### Logging Levels
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

### Log Formats
- **CLI Mode**: Human-readable logs to stderr
- **TUI Mode**: Silent logging to file (preserves interface)
- **Server Mode**: Structured JSON logging

### Configuration
- **Server Config**: `~/.config/espbrew/espbrew-boards.ron`
- **Board Assignments**: Persistent MAC-based board mapping
- **Log Files**: `./logs/{operation}.log` for build/flash operations

## Serial Monitoring

### Local Monitoring
```bash
# Basic monitoring with auto port detection
espbrew --cli monitor

# Monitor specific port
espbrew --cli monitor --port /dev/ttyUSB0

# Custom baud rate
espbrew --cli monitor --port /dev/ttyUSB0 --baud-rate 921600
```

### Automation Features
```bash
# Timeout-based monitoring (exits after 30 seconds)
espbrew --cli monitor --timeout 30

# Success pattern monitoring (exits when pattern is found)
espbrew --cli monitor --success-pattern "System ready"

# Failure pattern monitoring (exits with error on pattern)
espbrew --cli monitor --failure-pattern "Error|panic|assertion failed"

# Combined monitoring with multiple conditions
espbrew --cli monitor \
  --success-pattern "Application started" \
  --failure-pattern "Error|Failed" \
  --timeout 60
```

### Advanced Pattern Matching
```bash
# Regex patterns for complex matching
espbrew --cli monitor --success-pattern "WiFi.*connected"
espbrew --cli monitor --failure-pattern "Boot.*failed|Exception"

# Case-insensitive patterns
espbrew --cli monitor --success-pattern "(?i)system ready"
```

### Configuration Options
| Parameter | Description | Default |
|-----------|-------------|---------|
| `--port` | Serial port to monitor | Auto-detect |
| `--baud-rate` | Serial communication speed | 115200 |
| `--timeout` | Maximum duration in seconds (0 = infinite) | 0 |
| `--success-pattern` | Regex pattern for success exit | None |
| `--failure-pattern` | Regex pattern for failure exit | None |
| `--reset` | Reset device before monitoring | false |
| `--non-interactive` | Disable keyboard input | false |

## Remote Monitoring

Remote monitoring provides the same features as local monitoring but operates through ESPBrew servers for network-based access.

### Remote Monitoring Examples
```bash
# Basic remote monitoring with auto-discovery
espbrew --cli remote-monitor

# Monitor specific board by MAC address
espbrew --cli remote-monitor --mac AA:BB:CC:DD:EE:FF

# Monitor board by logical name
espbrew --cli remote-monitor --name "production-esp32"

# Remote monitoring with timeout and pattern matching
espbrew --cli remote-monitor \
  --timeout 60 \
  --success-pattern "WiFi.*connected" \
  --failure-pattern "Error|Failed" \
  --reset
```

### Server Setup
```bash
# Start ESPBrew Server
cargo run --bin espbrew-server --release

# Server runs on port 8080 by default
# Clients connect via HTTP API + WebSocket streaming
```

### Features
- **mDNS Discovery**: Automatic server discovery
- **WebSocket Streaming**: Real-time log streaming with ANSI colors
- **Multiple Clients**: Multiple users can monitor the same board
- **Session Management**: Automatic cleanup and keep-alive
- **Same Syntax**: Identical parameters to local monitoring

### Use Cases
- **Distributed teams**: Collaborative debugging across locations
- **Automated testing**: Monitor devices in test farms
- **Remote deployment**: Verify remote flashing and boot success
- **Production monitoring**: Track deployed device health

## Advanced Usage

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
      - name: Install ESPBrew
        run: curl -L https://georgik.github.io/espbrew/install.sh | bash
      - name: Build All Boards
        run: espbrew --cli build
```

### Remote Flashing
```bash
# Multi-binary ESP-IDF flash via API
curl -X POST http://localhost:8080/api/v1/flash \
  -F "board_id=board_MAC8CBFEAB34E08" \
  -F "binary_count=3" \
  -F "binary_0=@bootloader.bin" \
  -F "binary_0_offset=0x0" \
  -F "binary_1=@partition-table.bin" \
  -F "binary_1_offset=0x8000" \
  -F "binary_2=@app.bin" \
  -F "binary_2_offset=0x10000"

# Rust project remote flash
espbrew --cli remote-flash --name "M5Stack Core S3"
```

## Project Structure

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

## Troubleshooting

### Common Issues

**Board Not Detected**
- Check `sdkconfig.defaults.{board_name}` exists
- ESP-IDF is only required for building ESP-IDF projects, not for flashing
- On Windows, use `espbrew boards` to list all detected serial ports
- Supported ESP32 boards with VID: `0x303A`, `0x1001`, `0x10C4` (CP210x), `0x0403` (FTDI), `0x1A86` (CH340)

**Build Failures**
- Check logs in `./logs/{board}.log`
- ESP-IDF projects: Verify ESP-IDF installation and PATH
- Rust projects: Ensure correct target installed (`rustup target add xtensa-esp32s3-none-elf`)

**Flashing Issues**
- ESPBrew handles flashing using espflash - no ESP-IDF installation required
- Check USB cable connection and board power
- Verify serial port permissions (`sudo usermod -a -G dialout $USER` on Linux)

**Remote Connection Failed**
- Start server: `cargo run --bin espbrew-server --release`
- Check firewall allows port 8080

**Component Actions Failing**
- Ensure Git installed and repository accessible
- Check write permissions for `components/` directory

## Contributing

ESPBrew welcomes contributions and maintains high code quality standards with zero-warning builds.

### Development Setup
```bash
git clone https://github.com/georgik/espbrew.git
cd espbrew
cargo build --release  # Must pass with zero warnings
cargo test              # All tests must pass
```

### Guidelines
- **Zero warnings**: All builds must pass without compiler warnings
- **Structured logging**: Follow the existing logging architecture
- **TUI safety**: Never use `println!`/`eprintln!` in TUI components
- **Shell commands**: Use single quotes to avoid syntax issues

### Focus Areas
- **Framework support**: Enhanced support for existing frameworks
- **New platforms**: Additional embedded development platforms
- **UI improvements**: Better TUX and interactive features
- **Performance**: Build optimization and caching
- **Integration**: IDE plugins and CI/CD enhancements
- **Testing**: Expanded test coverage
- **Documentation**: Framework-specific guides

See [CONTRIBUTING.md](CONTRIBUTING.md) for complete development guidelines.

## License

MIT License - see [LICENSE](LICENSE) for details.

## Dependencies

Built with:
- **Ratatui** - Terminal user interfaces
- **Tokio** - Async runtime
- **Warp** - Web server framework
- **Clap** - CLI argument parsing
- **espflash** - ESP32 flashing utilities

---

**ESPBrew** - Multi-framework ESP32 build tool supporting 10 development frameworks

**Version 0.7.0** features:
- Advanced local and remote monitoring with timeout control and pattern matching
- ANSI color preservation for terminal output
- CI/CD ready monitoring with guaranteed exit conditions
- Network streaming for distributed teams
