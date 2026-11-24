#!/usr/bin/env bash
set -e

# Builds the GTK app in release mode and packs it into a tarball that
# install.sh can consume.

cd "$(dirname "$0")/../.."

APP_NAME="FileManuplator"
TAR_NAME="${APP_NAME}-Linux.tar.gz"
BUILD_DIR="${APP_NAME}-Linux"

echo "Building release binary..."
cargo build --release -p fm-gtk

echo "Scaffolding Linux tarball structure..."
rm -rf "$BUILD_DIR"
mkdir -p "$BUILD_DIR"

echo "Copying application resources..."
install -m 755 target/release/fm-gtk "$BUILD_DIR/fm-gtk"
if [ -d "bin" ]; then
    cp -r bin "$BUILD_DIR/"
fi
cp README.md "$BUILD_DIR/" 2>/dev/null || true
cp packaging/linux/icon.png "$BUILD_DIR/icon.png"

echo "Copying installer to root of tarball..."
cp packaging/linux/install.sh "$BUILD_DIR/"
# The installer now sits at the tarball root rather than packaging/linux/.
sed -i 's|cd "$(dirname "$0")/../.."|cd "$(dirname "$0")"|g' "$BUILD_DIR/install.sh"
chmod +x "$BUILD_DIR/install.sh"

echo "Creating tarball..."
tar -czf "$TAR_NAME" "$BUILD_DIR"
rm -rf "$BUILD_DIR"

echo "Linux packaging complete: $TAR_NAME"
