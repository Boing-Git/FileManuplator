use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// =============================================================================
//  Conversion Routing (mirrors Python config.py)
// =============================================================================

/// External binaries filemanuplator can route work to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    ImageMagick,
    Ffmpeg,
    Pandoc,
}

impl Tool {
    pub fn as_str(&self) -> &'static str {
        match self {
            Tool::ImageMagick => "imagemagick",
            Tool::Ffmpeg => "ffmpeg",
            Tool::Pandoc => "pandoc",
        }
    }
}

impl std::fmt::Display for Tool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Describes how a given input MIME type can be converted.
#[derive(Debug, Clone)]
pub struct ConversionRoute {
    pub tool: Tool,
    /// Binary name candidates to try (e.g. "magick" then "convert").
    pub binary_candidates: Vec<String>,
    /// Allowed output extensions.
    pub targets: Vec<String>,
    /// Pandoc-only: the reader name (e.g. "markdown", "html", "docx").
    pub pandoc_from: String,
    /// Pandoc-only: if true, input is raw text (code, config) — wrap in fence.
    pub raw_text: bool,
}

impl ConversionRoute {
    pub fn new(tool: Tool, binary_candidates: &[&str], targets: &[&str]) -> Self {
        Self {
            tool,
            binary_candidates: binary_candidates.iter().map(|s| s.to_string()).collect(),
            targets: targets.iter().map(|s| s.to_string()).collect(),
            pandoc_from: "markdown".to_string(),
            raw_text: false,
        }
    }

    pub fn pandoc(binary_candidates: &[&str], targets: &[&str], from: &str, raw: bool) -> Self {
        Self {
            tool: Tool::Pandoc,
            binary_candidates: binary_candidates.iter().map(|s| s.to_string()).collect(),
            targets: targets.iter().map(|s| s.to_string()).collect(),
            pandoc_from: from.to_string(),
            raw_text: raw,
        }
    }
}

/// Result of a conversion attempt.
#[derive(Debug, Clone)]
pub struct ConversionResult {
    pub success: bool,
    pub output_path: String,
    pub command: Vec<String>,
    pub stdout: String,
    pub stderr: String,
    pub returncode: i32,
}

/// Pandoc writer name for each output extension.
pub fn pandoc_writer(ext: &str) -> Option<&'static str> {
    match ext {
        "md" | "markdown" => Some("markdown"),
        "txt" | "plain" => Some("plain"),
        "html" => Some("html"),
        "pdf" => Some("pdf"),
        "docx" => Some("docx"),
        "odt" => Some("odt"),
        "epub" => Some("epub3"),
        "rst" => Some("rst"),
        _ => None,
    }
}

// =============================================================================
//  TOML Plugin Types (extends conversion with custom pipelines)
// =============================================================================

/// A TOML plugin definition — can define UI tabs with interactive widgets
/// AND/OR simple MIME→target conversion routes with pipeline actions.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Plugin {
    pub plugin: PluginMeta,
    pub tab: Option<TabDef>,
    #[serde(default)]
    pub ui: Vec<UiWidget>,
    #[serde(default)]
    pub pipeline: HashMap<String, Vec<PipelineStep>>,
    /// MIME types this plugin handles (for converter integration).
    #[serde(default)]
    pub mime_types: Vec<String>,
    /// Target format this plugin produces (for converter integration).
    #[serde(default)]
    pub target: Option<String>,
    #[serde(default)]
    pub extensions: Option<Vec<String>>,
}

impl Plugin {
    /// The pipeline frontends should run: the one named `default`, or the only
    /// one if the plugin defines exactly one. Picking "any" pipeline out of the
    /// map would be nondeterministic, so several unnamed pipelines is an error.
    pub fn default_pipeline(&self) -> Result<&[PipelineStep], String> {
        if let Some(steps) = self.pipeline.get("default") {
            return Ok(steps);
        }
        match self.pipeline.len() {
            0 => Err("Plugin has no pipeline defined.".to_string()),
            1 => Ok(self.pipeline.values().next().unwrap()),
            _ => Err("Plugin defines several pipelines but none is named 'default'.".to_string()),
        }
    }

    /// File name to suggest for this plugin's output: the input's own name,
    /// or with the plugin's name as extension when `change_extension` is on.
    pub fn output_name(&self, input_name: &str) -> String {
        if self.plugin.change_extension.unwrap_or(true) {
            std::path::Path::new(input_name)
                .with_extension(self.plugin.name.to_lowercase().replace(' ', "_"))
                .to_string_lossy()
                .into_owned()
        } else {
            input_name.to_string()
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct PluginMeta {
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: String,
    pub change_extension: Option<bool>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct TabDef {
    pub title: String,
    pub icon: Option<String>,
    pub closable: Option<bool>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(untagged)]
pub enum BoolOrString {
    Bool(bool),
    String(String),
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct UiWidget {
    #[serde(rename = "type")]
    pub widget_type: String,
    pub id: Option<String>,
    pub label: Option<String>,
    pub filter: Option<Vec<String>>,
    pub text: Option<String>,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub default: Option<toml::Value>,
    pub options: Option<Vec<String>>,
    pub on_click: Option<String>,
    pub image_source: Option<String>,
    pub lock_aspect_ratio: Option<String>,
    pub aspect_ratio_source: Option<String>,
    pub show_grid: Option<bool>,
    pub shade_outside: Option<f64>,
    pub lock_inside_image: Option<BoolOrString>,
    pub overwrites: Option<Vec<String>>,
    pub show_handles: Option<bool>,
    pub draggable_selection: Option<bool>,
    /// Colour role from the active scheme: `primary`, `secondary`, `tertiary`
    /// or `error` (see [`crate::theme::PLUGIN_ROLES`]).
    pub color: Option<String>,
    /// `html_view` only: the HTML file to run, usually `"{file.path}"`.
    pub source: Option<String>,
    /// Any setting the schema does not know. Collected so a misspelled field is
    /// reported instead of silently ignored.
    #[serde(flatten)]
    pub extra: std::collections::BTreeMap<String, toml::Value>,
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct PipelineStep {
    pub step: i32,
    pub action: String,
    pub target_file: Option<String>,
    pub output_file: Option<String>,
    pub target_dir: Option<String>,
    pub output_dir: Option<String>,
    pub condition: Option<String>,

    // Action-specific args
    pub crop_data: Option<String>,
    pub value: Option<String>,
    pub lut_file: Option<String>,
    pub degrees: Option<String>,
    pub paths: Option<Vec<String>>,
    pub filter_extensions: Option<Vec<String>>,
    pub width: Option<String>,
    pub height: Option<String>,
    pub filter_type: Option<String>,
    pub x: Option<String>,
    pub y: Option<String>,
    pub brightness: Option<String>,
    pub contrast: Option<String>,
    pub format: Option<String>,
    pub args: Option<String>,
    /// `exec` only: the program to run.
    pub program: Option<String>,
    /// `exec` only: the argument list, passed through without a shell.
    pub argv: Option<Vec<String>>,
    pub destination: Option<String>,
    pub chunk_size: Option<String>,
    pub output_pattern: Option<String>,
    pub pattern: Option<String>,
    pub algorithm: Option<String>,
    pub message: Option<String>,
    pub target_path: Option<String>,
    pub text_content: Option<String>,
    pub text: Option<String>,
    pub executable: Option<bool>,
    pub rules: Option<Vec<RegexRule>>,
    /// `export_image` only: `contain`, `cover` or `stretch`.
    pub fit: Option<String>,
    /// `export_image` only: largest JPEG output size in KiB.
    pub max_kb: Option<String>,
    /// Any setting the schema does not know. Collected so a misspelled field is
    /// reported instead of silently ignored.
    #[serde(flatten)]
    pub extra: std::collections::BTreeMap<String, toml::Value>,
}

impl PipelineStep {
    /// Every field of this step that may contain `{placeholders}`.
    pub fn template_strings(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        let singles = [
            &self.target_file,
            &self.output_file,
            &self.target_dir,
            &self.output_dir,
            &self.crop_data,
            &self.value,
            &self.lut_file,
            &self.degrees,
            &self.width,
            &self.height,
            &self.x,
            &self.y,
            &self.brightness,
            &self.contrast,
            &self.args,
            &self.program,
            &self.destination,
            &self.chunk_size,
            &self.output_pattern,
            &self.pattern,
            &self.message,
            &self.target_path,
            &self.text_content,
            &self.text,
            &self.fit,
            &self.max_kb,
        ];
        for s in singles.into_iter().flatten() {
            out.push(s.clone());
        }
        for list in [&self.paths, &self.argv].into_iter().flatten() {
            out.extend(list.iter().cloned());
        }
        if let Some(rules) = &self.rules {
            out.extend(rules.iter().map(|r| r.replacement.clone()));
        }
        out
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct RegexRule {
    pub pattern: String,
    pub replacement: String,
}

// =============================================================================
//  File Context (shared by both converter engine and plugin engine)
// =============================================================================

#[derive(Debug, Clone)]
pub struct FileContext {
    pub file_path: String,
    pub file_name: String,
    pub file_stem: String,
    pub file_ext: String,
    pub file_dir: String,
    pub file_mime: String,
    pub file_size: String,
    pub output_dir: String,
    pub output_path: String,
    pub variables: HashMap<String, String>,
}

impl FileContext {
    pub fn to_map(&self) -> HashMap<String, String> {
        let mut map = self.variables.clone();
        map.insert("file.path".to_string(), self.file_path.clone());
        map.insert("file.name".to_string(), self.file_name.clone());
        map.insert("file.stem".to_string(), self.file_stem.clone());
        map.insert("file.ext".to_string(), self.file_ext.clone());
        map.insert("file.dir".to_string(), self.file_dir.clone());
        map.insert("file.mime".to_string(), self.file_mime.clone());
        map.insert("file.size".to_string(), self.file_size.clone());
        map.insert("output.dir".to_string(), self.output_dir.clone());
        map.insert("output.path".to_string(), self.output_path.clone());
        map
    }

    /// Build a FileContext from an input path and output path.
    pub fn from_paths(input: &std::path::Path, output: &std::path::Path, mime: &str) -> Self {
        let size = std::fs::metadata(input)
            .map(|m| m.len().to_string())
            .unwrap_or_else(|_| "0".to_string());
        Self {
            file_path: input.to_string_lossy().into_owned(),
            file_name: input
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
            file_stem: input
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
            file_ext: input
                .extension()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
            file_dir: input
                .parent()
                .unwrap_or(std::path::Path::new(""))
                .to_string_lossy()
                .into_owned(),
            file_mime: mime.to_string(),
            file_size: size,
            output_dir: output
                .parent()
                .unwrap_or(std::path::Path::new(""))
                .to_string_lossy()
                .into_owned(),
            output_path: output.to_string_lossy().into_owned(),
            variables: HashMap::new(),
        }
    }
}
