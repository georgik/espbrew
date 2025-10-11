#!/bin/bash
# ESPBrew Generated Script - Professional Multi-Board Build
# Generated: 2025-10-10 13:06:35
# Boards: 1
# Tool: idf-build-apps (ESP-IDF professional build tool)

set -e  # Exit on any error

echo "🍺 ESPBrew Professional Build - Using idf-build-apps"
echo "📁 Project: $(pwd)"
echo "📊 Strategy: idf-build-apps (professional, zero conflicts)"
echo "🎯 Boards: 1"
echo

# Check if idf-build-apps is installed
if ! command -v idf-build-apps &> /dev/null; then
    echo "❌ idf-build-apps not found!"
    echo "💡 Install with: pip install idf-build-apps"
    echo "📖 More info: https://github.com/espressif/idf-build-apps"
    exit 1
fi

echo "🎆 Using professional idf-build-apps for optimal build performance"
echo "📂 Config files:"
    /Users/georgik/projects/espbrew/tests/fixtures/jaculus_esp32s3_project/jaculus.json
echo

# Build all configurations
echo "🔨 Building all boards..."
idf-build-apps find \\
    --build-dir ./build \\
    --config-file sdkconfig.defaults.* \\
    --target "*" \\
    --recursive

idf-build-apps build \\
    --build-dir ./build \\
    --config-file sdkconfig.defaults.* \\
    --target "*" \\
    --parallel-count $(nproc) \\
    --parallel-index 1

echo
echo "✅ All 1 boards built successfully!"
echo "🎉 Professional build completed with zero conflicts!"
echo "📦 Build artifacts available in ./build/"
