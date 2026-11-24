//! Checks a parsed plugin for mistakes and explains them in the plugin author's
//! own terms, before anything runs.
//!
//! The TOML schema is deliberately permissive (every action shares one struct),
//! so serde alone accepts a misspelled field, an action that does not exist, or
//! a `{placeholder}` that refers to nothing. Those mistakes used to surface as a
//! tab that quietly did nothing. This module turns each of them into a message
//! that names the step or widget at fault.

use crate::types::{Plugin, UiWidget};

/// Every action name [`crate::engine::execute_pipeline`] can dispatch.
pub const KNOWN_ACTIONS: &[&str] = &[
    "adjust",
    "adjust_brightness",
    "adjust_contrast",
    "chdman",
    "checksum",
    "convert_format",
    "copy",
    "create_directories",
    "crop_image_visually",
    "delete",
    "echo",
    "exec",
    "export_image",
    "ffmpeg",
    "join_bytes",
    "join_csv",
    "magick",
    "pandoc",
    "par2",
    "regex_copy",
    "resize",
    "rotate",
    "split_bytes",
    "split_csv",
    "wkhtmltopdf",
    "write_text",
];

/// Every widget type the GTK frontend can render.
pub const KNOWN_WIDGETS: &[&str] = &[
    "button",
    "file_picker",
    "html_view",
    "image_canvas",
    "notice",
    "picker",
    "radio_group",
    "slider",
    "text_input",
];

/// Actions that start another program. Phones have none of these programs,
/// so a plugin using one is desktop-only.
pub const EXTERNAL_ACTIONS: &[&str] = &[
    "chdman",
    "exec",
    "ffmpeg",
    "magick",
    "pandoc",
    "par2",
    "wkhtmltopdf",
];

/// The external actions `plugin` uses, for "desktop only" labels.
pub fn external_actions(plugin: &Plugin) -> Vec<String> {
    let mut found: Vec<String> = plugin
        .pipeline
        .values()
        .flatten()
        .filter(|s| EXTERNAL_ACTIONS.contains(&s.action.as_str()))
        .map(|s| s.action.clone())
        .collect();
    found.sort();
    found.dedup();
    found
}

/// Variables the engine always provides.
const BUILTIN_VARS: &[&str] = &[
    "file.path",
    "file.name",
    "file.stem",
    "file.ext",
    "file.dir",
    "file.mime",
    "file.size",
    "output.path",
    "output.dir",
];

/// Variables a `file_picker` adds once an image has been chosen.
const IMAGE_VARS: &[&str] = &[
    "file.width",
    "file.height",
    "file.aspect_ratio",
    "image.width",
    "image.height",
    "image.aspect_ratio",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Severity {
    /// The plugin will not work correctly.
    Error,
    /// The plugin will load, but something looks wrong.
    Warning,
}

#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub severity: Severity,
    /// Where the problem is, e.g. `step 2` or `widget 'crf'`.
    pub location: String,
    pub message: String,
}

impl std::fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let label = match self.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
        };
        write!(f, "{}: {}: {}", label, self.location, self.message)
    }
}

/// Suggest the closest known name, for "did you mean" hints.
fn closest<'a>(word: &str, options: &[&'a str]) -> Option<&'a str> {
    options
        .iter()
        .map(|o| (edit_distance(word, o), *o))
        .filter(|(d, _)| *d <= 3)
        .min_by_key(|(d, _)| *d)
        .map(|(_, o)| o)
}

fn edit_distance(a: &str, b: &str) -> usize {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur = vec![0usize; b.len() + 1];
    for i in 1..=a.len() {
        cur[0] = i;
        for j in 1..=b.len() {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            cur[j] = (prev[j] + 1).min(cur[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}

/// Collect every `{placeholder}` appearing in a template string.
fn placeholders(text: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find('{') {
        rest = &rest[start + 1..];
        if let Some(end) = rest.find('}') {
            let name = &rest[..end];
            if !name.is_empty() && !name.contains('{') {
                found.push(name.to_string());
            }
            rest = &rest[end + 1..];
        } else {
            break;
        }
    }
    found
}

fn widget_id(w: &UiWidget) -> String {
    w.id.clone().unwrap_or_default()
}

/// Check a plugin and return everything that looks wrong.
pub fn validate(plugin: &Plugin) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    let err = |location: String, message: String| Diagnostic {
        severity: Severity::Error,
        location,
        message,
    };
    let warn = |location: String, message: String| Diagnostic {
        severity: Severity::Warning,
        location,
        message,
    };

    // ---- widgets -------------------------------------------------------
    let mut seen_ids: Vec<String> = Vec::new();
    for (i, w) in plugin.ui.iter().enumerate() {
        let id = widget_id(w);
        let at = if id.is_empty() {
            format!("widget #{}", i + 1)
        } else {
            format!("widget '{}'", id)
        };

        if !KNOWN_WIDGETS.contains(&w.widget_type.as_str()) {
            let hint = closest(&w.widget_type, KNOWN_WIDGETS)
                .map(|s| format!(" (did you mean '{}'?)", s))
                .unwrap_or_default();
            out.push(err(
                at.clone(),
                format!(
                    "unknown widget type '{}'{}. Available: {}.",
                    w.widget_type,
                    hint,
                    KNOWN_WIDGETS.join(", ")
                ),
            ));
        }

        if id.is_empty() {
            // A notice only shows text, so it has no value to refer to.
            if w.widget_type != "notice" {
                out.push(err(
                    at.clone(),
                    "missing 'id'. Every widget needs a unique id so the pipeline can use its value."
                        .to_string(),
                ));
            }
        } else if seen_ids.contains(&id) {
            out.push(err(at.clone(), format!("duplicate id '{}'. Two widgets cannot share an id — the second one silently wins.", id)));
        } else {
            seen_ids.push(id.clone());
        }

        for key in w.extra.keys() {
            let hint = closest(key, WIDGET_FIELDS)
                .map(|s| format!(" (did you mean '{}'?)", s))
                .unwrap_or_default();
            out.push(err(
                at.clone(),
                format!("unknown setting '{}'{}.", key, hint),
            ));
        }

        if let Some(color) = &w.color {
            let roles = crate::theme::PLUGIN_ROLES;
            if !roles.contains(&color.as_str()) {
                let hint = closest(color, roles)
                    .map(|s| format!(" (did you mean '{}'?)", s))
                    .unwrap_or_default();
                out.push(err(
                    at.clone(),
                    format!(
                        "unknown color '{}'{}. Use one of: {}.",
                        color,
                        hint,
                        roles.join(", ")
                    ),
                ));
            } else if matches!(w.widget_type.as_str(), "image_canvas" | "html_view") {
                out.push(warn(
                    at.clone(),
                    format!("'color' has no effect on a {}.", w.widget_type),
                ));
            }
        }

        if w.widget_type == "notice" && w.text.as_deref().unwrap_or("").is_empty() {
            out.push(err(at.clone(), "a notice needs 'text'.".to_string()));
        }
        if w.widget_type == "html_view" && w.source.is_none() {
            out.push(err(
                at.clone(),
                "an html_view needs 'source', e.g. source = \"{file.path}\".".to_string(),
            ));
        }

        if w.widget_type == "button" {
            match w.on_click.as_deref() {
                Some("execute_pipeline") => {}
                Some(other) => out.push(err(
                    at.clone(),
                    format!(
                        "unknown on_click '{}'. The only supported action is \"execute_pipeline\".",
                        other
                    ),
                )),
                None => out.push(err(
                    at.clone(),
                    "a button needs on_click = \"execute_pipeline\".".to_string(),
                )),
            }
        }

        if let Some(overwrites) = &w.overwrites {
            for target in overwrites {
                if !plugin.ui.iter().any(|o| widget_id(o) == *target) {
                    out.push(err(
                        at.clone(),
                        format!("overwrites '{}', but no widget has that id.", target),
                    ));
                }
            }
        }

        if matches!(w.widget_type.as_str(), "picker" | "radio_group")
            && w.options.as_ref().map(|o| o.is_empty()).unwrap_or(true)
        {
            out.push(err(
                at.clone(),
                format!("a {} needs a non-empty 'options' list.", w.widget_type),
            ));
        }

        if w.widget_type == "slider"
            && let (Some(min), Some(max)) = (w.min, w.max)
            && min >= max
        {
            out.push(err(
                at.clone(),
                format!("min ({}) must be less than max ({}).", min, max),
            ));
        }
    }

    // ---- known variable names -----------------------------------------
    let mut known: Vec<String> = BUILTIN_VARS.iter().map(|s| s.to_string()).collect();
    if plugin.ui.iter().any(|w| w.widget_type == "file_picker") {
        known.extend(IMAGE_VARS.iter().map(|s| s.to_string()));
    }
    known.extend(seen_ids.iter().cloned());
    for w in plugin.ui.iter().filter(|w| w.widget_type == "image_canvas") {
        known.push(format!("{}.crop_data", widget_id(w)));
    }
    for w in plugin.ui.iter().filter(|w| w.widget_type == "html_view") {
        for ph in placeholders(w.source.as_deref().unwrap_or("")) {
            if !known.contains(&ph) {
                out.push(err(
                    format!("widget '{}'", widget_id(w)),
                    format!(
                        "source '{{{}}}' does not refer to any widget id or built-in variable.",
                        ph
                    ),
                ));
            }
        }
    }

    // ---- tab ------------------------------------------------------------
    if plugin.tab.is_some() && plugin.ui.is_empty() {
        out.push(warn(
            "[tab]".to_string(),
            "this plugin defines a tab but no [[ui]] widgets, so the tab will be empty."
                .to_string(),
        ));
    }
    if !plugin.ui.is_empty() && plugin.tab.is_none() {
        out.push(warn(
            "[[ui]]".to_string(),
            "widgets are defined but there is no [tab] block, so they are never shown.".to_string(),
        ));
    }
    let runs_html = plugin.ui.iter().any(|w| w.widget_type == "html_view");
    if !plugin.ui.is_empty() && !runs_html && !plugin.ui.iter().any(|w| w.widget_type == "button") {
        out.push(warn(
            "[[ui]]".to_string(),
            "no button widget, so there is no way to run the pipeline from the tab.".to_string(),
        ));
    }

    // ---- pipeline -------------------------------------------------------
    // An html_view is the whole point of some plugins; they need no pipeline.
    if !runs_html && (plugin.pipeline.is_empty() || plugin.pipeline.values().all(|v| v.is_empty()))
    {
        out.push(err(
            "[pipeline]".to_string(),
            "no pipeline steps — this plugin cannot do anything.".to_string(),
        ));
    }
    if plugin.pipeline.len() > 1 && !plugin.pipeline.contains_key("default") {
        out.push(err(
            "[pipeline]".to_string(),
            format!(
                "{} pipelines are defined but none is named 'default', so the app cannot tell which to run.",
                plugin.pipeline.len()
            ),
        ));
    }
    if plugin.target.is_none() && plugin.tab.is_none() {
        out.push(err("[plugin]".to_string(), "needs either a 'target' (to appear as a conversion format) or a [tab] block (to appear as a tab).".to_string()));
    }

    for (name, steps) in &plugin.pipeline {
        let mut seen_steps: Vec<i32> = Vec::new();
        for step in steps {
            let at = if plugin.pipeline.len() > 1 {
                format!("pipeline '{}' step {}", name, step.step)
            } else {
                format!("step {}", step.step)
            };

            if seen_steps.contains(&step.step) {
                out.push(warn(
                    at.clone(),
                    format!("duplicate step number {}.", step.step),
                ));
            }
            seen_steps.push(step.step);

            if !KNOWN_ACTIONS.contains(&step.action.as_str()) {
                let hint = closest(&step.action, KNOWN_ACTIONS)
                    .map(|s| format!(" (did you mean '{}'?)", s))
                    .unwrap_or_default();
                out.push(err(
                    at.clone(),
                    format!("unknown action '{}'{}.", step.action, hint),
                ));
            }

            for key in step.extra.keys() {
                let hint = closest(key, STEP_FIELDS)
                    .map(|s| format!(" (did you mean '{}'?)", s))
                    .unwrap_or_default();
                out.push(err(
                    at.clone(),
                    format!("unknown setting '{}'{}.", key, hint),
                ));
            }

            // Required fields per action.
            let missing: &[(&str, bool)] = &match step.action.as_str() {
                "split_bytes" => [
                    ("chunk_size", step.chunk_size.is_none()),
                    ("output_pattern", step.output_pattern.is_none()),
                ],
                "split_csv" => [
                    ("chunk_size", step.chunk_size.is_none()),
                    ("output_pattern", step.output_pattern.is_none()),
                ],
                "write_text" => [
                    ("target_path", step.target_path.is_none()),
                    ("text", step.text.is_none() && step.text_content.is_none()),
                ],
                "exec" => [("program", step.program.is_none()), ("", false)],
                "regex_copy" => [("rules", step.rules.is_none()), ("", false)],
                "resize" => [
                    ("width", step.width.is_none()),
                    ("height", step.height.is_none()),
                ],
                "checksum" => [("algorithm", step.algorithm.is_none()), ("", false)],
                "export_image" => [
                    ("target_file", step.target_file.is_none()),
                    ("output_file", step.output_file.is_none()),
                ],
                "ffmpeg" | "magick" | "pandoc" | "wkhtmltopdf" | "chdman" | "par2" => [
                    ("args", step.args.is_none() && step.value.is_none()),
                    ("", false),
                ],
                _ => [("", false), ("", false)],
            };
            for (field, is_missing) in missing {
                if *is_missing && !field.is_empty() {
                    out.push(err(
                        at.clone(),
                        format!("action '{}' requires '{}'.", step.action, field),
                    ));
                }
            }

            if let Some(cond) = &step.condition
                && let Err(e) = crate::condition::evaluate(cond, &|s| s.to_string())
            {
                out.push(err(at.clone(), format!("invalid condition: {}", e)));
            }

            // Unknown {placeholders}.
            for text in step.template_strings() {
                for ph in placeholders(&text) {
                    if known.contains(&ph) {
                        continue;
                    }
                    let options: Vec<&str> = known.iter().map(|s| s.as_str()).collect();
                    let hint = closest(&ph, &options)
                        .map(|s| format!(" (did you mean '{{{}}}'?)", s))
                        .unwrap_or_default();
                    out.push(err(
                        at.clone(),
                        format!(
                            "'{{{}}}' does not refer to any widget id or built-in variable{}.",
                            ph, hint
                        ),
                    ));
                }
            }
        }
    }

    out
}

/// Field names accepted on a `[[pipeline.*]]` step, for "did you mean" hints.
const STEP_FIELDS: &[&str] = &[
    "step",
    "action",
    "condition",
    "target_file",
    "output_file",
    "target_dir",
    "output_dir",
    "crop_data",
    "value",
    "lut_file",
    "degrees",
    "paths",
    "filter_extensions",
    "width",
    "height",
    "filter_type",
    "x",
    "y",
    "brightness",
    "contrast",
    "format",
    "args",
    "program",
    "argv",
    "destination",
    "chunk_size",
    "output_pattern",
    "pattern",
    "algorithm",
    "message",
    "target_path",
    "text_content",
    "text",
    "executable",
    "rules",
    "fit",
    "max_kb",
];

/// Field names accepted on a `[[ui]]` widget.
const WIDGET_FIELDS: &[&str] = &[
    "type",
    "id",
    "label",
    "filter",
    "text",
    "min",
    "max",
    "default",
    "options",
    "on_click",
    "image_source",
    "lock_aspect_ratio",
    "aspect_ratio_source",
    "show_grid",
    "shade_outside",
    "lock_inside_image",
    "overwrites",
    "show_handles",
    "draggable_selection",
    "color",
    "source",
];

#[cfg(test)]
mod tests {
    use super::*;

    fn check(src: &str) -> Vec<String> {
        let p: Plugin = toml::from_str(src).expect("should parse");
        validate(&p).iter().map(|d| d.to_string()).collect()
    }

    const BASE: &str = r#"
target = "t"
[plugin]
name = "T"
version = "1.0"
[pipeline]
[[pipeline.process]]
step = 1
action = "copy"
"#;

    #[test]
    fn a_correct_plugin_is_clean() {
        assert!(check(BASE).is_empty(), "{:?}", check(BASE));
    }

    #[test]
    fn catches_unknown_action_with_suggestion() {
        let d = check(&BASE.replace(r#"action = "copy""#, r#"action = "cpoy""#));
        assert!(
            d.iter()
                .any(|m| m.contains("unknown action 'cpoy'") && m.contains("did you mean 'copy'")),
            "{d:?}"
        );
    }

    #[test]
    fn catches_misspelled_field() {
        let d = check(&BASE.replace(
            r#"action = "copy""#,
            "action = \"split_bytes\"\nchunk_sze = \"24M\"\noutput_pattern = \"x\"",
        ));
        assert!(
            d.iter()
                .any(|m| m.contains("unknown setting 'chunk_sze'") && m.contains("chunk_size")),
            "{d:?}"
        );
    }

    #[test]
    fn catches_missing_required_field() {
        let d = check(&BASE.replace(r#"action = "copy""#, r#"action = "split_bytes""#));
        assert!(
            d.iter().any(|m| m.contains("requires 'chunk_size'")),
            "{d:?}"
        );
    }

    #[test]
    fn catches_unknown_placeholder() {
        let d = check(&BASE.replace(
            r#"action = "copy""#,
            "action = \"copy\"\ndestination = \"{output.dir}/{no_such}\"",
        ));
        assert!(
            d.iter()
                .any(|m| m.contains("'{no_such}' does not refer to")),
            "{d:?}"
        );
    }

    #[test]
    fn catches_duplicate_widget_ids_and_unknown_type() {
        let src = r#"
target = "t"
[plugin]
name = "T"
version = "1.0"
[tab]
title = "T"
[[ui]]
type = "text_input"
id = "w"
[[ui]]
type = "checkbox"
id = "w"
[[ui]]
type = "button"
id = "go"
on_click = "execute_pipeline"
[pipeline]
[[pipeline.process]]
step = 1
action = "copy"
"#;
        let d = check(src);
        assert!(d.iter().any(|m| m.contains("duplicate id 'w'")), "{d:?}");
        assert!(
            d.iter()
                .any(|m| m.contains("unknown widget type 'checkbox'")),
            "{d:?}"
        );
    }

    #[test]
    fn catches_empty_pipeline_and_no_target() {
        let d = check("[plugin]\nname = \"T\"\nversion = \"1.0\"\n");
        assert!(d.iter().any(|m| m.contains("no pipeline steps")), "{d:?}");
        assert!(
            d.iter().any(|m| m.contains("needs either a 'target'")),
            "{d:?}"
        );
    }

    #[test]
    fn catches_ambiguous_pipelines() {
        let src = BASE.to_string() + "\n[[pipeline.other]]\nstep = 1\naction = \"copy\"\n";
        let d = check(&src);
        assert!(
            d.iter().any(|m| m.contains("none is named 'default'")),
            "{d:?}"
        );
    }

    #[test]
    fn catches_bad_button_and_overwrites() {
        let src = r#"
target = "t"
[plugin]
name = "T"
version = "1.0"
[tab]
title = "T"
[[ui]]
type = "picker"
id = "p"
options = ["a"]
overwrites = ["nope"]
[[ui]]
type = "button"
id = "b"
on_click = "do_magic"
[pipeline]
[[pipeline.process]]
step = 1
action = "copy"
"#;
        let d = check(src);
        assert!(
            d.iter().any(|m| m.contains("unknown on_click 'do_magic'")),
            "{d:?}"
        );
        assert!(d.iter().any(|m| m.contains("overwrites 'nope'")), "{d:?}");
    }

    #[test]
    fn checks_color_roles_notices_and_html_views() {
        let d = check(&format!(
            "{BASE}{}",
            r#"
[tab]
title = "T"
[[ui]]
type = "notice"
text = "This deletes the original."
color = "error"
[[ui]]
type = "button"
id = "go"
label = "Go"
on_click = "execute_pipeline"
color = "eror"
[[ui]]
type = "notice"
"#
        ));
        assert!(
            d.iter()
                .any(|m| m.contains("unknown color 'eror' (did you mean 'error'?)")),
            "{d:?}"
        );
        assert!(
            d.iter().any(|m| m.contains("a notice needs 'text'")),
            "{d:?}"
        );
        assert!(!d.iter().any(|m| m.contains("missing 'id'")), "{d:?}");
    }

    #[test]
    fn an_html_view_plugin_needs_no_pipeline_or_button() {
        let d = check(
            r#"
[plugin]
name = "HTML Runner"
version = "1.0"
[tab]
title = "HTML"
[[ui]]
type = "file_picker"
id = "page"
[[ui]]
type = "html_view"
id = "view"
source = "{file.path}"
"#,
        );
        assert!(d.is_empty(), "{d:?}");

        let d = check(
            r#"
[plugin]
name = "HTML Runner"
version = "1.0"
[tab]
title = "HTML"
[[ui]]
type = "html_view"
id = "view"
source = "{file.paht}"
"#,
        );
        assert!(
            d.iter().any(|m| m.contains("source '{file.paht}'")),
            "{d:?}"
        );
    }

    #[test]
    fn external_actions_are_listed_once() {
        let p: Plugin = toml::from_str(&format!(
            "{BASE}{}",
            r#"
[[pipeline.process]]
step = 2
action = "ffmpeg"
args = "-i {file.path} {output.path}"
[[pipeline.process]]
step = 3
action = "ffmpeg"
args = "-i {file.path} {output.path}"
"#
        ))
        .unwrap();
        assert_eq!(external_actions(&p), vec!["ffmpeg".to_string()]);
    }
}
