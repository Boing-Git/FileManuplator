//! Loads every plugin from the sibling `FM-Plugins` checkout (and `PhotoForge/`)
//! and checks that it parses and that every step condition is valid syntax.
//! Skipped when the plugin repo isn't checked out next to this one.

use fm_core::condition;
use fm_core::types::Plugin;
use std::path::{Path, PathBuf};

fn plugin_files() -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let dirs = [root.join("FM-Plugins"), root.join("FM/PhotoForge")];
    let mut files = Vec::new();
    for dir in dirs {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("toml") {
                    files.push(path);
                }
            }
        }
    }
    files.sort();
    files
}

#[test]
fn shipped_plugins_parse_and_have_valid_conditions() {
    let files = plugin_files();
    if files.is_empty() {
        eprintln!("no FM-Plugins checkout found; skipping");
        return;
    }

    let mut problems = Vec::new();
    for path in &files {
        let text = std::fs::read_to_string(path).unwrap();
        let plugin: Plugin = match toml::from_str(&text) {
            Ok(p) => p,
            Err(e) => {
                problems.push(format!("{}: {e}", path.display()));
                continue;
            }
        };
        for (name, steps) in &plugin.pipeline {
            for step in steps {
                if let Some(cond) = &step.condition
                    && let Err(e) = condition::evaluate(cond, &|s| s.to_string())
                {
                    problems.push(format!(
                        "{}: pipeline '{name}' step {}: {e}",
                        path.display(),
                        step.step
                    ));
                }
            }
        }
    }
    assert!(problems.is_empty(), "\n{}", problems.join("\n"));
}

/// A plugin from the checkouts next to this repo, or None to skip the test.
fn plugin_at(rel: &str) -> Option<Plugin> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .join(rel);
    let text = std::fs::read_to_string(path).ok()?;
    Some(toml::from_str(&text).unwrap())
}

#[test]
fn smart_sorter_routes_a_pdf_to_documents() {
    let dir = std::env::temp_dir().join("fm_smartsort");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let input = dir.join("paper.pdf");
    std::fs::write(&input, b"%PDF-1.4\n").unwrap();
    let out_dir = dir.join("sorted");
    std::fs::create_dir_all(&out_dir).unwrap();

    let Some(plugin) = plugin_at("FM-Plugins/smart_sorter.toml") else {
        eprintln!("no FM-Plugins checkout; skipping");
        return;
    };
    let ctx = fm_core::types::FileContext::from_paths(
        &input,
        &out_dir.join("paper.pdf"),
        "application/pdf",
    );
    fm_core::engine::execute_pipeline(
        plugin.default_pipeline().unwrap(),
        &ctx,
        &std::collections::HashMap::new(),
    )
    .unwrap();

    assert!(
        out_dir.join("Documents/paper.pdf").exists(),
        "pdf should land in Documents/, found: {:?}",
        std::fs::read_dir(&out_dir)
            .unwrap()
            .flatten()
            .map(|e| e.path())
            .collect::<Vec<_>>()
    );
    assert!(
        !out_dir.join("Others/paper.pdf").exists(),
        "pdf fell through to Others/"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn media_renamer_keeps_the_extension_separated_by_a_dot() {
    let dir = std::env::temp_dir().join("fm_renamer");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let input = dir.join("Some.Movie.2020.1080p.x264-GROUP.mkv");
    std::fs::write(&input, b"video").unwrap();
    let out_dir = dir.join("out");
    std::fs::create_dir_all(&out_dir).unwrap();

    let Some(plugin) = plugin_at("FM-Plugins/media_renamer.toml") else {
        eprintln!("no FM-Plugins checkout; skipping");
        return;
    };
    let ctx =
        fm_core::types::FileContext::from_paths(&input, &out_dir.join("x.mkv"), "video/x-matroska");
    fm_core::engine::execute_pipeline(
        plugin.default_pipeline().unwrap(),
        &ctx,
        &std::collections::HashMap::new(),
    )
    .unwrap();

    let produced: Vec<String> = std::fs::read_dir(&out_dir)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        produced,
        vec!["Some Movie 2020.mkv".to_string()],
        "got {produced:?}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn every_shipped_plugin_passes_validation() {
    let files = plugin_files();
    if files.is_empty() {
        return;
    }
    let mut problems = Vec::new();
    for path in &files {
        let text = std::fs::read_to_string(path).unwrap();
        match toml::from_str::<Plugin>(&text) {
            Ok(p) => {
                for d in fm_core::validate::validate(&p) {
                    problems.push(format!("{}: {d}", path.display()));
                }
            }
            Err(e) => problems.push(format!("{}: {e}", path.display())),
        }
    }
    assert!(problems.is_empty(), "\n{}", problems.join("\n"));
}
