#!/usr/bin/env bash
# This script is designed to run inside the MSYS2 UCRT64 environment on Windows

set -e

# Ensure we are in an MSYS2 environment
if [[ -z "$MSYSTEM" ]]; then
    echo "Error: This script must be run inside MSYS2 (UCRT64) shell."
    exit 1
fi

APP_NAME="Any2Any"
MAIN_SCRIPT="src/main.py"

echo "Building Windows executable with PyInstaller..."
# Force PyInstaller to include GTK schemas and typelibs
# Use --onedir and --windowed flags (do NOT use --onefile as it causes slow GTK extraction times)
GI_REPO_PATH=$(cygpath -m /ucrt64/lib/girepository-1.0)

pyinstaller --noconfirm --onedir --windowed \
    --name "$APP_NAME" \
    --add-data "${GI_REPO_PATH};lib/girepository-1.0" \
    "$MAIN_SCRIPT"

echo "Copying libmagic-1.dll to prevent magic crashes on boot..."
# Explicitly copy libmagic-1.dll from the MSYS2 bin folder into the dist/ folder
cp /ucrt64/bin/libmagic-1.dll "dist/${APP_NAME}/"

echo "Windows packaging complete! Output is in dist/${APP_NAME}/"
