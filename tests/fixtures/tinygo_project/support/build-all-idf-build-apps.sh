#!/bin/bash
# ESPBrew Generated Script - Professional Multi-Board Build
# Generated: 2025-10-10 13:47:47
# Project Type: TinyGo
# Boards: 1
# Tool: sequential builds

set -e  # Exit on any error

echo "🍺 ESPBrew Professional Build - TinyGo Project"
echo "📁 Project: $(pwd)"
echo "📊 Strategy: sequential builds (professional, optimized for TinyGo)"
echo "🎯 Boards: 1"
echo

echo "🔧 Using sequential builds for optimal build performance"
echo "📂 Config files:"
    /Users/georgik/projects/espbrew/tests/fixtures/tinygo_project/main.go
echo

# Build all configurations
# Using sequential builds for this project type
./support/build-all-sequential.sh

echo
echo "✅ All 1 boards built successfully!"
echo "🎉 Professional build completed with zero conflicts!"
echo "📦 Build artifacts available in build directories"
