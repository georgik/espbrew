# 🍺 ESPBrew - ESP32 Multi-Board Build Manager

A TUI (Terminal User Interface) and CLI tool for managing ESP-IDF builds across multiple board configurations. It automatically discovers board configurations, generates build scripts, and provides real-time build monitoring.

![ESP32 Multi-Board Support](https://img.shields.io/badge/ESP32-Multi--Board-blue)
![Rust](https://img.shields.io/badge/rust-1.70+-orange.svg)
![License](https://img.shields.io/badge/license-MIT-green.svg)

## ✨ Features

### Multi-Board Management
- Auto-Discovery: Automatically finds all `sdkconfig.defaults.*` configurations
- Board-Specific Builds: Each board gets its own build directory (`build.{board_name}`)
- Target Detection: Automatically detects ESP32 target (S3, P4, C6, C3) from config files
- Parallel Builds: Builds all boards simultaneously

### Dual Interface
- Interactive TUI: Terminal interface with real-time monitoring (builds start when you press 'b')
- CLI Mode: Headless builds for CI/CD pipelines (builds start immediately)
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

```bash
# Clone and build ESPBrew
git clone <repo> && cd espbrew
cargo build --release

# Or install directly (when published)
cargo install espbrew
```

### Basic Usage

```bash
# Interactive TUI mode (default) - uses current directory
# TUI does NOT auto-start builds. Press 'b' to start building all boards.
espbrew

# Interactive TUI mode with specific directory
espbrew /path/to/your/esp-idf-project

# CLI-only mode - uses current directory
# CLI auto-starts builds immediately for all detected boards
espbrew --cli-only

# CLI-only mode with specific directory
espbrew --cli-only /path/to/your/esp-idf-project

# Help and options
espbrew --help
```

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
- **↑/↓ or j/k**: Navigate between boards
- **Enter**: Flash selected board (if build successful)
- **b**: Build all boards (TUI does not auto-start builds)
- **r**: Refresh board list
- **h or ?**: Toggle help
- **q**: Quit

### Interface Layout

```
┌─ 🍺 ESP Boards ────────────┬─ Board Details ─────────────────┐
│ ⏳ esp32_s3_box_3         │ Board: esp32_s3_box_3           │
│ ⚙️  m5_atom_s3             │ Status: ⚙️  Building            │
│ ✅ esp32_p4_function_ev    │ Config: sdkconfig.defaults.m5*  │
│ ❌ m5stack_tab5            │ Build Dir: build.m5_atom_s3     │
└───────────────────────────┼─ Build Log ─────────────────────┤
                            │ [CMake] Configuring done        │
                            │ [CMake] Generating done          │
                            │ [Build] Building ESP-IDF app     │
                            │ [Build] Compiling main.c         │
                            └─────────────────────────────────┘
```

## 🛠️ CLI Mode

Perfect for CI/CD pipelines and automated builds:

```bash
# Run all builds without interaction (current directory)
espbrew --cli-only

# Run all builds for specific project
espbrew --cli-only ./my-project

# Example output:
🍺 ESPBrew CLI Mode - Building all boards...
Found 4 boards:
  - esp32_s3_box_3 (sdkconfig.defaults.esp32_s3_box_3)
  - m5_atom_s3 (sdkconfig.defaults.m5_atom_s3)
  - esp32_p4_function_ev (sdkconfig.defaults.esp32_p4_function_ev)
  - m5stack_tab5 (sdkconfig.defaults.m5stack_tab5)

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

### Build Scripts (`./support/build_*.sh`)
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
SDKCONFIG_DEFAULTS="sdkconfig.defaults.esp32_s3_box_3" idf.py -B "build.esp32_s3_box_3" set-target $TARGET
SDKCONFIG_DEFAULTS="sdkconfig.defaults.esp32_s3_box_3" idf.py -B "build.esp32_s3_box_3" build

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

## 🎯 Supported Board Patterns

ESPBrew automatically detects ESP32 targets from config files:

| Pattern in Config | Detected Target | Example Board |
|-------------------|----------------|---------------|
| `esp32p4` or `CONFIG_IDF_TARGET="esp32p4"` | `esp32p4` | M5Stack Tab5, ESP32-P4-Function-EV |
| `esp32c6` or `CONFIG_IDF_TARGET="esp32c6"` | `esp32c6` | ESP32-C6-DevKit |
| `esp32c3` or `CONFIG_IDF_TARGET="esp32c3"` | `esp32c3` | ESP32-C3-LCDKit |
| Default | `esp32s3` | M5 Atom S3, ESP32-S3-BOX-3, M5Stack CoreS3 |

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

## 📊 Project Structure

ESPBrew creates the following structure:

```
your-project/
├── sdkconfig.defaults.*          # Your board configs
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
- **Enhanced TUI**: More interactive features and better error handling
- **Performance**: Optimize build parallelization
- **Integration**: More IDE and CI/CD integrations

## 📄 License

MIT License - see LICENSE file for details.

## 🙏 Credits

- **Ratatui**: Beautiful terminal user interfaces
- **Tokio**: Async runtime for concurrent builds  
- **ESP-IDF**: Espressif IoT Development Framework
- **Clap**: Command line argument parsing

---

🍺 ESPBrew - Simplifying ESP32 multi-board development
