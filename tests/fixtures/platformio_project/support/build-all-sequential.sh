#!/bin/bash
# ESPBrew Generated Script - Build All Boards (Sequential)
# Generated: 2025-10-10 12:45:05
# Boards: 4

set -e  # Exit on any error

echo "🍺 ESPBrew Sequential Build - Building 4 board(s)"
echo "📁 Project: $(pwd)"
echo "📊 Strategy: Sequential (avoids component manager conflicts)"
echo

echo "🔨 Building esp32c3 (1/4)"
./support/build-esp32c3.sh
echo "🔨 Building esp32c6 (2/4)"
./support/build-esp32c6.sh
echo "🔨 Building esp32s2 (3/4)"
./support/build-esp32s2.sh
echo "🔨 Building esp32s3 (4/4)"
./support/build-esp32s3.sh

echo
echo "✅ All 4 boards built successfully!"
echo "🎉 Sequential build completed!"
