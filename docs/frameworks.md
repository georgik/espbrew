# Framework Support

ESPBrew supports 10 ESP32 development frameworks. Each framework has specific requirements and project structures.

## ESP-IDF (C/C++)

Traditional ESP32 development using Espressif's IoT Development Framework.

### Project Structure

```
my-esp-idf-project/
├── CMakeLists.txt
├── main/
├── components/
├── sdkconfig.defaults.esp32s3      # ESP32-S3 config
├── sdkconfig.defaults.esp32c6      # ESP32-C6 config
└── sdkconfig.defaults               # Base config
```

### Requirements

- ESP-IDF v5.0 or later
- CMake 3.16+
- Ninja build system

### Build Commands

```bash
espbrew --cli build
espbrew --cli flash --port /dev/ttyUSB0
```

## Rust no_std

Embedded Rust with esp-hal, Embassy, or embedded-hal.

### Project Structure

```
my-rust-project/
├── Cargo.toml
├── .cargo/config.toml               # Or config_*.toml
├── src/main.rs
└── target/xtensa-esp32s3-none-elf/   # Auto-detected chip
```

### Supported Frameworks

- esp-hal
- Embassy
- embedded-hal

### ELF-to-Binary Conversion

ESPBrew automatically converts Rust ELF binaries to ESP32 flash images using `espflash save-image` during flashing.

### Build Commands

```bash
espbrew --cli build
espbrew --cli flash --port /dev/ttyUSB0
```

## Arduino ESP32

Arduino framework for ESP32 using arduino-cli.

### Project Structure

```
my-arduino-project/
├── sketch.ino
├── boards.json                      # Multi-board config (optional)
└── build/
```

### Supported Boards

- ESP32 original
- ESP32-S2, S3
- ESP32-C3, C6, H2, P4
- M5Stack boards

### Requirements

- arduino-cli
- ESP32 Arduino core (auto-installed)

### Build Commands

```bash
espbrew --cli build
espbrew --cli flash --port /dev/ttyUSB0
```

## PlatformIO

Cross-platform development with multi-environment support.

### Project Structure

```
my-platformio-project/
├── platformio.ini                   # Multi-environment config
├── src/
├── lib/
└── [env:esp32s3]                    # Auto-detected environments
```

### Build System

- `pio run` for building
- `pio upload` for flashing
- `pio device monitor` for monitoring

### Requirements

- PlatformIO core (pio)

### Build Commands

```bash
espbrew --cli build
espbrew --cli flash --port /dev/ttyUSB0
```

## MicroPython

Python implementation for microcontrollers.

### Project Structure

```
my-micropython-project/
├── main.py                          # Entry point
├── boot.py                          # Boot configuration
├── lib/                             # Libraries
└── requirements.txt                 # Dependencies (optional)
```

### Tools

- `mpremote` (preferred)
- `ampy` (fallback)
- Screen monitoring

### Build Commands

```bash
# Upload Python files
espbrew --cli flash --port /dev/ttyUSB0

# Monitor output
espbrew --cli monitor --port /dev/ttyUSB0
```

## CircuitPython

Python for embedded systems by Adafruit.

### Project Structure

```
my-circuitpython-project/
├── code.py                          # Entry point
├── lib/                             # Libraries
└── requirements.txt                 # Dependencies
```

### Upload Methods

- Mass storage (CIRCUITPY drive)
- `circup` for dependency management
- `mpremote` for file operations
- `ampy` as fallback

### Build Commands

```bash
# Upload files
espbrew --cli flash --port /dev/ttyUSB0

# Monitor REPL
espbrew --cli monitor --port /dev/ttyUSB0
```

## Zephyr RTOS

Real-time operating system for embedded systems.

### Project Structure

```
my-zephyr-project/
├── prj.conf                         # Project configuration
├── CMakeLists.txt                   # Build configuration
├── src/
└── west.yml                         # West manifest
```

### Build System

- `west build` for building
- `west flash` for flashing
- Zephyr SDK

### Requirements

- West tool
- Zephyr SDK
- Python dependencies

### Build Commands

```bash
west build
west flash
```

## NuttX RTOS

POSIX-compliant real-time operating system.

### Project Structure

```
my-nuttx-project/
├── Makefile
├── nuttx/
├── apps/
└── .config
```

### Build System

- Make-based build system
- Config tool for configuration

### Build Commands

```bash
make
espbrew --cli flash --port /dev/ttyUSB0
```

## TinyGo

Go language for embedded systems.

### Project Structure

```
my-tinygo-project/
├── main.go
└── go.mod
```

### Build System

- `tinygo` compiler
- ESP32 support

### Requirements

- TinyGo compiler
- Go modules

### Build Commands

```bash
tinygo flash -target esp32-s3-monitor
```

## Jaculus

JavaScript/TypeScript runtime for ESP32.

### Project Structure

```
my-jaculus-project/
├── package.json
├── src/
└── tsconfig.json
```

### Requirements

- Jaculus CLI tools
- Node.js (for tooling)

### Build Commands

```bash
espbrew --cli build
espbrew --cli flash --port /dev/ttyUSB0
```

## Framework Support Matrix

| Framework | Build | Flash | Monitor | Multi-Board |
|-----------|-------|-------|---------|-------------|
| ESP-IDF    | Yes   | Yes   | Yes     | Yes         |
| Rust no_std| Yes   | Yes   | Yes     | Yes         |
| Arduino    | Yes   | Yes   | Yes     | Yes         |
| PlatformIO | Yes   | Yes   | Yes     | Yes         |
| MicroPython| N/A   | Upload | Yes     | N/A         |
| CircuitPython| N/A | Upload | Yes     | N/A         |
| Zephyr    | Yes   | Yes   | Yes     | Yes         |
| NuttX     | Yes   | Yes   | Yes     | Yes         |
| TinyGo    | Yes   | Yes   | Yes     | Yes         |
| Jaculus   | Yes   | Yes   | Yes     | Yes         |

## Adding Support for New Frameworks

To add support for a new framework:

1. Create handler in `src/projects/handlers/`
2. Implement `ProjectHandler` trait
3. Add detection logic
4. Register in `ProjectRegistry`
5. Add tests

See existing handlers for reference implementations.
