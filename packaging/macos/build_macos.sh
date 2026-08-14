#!/usr/bin/env bash
set -e

APP_NAME="FileManuplator"
APP_BUNDLE="${APP_NAME}.app"
CONTENTS="${APP_BUNDLE}/Contents"
MACOS="${CONTENTS}/MacOS"
RESOURCES="${CONTENTS}/Resources"

echo "Scaffolding Apple .app bundle structure..."
mkdir -p "${MACOS}"
mkdir -p "${RESOURCES}"

echo "Copying Info.plist..."
cp packaging/macos/Info.plist "${CONTENTS}/"

echo "Copying launcher script..."
cp packaging/macos/launcher.sh "${MACOS}/${APP_NAME}"
chmod +x "${MACOS}/${APP_NAME}"

echo "Copying application resources..."
cp -r src "${RESOURCES}/"
cp -r scripts "${RESOURCES}/"

echo "Building DMG..."
DMG_NAME="${APP_NAME}.dmg"
VOL_NAME="${APP_NAME} Installer"

# Create a temporary folder to stage DMG contents
STAGE_DIR=$(mktemp -d)
cp -r "${APP_BUNDLE}" "${STAGE_DIR}/"
# Add a symlink to /Applications for easy drag-and-drop installation
ln -s /Applications "${STAGE_DIR}/Applications"

echo "Creating disk image..."
hdiutil create -volname "${VOL_NAME}" -srcfolder "${STAGE_DIR}" -ov -format UDZO "${DMG_NAME}"

rm -rf "${STAGE_DIR}"

echo "macOS packaging complete: ${DMG_NAME}"
