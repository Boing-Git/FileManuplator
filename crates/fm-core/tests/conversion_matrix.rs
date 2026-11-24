//! Every Converter route, every target: make a real sample of each input type,
//! detect it and route it the way the Converter tab does, convert it to each
//! offered format, and check the result is a non-empty file of the right kind.
//! Inputs whose tools are missing are skipped (CI and nix-shell have them all).

use fm_core::config::resolve_route;
use fm_core::converter::{resolve_binary, run_conversion};
use fm_core::detector::detect_mime_type;
use std::path::{Path, PathBuf};
use std::process::Command;

fn have(program: &str) -> bool {
    Command::new(program)
        .arg("-version")
        .output()
        .or_else(|_| Command::new(program).arg("--version").output())
        .is_ok()
}

fn run(program: &str, args: &[&str]) -> bool {
    Command::new(program)
        .args(args)
        .output()
        .is_ok_and(|o| o.status.success())
}

/// Sample inputs: (file name, how to make it). `None` when a tool is missing.
fn make_samples(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let p = |name: &str| dir.join(name);
    let s = |path: &PathBuf| path.to_string_lossy().into_owned();

    // Images, written by the image crate so no tool is needed to make them.
    let img = image::RgbImage::from_fn(64, 48, |x, y| {
        image::Rgb([(x * 4) as u8, (y * 5) as u8, 128])
    });
    for ext in ["png", "jpg", "webp", "gif", "bmp", "tiff"] {
        let path = p(&format!("sample.{ext}"));
        image::DynamicImage::ImageRgb8(img.clone())
            .save(&path)
            .unwrap();
        out.push(path);
    }

    if have("ffmpeg") {
        for (name, extra) in [
            (
                "sample.mp4",
                vec!["-c:v", "libx264", "-pix_fmt", "yuv420p", "-c:a", "aac"],
            ),
            (
                "sample.mkv",
                vec!["-c:v", "libx264", "-pix_fmt", "yuv420p", "-c:a", "aac"],
            ),
            ("sample.webm", vec!["-c:v", "libvpx-vp9", "-c:a", "libopus"]),
            ("sample.avi", vec!["-c:v", "mpeg4", "-c:a", "mp3"]),
            (
                "sample.mov",
                vec!["-c:v", "libx264", "-pix_fmt", "yuv420p", "-c:a", "aac"],
            ),
        ] {
            let path = p(name);
            let mut args = vec![
                "-y",
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                "testsrc=duration=1:size=64x48:rate=10",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=440:duration=1",
            ];
            args.extend(extra);
            let target = s(&path);
            args.push(&target);
            if run("ffmpeg", &args) {
                out.push(path);
            }
        }
        for (name, codec) in [
            ("sample.mp3", "libmp3lame"),
            ("sample.wav", "pcm_s16le"),
            ("sample.flac", "flac"),
            ("sample.ogg", "libvorbis"),
        ] {
            let path = p(name);
            let target = s(&path);
            if run(
                "ffmpeg",
                &[
                    "-y",
                    "-v",
                    "error",
                    "-f",
                    "lavfi",
                    "-i",
                    "sine=frequency=440:duration=1",
                    "-c:a",
                    codec,
                    &target,
                ],
            ) {
                out.push(path);
            }
        }
    }

    let md = p("sample.md");
    std::fs::write(&md, "# Title\n\nSome *text* and a list:\n\n- one\n- two\n").unwrap();
    out.push(md.clone());
    std::fs::write(p("sample.txt"), "plain text\nsecond line\n").unwrap();
    out.push(p("sample.txt"));
    std::fs::write(p("sample.py"), "def f(x):\n    return x * 2\n").unwrap();
    out.push(p("sample.py"));
    std::fs::write(p("sample.json"), "{\"a\": [1, 2, 3]}\n").unwrap();
    out.push(p("sample.json"));
    std::fs::write(p("sample.csv"), "a,b\n1,2\n").unwrap();
    out.push(p("sample.csv"));
    std::fs::write(
        p("sample.html"),
        "<html><body><h1>Hi</h1><p>para</p></body></html>\n",
    )
    .unwrap();
    out.push(p("sample.html"));

    if have("pandoc") {
        for ext in ["docx", "rtf", "epub"] {
            let path = p(&format!("sample.{ext}"));
            let target = s(&path);
            let mut args = vec![s(&md), "-s".into(), "-o".into(), target];
            if ext == "epub" {
                args.extend(["--metadata".into(), "title=Sample".into()]);
            }
            let args: Vec<&str> = args.iter().map(String::as_str).collect();
            if run("pandoc", &args) {
                out.push(path);
            }
        }
    }
    out
}

/// What `file` may call a correct output of each target format.
fn expected_kind(target: &str) -> &'static [&'static str] {
    match target {
        "png" | "jpg" | "webp" | "gif" | "bmp" | "tiff" | "avif" => &["image/"],
        "mp4" | "mkv" | "webm" | "avi" | "mov" => &["video/"],
        "mp3" | "wav" | "flac" | "ogg" | "aac" | "m4a" => &["audio/"],
        "pdf" => &["pdf"],
        "docx" => &["wordprocessingml"],
        "odt" => &["opendocument.text"],
        "epub" => &["epub"],
        "html" => &["html"],
        // Text targets: anything textual except a format that should have
        // been converted (e.g. RTF markup passed through as "txt").
        _ => &["text/plain", "text/x-", "text/csv", "json", "text/markdown"],
    }
}

#[test]
fn every_route_converts_to_every_target() {
    let dir = std::env::temp_dir().join("fm_conversion_matrix");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("out")).unwrap();

    let mut failures = Vec::new();
    let mut checked = 0;
    for input in make_samples(&dir) {
        let mime = detect_mime_type(&input);
        let (route, _) = resolve_route(&mime, &input);
        let name = input.file_name().unwrap().to_string_lossy().into_owned();
        let Some(route) = route else {
            failures.push(format!("{name}: detected as {mime}, which has no route"));
            continue;
        };
        if resolve_binary(&route).is_err() {
            eprintln!("skipping {name}: {} is not installed", route.tool);
            continue;
        }
        for target in &route.targets {
            // A PDF needs a LaTeX/HTML engine that a minimal system may lack.
            if target == "pdf" && fm_core::converter::resolve_pdf_engine().is_none() {
                continue;
            }
            let output = dir.join("out").join(format!("{name}.{target}"));
            let result = run_conversion(&route, &input, &output);
            checked += 1;
            let size = std::fs::metadata(&output).map(|m| m.len()).unwrap_or(0);
            if !result.success || size == 0 {
                let why = result
                    .stderr
                    .lines()
                    .rev()
                    .find(|l| !l.trim().is_empty())
                    .unwrap_or("")
                    .to_string();
                failures.push(format!("{name} ({mime}) -> {target}: {why}"));
                continue;
            }
            let got = detect_mime_type(&output);
            if !expected_kind(target).iter().any(|k| got.contains(k)) {
                failures.push(format!("{name} ({mime}) -> {target}: the output is {got}"));
            }
        }
    }
    eprintln!("checked {checked} conversions");
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}
