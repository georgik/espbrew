# Usage Guide

ESPBrew provides multiple interfaces for different workflows:

## TUI Mode (Interactive)

The default mode when you run `espbrew` without arguments.

```bash
# Interactive TUI with current directory
espbrew

# Interactive TUI with specific directory
espbrew /path/to/your/esp32-project
```

### TUI Navigation

| Key | Action |
|-----|--------|
| `Tab` | Switch between panes |
| `Enter` | Select item / Start action |
| `b` | Build selected board |
| `f` | Flash selected board |
| `m` | Monitor selected board |
| `r` | Reset selected board |
| `h` | Show help |
| `q` | Quit |

### TUI Panes

- **Boards**: Connected ESP32 devices and virtual targets
- **Components**: Buildable components of the project
- **Actions**: Available operations for selected items

## CLI Mode (Automation)

CLI mode is ideal for CI/CD pipelines and scripting.

```bash
# List boards and components (default CLI behavior)
espbrew --cli

# List connected USB boards
espbrew boards

# Build all boards
espbrew --cli build

# Build specific board
espbrew --cli build --board esp32s3

# Flash to local board
espbrew --cli flash --port /dev/ttyUSB0

# Flash with force rebuild
espbrew --cli flash --port /dev/ttyUSB0 --force-rebuild

# Monitor serial output
espbrew --cli monitor --port /dev/ttyUSB0

# Monitor with timeout (non-blocking)
espbrew --cli monitor --timeout 30

# Monitor with success/failure pattern detection
espbrew --cli monitor --success-pattern "System ready" --failure-pattern "Error:"
```

## Server Mode (Remote Management)

Start the ESPBrew server for web-based board management:

```bash
# Start ESPBrew Server
cargo run --bin espbrew-server --release

# Access web dashboard
open http://localhost:8080
```

The server provides:
- Web interface for board management
- Remote flashing capabilities
- Real-time monitoring
- Multi-board management

## Project Detection

ESPBrew automatically detects project type based on files present:

| Framework | Detection Files |
|-----------|-----------------|
| ESP-IDF | `CMakeLists.txt`, `sdkconfig` |
| Rust no_std | `Cargo.toml`, `.cargo/config.toml` |
| Arduino | `*.ino` file, `boards.json` |
| PlatformIO | `platformio.ini` |
| MicroPython | `main.py`, `boot.py` |
| CircuitPython | `code.py` |
| Zephyr | `prj.conf`, `west.yml` |
| NuttX | Makefile with NuttX structure |
| TinyGo | `go.mod` with TinyGo imports |
| Jaculus | `package.json` with Jaculus config |

## Build Strategies

ESPBrew supports multiple build strategies for multi-board projects:

```bash
# Default: Professional parallel build with ESP-IDF build-apps
espbrew --cli build --build-strategy idf-build-apps

# Safe: Sequential builds (no conflicts)
espbrew --cli build --build-strategy sequential

# Fast: Parallel builds (may have conflicts)
espbrew --cli build --build-strategy parallel
```

## Verbosity Control

Control logging output:

```bash
# Default logging
espbrew

# Debug logging
espbrew -v

# Trace logging
espbrew -vv

# Quiet mode (errors only)
espbrew -q
```

## Remote Operations

When using ESPBrew server for remote board management:

```bash
# Set server URL
espbrew --server-url http://192.168.1.100:8080 --cli flash

# Target specific board by MAC address
espbrew --board-mac AA:BB:CC:DD:EE:FF --cli flash

# Remote monitor
espbrew --server-url http://192.168.1.100:8080 --cli remote-monitor --timeout 60
```

## Common Workflows

### Flash and Monitor

```bash
# Flash and immediately start monitoring
espbrew --cli flash --port /dev/ttyUSB0
espbrew --cli monitor --port /dev/ttyUSB0
```

### CI/CD Pipeline

```bash
# Build and flash in CI environment
espbrew --cli build --board esp32s3
espbrew --cli flash --port /dev/ttyUSB0 --force-rebuild
espbrew --cli monitor --timeout 30 --success-pattern "WiFi connected" --failure-pattern "Error"
```

### Multi-Board Testing

```bash
# Build for all boards
espbrew --cli build

# Flash to specific boards
espbrew --cli flash --port /dev/ttyUSB0  # Board 1
espbrew --cli flash --port /dev/ttyUSB1  # Board 2
```
