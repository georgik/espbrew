# ESPBrew - ESP32 Multi-Framework Build Tool - Proof of Concept

** Warning ** This flavor of project is on hold, look at more advanced solution: https://codeberg.org/georgik/espbrew-go

ESPBrew is a command-line tool that supports building and flashing ESP32 projects across multiple development frameworks. It provides both a terminal user interface (TUI) and CLI mode for automating builds, along with optional remote board management capabilities.

## Supported Frameworks


## Features

- **Multi-Framework Support**: Single tool for ESP-IDF, Rust, Arduino, PlatformIO, MicroPython, CircuitPython, Zephyr, NuttX, TinyGo, and Jaculus
- **TUI Mode**: Interactive terminal interface for development
- **CLI Mode**: Command-line interface suitable for CI/CD automation
- **Server Mode**: Optional web dashboard for remote board management
- **Cluster Mode**: Distributed ESP32 device management for parallel flashing
- **Smart Caching**: Skips rebuilds when binaries are up to date
- **Multi-Board Support**: Automatic board detection and configuration

## Quick Start

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

## Basic Usage

```bash
# Interactive TUI mode
espbrew

# List boards
espbrew boards

# Build and flash
espbrew --cli build
espbrew --cli flash --port /dev/ttyUSB0

# Monitor serial output
espbrew --cli monitor --port /dev/ttyUSB0
```

## Documentation

For detailed documentation, see:

- [Installation Guide](docs/installation.md)
- [Usage Guide](docs/usage.md)
- [Cluster Mode](docs/cluster.md)
- [Testing Guide](docs/testing.md)
- [Framework Support](docs/frameworks.md)

## Flashing without ESP-IDF

For most operations, ESPBrew uses `espflash` for flashing, which means you don't need to install ESP-IDF unless you're building ESP-IDF projects.

**Supported chips**: ESP32, ESP32-S2, ESP32-S3, ESP32-C2, ESP32-C3, ESP32-C5, ESP32-C6, ESP32-H2, ESP32-P4

**Platforms**: macOS, Linux, Windows

## License

MIT
