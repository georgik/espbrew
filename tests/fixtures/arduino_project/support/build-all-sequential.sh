#!/bin/bash
# ESPBrew Generated Script - Build All Boards (Sequential)
# Generated: 2025-10-10 11:50:47
# Boards: 2

set -e  # Exit on any error

echo "🍺 ESPBrew Sequential Build - Building 2 board(s)"
echo "📁 Project: $(pwd)"
echo "📊 Strategy: Sequential (avoids component manager conflicts)"
echo

echo "🔨 Building arduino_project-esp32c6 (1/2)"
./support/build-arduino_project-esp32c6.sh
echo "🔨 Building arduino_project-esp32s3 (2/2)"
./support/build-arduino_project-esp32s3.sh

echo
echo "✅ All 2 boards built successfully!"
echo "🎉 Sequential build completed!"
