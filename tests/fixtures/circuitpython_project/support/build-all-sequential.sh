#!/bin/bash
# ESPBrew Generated Script - Build All Boards (Sequential)
# Generated: 2025-10-10 12:51:24
# Boards: 1

set -e  # Exit on any error

echo "🍺 ESPBrew Sequential Build - Building 1 board(s)"
echo "📁 Project: $(pwd)"
echo "📊 Strategy: Sequential (avoids component manager conflicts)"
echo

echo "🔨 Building esp32s3 (1/1)"
./support/build-esp32s3.sh

echo
echo "✅ All 1 boards built successfully!"
echo "🎉 Sequential build completed!"
