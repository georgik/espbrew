#!/bin/bash
# ESPBrew Generated Script - Flash All Boards
# Generated: 2025-10-10 12:45:05
# Boards: 4

set -e  # Exit on any error

echo "🍺 ESPBrew Flash All - Flashing 4 board(s)"
echo "📁 Project: $(pwd)"
echo "⚠️  Make sure only one board is connected at a time!"
echo

read -p "🔌 Connect the first board and press Enter to continue..."
echo

echo "🔥 Flashing esp32c3 (1/4)"
./support/flash-esp32c3.sh
read -p "🔌 Connect the next board and press Enter..."
echo
echo "🔥 Flashing esp32c6 (2/4)"
./support/flash-esp32c6.sh
read -p "🔌 Connect the next board and press Enter..."
echo
echo "🔥 Flashing esp32s2 (3/4)"
./support/flash-esp32s2.sh
read -p "🔌 Connect the next board and press Enter..."
echo
echo "🔥 Flashing esp32s3 (4/4)"
./support/flash-esp32s3.sh

echo
echo "✅ All 4 boards flashed successfully!"
echo "🎉 Flash all completed!"
