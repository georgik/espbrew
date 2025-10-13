#!/bin/bash
# ESPBrew Generated Script - Build All Boards (Parallel)
# Generated: 2025-10-10 12:45:05
# Boards: 4

set -e  # Exit on any error

echo "🍺 ESPBrew Parallel Build - Building 4 board(s)"
echo "📁 Project: $(pwd)"
echo "📊 Strategy: Parallel (faster but may cause component manager conflicts)"
echo "⚠️  Warning: Parallel builds may interfere with ESP-IDF component manager"
echo

echo "🚀 Starting parallel builds..."
./support/build-esp32c3.sh &
./support/build-esp32c6.sh &
./support/build-esp32s2.sh &
./support/build-esp32s3.sh &

echo "⏳ Waiting for all builds to complete..."
wait

echo
echo "✅ All 4 boards built successfully!"
echo "🎉 Parallel build completed!"
