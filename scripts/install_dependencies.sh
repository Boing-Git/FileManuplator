#!/usr/bin/env bash
set -e

# Installs what FileManipulator needs to build and run:
#   - GTK4 + Libadwaita development packages (the GUI)
#   - pkg-config and a C toolchain (for the Rust GTK bindings)
#   - the conversion engines the app routes work to
#
# Rust itself is not installed here — use rustup (https://rustup.rs) or your
# distribution's rust package.

echo "Detecting OS for installing dependencies..."

OS="$(uname -s)"
case "${OS}" in
    Linux*)
        if [ -f /etc/os-release ]; then
            . /etc/os-release
        elif [ -f /usr/lib/os-release ]; then
            . /usr/lib/os-release
        fi
        echo "Linux detected (${NAME:-${ID:-Unknown}})."

        if [[ "${ID}" =~ (ubuntu|debian|mint|pop|elementary) ]] || [[ "${ID_LIKE}" =~ (ubuntu|debian) ]] || command -v apt-get >/dev/null 2>&1; then
            echo "Using apt-get..."
            sudo apt-get update
            sudo apt-get install -y \
                build-essential pkg-config libgtk-4-dev libadwaita-1-dev libwebkitgtk-6.0-dev libssl-dev \
                flatpak flatpak-builder curl wget unzip file \
                ffmpeg imagemagick pandoc wkhtmltopdf mame-tools par2
        elif [[ "${ID}" =~ (arch|manjaro|endeavouros|garuda|artix) ]] || [[ "${ID_LIKE}" =~ arch ]] || command -v pacman >/dev/null 2>&1; then
            echo "Using pacman..."
            sudo pacman -Syu --needed --noconfirm \
                base-devel pkgconf gtk4 libadwaita webkitgtk-6.0 openssl \
                flatpak flatpak-builder curl wget unzip file \
                ffmpeg imagemagick pandoc wkhtmltopdf mame-tools par2cmdline
        elif [[ "${ID}" =~ (fedora|rhel|centos|rocky|almalinux) ]] || [[ "${ID_LIKE}" =~ (fedora|rhel) ]] || command -v dnf >/dev/null 2>&1; then
            echo "Using dnf..."
            sudo dnf install -y \
                gcc pkgconf gtk4-devel libadwaita-devel webkitgtk6.0-devel openssl-devel \
                flatpak flatpak-builder curl wget unzip file \
                ffmpeg ImageMagick pandoc wkhtmltopdf mame-tools par2cmdline
        elif [[ "${ID}" =~ (opensuse|suse) ]] || [[ "${ID_LIKE}" =~ suse ]] || command -v zypper >/dev/null 2>&1; then
            echo "Using zypper..."
            sudo zypper install -y \
                gcc pkg-config gtk4-devel libadwaita-devel webkitgtk-6_0-devel libopenssl-devel \
                curl wget unzip file \
                ffmpeg ImageMagick pandoc wkhtmltopdf mame-tools par2cmdline
        elif [[ "${ID}" == "nixos" ]]; then
            echo "NixOS detected. Everything is declared in shell.nix — just run:"
            echo "  nix-shell"
            exit 0
        else
            echo "Unsupported Linux distribution (${NAME:-${ID}})."
            echo "Please install manually: GTK4 + Libadwaita + WebKitGTK 6.0 development packages, pkg-config,"
            echo "a C compiler, OpenSSL, file, FFmpeg, ImageMagick, Pandoc, wkhtmltopdf,"
            echo "mame-tools (chdman) and par2."
            exit 1
        fi

        # Download portable copies of the conversion engines for bundling.
        SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
        if [ -f "$SCRIPT_DIR/download_binaries.sh" ]; then
            echo "Downloading portable binaries for Linux..."
            bash "$SCRIPT_DIR/download_binaries.sh"
        fi
        ;;

    Darwin*)
        echo "macOS detected."
        if command -v brew >/dev/null 2>&1; then
            echo "Using Homebrew..."
            yes | brew install pkg-config gtk4 libadwaita openssl \
                ffmpeg imagemagick pandoc rom-tools par2
            yes | brew install --cask wkhtmltopdf || true
        else
            echo "Homebrew is required on macOS. Please install it first: https://brew.sh/"
            exit 1
        fi
        ;;

    *)
        echo "Unsupported OS: ${OS}. FileManipulator currently targets Linux (GTK4)."
        exit 1
        ;;
esac

if ! command -v cargo >/dev/null 2>&1; then
    echo
    echo "Rust was not found. Install it with:"
    echo "  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
fi

echo "Dependencies installed."
