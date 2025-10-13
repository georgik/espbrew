#!/bin/bash
# ESPBrew Generated Script - Flash All Boards
# Generated: 2025-10-10 13:16:21
# Boards: 1

set -e  # Exit on any error

echo "🍺 ESPBrew Flash All - Flashing 1 board(s)"
echo "📁 Project: $(pwd)"
echo "⚠️  Make sure only one board is connected at a time!"
echo

read -p "🔌 Connect the first board and press Enter to continue..."
echo

echo "🔥 Flashing esp32-s3-usb-otg (1/1)"
./support/flash-esp32-s3-usb-otg.sh

echo
echo "✅ All 1 boards flashed successfully!"
echo "🎉 Flash all completed!"
