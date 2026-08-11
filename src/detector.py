"""File type detection using libmagic (content-based, not extension-based)."""

from __future__ import annotations

from pathlib import Path

import magic


def detect_mime_type(filepath: Path) -> str:
    """Return the MIME type of a file by inspecting its content (magic bytes).

    Raises:
        FileNotFoundError: if the path does not exist or is not a regular file.
    """
    if not filepath.is_file():
        raise FileNotFoundError(f"No such file: {filepath}")

    mime = magic.Magic(mime=True)
    return mime.from_file(str(filepath))
