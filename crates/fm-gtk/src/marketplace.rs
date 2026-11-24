use adw::prelude::*;
use fm_core::types::Plugin;
use gtk4 as gtk;
use libadwaita as adw;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::Read;
use std::rc::Rc;
use std::thread;

#[derive(Serialize, Deserialize, Clone)]
struct RemotePlugin {
    file_name: String,
    target: String,
    description: String,
    mime_types: Vec<String>,
    content: String,
}

pub fn build_marketplace_page(nav_view: adw::NavigationView) -> adw::NavigationPage {
    let toolbar = adw::ToolbarView::new();
    let header = adw::HeaderBar::new();
    let title_w = adw::WindowTitle::new("Plugins & Marketplace", "");
    header.set_title_widget(Some(&title_w));

    // Add back button
    let back_btn = gtk::Button::from_icon_name("go-previous-symbolic");
    let nav_view_c = nav_view.clone();
    back_btn.connect_clicked(move |_| {
        nav_view_c.pop();
    });
    header.pack_start(&back_btn);

    toolbar.add_top_bar(&header);

    let scroller = gtk::ScrolledWindow::new();
    scroller.set_vexpand(true);
    let clamp = adw::Clamp::new();
    clamp.set_maximum_size(800);

    let outer = gtk::Box::new(gtk::Orientation::Vertical, 18);
    outer.set_margin_top(18);
    outer.set_margin_bottom(24);
    outer.set_margin_start(18);
    outer.set_margin_end(18);

    clamp.set_child(Some(&outer));
    scroller.set_child(Some(&clamp));
    toolbar.set_content(Some(&scroller));

    let page = adw::NavigationPage::new(&toolbar, "marketplace");

    let cache_dir = fm_core::paths::marketplace_cache_dir();
    let user_dir = fm_core::paths::plugin_dir();

    fs::create_dir_all(&cache_dir).ok();
    fs::create_dir_all(&user_dir).ok();

    let outer_box = Rc::new(outer);
    let (ui_tx, ui_rx) = async_channel::unbounded::<()>();
    let is_loading = Rc::new(std::cell::Cell::new(true));

    // Refresh function
    let refresh = {
        let outer_box = outer_box.clone();
        let cache_dir = cache_dir.clone();
        let user_dir = user_dir.clone();
        let ui_tx = ui_tx.clone();
        let is_loading = is_loading.clone();

        Rc::new(move || {
            // Clear outer box
            while let Some(child) = outer_box.first_child() {
                outer_box.remove(&child);
            }

            // Surface plugins that failed to load, with the reason, so an author
            // is not left guessing why their file did nothing.
            let report = fm_core::parser::load_plugins_report(&user_dir);
            if !report.failed.is_empty() || report.loaded.iter().any(|l| !l.warnings.is_empty()) {
                let problems = adw::PreferencesGroup::new();
                problems.set_title("Plugins with problems");
                for failure in &report.failed {
                    let name = failure
                        .path
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    let row = adw::ActionRow::builder()
                        .title(gtk::glib::markup_escape_text(&name))
                        .subtitle(gtk::glib::markup_escape_text(&failure.message))
                        .build();
                    row.set_subtitle_lines(0);
                    row.add_prefix(&gtk::Image::from_icon_name("dialog-error-symbolic"));
                    problems.add(&row);
                }
                for loaded in &report.loaded {
                    for warning in &loaded.warnings {
                        let name = loaded
                            .path
                            .file_name()
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_default();
                        let row = adw::ActionRow::builder()
                            .title(gtk::glib::markup_escape_text(&name))
                            .subtitle(gtk::glib::markup_escape_text(&warning.to_string()))
                            .build();
                        row.set_subtitle_lines(0);
                        row.add_prefix(&gtk::Image::from_icon_name("dialog-warning-symbolic"));
                        problems.add(&row);
                    }
                }
                outer_box.append(&problems);
            }

            let pref_group = adw::PreferencesGroup::new();
            let mut count = 0;

            let mut installed_files = HashMap::new();
            if let Ok(entries) = fs::read_dir(&user_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                    if (ext == "yaml" || ext == "yml" || ext == "toml")
                        && let Some(name) = path.file_name().and_then(|n| n.to_str())
                    {
                        installed_files.insert(name.to_string(), path);
                    }
                }
            }

            let mut marketplace_files = HashMap::new();
            if let Ok(content) = fs::read_to_string(cache_dir.join("plugins.json"))
                && let Ok(entries) = serde_json::from_str::<Vec<RemotePlugin>>(&content)
            {
                for e in entries {
                    marketplace_files.insert(e.file_name.clone(), e);
                }
            }

            let mut all_names: HashSet<String> = HashSet::new();
            for k in installed_files.keys() {
                all_names.insert(k.clone());
            }
            for k in marketplace_files.keys() {
                all_names.insert(k.clone());
            }

            let mut all_names_sorted: Vec<_> = all_names.into_iter().collect();
            all_names_sorted.sort();

            for plugin_name in all_names_sorted {
                let is_installed = installed_files.contains_key(&plugin_name);

                let (target, desc, mimes) = if is_installed {
                    let file_path = installed_files.get(&plugin_name).unwrap();
                    let content = fs::read_to_string(file_path).unwrap_or_default();
                    if let Ok(p) = toml::from_str::<Plugin>(&content) {
                        let t = p.target.unwrap_or_else(|| plugin_name.clone());
                        let d = p.plugin.description;
                        let m = if p.mime_types.is_empty() {
                            vec!["*/*".to_string()]
                        } else {
                            p.mime_types
                        };
                        (t, d, m)
                    } else {
                        continue;
                    }
                } else if let Some(e) = marketplace_files.get(&plugin_name) {
                    (
                        e.target.clone(),
                        e.description.clone(),
                        e.mime_types.clone(),
                    )
                } else {
                    continue;
                };

                let mut mimes_str = mimes.join(", ");
                if mimes_str.len() > 40 {
                    mimes_str = format!("{}...", &mimes_str[..37]);
                }

                let desc_escaped = gtk::glib::markup_escape_text(&desc);
                let card = adw::ActionRow::builder()
                    .title(format!("Target: {}", target))
                    .subtitle(desc_escaped)
                    .build();

                let badge = gtk::Label::new(Some(&mimes_str));
                badge.add_css_class("pill");
                badge.set_valign(gtk::Align::Center);
                badge.set_margin_end(12);

                let box_ = gtk::Box::new(gtk::Orientation::Horizontal, 0);
                box_.append(&badge);

                let action_btn = gtk::Button::new();
                action_btn.set_valign(gtk::Align::Center);

                if is_installed {
                    action_btn.set_label("Uninstall");
                    action_btn.add_css_class("destructive-action");
                    let fname = plugin_name.clone();
                    let udir = user_dir.clone();
                    let tx_c = ui_tx.clone();
                    action_btn.connect_clicked(move |_| {
                        let _ = fs::remove_file(udir.join(&fname));
                        let _ = tx_c.send_blocking(());
                    });
                } else {
                    action_btn.set_label("Install");
                    action_btn.add_css_class("suggested-action");
                    let fname = plugin_name.clone();
                    let udir = user_dir.clone();
                    let remote_content = marketplace_files
                        .get(&plugin_name)
                        .map(|r| r.content.clone());
                    let tx_c = ui_tx.clone();
                    action_btn.connect_clicked(move |_| {
                        if let Some(content) = &remote_content {
                            let _ = fs::write(udir.join(&fname), content);
                            let _ = tx_c.send_blocking(());
                        }
                    });
                }

                box_.append(&action_btn);
                card.add_suffix(&box_);
                card.add_prefix(&gtk::Image::from_icon_name("application-x-addon"));

                pref_group.add(&card);
                count += 1;
            }

            if count == 0 {
                if is_loading.get() {
                    let spinner = gtk::Spinner::new();
                    spinner.set_halign(gtk::Align::Center);
                    spinner.set_valign(gtk::Align::Center);
                    spinner.set_size_request(48, 48);
                    spinner.start();

                    let box_ = gtk::Box::new(gtk::Orientation::Vertical, 12);
                    box_.set_valign(gtk::Align::Center);
                    box_.set_vexpand(true);
                    box_.append(&spinner);

                    let label = gtk::Label::new(Some("Fetching marketplace plugins..."));
                    box_.append(&label);

                    outer_box.append(&box_);
                } else {
                    let empty = adw::StatusPage::builder()
                        .title("No Plugins Found")
                        .description("No plugins were found in the marketplace.")
                        .icon_name("edit-clear-symbolic")
                        .build();
                    outer_box.append(&empty);
                }
            } else {
                outer_box.append(&pref_group);
            }
        })
    };

    // Initial draw
    refresh();

    let (fetch_tx, fetch_rx) = async_channel::unbounded::<()>();
    let c_dir = cache_dir.clone();

    thread::spawn(move || {
        let result = (|| -> Result<(), Box<dyn std::error::Error>> {
            let req = ureq::get(
                "https://github.com/Boing-Git/FileManuplator-Plugins/archive/refs/heads/main.zip",
            )
            .header("User-Agent", "FileManipulator")
            .call()?;

            let mut buf = Vec::new();
            req.into_body().into_reader().read_to_end(&mut buf)?;

            let cursor = std::io::Cursor::new(buf);
            let mut archive = zip::ZipArchive::new(cursor)?;

            let mut plugins = Vec::new();

            for i in 0..archive.len() {
                let mut file = archive.by_index(i)?;
                if file.is_file()
                    && let Some(path) = file.enclosed_name()
                {
                    let name = path
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned();
                    if name.ends_with(".toml") {
                        let mut toml_text = String::new();
                        if file.read_to_string(&mut toml_text).is_ok()
                            && let Ok(p) = toml::from_str::<Plugin>(&toml_text)
                        {
                            plugins.push(RemotePlugin {
                                file_name: name.clone(),
                                target: p.target.unwrap_or_else(|| name.clone()),
                                description: p.plugin.description.clone(),
                                mime_types: if p.mime_types.is_empty() {
                                    vec!["*/*".to_string()]
                                } else {
                                    p.mime_types.clone()
                                },
                                content: toml_text,
                            });
                        }
                    }
                }
            }

            let plugins_json = serde_json::to_string(&plugins)?;
            fs::create_dir_all(&c_dir)?;
            fs::write(c_dir.join("plugins.json"), plugins_json)?;

            Ok::<(), Box<dyn std::error::Error>>(())
        })();

        if let Err(e) = result {
            eprintln!("Marketplace fetch error: {}", e);
        }

        let _ = fetch_tx.send_blocking(());
    });

    let refresh_c1 = Rc::clone(&refresh);
    gtk::glib::MainContext::default().spawn_local(async move {
        if fetch_rx.recv().await.is_ok() {
            is_loading.set(false);
            refresh_c1();
        }
    });

    let refresh_c2 = Rc::clone(&refresh);
    gtk::glib::MainContext::default().spawn_local(async move {
        while ui_rx.recv().await.is_ok() {
            refresh_c2();
        }
    });

    page
}
