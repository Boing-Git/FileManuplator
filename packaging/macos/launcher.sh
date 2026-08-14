#!/usr/bin/env bash

# Temporarily add Homebrew bin directory to PATH for Apple Silicon and Intel Macs
export PATH="/opt/homebrew/bin:/usr/local/bin:$PATH"

# Prioritize Homebrew's python3 if available, as system python lacks gi
if command -v python3 >/dev/null 2>&1; then
    PYTHON_CMD="python3"
else
    # Fallback if something is very wrong with the PATH
    PYTHON_CMD="/usr/bin/python3"
fi

# Ensure GTK typelibs can be found by PyGObject
export GI_TYPELIB_PATH="/opt/homebrew/lib/girepository-1.0:/usr/local/lib/girepository-1.0:${GI_TYPELIB_PATH:-}"

# Check for GTK4 and PyGObject via Python import test
if ! "$PYTHON_CMD" -c "import gi; gi.require_version('Gtk', '4.0')" &> /dev/null; then
    osascript -e 'display dialog "Missing required dependencies: gtk4 or pygobject3. Please install them via Homebrew." buttons {"OK"} default button "OK" with title "Dependency Error" with icon stop'
    exit 1
fi

# Get the path to the app bundle resources folder
RESOURCES_PATH="$(dirname "$0")/../Resources"

# Launch the application
exec "$PYTHON_CMD" "${RESOURCES_PATH}/src/main.py" "$@"
