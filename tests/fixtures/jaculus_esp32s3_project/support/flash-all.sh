#!/bin/bash
# ESPBrew Generated Script - Flash All Boards
# Generated: 2025-10-10 13:06:35
# Boards: 1

set -e  # Exit on any error

echo "🍺 ESPBrew Flash All - Flashing 1 board(s)"
echo "📁 Project: $(pwd)"
echo "⚠️  Make sure only one board is connected at a time!"
echo

read -p "🔌 Connect the first board and press Enter to continue..."
echo

echo "🔥 Flashing jaculus-esp32s3 (1/1)"
./support/flash-jaculus-esp32s3.sh

echo
echo "✅ All 1 boards flashed successfully!"
echo "🎉 Flash all completed!"
