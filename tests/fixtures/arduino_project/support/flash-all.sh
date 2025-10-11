#!/bin/bash
# ESPBrew Generated Script - Flash All Boards
# Generated: 2025-10-10 11:50:47
# Boards: 2

set -e  # Exit on any error

echo "🍺 ESPBrew Flash All - Flashing 2 board(s)"
echo "📁 Project: $(pwd)"
echo "⚠️  Make sure only one board is connected at a time!"
echo

read -p "🔌 Connect the first board and press Enter to continue..."
echo

echo "🔥 Flashing arduino_project-esp32c6 (1/2)"
./support/flash-arduino_project-esp32c6.sh
read -p "🔌 Connect the next board and press Enter..."
echo
echo "🔥 Flashing arduino_project-esp32s3 (2/2)"
./support/flash-arduino_project-esp32s3.sh

echo
echo "✅ All 2 boards flashed successfully!"
echo "🎉 Flash all completed!"
