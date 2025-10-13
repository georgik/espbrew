#!/bin/bash
# ESPBrew Generated Script - Build All Boards (Parallel)
# Generated: 2025-10-10 13:47:47
# Boards: 1

set -e  # Exit on any error

echo "🍺 ESPBrew Parallel Build - Building 1 board(s)"
echo "📁 Project: $(pwd)"
echo "📊 Strategy: Parallel (faster but may cause component manager conflicts)"
echo "⚠️  Warning: Parallel builds may interfere with ESP-IDF component manager"
echo

echo "🚀 Starting parallel builds..."
./support/build-esp32-coreboard-v2.sh &

echo "⏳ Waiting for all builds to complete..."
wait

echo
echo "✅ All 1 boards built successfully!"
echo "🎉 Parallel build completed!"
