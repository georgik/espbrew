#!/bin/bash
# ESPBrew Generated Script - Flash All Boards
# Generated: 2025-10-10 13:15:57
# Boards: 1

set -e  # Exit on any error

echo "🍺 ESPBrew Flash All - Flashing 1 board(s)"
echo "📁 Project: $(pwd)"
echo "⚠️  Make sure only one board is connected at a time!"
echo

read -p "🔌 Connect the first board and press Enter to continue..."
echo

echo "🔥 Flashing esp32-core (1/1)"
./support/flash-esp32-core.sh

echo
echo "✅ All 1 boards flashed successfully!"
echo "🎉 Flash all completed!"
