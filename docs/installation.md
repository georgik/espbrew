# Installation Guide

## Installation Methods

### Install from Releases (Recommended)

The quickest way to install ESPBrew is using the installation script:

```bash
curl -L https://georgik.github.io/espbrew/install.sh | bash
```

This will:
- Download the latest release for your platform
- Install to `~/.espbrew/bin/`
- Add to PATH if needed

### Homebrew (macOS)

```bash
brew tap georgik/espbrew
brew install espbrew
```

### Build from Source

If you prefer to build from source or need the latest development version:

```bash
# Clone repository
git clone https://github.com/georgik/espbrew.git
cd espbrew

# Build release binary
cargo build --release

# The binary will be at target/release/espbrew
```

### System Requirements

**Required:**
- Rust 1.70 or later (for building from source)
- Cargo package manager

**Optional but Recommended:**
- `espflash` - For flashing operations (auto-downloaded if missing)
- `idf.py` - Only required for ESP-IDF projects
- `arduino-cli` - Only required for Arduino projects
- `pio` - Only required for PlatformIO projects
- `mpremote` - For MicroPython (preferred)
- `ampy` - Fallback for MicroPython
- `circup` - For CircuitPython dependency management

## Platform-Specific Notes

### macOS

No additional requirements. ESPBrew works out of the box.

### Linux

You may need to add your user to the `dialout` group to access USB serial ports:

```bash
sudo usermod -a -G dialout $USER
```

Log out and back in for changes to take effect.

### Windows

Install USB serial drivers:
- CH340/CH341 drivers for common ESP32 boards
- CP210x drivers for boards using Silicon Labs chips
- FTDI drivers for boards using FTDI chips

## Upgrading

### Installed from Releases

```bash
curl -L https://georgik.github.io/espbrew/install.sh | bash
```

### Homebrew

```bash
brew upgrade espbrew
```

### Built from Source

```bash
cd espbrew
git pull
cargo build --release
```

## Uninstallation

### Installed from Releases

```bash
rm -rf ~/.espbrew
```

Remove from PATH by editing your shell configuration file.

### Homebrew

```bash
brew uninstall espbrew
brew untap georgik/espbrew
```

## Verifying Installation

Run the following command to verify ESPBrew is installed correctly:

```bash
espbrew --version
```

You should see version information printed.

To check connected boards:

```bash
espbrew boards
```

This will list all connected USB serial devices that may be ESP32 boards.
