#!/bin/bash
# ESPBrew Generated Script - Clean All Builds
# Generated: 2025-10-10 12:51:24
# Boards: 1

echo "🍺 ESPBrew Clean All - Cleaning 1 board(s)"
echo "📁 Project: $(pwd)"
echo "🗑️  This will remove all build directories"
echo

read -p "⚠️  Are you sure you want to clean all builds? (y/N): " confirm
if [[ $confirm != [yY] && $confirm != [yY][eE][sS] ]]; then
    echo "❌ Clean cancelled"
    exit 0
fi

echo "🧹 Cleaning all build directories..."
echo "🧹 Cleaning esp32s3..."
rm -rf "/Users/georgik/projects/espbrew/tests/fixtures/circuitpython_project"

# Also clean common directories
echo "🧹 Cleaning common build artifacts..."
rm -rf build/ managed_components/ dependencies.lock

echo
echo "✅ All build directories cleaned!"
echo "🎉 Clean all completed!"
