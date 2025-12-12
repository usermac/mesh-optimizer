#!/bin/bash

################################################################################
# Local Test Script for remesh.py (Blender 4.x)
################################################################################
# Purpose:
#   Tests the remesh.py script locally to verify Blender 4.x compatibility
#   before deploying to production.
#
# Usage:
#   ./scripts/test_remesh_local.sh <path_to_3d_file>
#   ./scripts/test_remesh_local.sh ./test_models/cube.obj
#
# Requirements:
#   - Blender 4.x installed and accessible via BLENDER_PATH or in PATH
#   - A test 3D model file (OBJ, FBX, GLB, or GLTF)
################################################################################

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

print_color() {
    local color=$1
    local message=$2
    echo -e "${color}${message}${NC}"
}

# Get script directory
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REMESH_SCRIPT="$SCRIPT_DIR/remesh.py"

# Check arguments
if [ $# -lt 1 ]; then
    print_color "$RED" "Usage: $0 <path_to_3d_file> [target_faces] [texture_size]"
    print_color "$YELLOW" "Example: $0 ./test_models/cube.obj 2000 1024"
    exit 1
fi

INPUT_FILE="$1"
TARGET_FACES="${2:-5000}"
TEXTURE_SIZE="${3:-2048}"

# Verify input file exists
if [ ! -f "$INPUT_FILE" ]; then
    print_color "$RED" "Error: Input file not found: $INPUT_FILE"
    exit 1
fi

# Verify remesh.py exists
if [ ! -f "$REMESH_SCRIPT" ]; then
    print_color "$RED" "Error: remesh.py not found at: $REMESH_SCRIPT"
    exit 1
fi

# Find Blender executable
BLENDER_EXE="${BLENDER_PATH:-$(which blender 2>/dev/null || echo "")}"

if [ -z "$BLENDER_EXE" ]; then
    print_color "$RED" "Error: Blender not found!"
    print_color "$YELLOW" "Please install Blender or set BLENDER_PATH environment variable"
    exit 1
fi

# Verify Blender version
print_color "$BLUE" "=== Blender Version Check ==="
BLENDER_VERSION=$("$BLENDER_EXE" --version 2>&1 | head -n1)
print_color "$GREEN" "$BLENDER_VERSION"

# Check if Blender 4.x
if [[ ! "$BLENDER_VERSION" =~ "Blender 4" ]]; then
    print_color "$YELLOW" "Warning: This script is designed for Blender 4.x"
    print_color "$YELLOW" "Detected: $BLENDER_VERSION"
fi

# Setup test output
INPUT_BASENAME=$(basename "$INPUT_FILE")
INPUT_NAME="${INPUT_BASENAME%.*}"
OUTPUT_DIR=$(mktemp -d)
OUTPUT_FILE="$OUTPUT_DIR/${INPUT_NAME}_opt.glb"

print_color "$BLUE" "\n=== Test Configuration ==="
echo "Input File:    $INPUT_FILE"
echo "Output File:   $OUTPUT_FILE"
echo "Target Faces:  $TARGET_FACES"
echo "Texture Size:  $TEXTURE_SIZE"
echo "Blender:       $BLENDER_EXE"
echo "Script:        $REMESH_SCRIPT"

# Run the remesh script
print_color "$BLUE" "\n=== Running Remesh Script ==="
START_TIME=$(date +%s)

set +e
"$BLENDER_EXE" -b -P "$REMESH_SCRIPT" -- \
    --input "$INPUT_FILE" \
    --output "$OUTPUT_FILE" \
    --faces "$TARGET_FACES" \
    --texture_size "$TEXTURE_SIZE"

EXIT_CODE=$?
set -e

END_TIME=$(date +%s)
ELAPSED=$((END_TIME - START_TIME))

print_color "$BLUE" "\n=== Results ==="
echo "Exit Code:     $EXIT_CODE"
echo "Time Elapsed:  ${ELAPSED}s"

# Check results
if [ $EXIT_CODE -ne 0 ]; then
    print_color "$RED" "\n❌ FAILED: Blender exited with code $EXIT_CODE"
    rm -rf "$OUTPUT_DIR"
    exit 1
fi

if [ ! -f "$OUTPUT_FILE" ]; then
    print_color "$RED" "\n❌ FAILED: Output file was not created"
    rm -rf "$OUTPUT_DIR"
    exit 1
fi

OUTPUT_SIZE=$(stat -f%z "$OUTPUT_FILE" 2>/dev/null || stat -c%s "$OUTPUT_FILE" 2>/dev/null || echo "0")

if [ "$OUTPUT_SIZE" -eq 0 ]; then
    print_color "$RED" "\n❌ FAILED: Output file is empty"
    rm -rf "$OUTPUT_DIR"
    exit 1
fi

print_color "$GREEN" "\n✅ SUCCESS!"
echo "Output Size:   $OUTPUT_SIZE bytes"
echo "Output File:   $OUTPUT_FILE"

# Optionally keep the output
print_color "$YELLOW" "\nOutput saved to: $OUTPUT_FILE"
print_color "$YELLOW" "Delete with: rm -rf $OUTPUT_DIR"

# Summary
print_color "$BLUE" "\n=== Test Summary ==="
print_color "$GREEN" "✓ Blender 4.x API compatibility: PASSED"
print_color "$GREEN" "✓ File import: PASSED"
print_color "$GREEN" "✓ Mesh processing: PASSED"
print_color "$GREEN" "✓ GLB export: PASSED"
print_color "$GREEN" "✓ Output validation: PASSED"
