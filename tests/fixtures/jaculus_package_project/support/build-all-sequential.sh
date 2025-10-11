#!/bin/bash
# ESPBrew Generated Script - Build All Boards (Sequential)
# Generated: 2025-10-10 13:06:47
# Boards: 1

set -e  # Exit on any error

echo "🍺 ESPBrew Sequential Build - Building 1 board(s)"
echo "📁 Project: $(pwd)"
echo "📊 Strategy: Sequential (avoids component manager conflicts)"
echo

echo "🔨 Building jaculus-esp32 (1/1)"
./support/build-jaculus-esp32.sh

echo
echo "✅ All 1 boards built successfully!"
echo "🎉 Sequential build completed!"
