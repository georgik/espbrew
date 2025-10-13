#!/bin/bash
# ESPBrew Generated Script - Build All Boards (Sequential)
# Generated: 2025-10-10 13:15:57
# Boards: 1

set -e  # Exit on any error

echo "🍺 ESPBrew Sequential Build - Building 1 board(s)"
echo "📁 Project: $(pwd)"
echo "📊 Strategy: Sequential (avoids component manager conflicts)"
echo

echo "🔨 Building esp32-core (1/1)"
./support/build-esp32-core.sh

echo
echo "✅ All 1 boards built successfully!"
echo "🎉 Sequential build completed!"
