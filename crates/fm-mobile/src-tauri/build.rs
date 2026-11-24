use std::fmt::Write as _;
use std::path::PathBuf;
use std::{env, fs};

fn main() {
    bundle_plugins();
    tauri_build::build();
}

/// Compile every plugin from the FM-Plugins checkout into the app, so it has
/// them offline. FM_PLUGINS_DIR points elsewhere; by default the checkout
/// next to this repository is used (as the fm-core tests do).
fn bundle_plugins() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let dir = env::var_os("FM_PLUGINS_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| manifest.join("../../../../FM-Plugins"));
    println!("cargo:rerun-if-env-changed=FM_PLUGINS_DIR");
    println!("cargo:rerun-if-changed={}", dir.display());

    let mut files: Vec<PathBuf> = fs::read_dir(&dir)
        .map(|entries| {
            entries
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|e| e == "toml"))
                .collect()
        })
        .unwrap_or_default();
    files.sort();
    if files.is_empty() {
        println!(
            "cargo:warning=no plugins in {}; the app will only have the converter",
            dir.display()
        );
    }

    let mut out = String::from("pub const PLUGINS: &[(&str, &str)] = &[\n");
    for file in &files {
        let path = file.canonicalize().unwrap();
        let name = file.file_name().unwrap().to_string_lossy();
        let _ = writeln!(
            out,
            "    ({name:?}, include_str!({:?})),",
            path.display().to_string()
        );
    }
    out.push_str("];\n");
    let target = PathBuf::from(env::var("OUT_DIR").unwrap()).join("bundled_plugins.rs");
    fs::write(target, out).unwrap();
}
