//! Runs every FM-Plugins conversion target on a real sample and checks the
//! files it writes. Skips without an FM-Plugins checkout next to this repo,
//! and skips a plugin whose program is not installed.

use fm_core::types::{FileContext, Plugin};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

fn plugin(name: &str) -> Option<Plugin> {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("../../../FM-Plugins/{name}.toml"));
    Some(toml::from_str(&std::fs::read_to_string(path).ok()?).unwrap())
}

fn installed(program: &str) -> bool {
    std::process::Command::new(program)
        .arg("--version")
        .output()
        .is_ok()
}

/// Run `name` on `input` (written into a fresh in/ folder) with the output
/// folder out/; returns out/ or None when the plugin cannot run here.
fn run(
    name: &str,
    input_name: &str,
    bytes: &[u8],
    needs: Option<&str>,
) -> Option<(PathBuf, PathBuf)> {
    let plugin = plugin(name)?;
    if needs.is_some_and(|p| !installed(p)) {
        eprintln!("{name}: {} not installed; skipping", needs.unwrap());
        return None;
    }
    let dir = std::env::temp_dir().join(format!("fm_plugin_outputs_{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("in")).unwrap();
    std::fs::create_dir_all(dir.join("out")).unwrap();
    let input = dir.join("in").join(input_name);
    std::fs::write(&input, bytes).unwrap();
    let output = dir
        .join("out")
        .join(format!("result.{}", plugin.target.as_deref().unwrap()));
    let ctx = FileContext::from_paths(
        &input,
        &output,
        &fm_core::detector::detect_mime_type(&input),
    );
    fm_core::engine::execute_pipeline(plugin.default_pipeline().unwrap(), &ctx, &HashMap::new())
        .unwrap_or_else(|e| panic!("{name}: {e}"));
    Some((input, dir.join("out")))
}

fn listing(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

#[test]
fn splitters_split_and_join_restores_the_original() {
    let data: Vec<u8> = (0..30 * 1024 * 1024u32).map(|i| (i % 251) as u8).collect();
    let Some((_, out)) = run("discord_splitter", "big.bin", &data, None) else {
        return;
    };
    assert_eq!(listing(&out), ["big.bin.part00", "big.bin.part01"]);

    // Joining is run on the first part, exactly as a user would.
    let joiner = plugin("discord_join").unwrap();
    let part = out.join("big.bin.part00");
    let joined = out.join("big.bin");
    let ctx = FileContext::from_paths(&part, &joined, "application/octet-stream");
    fm_core::engine::execute_pipeline(joiner.default_pipeline().unwrap(), &ctx, &HashMap::new())
        .unwrap();
    assert!(
        std::fs::read(&joined).unwrap() == data,
        "joined file differs"
    );
}

#[test]
fn fat32_splitter_copies_small_files_under_their_own_name() {
    let Some((_, out)) = run("fat32_splitter", "small.bin", b"hello", None) else {
        return;
    };
    assert_eq!(listing(&out), ["small.bin"]);
}

#[test]
fn csv_splitter_writes_only_the_parts() {
    let Some((_, out)) = run("csv_splitter", "data.csv", b"a,b\n1,2\n3,4\n", None) else {
        return;
    };
    assert_eq!(listing(&out), ["data_part_000.csv"]);
    assert_eq!(
        std::fs::read_to_string(out.join("data_part_000.csv")).unwrap(),
        "a,b\n1,2\n3,4\n"
    );
}

#[test]
fn par2_protector_writes_checksum_and_recovery_files() {
    let Some((_, out)) = run("par2_protector", "doc.bin", &[42u8; 4096], Some("par2")) else {
        return;
    };
    let files = listing(&out);
    assert!(files.contains(&"doc.bin.sha256".to_string()), "{files:?}");
    assert!(files.contains(&"doc.bin.par2".to_string()), "{files:?}");
}

#[test]
fn image_and_video_optimizers_write_their_format() {
    let mut jpg = Vec::new();
    image::DynamicImage::ImageRgb8(image::RgbImage::new(32, 32))
        .write_to(
            &mut std::io::Cursor::new(&mut jpg),
            image::ImageFormat::Jpeg,
        )
        .unwrap();
    if let Some((_, out)) = run("webp_privacy_optimizer", "photo.jpg", &jpg, Some("magick")) {
        assert_eq!(listing(&out), ["photo.webp"]);
    }
}
