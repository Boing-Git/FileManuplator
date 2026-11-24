//! The mobile app's Rust side. The page in `ui/` draws everything; it asks
//! this side for the bundled plugins, for colour schemes (the same ones the
//! desktop app uses) and to run a pipeline on the bytes of a file it picked.

use fm_core::types::{PipelineStep, Plugin};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;
use tauri::ipc::{InvokeBody, Request, Response};
use tauri::{AppHandle, Manager, Runtime, State};

/// Every plugin from FM-Plugins, compiled in by build.rs as (file name, TOML).
mod bundled {
    include!(concat!(env!("OUT_DIR"), "/bundled_plugins.rs"));
}

const LAVENDER: u32 = 0x6750A4;

/// Image formats the converter can write on the device.
const IMAGE_TARGETS: &[&str] = &["png", "jpg", "webp", "gif", "bmp", "tiff"];

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PluginInfo {
    id: &'static str,
    plugin: Option<Plugin>,
    /// Actions that need a desktop program (ffmpeg, ImageMagick, …).
    desktop_only: Vec<String>,
    /// Why the plugin failed to load, if it did.
    problem: Option<String>,
}

#[tauri::command]
fn plugins() -> Vec<PluginInfo> {
    bundled::PLUGINS
        .iter()
        .map(|(id, text)| match fm_core::parser::parse_plugin(text) {
            Ok((plugin, _warnings)) => PluginInfo {
                id,
                desktop_only: fm_core::validate::external_actions(&plugin),
                plugin: Some(plugin),
                problem: None,
            },
            Err(problem) => PluginInfo {
                id,
                plugin: None,
                desktop_only: Vec::new(),
                problem: Some(problem),
            },
        })
        .collect()
}

/// Every M3 colour role of a scheme, keyed by role name.
#[tauri::command]
fn scheme(id: String, dark: bool) -> HashMap<&'static str, String> {
    let seed = fm_core::theme::seed_for(&id).unwrap_or(LAVENDER);
    fm_core::theme::roles(seed, dark).into_iter().collect()
}

#[derive(Serialize)]
struct Swatch {
    id: &'static str,
    label: &'static str,
    primary: String,
}

/// The preset schemes, each with its primary colour in the current mode.
#[tauri::command]
fn swatches(dark: bool) -> Vec<Swatch> {
    fm_core::theme::PRESETS
        .iter()
        .map(|(id, label, seed)| Swatch {
            id,
            label,
            primary: fm_core::theme::roles(*seed, dark)
                .into_iter()
                .find(|(role, _)| *role == "primary")
                .map(|(_, hex)| hex)
                .unwrap_or_default(),
        })
        .collect()
}

#[tauri::command]
fn image_targets() -> &'static [&'static str] {
    IMAGE_TARGETS
}

/// What the page sends ahead of the file's bytes.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RunMeta {
    file_name: String,
    /// Run this bundled plugin's pipeline…
    plugin_id: Option<String>,
    /// …or convert an image to this format.
    target: Option<String>,
    #[serde(default)]
    state: HashMap<String, String>,
}

#[derive(Serialize)]
struct RunResult {
    name: String,
    size: usize,
}

/// The latest result, held until the user saves it.
#[derive(Default)]
struct LastOutput(Mutex<Option<(String, Vec<u8>)>>);

/// Run a plugin or a conversion. The request body is a 4-byte little-endian
/// length, that many bytes of JSON [`RunMeta`], then the file itself.
#[tauri::command(async)]
fn process<R: Runtime>(
    app: AppHandle<R>,
    last: State<'_, LastOutput>,
    request: Request<'_>,
) -> Result<RunResult, String> {
    let InvokeBody::Raw(body) = request.body() else {
        return Err("Expected the file's bytes.".into());
    };
    let (meta, file) = split_body(body)?;

    let (steps, output_name) = match (&meta.plugin_id, &meta.target) {
        (Some(id), _) => {
            let plugin = find_plugin(id)?;
            let steps = plugin.default_pipeline()?.to_vec();
            (steps, plugin.output_name(&meta.file_name))
        }
        (None, Some(target)) if IMAGE_TARGETS.contains(&target.as_str()) => {
            let stem = std::path::Path::new(&meta.file_name)
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| "converted".into());
            (vec![convert_step()], format!("{stem}.{target}"))
        }
        _ => return Err("Nothing to run: no plugin or target format given.".into()),
    };
    let output_name = fm_core::engine::safe_file_name(&output_name);

    let work = app
        .path()
        .app_cache_dir()
        .map_err(|e| e.to_string())?
        .join("run");
    let bytes = fm_core::engine::run_on_bytes(
        &steps,
        &meta.file_name,
        file,
        &output_name,
        &meta.state,
        &work,
    )?;
    stage_for_ios_export(&app, &output_name, &bytes)?;

    let size = bytes.len();
    *last.0.lock().map_err(|e| e.to_string())? = Some((output_name.clone(), bytes));
    Ok(RunResult {
        name: output_name,
        size,
    })
}

/// The latest result's bytes, for previews.
#[tauri::command]
fn last_output(last: State<'_, LastOutput>) -> Result<Response, String> {
    let guard = last.0.lock().map_err(|e| e.to_string())?;
    let (_, bytes) = guard.as_ref().ok_or("Nothing has been made yet.")?;
    Ok(Response::new(bytes.clone()))
}

/// Write the latest result where the save dialog pointed: a path on desktop,
/// a `content://` URI on Android. On iOS the dialog has already exported the
/// copy staged in Documents, so there is nothing left to write.
#[tauri::command]
fn save_output<R: Runtime>(
    app: AppHandle<R>,
    last: State<'_, LastOutput>,
    dest: String,
) -> Result<(), String> {
    if cfg!(target_os = "ios") {
        return Ok(());
    }
    use std::io::Write as _;
    use tauri_plugin_fs::FsExt as _;

    let guard = last.0.lock().map_err(|e| e.to_string())?;
    let (_, bytes) = guard.as_ref().ok_or("Nothing has been made yet.")?;
    let mut options = tauri_plugin_fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    let path: tauri_plugin_fs::FilePath = dest.parse().map_err(|_| "Bad destination.")?;
    let mut file = app
        .fs()
        .open(path, options)
        .map_err(|e| format!("Cannot open the destination: {e}"))?;
    file.write_all(bytes)
        .map_err(|e| format!("Cannot write the file: {e}"))
}

/// iOS's save dialog can only export a file that already exists in the app's
/// Documents folder under the suggested name, so put the result there first.
/// (Documents is also what the Files app shows as this app's folder.)
fn stage_for_ios_export<R: Runtime>(
    app: &AppHandle<R>,
    name: &str,
    bytes: &[u8],
) -> Result<(), String> {
    if cfg!(target_os = "ios") {
        let dir = app.path().document_dir().map_err(|e| e.to_string())?;
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        std::fs::write(dir.join(name), bytes).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn find_plugin(id: &str) -> Result<Plugin, String> {
    let (_, text) = bundled::PLUGINS
        .iter()
        .find(|(file, _)| *file == id)
        .ok_or_else(|| format!("No plugin named '{id}'."))?;
    fm_core::parser::parse_plugin(text).map(|(plugin, _)| plugin)
}

/// A one-step pipeline that re-encodes an image in its output's format.
fn convert_step() -> PipelineStep {
    PipelineStep {
        step: 1,
        action: "export_image".into(),
        target_file: Some("{file.path}".into()),
        output_file: Some("{output.path}".into()),
        ..Default::default()
    }
}

fn split_body(body: &[u8]) -> Result<(RunMeta, &[u8]), String> {
    let len = body
        .get(..4)
        .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize)
        .ok_or("Empty request.")?;
    let meta = body.get(4..4 + len).ok_or("Truncated request.")?;
    let meta: RunMeta = serde_json::from_slice(meta).map_err(|e| format!("Bad request: {e}"))?;
    Ok((meta, &body[4 + len..]))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .manage(LastOutput::default())
        .invoke_handler(tauri::generate_handler![
            plugins,
            scheme,
            swatches,
            image_targets,
            process,
            last_output,
            save_output
        ])
        .run(tauri::generate_context!())
        .expect("error while running the app");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn body_splits_into_meta_and_file() {
        let meta = br#"{"fileName":"a.png","target":"jpg"}"#;
        let mut body = (meta.len() as u32).to_le_bytes().to_vec();
        body.extend_from_slice(meta);
        body.extend_from_slice(b"PNGDATA");
        let (parsed, file) = split_body(&body).unwrap();
        assert_eq!(parsed.file_name, "a.png");
        assert_eq!(parsed.target.as_deref(), Some("jpg"));
        assert_eq!(file, b"PNGDATA");
        assert!(split_body(&body[..6]).is_err());
    }

    #[test]
    fn converts_an_image_over_ipc() {
        use tauri::test::{INVOKE_KEY, get_ipc_response, mock_builder, mock_context, noop_assets};
        use tauri::webview::InvokeRequest;

        let app = mock_builder()
            .manage(LastOutput::default())
            .invoke_handler(tauri::generate_handler![process, last_output])
            .build(mock_context(noop_assets()))
            .unwrap();
        let webview = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
            .build()
            .unwrap();
        let request = |cmd: &str, body: InvokeBody| InvokeRequest {
            cmd: cmd.into(),
            callback: tauri::ipc::CallbackFn(0),
            error: tauri::ipc::CallbackFn(1),
            // The app's own origin on Linux/macOS (Android/Windows use http://tauri.localhost).
            url: "tauri://localhost".parse().unwrap(),
            body,
            headers: Default::default(),
            invoke_key: INVOKE_KEY.to_string(),
        };

        let meta = br#"{"fileName":"icon.png","target":"jpg"}"#;
        let mut body = (meta.len() as u32).to_le_bytes().to_vec();
        body.extend_from_slice(meta);
        body.extend_from_slice(include_bytes!("../icons/32x32.png"));
        let result: serde_json::Value =
            get_ipc_response(&webview, request("process", InvokeBody::Raw(body)))
                .unwrap()
                .deserialize()
                .unwrap();
        assert_eq!(result["name"], "icon.jpg");

        let bytes =
            get_ipc_response(&webview, request("last_output", InvokeBody::default())).unwrap();
        let tauri::ipc::InvokeResponseBody::Raw(bytes) = bytes else {
            panic!("last_output should answer with raw bytes");
        };
        assert_eq!(&bytes[..3], &[0xFF, 0xD8, 0xFF], "not a JPEG");
        assert_eq!(bytes.len() as u64, result["size"].as_u64().unwrap());
    }

    #[test]
    fn bundled_plugins_load() {
        for info in plugins() {
            assert!(info.problem.is_none(), "{}: {:?}", info.id, info.problem);
        }
    }
}
