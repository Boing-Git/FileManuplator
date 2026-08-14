#!/usr/bin/env bash
set -e

echo "Detecting OS for installing dependencies..."

OS="$(uname -s)"
case "${OS}" in
    Linux*)
        echo "Linux detected."
        if command -v apt-get >/dev/null 2>&1; then
            echo "Using apt-get..."
            sudo apt-get update
            sudo apt-get install -y \
                python3 python3-pip python3-gi python3-gi-cairo gir1.2-gtk-4.0 gir1.2-adw-1 \
                libgtk-4-dev libadwaita-1-dev pkg-config flatpak flatpak-builder curl wget unzip
        elif command -v pacman >/dev/null 2>&1; then
            echo "Using pacman..."
            sudo pacman -Syu --needed --noconfirm \
                python python-pip python-gobject gtk4 libadwaita pkgconf flatpak flatpak-builder curl wget unzip
        elif command -v dnf >/dev/null 2>&1; then
            echo "Using dnf..."
            sudo dnf install -y \
                python3 python3-pip python3-gobject gtk4-devel libadwaita-devel pkgconf flatpak flatpak-builder curl wget unzip
        else
            echo "Unsupported Linux package manager. Please install GTK4, Libadwaita, PyGObject, and Python3 manually."
            exit 1
        fi
        
        # Install Python dependencies
        echo "Installing Python packages..."
        pip3 install --break-system-packages typer rich pyyaml || pip3 install typer rich pyyaml

        # Download portable binaries for Linux
        echo "Downloading portable binaries for Linux..."
        SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
        if [ -f "$SCRIPT_DIR/download_binaries.sh" ]; then
            bash "$SCRIPT_DIR/download_binaries.sh"
        else
            echo "Warning: download_binaries.sh not found in $SCRIPT_DIR"
        fi
        ;;
        
    Darwin*)
        echo "macOS detected."
        if command -v brew >/dev/null 2>&1; then
            echo "Using Homebrew..."
            yes | brew install python gtk4 libadwaita pygobject3 ffmpeg imagemagick pandoc
            pip3 install --break-system-packages typer rich pyyaml || pip3 install typer rich pyyaml
        else
            echo "Homebrew is required on macOS. Please install it first: https://brew.sh/"
            exit 1
        fi
        ;;
        
    MINGW* | MSYS* | CYGWIN*)
        echo "Windows (MSYS2/MinGW) detected."
        if command -v pacman >/dev/null 2>&1; then
            echo "Using pacman..."
            pacman -Syu --needed --noconfirm \
                mingw-w64-ucrt-x86_64-python \
                mingw-w64-ucrt-x86_64-python-pip \
                mingw-w64-ucrt-x86_64-python-yaml \
                mingw-w64-ucrt-x86_64-gtk4 \
                mingw-w64-ucrt-x86_64-libadwaita \
                mingw-w64-ucrt-x86_64-python-gobject \
                mingw-w64-ucrt-x86_64-pyinstaller \
                mingw-w64-ucrt-x86_64-imagemagick \
                mingw-w64-ucrt-x86_64-ffmpeg \
                mingw-w64-ucrt-x86_64-file \
                unzip curl wget
            
            # Install Python dependencies
            echo "Installing Python packages..."
            # For MSYS2 UCRT64 Python
            if command -v /ucrt64/bin/python3 >/dev/null 2>&1; then
                /ucrt64/bin/python3 -m pip install --break-system-packages typer rich || /ucrt64/bin/python3 -m pip install typer rich
            else
                python3 -m pip install --break-system-packages typer rich || python3 -m pip install typer rich
            fi

            # Download Pandoc manually for Windows since it's not in UCRT64 repo
            echo "Downloading portable Pandoc for Windows..."
            PANDOC_VERSION="3.1.9"
            curl -L -o pandoc.zip "https://github.com/jgm/pandoc/releases/download/${PANDOC_VERSION}/pandoc-${PANDOC_VERSION}-windows-x86_64.zip"
            unzip -o pandoc.zip
            cp "pandoc-${PANDOC_VERSION}/pandoc.exe" /ucrt64/bin/
            rm -rf "pandoc-${PANDOC_VERSION}" pandoc.zip
        else
            echo "MSYS2 pacman not found. Ensure you are running this in the MSYS2 UCRT64 environment."
            exit 1
        fi
        ;;
        
    *)
        echo "Unsupported OS: ${OS}"
        exit 1
        ;;
esac

echo "Dependencies installed successfully!"
