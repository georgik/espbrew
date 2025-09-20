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
- **macOS** (Apple Silicon)
- **Linux** (x86_64)
- **Windows** (x86_64)

### Basic Usage

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
  - **Remove**: Delete component directory
  - **Open in Editor**: Open component in system editor
- **Board Actions**: Press Enter in Board List to access board actions:
  - **Build**: Build the project for the selected board
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

# Build all boards
espbrew --cli-only build

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
