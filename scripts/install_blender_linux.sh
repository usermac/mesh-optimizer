#!/bin/bash

# Exit immediately if a command exits with a non-zero status
set -e

# Configuration
BLENDER_VERSION="4.1.0"
BLENDER_MAJOR="4.1"
INSTALL_DIR="/opt/blender"
TEMP_DIR="/tmp/blender_install"

echo "=== Blender Installation Helper ==="
echo "Target Version: $BLENDER_VERSION"

# Ensure we are running as root
if [ "$EUID" -ne 0 ]; then
  echo "Please run as root (sudo ./install_blender_linux.sh)"
  exit 1
fi

# 1. Install Dependencies
# These are commonly required for Blender on headless Linux servers
echo ">>> Installing system dependencies..."
apt-get update
apt-get install -y \
    wget \
    xz-utils \
    libxi6 \
    libxrender1 \
    libgl1 \
    libxkbcommon0 \
    libsm6 \
    libx11-6

# 2. Download Blender
echo ">>> Downloading Blender..."
mkdir -p $TEMP_DIR
cd $TEMP_DIR

# Construct URL (e.g., https://download.blender.org/release/Blender4.1/blender-4.1.0-linux-x64.tar.xz)
DOWNLOAD_URL="https://download.blender.org/release/Blender$BLENDER_MAJOR/blender-$BLENDER_VERSION-linux-x64.tar.xz"
wget -O blender.tar.xz "$DOWNLOAD_URL"

# 3. Extract Archive
echo ">>> Extracting..."
tar -xf blender.tar.xz

# 4. Install files
echo ">>> Installing to $INSTALL_DIR..."
# Clean previous install if exists
rm -rf $INSTALL_DIR
mkdir -p $INSTALL_DIR

# Find the extracted folder (name varies by exact version string)
EXTRACTED_FOLDER=$(find . -maxdepth 1 -type d -name "blender-*-linux-*" | head -n 1)

if [ -z "$EXTRACTED_FOLDER" ]; then
    echo "Error: Could not find extracted folder."
    exit 1
fi

mv "$EXTRACTED_FOLDER"/* "$INSTALL_DIR"/

# 5. Cleanup
echo ">>> Cleaning up..."
cd /
rm -rf $TEMP_DIR

# 6. Verify Installation
echo ">>> Verifying installation..."
if "$INSTALL_DIR/blender" --version; then
    echo ""
    echo "✅ Blender installed successfully!"
    echo "---------------------------------------------------"
    echo "Path: $INSTALL_DIR/blender"
    echo ""
    echo "ACTION REQUIRED:"
    echo "Add the following line to your .env file:"
    echo "BLENDER_PATH=$INSTALL_DIR/blender"
    echo "---------------------------------------------------"
else
    echo "❌ Installation verification failed."
    exit 1
fi
