"""Central routing table: MIME type -> (tool, allowed output extensions).

This is the only place that knows which external binary handles which
conversion. Adding support for a new format means adding an entry here —
nothing else in the codebase should need to change.
"""

from __future__ import annotations

from dataclasses import dataclass
from enum import Enum
from pathlib import Path


class Tool(str, Enum):
    """External binaries any2any can route work to."""

    IMAGEMAGICK = "imagemagick"
    FFMPEG = "ffmpeg"
    PANDOC = "pandoc"


@dataclass(frozen=True)
class ConversionRoute:
    """Describes how a given input MIME type can be converted."""

    tool: Tool
    # Binary name to invoke. ImageMagick 7 uses "magick"; ImageMagick 6 (still
    # common on many distros) uses "convert". We resolve the actual binary at
    # runtime in engine.py via shutil.which, trying each candidate in order.
    binary_candidates: tuple[str, ...]
    targets: tuple[str, ...]
    # PANDOC only: the pandoc reader name to use for this input (e.g.
    # "markdown", "html", "docx"). Ignored for other tools.
    pandoc_from: str = "markdown"
    # PANDOC only: True means the input is NOT actually markdown/html/docx
    # prose — it's plain text, source code, config, logs, etc. Pandoc's
    # default markdown reader reflows unindented lines as paragraphs and
    # reinterprets #, *, _, -, etc. as markdown syntax, which silently
    # mangles code structure. When raw_text is True, engine.py wraps the
    # original content in a fenced code block (which pandoc treats as
    # opaque, verbatim text) instead of letting pandoc parse it directly.
    raw_text: bool = False


# Pandoc writer name for each output extension any2any exposes. Passed
# explicitly via `-t` so the writer never depends on guessing from the
# output filename's extension.
PANDOC_WRITERS: dict[str, str] = {
    "md": "markdown",
    "markdown": "markdown",
    "txt": "plain",
    "html": "html",
    "pdf": "pdf",
    "docx": "docx",
    "odt": "odt",
    "epub": "epub3",
    "rst": "rst",
}

# MIME type -> ConversionRoute
ROUTES: dict[str, ConversionRoute] = {
    # --- Images (ImageMagick) ---
    "image/png": ConversionRoute(
        tool=Tool.IMAGEMAGICK,
        binary_candidates=("magick", "convert"),
        targets=("jpg", "webp", "gif", "bmp", "tiff", "avif"),
    ),
    "image/jpeg": ConversionRoute(
        tool=Tool.IMAGEMAGICK,
        binary_candidates=("magick", "convert"),
        targets=("png", "webp", "gif", "bmp", "tiff", "avif"),
    ),
    "image/webp": ConversionRoute(
        tool=Tool.IMAGEMAGICK,
        binary_candidates=("magick", "convert"),
        targets=("png", "jpg", "gif", "bmp", "tiff"),
    ),
    "image/gif": ConversionRoute(
        tool=Tool.IMAGEMAGICK,
        binary_candidates=("magick", "convert"),
        targets=("png", "jpg", "webp"),
    ),
    "image/bmp": ConversionRoute(
        tool=Tool.IMAGEMAGICK,
        binary_candidates=("magick", "convert"),
        targets=("png", "jpg", "webp"),
    ),
    "image/tiff": ConversionRoute(
        tool=Tool.IMAGEMAGICK,
        binary_candidates=("magick", "convert"),
        targets=("png", "jpg", "webp"),
    ),
    # --- Video (ffmpeg) ---
    "video/mp4": ConversionRoute(
        tool=Tool.FFMPEG,
        binary_candidates=("ffmpeg",),
        targets=("mkv", "webm", "avi", "mov", "gif", "mp3"),
    ),
    "video/x-matroska": ConversionRoute(
        tool=Tool.FFMPEG,
        binary_candidates=("ffmpeg",),
        targets=("mp4", "webm", "avi", "mov", "gif", "mp3"),
    ),
    "video/webm": ConversionRoute(
        tool=Tool.FFMPEG,
        binary_candidates=("ffmpeg",),
        targets=("mp4", "mkv", "avi", "mov", "gif"),
    ),
    "video/x-msvideo": ConversionRoute(
        tool=Tool.FFMPEG,
        binary_candidates=("ffmpeg",),
        targets=("mp4", "mkv", "webm", "mov", "gif"),
    ),
    "video/quicktime": ConversionRoute(
        tool=Tool.FFMPEG,
        binary_candidates=("ffmpeg",),
        targets=("mp4", "mkv", "webm", "avi", "gif"),
    ),
    # --- Audio (ffmpeg) ---
    "audio/mpeg": ConversionRoute(
        tool=Tool.FFMPEG,
        binary_candidates=("ffmpeg",),
        targets=("wav", "flac", "ogg", "aac", "m4a"),
    ),
    "audio/x-wav": ConversionRoute(
        tool=Tool.FFMPEG,
        binary_candidates=("ffmpeg",),
        targets=("mp3", "flac", "ogg", "aac", "m4a"),
    ),
    "audio/wav": ConversionRoute(
        tool=Tool.FFMPEG,
        binary_candidates=("ffmpeg",),
        targets=("mp3", "flac", "ogg", "aac", "m4a"),
    ),
    "audio/flac": ConversionRoute(
        tool=Tool.FFMPEG,
        binary_candidates=("ffmpeg",),
        targets=("mp3", "wav", "ogg", "aac", "m4a"),
    ),
    "audio/ogg": ConversionRoute(
        tool=Tool.FFMPEG,
        binary_candidates=("ffmpeg",),
        targets=("mp3", "wav", "flac", "aac", "m4a"),
    ),
    # --- Real prose documents (pandoc parses these normally) ---
    "text/markdown": ConversionRoute(
        tool=Tool.PANDOC,
        binary_candidates=("pandoc",),
        targets=("docx", "pdf", "html", "epub", "rst", "txt"),
        pandoc_from="markdown",
        raw_text=False,
    ),
    "text/x-markdown": ConversionRoute(
        tool=Tool.PANDOC,
        binary_candidates=("pandoc",),
        targets=("docx", "pdf", "html", "epub", "rst", "txt"),
        pandoc_from="markdown",
        raw_text=False,
    ),
    "application/vnd.openxmlformats-officedocument.wordprocessingml.document": ConversionRoute(
        tool=Tool.PANDOC,
        binary_candidates=("pandoc",),
        targets=("md", "pdf", "html", "odt", "txt"),
        pandoc_from="docx",
        raw_text=False,
    ),
    "text/html": ConversionRoute(
        tool=Tool.PANDOC,
        binary_candidates=("pandoc",),
        targets=("md", "docx", "pdf", "txt"),
        pandoc_from="html",
        raw_text=False,
    ),
    "application/rtf": ConversionRoute(
        tool=Tool.PANDOC,
        binary_candidates=("pandoc",),
        targets=("md", "docx", "html", "txt"),
        pandoc_from="rtf",
        raw_text=False,
    ),
    "application/epub+zip": ConversionRoute(
        tool=Tool.PANDOC,
        binary_candidates=("pandoc",),
        targets=("md", "docx", "html", "txt"),
        pandoc_from="epub",
        raw_text=False,
    ),
    # --- Plain text, config, and source code (structure preserved verbatim) ---
    # Everything below is content pandoc must NOT try to parse as markdown —
    # it gets wrapped in a fenced code block by engine.py before conversion,
    # so indentation, brackets, quotes, and symbols come through unchanged.
    "text/plain": ConversionRoute(
        tool=Tool.PANDOC,
        binary_candidates=("pandoc",),
        targets=("md", "html", "pdf", "docx", "txt"),
        raw_text=True,
    ),
    "text/x-python": ConversionRoute(
        tool=Tool.PANDOC,
        binary_candidates=("pandoc",),
        targets=("md", "html", "pdf", "docx", "txt"),
        raw_text=True,
    ),
    "text/x-script.python": ConversionRoute(
        tool=Tool.PANDOC,
        binary_candidates=("pandoc",),
        targets=("md", "html", "pdf", "docx", "txt"),
        raw_text=True,
    ),
    "text/x-shellscript": ConversionRoute(
        tool=Tool.PANDOC,
        binary_candidates=("pandoc",),
        targets=("md", "html", "pdf", "docx", "txt"),
        raw_text=True,
    ),
    "text/x-c": ConversionRoute(
        tool=Tool.PANDOC,
        binary_candidates=("pandoc",),
        targets=("md", "html", "pdf", "docx", "txt"),
        raw_text=True,
    ),
    "text/x-c++": ConversionRoute(
        tool=Tool.PANDOC,
        binary_candidates=("pandoc",),
        targets=("md", "html", "pdf", "docx", "txt"),
        raw_text=True,
    ),
    "text/x-java-source": ConversionRoute(
        tool=Tool.PANDOC,
        binary_candidates=("pandoc",),
        targets=("md", "html", "pdf", "docx", "txt"),
        raw_text=True,
    ),
    "text/x-php": ConversionRoute(
        tool=Tool.PANDOC,
        binary_candidates=("pandoc",),
        targets=("md", "html", "pdf", "docx", "txt"),
        raw_text=True,
    ),
    "text/x-ruby": ConversionRoute(
        tool=Tool.PANDOC,
        binary_candidates=("pandoc",),
        targets=("md", "html", "pdf", "docx", "txt"),
        raw_text=True,
    ),
    "text/x-perl": ConversionRoute(
        tool=Tool.PANDOC,
        binary_candidates=("pandoc",),
        targets=("md", "html", "pdf", "docx", "txt"),
        raw_text=True,
    ),
    "text/x-lua": ConversionRoute(
        tool=Tool.PANDOC,
        binary_candidates=("pandoc",),
        targets=("md", "html", "pdf", "docx", "txt"),
        raw_text=True,
    ),
    "application/javascript": ConversionRoute(
        tool=Tool.PANDOC,
        binary_candidates=("pandoc",),
        targets=("md", "html", "pdf", "docx", "txt"),
        raw_text=True,
    ),
    "text/javascript": ConversionRoute(
        tool=Tool.PANDOC,
        binary_candidates=("pandoc",),
        targets=("md", "html", "pdf", "docx", "txt"),
        raw_text=True,
    ),
    "application/json": ConversionRoute(
        tool=Tool.PANDOC,
        binary_candidates=("pandoc",),
        targets=("md", "html", "pdf", "docx", "txt"),
        raw_text=True,
    ),
    "application/xml": ConversionRoute(
        tool=Tool.PANDOC,
        binary_candidates=("pandoc",),
        targets=("md", "html", "pdf", "docx", "txt"),
        raw_text=True,
    ),
    "text/xml": ConversionRoute(
        tool=Tool.PANDOC,
        binary_candidates=("pandoc",),
        targets=("md", "html", "pdf", "docx", "txt"),
        raw_text=True,
    ),
    "text/csv": ConversionRoute(
        tool=Tool.PANDOC,
        binary_candidates=("pandoc",),
        targets=("md", "html", "pdf", "docx", "txt"),
        raw_text=True,
    ),
    "text/tab-separated-values": ConversionRoute(
        tool=Tool.PANDOC,
        binary_candidates=("pandoc",),
        targets=("md", "html", "pdf", "docx", "txt"),
        raw_text=True,
    ),
    "application/x-yaml": ConversionRoute(
        tool=Tool.PANDOC,
        binary_candidates=("pandoc",),
        targets=("md", "html", "pdf", "docx", "txt"),
        raw_text=True,
    ),
    "text/x-yaml": ConversionRoute(
        tool=Tool.PANDOC,
        binary_candidates=("pandoc",),
        targets=("md", "html", "pdf", "docx", "txt"),
        raw_text=True,
    ),
    "text/x-diff": ConversionRoute(
        tool=Tool.PANDOC,
        binary_candidates=("pandoc",),
        targets=("md", "html", "pdf", "docx", "txt"),
        raw_text=True,
    ),
    "text/x-log": ConversionRoute(
        tool=Tool.PANDOC,
        binary_candidates=("pandoc",),
        targets=("md", "html", "pdf", "docx", "txt"),
        raw_text=True,
    ),
    "text/x-ini": ConversionRoute(
        tool=Tool.PANDOC,
        binary_candidates=("pandoc",),
        targets=("md", "html", "pdf", "docx", "txt"),
        raw_text=True,
    ),
}

# Any MIME type not found in ROUTES gets checked against this: if it starts
# with "text/" or matches one of these exact application/* types, it's
# treated as plain/raw text (same handling as text/plain, see raw_text
# above) rather than rejected outright. This is what covers the long tail of
# "normal" everyday files — config files, source code in languages libmagic
# doesn't specifically fingerprint, logs, etc. — without hand-listing every
# possible MIME string.
_TEXT_FALLBACK_APPLICATION_TYPES = {
    "application/json",
    "application/xml",
    "application/javascript",
    "application/x-yaml",
    "application/x-sh",
    "application/x-perl",
    "application/toml",
}

_TEXT_FALLBACK_ROUTE = ConversionRoute(
    tool=Tool.PANDOC,
    binary_candidates=("pandoc",),
    targets=("md", "html", "pdf", "docx", "txt"),
    raw_text=True,
)


def get_route(mime_type: str) -> ConversionRoute | None:
    """Look up the conversion route for a MIME type, or None if unsupported.

    Falls back to generic raw-text handling for any text/* MIME type (or
    known text-like application/* type) that isn't explicitly mapped above,
    so ordinary files — scripts, configs, unusual source files — aren't
    rejected just because their exact MIME string isn't in the table.
    """
    route = ROUTES.get(mime_type)
    if route is not None:
        return route

    if mime_type.startswith("text/") or mime_type in _TEXT_FALLBACK_APPLICATION_TYPES:
        return _TEXT_FALLBACK_ROUTE

    return None


# libmagic has no content-only signature for markdown prose: a hand-written
# README and an arbitrary source file both come back as "text/plain" — the
# byte content genuinely doesn't distinguish them. Every other route in this
# table is chosen purely from content-sniffed MIME type, but for this one
# specific ambiguity we fall back to the extension, because it's the only
# signal that exists. This is the single deliberate exception to "detection
# ignores extensions."
MARKDOWN_EXTENSIONS = frozenset({".md", ".markdown", ".mdown", ".mkd"})


def resolve_route(mime_type: str, filepath: Path) -> tuple[ConversionRoute | None, bool]:
    """Resolve the effective route for a file.

    Returns (route, treated_as_markdown). treated_as_markdown is True only
    when content-detection returned the ambiguous "text/plain" and the
    filename has a markdown extension — in which case the file is parsed as
    real markdown prose (headers, emphasis, lists) rather than preserved
    verbatim as raw text.
    """
    if mime_type == "text/plain" and filepath.suffix.lower() in MARKDOWN_EXTENSIONS:
        return ROUTES["text/markdown"], True
    return get_route(mime_type), False
