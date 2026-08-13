#!/usr/bin/env bash

# Temporarily add Homebrew bin directory to PATH for Apple Silicon
export PATH="/opt/homebrew/bin:$PATH"

# Check for GTK4 and PyGObject via Python import test
if ! python3 -c "import gi; gi.require_version('Gtk', '4.0')" &> /dev/null; then
    osascript -e 'display dialog "Missing required dependencies: gtk4 or pygobject3. Please install them via Homebrew." buttons {"OK"} default button "OK" with title "Dependency Error" with icon stop'
    exit 1
fi

# Get the path to the app bundle resources folder
RESOURCES_PATH="$(dirname "$0")/../Resources"

# Launch the application
exec python3 "${RESOURCES_PATH}/src/main.py" "$@"
