//! Drives the video_trimmer UI plugin through the real engine with the exact
//! state map the GTK tab produces, to find what a second UI-plugin author hits.

use fm_core::engine::execute_pipeline;
use fm_core::types::{FileContext, Plugin};
use std::collections::HashMap;

/// The Trimmer from the FM-Plugins checkout next to this repo. The tests skip
/// without that checkout or without ffmpeg (CI provides both).
fn load() -> Option<Plugin> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../FM-Plugins/video_trimmer.toml");
    let text = std::fs::read_to_string(path).ok()?;
    std::process::Command::new("ffmpeg")
        .arg("-version")
        .output()
        .ok()?;
    Some(toml::from_str(&text).unwrap())
}

/// Exactly what build_plugin_tab inserts into tab_state for this plugin.
fn ui_state(input: &str, audio: &str) -> HashMap<String, String> {
    let mut s = HashMap::new();
    s.insert("input_video".into(), input.into());
    s.insert("file.path".into(), input.into());
    s.insert("start_time".into(), "00:00:00".into());
    s.insert("duration".into(), "1".into());
    s.insert("preset".into(), "ultrafast".into());
    s.insert("audio_mode".into(), audio.into());
    s.insert("crf".into(), 23.0_f64.to_string());
    s
}

fn make_video(path: &std::path::Path) {
    let st = std::process::Command::new("ffmpeg")
        .args([
            "-y",
            "-f",
            "lavfi",
            "-i",
            "testsrc=duration=2:size=64x64:rate=10",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:duration=2",
            "-c:v",
            "libx264",
            "-c:a",
            "aac",
            "-pix_fmt",
            "yuv420p",
        ])
        .arg(path)
        .output()
        .expect("ffmpeg missing");
    assert!(
        st.status.success(),
        "{}",
        String::from_utf8_lossy(&st.stderr)
    );
}

fn run(
    plugin: &Plugin,
    dir: &std::path::Path,
    name: &str,
    audio: &str,
) -> (Result<(), String>, std::path::PathBuf) {
    let input = dir.join(name);
    make_video(&input);
    let output = dir.join("out.mp4");
    let ctx = FileContext::from_paths(&input, &output, "video/mp4");
    let state = ui_state(&input.to_string_lossy(), audio);
    let r = execute_pipeline(plugin.default_pipeline().unwrap(), &ctx, &state);
    (r, output)
}

#[test]
fn a_plain_filename_works() {
    let Some(plugin) = load() else {
        eprintln!("no FM-Plugins checkout or no ffmpeg; skipping");
        return;
    };
    let dir = std::env::temp_dir().join("fmtest_plain");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let (r, out) = run(&plugin, &dir, "clip.mp4", "Keep audio");
    assert!(r.is_ok(), "pipeline failed: {r:?}");
    assert!(out.exists(), "no output produced");
    println!(
        "PLAIN: ok, {} bytes",
        std::fs::metadata(&out).unwrap().len()
    );
}

#[test]
fn b_filename_with_space_and_apostrophe() {
    let Some(plugin) = load() else {
        eprintln!("no FM-Plugins checkout or no ffmpeg; skipping");
        return;
    };
    let dir = std::env::temp_dir().join("fmtest_quote");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let (r, out) = run(&plugin, &dir, "my friend's clip.mp4", "Keep audio");
    println!("APOSTROPHE: result={r:?} output_exists={}", out.exists());
    if r.is_err() {
        println!("APOSTROPHE: -> a filename with ' cannot be processed at all");
    }
    assert!(r.is_ok() && out.exists(), "BROKEN on apostrophe filename");
}

#[test]
fn c_mute_branch_runs_only_second_step() {
    let Some(plugin) = load() else {
        eprintln!("no FM-Plugins checkout or no ffmpeg; skipping");
        return;
    };
    let dir = std::env::temp_dir().join("fmtest_mute");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let (r, out) = run(&plugin, &dir, "clip.mp4", "Mute");
    assert!(r.is_ok(), "pipeline failed: {r:?}");
    let probe = std::process::Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-select_streams",
            "a",
            "-show_entries",
            "stream=codec_type",
            "-of",
            "csv=p=0",
        ])
        .arg(&out)
        .output()
        .unwrap();
    let streams = String::from_utf8_lossy(&probe.stdout).trim().to_string();
    println!("MUTE: audio streams = {streams:?}");
    assert!(streams.is_empty(), "Mute produced audio -> wrong step ran");
}

#[test]
fn d_text_input_is_passed_to_shell_unescaped() {
    let Some(plugin) = load() else {
        eprintln!("no FM-Plugins checkout or no ffmpeg; skipping");
        return;
    };
    // What happens if a user types a normal shell character into a text field?
    let dir = std::env::temp_dir().join("fmtest_inject");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let input = dir.join("clip.mp4");
    make_video(&input);
    let canary = dir.join("CANARY_CREATED_BY_TEXT_INPUT");
    let output = dir.join("out.mp4");
    let ctx = FileContext::from_paths(&input, &output, "video/mp4");
    let mut state = ui_state(&input.to_string_lossy(), "Keep audio");
    // A user typing this into "Duration (seconds)" is not doing anything exotic.
    // `#` comments out the rest of the generated command line.
    state.insert(
        "duration".into(),
        format!("1 ; touch '{}' #", canary.display()),
    );
    let _ = execute_pipeline(plugin.default_pipeline().unwrap(), &ctx, &state);
    println!("INJECTION: canary exists = {}", canary.exists());
    assert!(!canary.exists(), "text_input value executed as shell code");
}
