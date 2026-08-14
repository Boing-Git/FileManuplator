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

echo "Building PKG installer..."
PKG_NAME="${APP_NAME}-installer.pkg"
PKG_BUILD_DIR="${APP_NAME}-pkg-build"

# Set up the build folders
mkdir -p "${PKG_BUILD_DIR}/payload"
mkdir -p "${PKG_BUILD_DIR}/scripts"

# Move app bundle into payload
cp -r "${APP_BUNDLE}" "${PKG_BUILD_DIR}/payload/"

# Copy postinstall script
cp packaging/macos/postinstall "${PKG_BUILD_DIR}/scripts/"
chmod +x "${PKG_BUILD_DIR}/scripts/postinstall"

echo "Creating package..."
pkgbuild --root "${PKG_BUILD_DIR}/payload" \
         --scripts "${PKG_BUILD_DIR}/scripts" \
         --install-location /Applications \
         --identifier dev.filemanuplator.Converter \
         --version 1.0.0 \
         "${PKG_NAME}"

rm -rf "${PKG_BUILD_DIR}"
rm -rf "${APP_BUNDLE}"

echo "macOS packaging complete: ${PKG_NAME}"
