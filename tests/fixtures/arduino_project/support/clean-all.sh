#!/bin/bash
# ESPBrew Generated Script - Clean All Builds
# Generated: 2025-10-10 11:50:47
# Boards: 2

echo "🍺 ESPBrew Clean All - Cleaning 2 board(s)"
echo "📁 Project: $(pwd)"
echo "🗑️  This will remove all build directories"
echo

read -p "⚠️  Are you sure you want to clean all builds? (y/N): " confirm
if [[ $confirm != [yY] && $confirm != [yY][eE][sS] ]]; then
    echo "❌ Clean cancelled"
    exit 0
fi

echo "🧹 Cleaning all build directories..."
echo "🧹 Cleaning arduino_project-esp32c6..."
rm -rf "/Users/georgik/projects/espbrew/tests/fixtures/arduino_project/build"
echo "🧹 Cleaning arduino_project-esp32s3..."
rm -rf "/Users/georgik/projects/espbrew/tests/fixtures/arduino_project/build"

# Also clean common directories
echo "🧹 Cleaning common build artifacts..."
rm -rf build/ managed_components/ dependencies.lock

echo
echo "✅ All build directories cleaned!"
echo "🎉 Clean all completed!"
