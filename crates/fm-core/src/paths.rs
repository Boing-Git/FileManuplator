use std::path::PathBuf;

/// Directory user plugins are loaded from: `~/.local/share/filemanuplator/plugins`.
pub fn plugin_dir() -> PathBuf {
    let mut path = dirs::data_local_dir().unwrap_or_default();
    path.push("filemanuplator");
    path.push("plugins");
    path
}

/// Directory used to cache marketplace metadata.
pub fn marketplace_cache_dir() -> PathBuf {
    let mut path = dirs::cache_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
    path.push("filemanuplator");
    path.push("marketplace_meta");
    path
}
