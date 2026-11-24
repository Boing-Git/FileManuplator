use crate::types::Plugin;
use crate::validate::{Diagnostic, Severity, validate};
use std::fs;
use std::path::{Path, PathBuf};

/// A plugin file that could not be loaded, and why.
#[derive(Debug, Clone)]
pub struct PluginError {
    pub path: PathBuf,
    pub message: String,
}

impl std::fmt::Display for PluginError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.path.display(), self.message)
    }
}

/// A plugin that loaded, together with anything questionable about it.
#[derive(Debug, Clone)]
pub struct LoadedPlugin {
    pub path: PathBuf,
    pub plugin: Plugin,
    pub warnings: Vec<Diagnostic>,
}

/// Everything found in a plugin directory.
#[derive(Debug, Clone, Default)]
pub struct LoadReport {
    pub loaded: Vec<LoadedPlugin>,
    pub failed: Vec<PluginError>,
}

impl LoadReport {
    pub fn plugins(&self) -> Vec<Plugin> {
        self.loaded.iter().map(|l| l.plugin.clone()).collect()
    }

    /// One line per problem, for logging or showing in the UI.
    pub fn problems(&self) -> Vec<String> {
        let mut out: Vec<String> = self.failed.iter().map(|e| e.to_string()).collect();
        for l in &self.loaded {
            let name = l
                .path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            out.extend(l.warnings.iter().map(|w| format!("{}: {}", name, w)));
        }
        out
    }
}

/// Parse and check every `.toml` file in `dir`.
///
/// A file that cannot be parsed, or whose contents contain an error-level
/// problem, is reported in `failed` rather than loaded — running a plugin with
/// an unknown action or a dangling `{placeholder}` only fails later, in a way
/// that is much harder for the author to understand.
pub fn load_plugins_report<P: AsRef<Path>>(dir: P) -> LoadReport {
    let mut report = LoadReport::default();
    let dir_path = dir.as_ref();

    let entries = match fs::read_dir(dir_path) {
        Ok(entries) => entries,
        // A missing plugin directory is normal, not an error.
        Err(_) => return report,
    };

    let mut paths: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("toml"))
        .collect();
    paths.sort();

    for path in paths {
        let content = match fs::read_to_string(&path) {
            Ok(c) => c,
            Err(e) => {
                report.failed.push(PluginError {
                    path,
                    message: format!("could not be read: {e}"),
                });
                continue;
            }
        };

        match parse_plugin(&content) {
            Ok((plugin, warnings)) => report.loaded.push(LoadedPlugin {
                path,
                plugin,
                warnings,
            }),
            Err(message) => report.failed.push(PluginError { path, message }),
        }
    }

    report
}

/// Parse and check one plugin's TOML. Any error-level problem fails it, for
/// the reason given on [`load_plugins_report`]; warnings come back with it.
pub fn parse_plugin(content: &str) -> Result<(Plugin, Vec<Diagnostic>), String> {
    let plugin: Plugin = toml::from_str(content).map_err(|e| e.message().to_string())?;
    let (errors, warnings): (Vec<_>, Vec<_>) = validate(&plugin)
        .into_iter()
        .partition(|d| d.severity == Severity::Error);
    if errors.is_empty() {
        Ok((plugin, warnings))
    } else {
        Err(errors
            .iter()
            .map(|d| d.to_string())
            .collect::<Vec<_>>()
            .join("; "))
    }
}

/// Load the valid plugins from `dir`, logging anything that failed.
pub fn load_plugins_from_dir<P: AsRef<Path>>(dir: P) -> Vec<Plugin> {
    let report = load_plugins_report(dir);
    for problem in report.problems() {
        eprintln!("Plugin problem — {problem}");
    }
    report.plugins()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_plugins() {
        let path = crate::paths::plugin_dir();
        let report = load_plugins_report(&path);
        println!(
            "Loaded {} plugins ({} failed) from {:?}",
            report.loaded.len(),
            report.failed.len(),
            path
        );
    }

    #[test]
    fn a_broken_plugin_is_reported_not_silently_skipped() {
        let dir = std::env::temp_dir().join("fm_parser_test");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("broken.toml"),
            "target = \"t\"\n[plugin]\nname = \"T\"\nversion = \"1\"\n[pipeline]\n[[pipeline.process]]\nstep = 1\naction = \"cpoy\"\n",
        )
        .unwrap();
        fs::write(
            dir.join("fine.toml"),
            "target = \"t2\"\n[plugin]\nname = \"OK\"\nversion = \"1\"\n[pipeline]\n[[pipeline.process]]\nstep = 1\naction = \"copy\"\n",
        )
        .unwrap();

        let report = load_plugins_report(&dir);
        assert_eq!(report.loaded.len(), 1);
        assert_eq!(report.failed.len(), 1);
        assert!(report.failed[0].message.contains("unknown action 'cpoy'"));
        let _ = fs::remove_dir_all(&dir);
    }
}
