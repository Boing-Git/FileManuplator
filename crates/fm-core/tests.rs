fn main() {
    let path = fm_core::paths::plugin_dir();
    let plugins = fm_core::parser::load_plugins_from_dir(&path);
    println!("Loaded: {}", plugins.len());
}
