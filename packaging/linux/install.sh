#!/usr/bin/env bash
set -e

# Installs the FileManipulator binary, the bundled conversion engines and a
# desktop entry into the user's home directory. Run it from a release tarball,
# or from a source checkout after `cargo build --release`.

cd "$(dirname "$0")/../.."

APP_NAME="filemanuplator"
APP_EXEC="filemanuplator"

INSTALL_DIR="$HOME/.local/share/$APP_NAME"
BIN_DIR="$HOME/.local/bin"
DESKTOP_DIR="$HOME/.local/share/applications"

# The release tarball ships the binary at the top level; a source checkout has
# it under target/release.
if [ -f "fm-gtk" ]; then
    BINARY="fm-gtk"
elif [ -f "target/release/fm-gtk" ]; then
    BINARY="target/release/fm-gtk"
else
    echo "Could not find the fm-gtk binary."
    echo "Build it first with: cargo build --release"
    exit 1
fi

echo "Creating installation directories..."
mkdir -p "$INSTALL_DIR/bin" "$BIN_DIR" "$DESKTOP_DIR"

echo "Installing FileManipulator to $INSTALL_DIR..."
install -m 755 "$BINARY" "$INSTALL_DIR/bin/$APP_EXEC"

# Bundled ffmpeg/magick/pandoc, if this tarball carries them.
if [ -d "bin" ]; then
    cp -r bin/* "$INSTALL_DIR/bin/" 2>/dev/null || true
fi

# The tarball carries the icon at its root; a checkout has it under packaging/.
ICON_SRC=""
for candidate in icon.png packaging/linux/icon.png; do
    [ -f "$candidate" ] && ICON_SRC="$candidate" && break
done
if [ -n "$ICON_SRC" ]; then
    install -Dm644 "$ICON_SRC" "$INSTALL_DIR/icon.png"
    ICON_PATH="$INSTALL_DIR/icon.png"
else
    ICON_PATH="system-file-manager"
fi

# Launcher puts the bundled engines ahead of the system ones on PATH.
echo "Creating launcher script in $BIN_DIR..."
cat << EOF > "$BIN_DIR/$APP_EXEC"
#!/usr/bin/env bash
export PATH="$INSTALL_DIR/bin:\$PATH"
exec "$INSTALL_DIR/bin/$APP_EXEC" "\$@"
EOF
chmod +x "$BIN_DIR/$APP_EXEC"

echo "Creating desktop entry..."
cat << EOF > "$DESKTOP_DIR/$APP_NAME.desktop"
[Desktop Entry]
Name=FileManipulator
Comment=Universal file converter
Exec="$BIN_DIR/$APP_EXEC" %U
Icon=$ICON_PATH
Type=Application
Terminal=false
Categories=Utility;
EOF

if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database "$DESKTOP_DIR" || true
fi

# The binary uses the system's GTK4 and libadwaita; say exactly what is missing.
MISSING=$(ldd "$INSTALL_DIR/bin/$APP_EXEC" 2>/dev/null | awk '/not found/ {print $1}')
if [ -n "$MISSING" ]; then
    echo
    echo "Warning: these libraries are missing, so FileManipulator will not start yet:"
    echo "$MISSING" | sed 's/^/  /'
    echo "Install them with your package manager, e.g."
    echo "  Debian/Ubuntu: sudo apt install libgtk-4-1 libadwaita-1-0"
    echo "  Fedora:        sudo dnf install gtk4 libadwaita"
    echo "  Arch:          sudo pacman -S gtk4 libadwaita"
    echo "(or use the AppImage or Flatpak, which bring their own)."
    echo
fi
echo "For the HTML Runner's preview, also install WebKitGTK 6.0 (optional)."

echo "Installation complete!"
echo "Run '$APP_EXEC' from your terminal, or find FileManipulator in your application menu."
