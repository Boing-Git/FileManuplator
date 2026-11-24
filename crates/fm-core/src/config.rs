//! Central routing table: MIME type → (tool, allowed output extensions).
//!
//! This is the Rust port of config.py — the only place that knows which
//! external binary handles which conversion.

use crate::types::{ConversionRoute, Tool};
use std::collections::HashMap;
use std::path::Path;
use std::sync::LazyLock;

/// Markdown file extensions that get parsed as real prose rather than raw text.
pub const MARKDOWN_EXTENSIONS: &[&str] = &[".md", ".markdown", ".mdown", ".mkd"];

/// Text-like application MIME types that should get the raw-text fallback.
const TEXT_FALLBACK_APP_TYPES: &[&str] = &[
    "application/json",
    "application/xml",
    "application/javascript",
    "application/x-yaml",
    "application/x-sh",
    "application/x-perl",
    "application/toml",
];

/// The full routing table: MIME type → ConversionRoute.
pub static ROUTES: LazyLock<HashMap<String, ConversionRoute>> = LazyLock::new(build_routes);

fn build_routes() -> HashMap<String, ConversionRoute> {
    let mut m = HashMap::new();

    // --- Images (ImageMagick & FFmpeg) ---
    m.insert(
        "image/png".into(),
        ConversionRoute::new(
            Tool::ImageMagick,
            &["magick", "convert"],
            &["jpg", "webp", "gif", "bmp", "tiff", "avif"],
        ),
    );
    m.insert(
        "image/jpeg".into(),
        ConversionRoute::new(
            Tool::ImageMagick,
            &["magick", "convert"],
            &["png", "webp", "gif", "bmp", "tiff", "avif"],
        ),
    );
    m.insert(
        "image/webp".into(),
        ConversionRoute::new(
            Tool::ImageMagick,
            &["magick", "convert"],
            &["png", "jpg", "gif", "bmp", "tiff"],
        ),
    );
    m.insert(
        "image/gif".into(),
        ConversionRoute::new(Tool::Ffmpeg, &["ffmpeg"], &["png", "jpg", "webp", "mp4"]),
    );
    m.insert(
        "image/bmp".into(),
        ConversionRoute::new(
            Tool::ImageMagick,
            &["magick", "convert"],
            &["png", "jpg", "webp"],
        ),
    );
    m.insert(
        "image/tiff".into(),
        ConversionRoute::new(
            Tool::ImageMagick,
            &["magick", "convert"],
            &["png", "jpg", "webp"],
        ),
    );

    // --- Video (ffmpeg) ---
    m.insert(
        "video/mp4".into(),
        ConversionRoute::new(
            Tool::Ffmpeg,
            &["ffmpeg"],
            &["mkv", "webm", "avi", "mov", "gif", "mp3"],
        ),
    );
    m.insert(
        "video/x-matroska".into(),
        ConversionRoute::new(
            Tool::Ffmpeg,
            &["ffmpeg"],
            &["mp4", "webm", "avi", "mov", "gif", "mp3"],
        ),
    );
    m.insert(
        "video/webm".into(),
        ConversionRoute::new(
            Tool::Ffmpeg,
            &["ffmpeg"],
            &["mp4", "mkv", "avi", "mov", "gif"],
        ),
    );
    m.insert(
        "video/x-msvideo".into(),
        ConversionRoute::new(
            Tool::Ffmpeg,
            &["ffmpeg"],
            &["mp4", "mkv", "webm", "mov", "gif"],
        ),
    );
    m.insert(
        "video/quicktime".into(),
        ConversionRoute::new(
            Tool::Ffmpeg,
            &["ffmpeg"],
            &["mp4", "mkv", "webm", "avi", "gif"],
        ),
    );

    // --- Audio (ffmpeg) ---
    m.insert(
        "audio/mpeg".into(),
        ConversionRoute::new(
            Tool::Ffmpeg,
            &["ffmpeg"],
            &["wav", "flac", "ogg", "aac", "m4a"],
        ),
    );
    m.insert(
        "audio/x-wav".into(),
        ConversionRoute::new(
            Tool::Ffmpeg,
            &["ffmpeg"],
            &["mp3", "flac", "ogg", "aac", "m4a"],
        ),
    );
    m.insert(
        "audio/wav".into(),
        ConversionRoute::new(
            Tool::Ffmpeg,
            &["ffmpeg"],
            &["mp3", "flac", "ogg", "aac", "m4a"],
        ),
    );
    m.insert(
        "audio/flac".into(),
        ConversionRoute::new(
            Tool::Ffmpeg,
            &["ffmpeg"],
            &["mp3", "wav", "ogg", "aac", "m4a"],
        ),
    );
    m.insert(
        "audio/ogg".into(),
        ConversionRoute::new(
            Tool::Ffmpeg,
            &["ffmpeg"],
            &["mp3", "wav", "flac", "aac", "m4a"],
        ),
    );

    // --- Real prose documents (pandoc parses normally) ---
    m.insert(
        "text/markdown".into(),
        ConversionRoute::pandoc(
            &["pandoc"],
            &["docx", "pdf", "html", "epub", "rst", "txt"],
            "markdown",
            false,
        ),
    );
    m.insert(
        "text/x-markdown".into(),
        ConversionRoute::pandoc(
            &["pandoc"],
            &["docx", "pdf", "html", "epub", "rst", "txt"],
            "markdown",
            false,
        ),
    );
    m.insert(
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document".into(),
        ConversionRoute::pandoc(
            &["pandoc"],
            &["md", "pdf", "html", "odt", "txt"],
            "docx",
            false,
        ),
    );
    m.insert(
        "text/html".into(),
        ConversionRoute::pandoc(&["pandoc"], &["md", "docx", "pdf", "txt"], "html", false),
    );
    m.insert(
        "application/rtf".into(),
        ConversionRoute::pandoc(&["pandoc"], &["md", "docx", "html", "txt"], "rtf", false),
    );
    m.insert(
        "application/epub+zip".into(),
        ConversionRoute::pandoc(&["pandoc"], &["md", "docx", "html", "txt"], "epub", false),
    );

    // --- Plain text, config, source code (structure preserved verbatim) ---
    let raw_text_types = [
        "text/plain",
        "text/x-python",
        "text/x-script.python",
        "text/x-shellscript",
        "text/x-c",
        "text/x-c++",
        "text/x-java-source",
        "text/x-php",
        "text/x-ruby",
        "text/x-perl",
        "text/x-lua",
        "application/javascript",
        "text/javascript",
        "application/json",
        "application/xml",
        "text/xml",
        "text/csv",
        "text/tab-separated-values",
        "application/x-yaml",
        "text/x-yaml",
        "text/x-diff",
        "text/x-log",
        "text/x-ini",
    ];
    for mime in &raw_text_types {
        m.insert(
            (*mime).to_string(),
            ConversionRoute::pandoc(
                &["pandoc"],
                &["md", "html", "pdf", "docx", "txt"],
                "markdown",
                true,
            ),
        );
    }

    m
}

/// The fallback route for any text/* or known text-like application/* type.
fn text_fallback_route() -> ConversionRoute {
    ConversionRoute::pandoc(
        &["pandoc"],
        &["md", "html", "pdf", "docx", "txt"],
        "markdown",
        true,
    )
}

/// Other names that versions of `file` give the same formats.
const MIME_ALIASES: &[(&str, &str)] = &[
    ("text/rtf", "application/rtf"),
    ("image/x-ms-bmp", "image/bmp"),
    ("audio/x-flac", "audio/flac"),
    ("audio/vnd.wave", "audio/wav"),
];

/// Look up the conversion route for a MIME type.
/// Falls back to generic raw-text handling for any text/* type not explicitly mapped.
pub fn get_route(mime_type: &str) -> Option<ConversionRoute> {
    let mime_type = MIME_ALIASES
        .iter()
        .find(|(alias, _)| *alias == mime_type)
        .map_or(mime_type, |(_, canonical)| canonical);
    if let Some(route) = ROUTES.get(mime_type) {
        return Some(route.clone());
    }
    if mime_type.starts_with("text/") || TEXT_FALLBACK_APP_TYPES.contains(&mime_type) {
        return Some(text_fallback_route());
    }
    None
}

/// Resolve the effective route for a file, handling the markdown extension tie-break.
/// Returns (route, treated_as_markdown).
pub fn resolve_route(mime_type: &str, filepath: &Path) -> (Option<ConversionRoute>, bool) {
    if mime_type == "text/plain"
        && let Some(ext) = filepath.extension().and_then(|e| e.to_str())
    {
        let dot_ext = format!(".{}", ext.to_lowercase());
        if MARKDOWN_EXTENSIONS.contains(&dot_ext.as_str()) {
            return (ROUTES.get("text/markdown").cloned(), true);
        }
    }
    (get_route(mime_type), false)
}
