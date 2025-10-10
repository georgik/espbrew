# 🍺 ESPBrew - ESP32 Multi-Board Development Platform

**ESPBrew** is a comprehensive ESP32 development platform that combines powerful CLI/TUI tools with network-based remote board management. It provides automatic board discovery, multi-project support, real-time monitoring, and a modern web dashboard for professional ESP32 development workflows.

![ESP32 Multi-Board](https://img.shields.io/badge/ESP32-Multi--Board-blue)
![Multi-Framework](https://img.shields.io/badge/Supports-ESP--IDF%20%7C%20Rust%20%7C%20Arduino-green)
![Rust](https://img.shields.io/badge/rust-1.75+-orange.svg)
![License](https://img.shields.io/badge/license-MIT-green.svg)
![Production Ready](https://img.shields.io/badge/status-Production%20Ready-brightgreen)

## ✨ Core Features

### 💻 **Multi-Framework ESP32 Support**
- **ESP-IDF Projects**: Traditional C/C++ projects with `sdkconfig.defaults.*` configs
- **Rust no_std**: Full esp-hal, Embassy & embedded frameworks support 🦀
- **Arduino ESP32**: arduino-cli integration with FQBN support 🎨
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
- **CLI Mode**: Perfect for CI/CD and automation
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
# List boards and components
espbrew --cli-only

# Build all boards
espbrew --cli-only build

# Flash to remote board
espbrew --cli-only remote-flash
```

#### Server Mode (Remote Management)
```bash
# Start ESPBrew Server
cargo run --bin espbrew-server --release

# Access web dashboard
open http://localhost:8080
```

## 📊 Supported Project Types

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

### 🎨 **Arduino ESP32 Projects**
```
my-arduino-project/
├── sketch.ino
├── boards.json                      # Multi-board config (optional)
└── build/
```

**Supported boards**: ESP32, ESP32-S2, ESP32-S3, ESP32-C3, ESP32-C6, ESP32-H2, ESP32-P4, M5Stack boards

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
        run: espbrew --cli-only build
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
espbrew --cli-only remote-flash --name "M5Stack Core S3"
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
- Verify ESP-IDF environment setup

**Build Failures**  
- Check logs in `./logs/{board}.log`
- Ensure target dependencies installed

**Remote Connection Failed**
- Start server: `cargo run --bin espbrew-server --release`
- Check firewall allows port 8080

**Component Actions Failing**
- Ensure Git installed and repository accessible
- Check write permissions for `components/` directory

## 🤝 Contributing

We welcome contributions! Focus areas:
- **Multi-Framework Support**: Additional project types
- **Enhanced TUI**: More interactive features
- **Performance**: Build optimization
- **Integration**: IDE and CI/CD improvements

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

**🍺 ESPBrew** - Professional ESP32 multi-board development platform
