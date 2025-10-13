#!/bin/bash
# ESPBrew Generated Script - Build All Boards (Sequential)
# Generated: 2025-10-10 13:16:21
# Boards: 1

set -e  # Exit on any error

echo "🍺 ESPBrew Sequential Build - Building 1 board(s)"
echo "📁 Project: $(pwd)"
echo "📊 Strategy: Sequential (avoids component manager conflicts)"
echo

echo "🔨 Building esp32-s3-usb-otg (1/1)"
./support/build-esp32-s3-usb-otg.sh

echo
echo "✅ All 1 boards built successfully!"
echo "🎉 Sequential build completed!"
