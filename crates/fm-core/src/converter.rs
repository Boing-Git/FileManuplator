//! Builds and executes the external-tool commands that do the actual conversion.
//!
//! Rust port of engine.py. filemanuplator never implements conversion logic
//! itself — this module's only job is to turn a (tool, input, output) triple
//! into an argv list and run it.

use crate::types::{ConversionResult, ConversionRoute, Tool, pandoc_writer};
use std::fs;
use std::path::Path;
use std::process::Command;

/// Find the first available binary from a route's candidates on PATH.
pub fn resolve_binary(route: &ConversionRoute) -> Result<String, String> {
    for candidate in &route.binary_candidates {
        if which(candidate) {
            return Ok(candidate.clone());
        }
    }
    Err(format!(
        "None of the required binaries ({}) were found on PATH. \
         Please install the corresponding system package.",
        route.binary_candidates.join(", ")
    ))
}

/// Check if a binary exists on PATH.
fn which(name: &str) -> bool {
    crate::exec::program_exists(name)
}

/// Read file as text, tolerating non-UTF-8.
fn read_text(path: &Path) -> Result<String, String> {
    let bytes =
        fs::read(path).map_err(|e| format!("Could not read '{}': {}", path.display(), e))?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

/// Pick a backtick fence longer than any backtick run in the text.
fn fence_for(text: &str) -> String {
    let mut longest = 0usize;
    let mut current = 0usize;
    for ch in text.chars() {
        if ch == '`' {
            current += 1;
            if current > longest {
                longest = current;
            }
        } else {
            current = 0;
        }
    }
    "`".repeat(longest.max(2) + 1)
}

/// Wrap raw text in a markdown fenced code block.
fn wrap_as_code_fence(text: &str, lang_hint: &str) -> String {
    let fence = fence_for(text);
    let body = if text.ends_with('\n') {
        text.to_string()
    } else {
        format!("{}\n", text)
    };
    format!("{}{}\n{}{}\n", fence, lang_hint, body, fence)
}

/// Build the argv list for ImageMagick or FFmpeg.
fn build_command(tool: Tool, binary: &str, input: &Path, output: &Path) -> Vec<String> {
    match tool {
        Tool::ImageMagick => vec![
            binary.to_string(),
            "-limit".to_string(),
            "memory".to_string(),
            "1GiB".to_string(),
            "-limit".to_string(),
            "map".to_string(),
            "2GiB".to_string(),
            input.to_string_lossy().into_owned(),
            output.to_string_lossy().into_owned(),
        ],
        Tool::Ffmpeg => vec![
            binary.to_string(),
            "-y".to_string(),
            "-i".to_string(),
            input.to_string_lossy().into_owned(),
            output.to_string_lossy().into_owned(),
        ],
        Tool::Pandoc => unreachable!("Pandoc uses _run_pandoc, not build_command"),
    }
}

fn run_subprocess(
    command: &[String],
    output_path: &Path,
    env_updates: &[(&str, String)],
) -> ConversionResult {
    let mut cmd = Command::new(&command[0]);
    cmd.args(&command[1..]);

    for (k, v) in env_updates {
        cmd.env(k, v);
    }

    match cmd.output() {
        Ok(output) => ConversionResult {
            success: output.status.success(),
            output_path: output_path.to_string_lossy().into_owned(),
            command: command.to_vec(),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            returncode: output.status.code().unwrap_or(-1),
        },
        Err(e) => ConversionResult {
            success: false,
            output_path: output_path.to_string_lossy().into_owned(),
            command: command.to_vec(),
            stdout: String::new(),
            stderr: format!("Failed to execute: {}", e),
            returncode: -1,
        },
    }
}

/// PDF engine preference order.
const PDF_ENGINE_CANDIDATES: &[&str] = &[
    "wkhtmltopdf",
    "weasyprint",
    "tectonic",
    "xelatex",
    "lualatex",
    "pdflatex",
    "context",
];

/// Find the first available pandoc PDF engine on PATH.
pub fn resolve_pdf_engine() -> Option<String> {
    for candidate in PDF_ENGINE_CANDIDATES {
        if which(candidate) {
            return Some(candidate.to_string());
        }
    }
    None
}

fn run_pandoc(binary: &str, pandoc_from: &str, input: &Path, output: &Path) -> ConversionResult {
    let target_ext = output
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    let writer = match pandoc_writer(&target_ext) {
        Some(w) => w,
        None => {
            return ConversionResult {
                success: false,
                output_path: output.to_string_lossy().into_owned(),
                command: vec![binary.to_string()],
                stdout: String::new(),
                stderr: format!(
                    "No pandoc writer configured for target extension '.{}'.",
                    target_ext
                ),
                returncode: -1,
            };
        }
    };

    let mut command = vec![
        binary.to_string(),
        input.to_string_lossy().into_owned(),
        "-f".to_string(),
        pandoc_from.to_string(),
        "-t".to_string(),
        writer.to_string(),
        "-o".to_string(),
        output.to_string_lossy().into_owned(),
    ];

    // Full documents need --standalone
    if ["html", "docx", "odt", "epub3", "pdf"].contains(&writer) {
        command.push("--standalone".to_string());
        command.push(format!(
            "--metadata=title:{}",
            input.file_stem().unwrap_or_default().to_string_lossy()
        ));
    }

    if writer == "pdf" {
        match resolve_pdf_engine() {
            Some(engine) => {
                command.push(format!("--pdf-engine={}", engine));
            }
            None => {
                return ConversionResult {
                    success: false,
                    output_path: output.to_string_lossy().into_owned(),
                    command,
                    stdout: String::new(),
                    stderr: "No PDF engine found. Install one of: wkhtmltopdf, weasyprint, \
                             tectonic, or a LaTeX distribution."
                        .to_string(),
                    returncode: -1,
                };
            }
        }
    }

    run_subprocess(&command, output, &[])
}

/// Run the actual conversion. This is the main entry point.
pub fn run_conversion(
    route: &ConversionRoute,
    input_path: &Path,
    output_path: &Path,
) -> ConversionResult {
    let binary = match resolve_binary(route) {
        Ok(b) => b,
        Err(msg) => {
            return ConversionResult {
                success: false,
                output_path: output_path.to_string_lossy().into_owned(),
                command: vec![],
                stdout: String::new(),
                stderr: msg,
                returncode: -1,
            };
        }
    };

    match route.tool {
        Tool::ImageMagick | Tool::Ffmpeg => {
            let command = build_command(route.tool, &binary, input_path, output_path);

            // ImageMagick spills large intermediates to disk; keep them beside
            // the output rather than filling /tmp.
            let env_updates: Vec<(&str, String)> = if route.tool == Tool::ImageMagick {
                let cache_dir = output_path
                    .parent()
                    .unwrap_or(Path::new("/tmp"))
                    .to_string_lossy()
                    .into_owned();
                vec![
                    ("MAGICK_TEMPORARY_PATH", cache_dir.clone()),
                    ("MAGICK_TMPDIR", cache_dir.clone()),
                    ("TMPDIR", cache_dir),
                ]
            } else {
                Vec::new()
            };

            run_subprocess(&command, output_path, &env_updates)
        }
        Tool::Pandoc => {
            let target_ext = output_path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_lowercase();

            if !route.raw_text {
                // Genuine prose — let pandoc parse normally
                return run_pandoc(&binary, &route.pandoc_from, input_path, output_path);
            }

            // raw_text: preserve structure by wrapping in code fence
            let original_text = match read_text(input_path) {
                Ok(t) => t,
                Err(msg) => {
                    return ConversionResult {
                        success: false,
                        output_path: output_path.to_string_lossy().into_owned(),
                        command: vec![],
                        stdout: String::new(),
                        stderr: msg,
                        returncode: -1,
                    };
                }
            };

            // txt target: just copy verbatim
            if target_ext == "txt" {
                return match fs::copy(input_path, output_path) {
                    Ok(_) => ConversionResult {
                        success: true,
                        output_path: output_path.to_string_lossy().into_owned(),
                        command: vec!["<copy>".into(), input_path.to_string_lossy().into_owned()],
                        stdout: String::new(),
                        stderr: String::new(),
                        returncode: 0,
                    },
                    Err(e) => ConversionResult {
                        success: false,
                        output_path: output_path.to_string_lossy().into_owned(),
                        command: vec![],
                        stdout: String::new(),
                        stderr: format!("Could not copy: {}", e),
                        returncode: -1,
                    },
                };
            }

            let lang_hint = input_path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_lowercase();
            let wrapped = wrap_as_code_fence(&original_text, &lang_hint);

            // md target: write wrapped fence directly
            if target_ext == "md" || target_ext == "markdown" {
                return match fs::write(output_path, &wrapped) {
                    Ok(()) => ConversionResult {
                        success: true,
                        output_path: output_path.to_string_lossy().into_owned(),
                        command: vec!["<wrap-as-code-fence>".into()],
                        stdout: String::new(),
                        stderr: String::new(),
                        returncode: 0,
                    },
                    Err(e) => ConversionResult {
                        success: false,
                        output_path: output_path.to_string_lossy().into_owned(),
                        command: vec![],
                        stdout: String::new(),
                        stderr: format!("Could not write: {}", e),
                        returncode: -1,
                    },
                };
            }

            // Other targets: write wrapped markdown to temp file, pandoc renders it
            let tmp_dir = std::env::temp_dir();
            let tmp_path = tmp_dir.join(format!("fm_wrapped_{}.md", std::process::id()));
            if let Err(e) = fs::write(&tmp_path, &wrapped) {
                return ConversionResult {
                    success: false,
                    output_path: output_path.to_string_lossy().into_owned(),
                    command: vec![],
                    stdout: String::new(),
                    stderr: format!("Could not write temp file: {}", e),
                    returncode: -1,
                };
            }

            let result = run_pandoc(&binary, "markdown", &tmp_path, output_path);
            let _ = fs::remove_file(&tmp_path);

            // Report real paths in result
            ConversionResult {
                output_path: output_path.to_string_lossy().into_owned(),
                command: vec![
                    binary,
                    input_path.to_string_lossy().into_owned(),
                    "-f".into(),
                    "markdown".into(),
                    "-t".into(),
                    target_ext,
                    "-o".into(),
                    output_path.to_string_lossy().into_owned(),
                ],
                ..result
            }
        }
    }
}
