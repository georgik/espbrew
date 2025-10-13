#!/bin/bash
# ESPBrew Generated Script - Build All Boards (Parallel)
# Generated: 2025-10-10 11:50:47
# Boards: 2

set -e  # Exit on any error

echo "🍺 ESPBrew Parallel Build - Building 2 board(s)"
echo "📁 Project: $(pwd)"
echo "📊 Strategy: Parallel (faster but may cause component manager conflicts)"
echo "⚠️  Warning: Parallel builds may interfere with ESP-IDF component manager"
echo

echo "🚀 Starting parallel builds..."
./support/build-arduino_project-esp32c6.sh &
./support/build-arduino_project-esp32s3.sh &

echo "⏳ Waiting for all builds to complete..."
wait

echo
echo "✅ All 2 boards built successfully!"
echo "🎉 Parallel build completed!"
