#!/usr/bin/env bash
set -e

APP_NAME="any2any"
APP_EXEC="any2any"

# 1. Creates ~/.local/share/APP_NAME and ~/.local/bin/APP_NAME.
INSTALL_DIR="$HOME/.local/share/$APP_NAME"
BIN_DIR="$HOME/.local/bin"
DESKTOP_DIR="$HOME/.local/share/applications"

echo "Creating installation directories..."
mkdir -p "$INSTALL_DIR/bin"
mkdir -p "$BIN_DIR"
mkdir -p "$DESKTOP_DIR"

echo "Installing Any2Any to $INSTALL_DIR..."
# Assuming we run this from the project root
cp -r src "$INSTALL_DIR/"
# Note: if there is an icon or other assets, copy them here too.

# 3. Pre-pends a bundled bin/ directory to the PATH before launching.
echo "Creating launcher script in $BIN_DIR..."
cat << 'EOF' > "$BIN_DIR/$APP_EXEC"
#!/usr/bin/env bash
# Pre-pend the bundled bin/ directory to the PATH
export PATH="$HOME/.local/share/any2any/bin:$PATH"
exec python3 "$HOME/.local/share/any2any/src/main.py" "$@"
EOF

chmod +x "$BIN_DIR/$APP_EXEC"

# 2. Generates a .desktop file in ~/.local/share/applications for native GNOME integration.
echo "Creating desktop entry..."
cat << EOF > "$DESKTOP_DIR/$APP_NAME.desktop"
[Desktop Entry]
Name=Any2Any
Comment=Universal file converter
Exec=$BIN_DIR/$APP_EXEC
Icon=$INSTALL_DIR/src/icon.png
Type=Application
Terminal=false
Categories=Utility;
EOF

echo "Installation complete!"
echo "You can now run '$APP_EXEC' from your terminal or find Any2Any in your application menu."
