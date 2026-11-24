//! File type detection via the `file` command or extension fallback.
//!
//! Rust port of detector.py. Uses the `file --mime-type` command since
//! the python-magic bindings don't have a direct Rust equivalent that's
//! universal. Falls back to extension-based guessing.

use std::path::Path;
use std::process::Command;

/// Detect the MIME type of a file.
/// Uses `file --mime-type -b` (available on all Linux/macOS systems).
/// Falls back to extension guessing if `file` is unavailable.
pub fn detect_mime_type(path: &Path) -> String {
    // Try using the `file` command (libmagic under the hood)
    if let Ok(output) = Command::new("file")
        .args(["--mime-type", "-b"])
        .arg(path.as_os_str())
        .output()
        && output.status.success()
    {
        let mime = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !mime.is_empty() {
            return mime;
        }
    }

    // Fallback: guess from extension
    guess_from_extension(path)
}

fn guess_from_extension(path: &Path) -> String {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "bmp" => "image/bmp",
        "tiff" | "tif" => "image/tiff",
        "avif" => "image/avif",
        "svg" => "image/svg+xml",
        "mp4" | "m4v" => "video/mp4",
        "mkv" => "video/x-matroska",
        "webm" => "video/webm",
        "avi" => "video/x-msvideo",
        "mov" => "video/quicktime",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "flac" => "audio/flac",
        "ogg" => "audio/ogg",
        "aac" => "audio/aac",
        "m4a" => "audio/mp4",
        "md" | "markdown" | "mdown" | "mkd" => "text/markdown",
        "html" | "htm" => "text/html",
        "css" => "text/css",
        "js" | "mjs" => "application/javascript",
        "json" => "application/json",
        "xml" => "application/xml",
        "yaml" | "yml" => "application/x-yaml",
        "toml" => "application/toml",
        "csv" => "text/csv",
        "tsv" => "text/tab-separated-values",
        "txt" | "log" => "text/plain",
        "py" => "text/x-python",
        "sh" | "bash" | "zsh" => "text/x-shellscript",
        "c" | "h" => "text/x-c",
        "cpp" | "cxx" | "cc" | "hpp" => "text/x-c++",
        "java" => "text/x-java-source",
        "rs" => "text/x-rust",
        "rb" => "text/x-ruby",
        "php" => "text/x-php",
        "pl" | "pm" => "text/x-perl",
        "lua" => "text/x-lua",
        "go" => "text/x-go",
        "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        "rtf" => "application/rtf",
        "epub" => "application/epub+zip",
        "pdf" => "application/pdf",
        "odt" => "application/vnd.oasis.opendocument.text",
        _ => "application/octet-stream",
    }
    .to_string()
}
