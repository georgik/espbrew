#!/bin/bash
# ESPBrew Generated Script - Clean All Builds
# Generated: 2025-10-10 13:15:57
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
echo "🧹 Cleaning esp32-core..."
rm -rf "/Users/georgik/projects/espbrew/tests/fixtures/nuttx_project"

# Also clean common directories
echo "🧹 Cleaning common build artifacts..."
rm -rf build/ managed_components/ dependencies.lock

echo
echo "✅ All build directories cleaned!"
echo "🎉 Clean all completed!"
