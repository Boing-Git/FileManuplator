use fm_core::config;
use fm_core::converter::{resolve_binary, run_conversion};
use fm_core::detector::detect_mime_type;
use fm_core::engine::execute_pipeline;
use fm_core::parser::load_plugins_from_dir;
use fm_core::types::{ConversionResult, ConversionRoute, FileContext, Plugin};

mod appearance;
mod marketplace;
mod phone_preview;

use adw::prelude::*;
use gtk4 as gtk;
use libadwaita as adw;

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::thread;

// ---------------------------------------------------------------------------
//  Application State
// ---------------------------------------------------------------------------

struct AppState {
    // Converter state
    current_file: Option<PathBuf>,
    current_mime: Option<String>,
    current_route: Option<ConversionRoute>,
    treated_as_markdown: bool,
    binary_error: Option<String>,
    selected_target: Option<String>,
    /// Plugin targets for the current MIME type: target_name -> plugin
    plugin_targets: HashMap<String, Plugin>,
    /// All loaded TOML plugins
    plugins: Vec<Plugin>,
}

impl AppState {
    fn new() -> Self {
        let plugin_path = fm_core::paths::plugin_dir();
        let plugins = load_plugins_from_dir(&plugin_path);
        eprintln!("Loaded {} plugins from {:?}", plugins.len(), plugin_path);

        Self {
            current_file: None,
            current_mime: None,
            current_route: None,
            treated_as_markdown: false,
            binary_error: None,
            selected_target: None,
            plugin_targets: HashMap::new(),
            plugins,
        }
    }

    fn reload_plugins(&mut self) {
        self.plugins = load_plugins_from_dir(fm_core::paths::plugin_dir());
    }

    /// Collect all available target formats for the current file.
    fn all_targets(&self) -> Vec<(String, String)> {
        let mut targets = Vec::new();
        if let Some(route) = &self.current_route {
            for t in &route.targets {
                targets.push((t.clone(), route.tool.to_string()));
            }
        }
        for t in self.plugin_targets.keys() {
            if !targets.iter().any(|(name, _)| name == t) {
                targets.push((t.clone(), "plugin".to_string()));
            }
        }
        targets
    }
}

// ---------------------------------------------------------------------------
//  Utilities
// ---------------------------------------------------------------------------

fn human_size(bytes: u64) -> String {
    let mut size = bytes as f64;
    for unit in &["B", "KB", "MB", "GB"] {
        if size < 1024.0 || *unit == "GB" {
            return if *unit == "B" {
                format!("{:.0} {}", size, unit)
            } else {
                format!("{:.1} {}", size, unit)
            };
        }
        size /= 1024.0;
    }
    format!("{:.1} TB", size)
}

fn icon_for_mime(mime: &str) -> &'static str {
    if mime.starts_with("image/") {
        return "image-x-generic";
    }
    if mime.starts_with("video/") {
        return "video-x-generic";
    }
    if mime.starts_with("audio/") {
        return "audio-x-generic";
    }
    if mime.contains("wordprocessingml")
        || mime == "application/rtf"
        || mime == "application/epub+zip"
    {
        return "x-office-document";
    }
    if mime == "text/html" {
        return "text-html";
    }
    if mime.starts_with("text/") {
        return "text-x-generic";
    }
    "application-x-generic"
}

// ---------------------------------------------------------------------------
//  Main
// ---------------------------------------------------------------------------

fn main() {
    let app = adw::Application::builder()
        .application_id("dev.filemanuplator.Converter")
        .build();
    app.connect_activate(build_ui);
    app.run();
}

fn build_ui(app: &adw::Application) {
    let state = Rc::new(RefCell::new(AppState::new()));

    // Load UI from XML
    let builder = gtk::Builder::from_string(include_str!("main.ui"));

    let window: adw::ApplicationWindow = builder.object("window").expect("Could not find window");
    window.set_application(Some(app));

    let pages: gtk::Stack = builder.object("pages").expect("Could not find pages");
    let nav_bar: gtk::Box = builder.object("nav_bar").expect("Could not find nav_bar");
    let plugins_btn: gtk::Button = builder
        .object("plugins_btn")
        .expect("Could not find plugins_btn");
    let toast_overlay: adw::ToastOverlay = builder
        .object("toast_overlay")
        .expect("Could not find toast_overlay");
    let header_bar: adw::HeaderBar = builder
        .object("header_bar")
        .expect("Could not find header_bar");

    // Colours first: the desktop accent is read before our own CSS exists.
    let theme = appearance::Theme::install(&window);
    header_bar.pack_end(&appearance::picker_button(&theme));

    // Home / Converter Page
    let nav_view = adw::NavigationView::new();
    let main_page = build_main_page(&state, &toast_overlay, &nav_view);
    nav_view.add(&main_page);

    // Marketplace btn opens in NavView inside the first tab
    let nav_view_c = nav_view.clone();
    plugins_btn.connect_clicked(move |_| {
        let mkt_page = marketplace::build_marketplace_page(nav_view_c.clone());
        nav_view_c.push(&mkt_page);
    });

    // Add Main Converter Tab
    add_destination(
        &pages,
        &nav_bar,
        &nav_view,
        "Converter",
        "view-refresh-symbolic",
    );

    // Add Plugin Tabs
    {
        let s = state.borrow();
        for plugin in &s.plugins {
            if let Some(tab_def) = &plugin.tab {
                let plugin_page = build_plugin_tab(&state, &toast_overlay, plugin);
                let icon = tab_def
                    .icon
                    .as_deref()
                    .unwrap_or("application-x-addon-symbolic");
                add_destination(&pages, &nav_bar, &plugin_page, &tab_def.title, icon);
            }
        }
    }

    // M3 Expressive components, plus each plugin colour role (roles.css is a
    // template where ROLE stands for one role name).
    let mut stylesheet = include_str!("style.css").to_string();
    for role in fm_core::theme::PLUGIN_ROLES {
        stylesheet.push_str(&include_str!("roles.css").replace("ROLE", role));
    }
    let css = gtk::CssProvider::new();
    css.load_from_string(&stylesheet);
    gtk::style_context_add_provider_for_display(
        &gtk::prelude::WidgetExt::display(&window),
        &css,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );

    window.present();
}

/// One M3 navigation-bar destination: a pill indicator behind the icon, the
/// label below, and Alt+N to jump to it (replaces AdwTabView's shortcut).
fn add_destination(
    pages: &gtk::Stack,
    nav_bar: &gtk::Box,
    child: &impl IsA<gtk::Widget>,
    title: &str,
    icon: &str,
) {
    pages.add_child(child);

    // The pill grows from the centre (CSS animates its min-width).
    let pill = gtk::Box::builder()
        .css_classes(["nav-pill"])
        .halign(gtk::Align::Center)
        .build();
    let indicator = gtk::Overlay::builder()
        .css_classes(["nav-indicator"])
        .halign(gtk::Align::Center)
        .child(&pill)
        .build();
    let image = gtk::Image::from_icon_name(icon);
    image.set_pixel_size(24);
    indicator.add_overlay(&image);

    let content = gtk::Box::new(gtk::Orientation::Vertical, 4);
    content.append(&indicator);
    content.append(&gtk::Label::new(Some(title)));

    let btn = gtk::ToggleButton::builder()
        .child(&content)
        .css_classes(["nav-item"])
        // Clicking a destination leaves keyboard focus in the page's content.
        .focus_on_click(false)
        .build();
    let index = nav_bar.observe_children().n_items();
    match nav_bar.first_child().and_downcast::<gtk::ToggleButton>() {
        Some(first) => btn.set_group(Some(&first)),
        None => btn.set_active(true),
    }

    let (pages_c, child_w) = (pages.clone(), child.clone().upcast::<gtk::Widget>());
    btn.connect_toggled(move |b| {
        if b.is_active() {
            pages_c.set_visible_child(&child_w);
        }
    });

    if index < 9 {
        let shortcuts = gtk::ShortcutController::new();
        shortcuts.set_scope(gtk::ShortcutScope::Global);
        shortcuts.add_shortcut(gtk::Shortcut::new(
            gtk::ShortcutTrigger::parse_string(&format!("<Alt>{}", index + 1)),
            Some(gtk::ActivateAction::get()),
        ));
        btn.add_controller(shortcuts);
    }

    nav_bar.append(&btn);
}

// ---------------------------------------------------------------------------
//  Main Convert Page
// ---------------------------------------------------------------------------

fn build_main_page(
    state: &Rc<RefCell<AppState>>,
    toast_overlay: &adw::ToastOverlay,
    _nav_view: &adw::NavigationView,
) -> adw::NavigationPage {
    // Scrollable content
    let scroller = gtk::ScrolledWindow::new();
    scroller.set_vexpand(true);
    let clamp = adw::Clamp::new();
    clamp.set_maximum_size(520);

    let root_box = gtk::Box::new(gtk::Orientation::Vertical, 20);
    root_box.set_margin_top(8);
    root_box.set_margin_bottom(32);
    root_box.set_margin_start(18);
    root_box.set_margin_end(18);
    root_box.set_valign(gtk::Align::Center);

    // -- Drop zone / file card stack --
    let file_stack = gtk::Stack::new();
    file_stack.set_transition_type(gtk::StackTransitionType::None);
    file_stack.set_vhomogeneous(false);

    // Empty state: drop zone
    let drop_status = adw::StatusPage::new();
    drop_status.set_icon_name(Some("document-open-symbolic"));
    drop_status.set_title("Drop a file here");
    drop_status.set_description(Some("or click to browse"));

    let drop_btn = gtk::Button::new();
    drop_btn.set_child(Some(&drop_status));
    drop_btn.add_css_class("card");
    drop_btn.set_size_request(-1, 220);

    file_stack.add_named(&drop_btn, Some("empty"));

    // File card (shown after file is loaded)
    let file_card = gtk::Box::new(gtk::Orientation::Vertical, 14);
    file_card.add_css_class("card");
    file_card.set_margin_top(4);
    file_card.set_margin_bottom(4);
    file_card.set_margin_start(4);
    file_card.set_margin_end(4);

    let top_row = gtk::Box::new(gtk::Orientation::Horizontal, 14);
    top_row.set_margin_top(16);
    top_row.set_margin_start(16);
    top_row.set_margin_end(16);
    top_row.set_margin_bottom(16);

    let file_icon = gtk::Image::from_icon_name("text-x-generic");
    file_icon.set_pixel_size(40);
    top_row.append(&file_icon);

    let text_box = gtk::Box::new(gtk::Orientation::Vertical, 2);
    text_box.set_hexpand(true);
    let file_name_label = gtk::Label::new(None);
    file_name_label.add_css_class("title-4");
    file_name_label.set_xalign(0.0);
    file_name_label.set_wrap(true);
    let file_mime_label = gtk::Label::new(None);
    file_mime_label.add_css_class("dim-label");
    file_mime_label.add_css_class("caption");
    file_mime_label.set_xalign(0.0);
    text_box.append(&file_name_label);
    text_box.append(&file_mime_label);
    top_row.append(&text_box);

    let change_btn = gtk::Button::from_icon_name("document-open-recent-symbolic");
    change_btn.add_css_class("flat");
    change_btn.set_valign(gtk::Align::Center);
    change_btn.set_tooltip_text(Some("Choose a different file"));
    top_row.append(&change_btn);

    file_card.append(&top_row);
    file_stack.add_named(&file_card, Some("file"));

    root_box.append(&file_stack);

    // -- Format selection group --
    let format_group = adw::PreferencesGroup::new();
    format_group.set_title("Convert to");
    format_group.set_visible(false);
    root_box.append(&format_group);

    // -- Binary error banner --
    let binary_banner = adw::Banner::new("");
    binary_banner.set_revealed(false);
    root_box.append(&binary_banner);

    // -- Save As button --
    let save_btn = gtk::Button::new();
    let save_content = adw::ButtonContent::new();
    save_content.set_icon_name("document-save-as-symbolic");
    save_content.set_label("Save As...");
    save_btn.set_child(Some(&save_content));
    save_btn.add_css_class("suggested-action");
    save_btn.add_css_class("pill");
    save_btn.set_sensitive(false);
    root_box.append(&save_btn);

    // -- Progress --
    let progress_box = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    progress_box.set_halign(gtk::Align::Center);
    let progress_spinner = gtk::Spinner::new();
    let progress_label = gtk::Label::new(None);
    progress_label.add_css_class("dim-label");
    progress_box.append(&progress_spinner);
    progress_box.append(&progress_label);
    progress_box.set_visible(false);
    root_box.append(&progress_box);

    // -- Error group --
    let error_group = adw::PreferencesGroup::new();
    error_group.set_title("Conversion failed");
    error_group.set_visible(false);
    root_box.append(&error_group);

    clamp.set_child(Some(&root_box));
    scroller.set_child(Some(&clamp));

    let page = adw::NavigationPage::builder()
        .title("Converter")
        .child(&scroller)
        .build();

    let toggle_buttons: Rc<RefCell<Vec<gtk::ToggleButton>>> = Rc::new(RefCell::new(Vec::new()));
    let format_group_children: Rc<RefCell<Vec<gtk::Widget>>> = Rc::new(RefCell::new(Vec::new()));
    let error_group_children: Rc<RefCell<Vec<gtk::Widget>>> = Rc::new(RefCell::new(Vec::new()));

    // ---- File loading logic ----
    let load_file = {
        let state = Rc::clone(state);
        let file_stack = file_stack.clone();
        let file_icon = file_icon.clone();
        let file_name_label = file_name_label.clone();
        let file_mime_label = file_mime_label.clone();
        let format_group = format_group.clone();
        let binary_banner = binary_banner.clone();
        let save_btn = save_btn.clone();
        let error_group = error_group.clone();
        let toggle_buttons = Rc::clone(&toggle_buttons);
        let format_group_children = Rc::clone(&format_group_children);

        Rc::new(move |path: PathBuf| {
            if !path.is_file() {
                return;
            }

            let mime = detect_mime_type(&path);
            let (route, treated_as_md) = config::resolve_route(&mime, &path);

            let mut s = state.borrow_mut();
            s.reload_plugins();

            // Find plugin targets for this MIME type
            s.plugin_targets.clear();
            let plugins = s.plugins.clone();
            for plugin in &plugins {
                let plugin_mimes = if plugin.mime_types.is_empty() {
                    vec!["*/*".to_string()]
                } else {
                    plugin.mime_types.clone()
                };
                let matches_mime = plugin_mimes.iter().any(|m| m == &mime || m == "*/*");

                let matches_ext = if let Some(exts) = &plugin.extensions {
                    let file_ext = path
                        .extension()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string();
                    let path_str = path.to_string_lossy().to_string();
                    exts.iter().any(|e| {
                        let e_clean = e.trim_start_matches('.');
                        if e_clean == file_ext || path_str.ends_with(e) {
                            return true;
                        }
                        if let Some(idx) = path_str.rfind(e) {
                            let remainder = &path_str[idx + e.len()..];
                            !remainder.is_empty() && remainder.chars().all(|c| c.is_ascii_digit())
                        } else {
                            false
                        }
                    })
                } else {
                    true
                };

                if let Some(target) = &plugin.target
                    && matches_mime
                    && matches_ext
                {
                    s.plugin_targets.insert(target.clone(), plugin.clone());
                }
            }

            // Check binary availability
            s.binary_error = None;
            if let Some(ref route) = route
                && let Err(msg) = resolve_binary(route)
            {
                s.binary_error = Some(msg);
            }

            s.current_file = Some(path.clone());
            s.current_mime = Some(mime.clone());
            s.current_route = route;
            s.treated_as_markdown = treated_as_md;
            s.selected_target = None;

            // Update file card
            file_icon.set_icon_name(Some(icon_for_mime(&mime)));
            file_name_label.set_label(
                path.file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .as_ref(),
            );
            let size = path
                .metadata()
                .map(|m| human_size(m.len()))
                .unwrap_or("unknown".into());
            file_mime_label.set_label(&format!("{} · {}", mime, size));
            file_stack.set_visible_child_name("file");

            // Update format group
            let all_targets = s.all_targets();

            // Clear old children from format_group
            let mut children = format_group_children.borrow_mut();
            for child in children.drain(..) {
                format_group.remove(&child);
            }

            if all_targets.is_empty() {
                format_group.set_visible(false);
                save_btn.set_sensitive(false);
                binary_banner.set_revealed(false);
                return;
            }

            format_group.set_visible(true);
            if let Some(ref route) = s.current_route {
                if s.plugin_targets.is_empty() {
                    format_group.set_description(Some(&format!("via {}", route.tool)));
                } else {
                    format_group.set_description(Some(&format!("via {} or plugins", route.tool)));
                }
            } else {
                format_group.set_description(Some("via plugins"));
            }

            // Build format pill buttons
            let flow = gtk::FlowBox::new();
            flow.set_selection_mode(gtk::SelectionMode::None);
            flow.set_max_children_per_line(6);
            flow.set_row_spacing(8);
            flow.set_column_spacing(8);
            flow.set_margin_top(4);
            flow.set_margin_bottom(4);

            let mut btns = Vec::new();
            let mut first: Option<gtk::ToggleButton> = None;

            for (fmt, _engine) in &all_targets {
                let btn = gtk::ToggleButton::with_label(&format!(".{}", fmt));
                btn.add_css_class("pill");

                let state_c = Rc::clone(&state);
                let save_btn_c = save_btn.clone();
                let fmt_c = fmt.clone();
                btn.connect_toggled(move |b| {
                    if b.is_active() {
                        let mut s = state_c.borrow_mut();
                        s.selected_target = Some(fmt_c.clone());
                        let is_plugin = s.plugin_targets.contains_key(&fmt_c);
                        let ready =
                            s.current_file.is_some() && (s.binary_error.is_none() || is_plugin);
                        save_btn_c.set_sensitive(ready);
                    }
                });

                if first.is_none() {
                    first = Some(btn.clone());
                }
                flow.append(&btn);
                btns.push(btn);
            }

            // Group the toggle buttons
            if let Some(ref f) = first {
                for b in &btns[1..] {
                    b.set_group(Some(f));
                }
            }

            let wrapper = gtk::ListBoxRow::new();
            wrapper.set_selectable(false);
            wrapper.set_activatable(false);
            wrapper.set_child(Some(&flow));
            format_group.add(&wrapper);
            children.push(wrapper.upcast::<gtk::Widget>());

            *toggle_buttons.borrow_mut() = btns;

            // Show binary error
            if let Some(ref err) = s.binary_error {
                binary_banner.set_title(err);
                binary_banner.set_revealed(true);
            } else {
                binary_banner.set_revealed(false);
            }

            error_group.set_visible(false);
            drop(s);

            // Auto-select first format
            if let Some(ref f) = first {
                f.set_active(true);
            }
        })
    };

    // Connect browse button
    {
        let load_file = Rc::clone(&load_file);
        drop_btn.connect_clicked(move |_| {
            let dialog = gtk::FileDialog::new();
            dialog.set_title("Choose a file to convert");
            let lf = Rc::clone(&load_file);
            dialog.open(
                None::<&gtk::Window>,
                None::<&gtk::gio::Cancellable>,
                move |result| {
                    if let Ok(file) = result
                        && let Some(path) = file.path()
                    {
                        lf(path);
                    }
                },
            );
        });
    }

    // Connect change button
    {
        let load_file = Rc::clone(&load_file);
        change_btn.connect_clicked(move |_| {
            let dialog = gtk::FileDialog::new();
            dialog.set_title("Choose a file to convert");
            let lf = Rc::clone(&load_file);
            dialog.open(
                None::<&gtk::Window>,
                None::<&gtk::gio::Cancellable>,
                move |result| {
                    if let Ok(file) = result
                        && let Some(path) = file.path()
                    {
                        lf(path);
                    }
                },
            );
        });
    }

    // Connect Save As button
    {
        let state = Rc::clone(state);
        let toast_overlay = toast_overlay.clone();
        let save_btn_c = save_btn.clone();
        let progress_box = progress_box.clone();
        let progress_spinner = progress_spinner.clone();
        let progress_label = progress_label.clone();
        let error_group = error_group.clone();

        save_btn.connect_clicked(move |_| {
            let s = state.borrow();
            let input = match &s.current_file {
                Some(f) => f.clone(),
                None => return,
            };
            let target = match &s.selected_target {
                Some(t) => t.clone(),
                None => return,
            };
            drop(s);

            let dialog = gtk::FileDialog::new();
            dialog.set_title("Save converted file as");

            let is_plugin = {
                let s = state.borrow();
                s.plugin_targets.contains_key(&target)
            };

            let suggested = if is_plugin {
                if target.contains("join") || target.contains("split") {
                    // For split/join, suggest the file stem (stripping .part00) or original file
                    input.with_extension("")
                } else {
                    input.clone()
                }
            } else {
                input.with_extension(&target)
            };

            if let Some(parent) = suggested.parent() {
                dialog.set_initial_folder(Some(&gtk::gio::File::for_path(parent)));
            }
            if let Some(name) = suggested.file_name() {
                let mut name_str = name.to_string_lossy().to_string();
                if name_str.ends_with('.') {
                    name_str.pop();
                }
                dialog.set_initial_name(Some(&name_str));
            }

            let state_c = Rc::clone(&state);
            let toast_c = toast_overlay.clone();
            let save_btn_c2 = save_btn_c.clone();
            let pbox = progress_box.clone();
            let pspinner = progress_spinner.clone();
            let plabel = progress_label.clone();
            let egrp = error_group.clone();
            let egrp_children = error_group_children.clone();

            dialog.save(
                None::<&gtk::Window>,
                None::<&gtk::gio::Cancellable>,
                move |result| {
                    if let Ok(file) = result
                        && let Some(out_path) = file.path()
                    {
                        start_conversion(
                            &state_c,
                            &input,
                            &out_path,
                            &target,
                            &ConversionWidgets {
                                toast_overlay: &toast_c,
                                save_btn: &save_btn_c2,
                                progress_box: &pbox,
                                progress_spinner: &pspinner,
                                progress_label: &plabel,
                                error_group: &egrp,
                                error_group_children: &egrp_children,
                            },
                        );
                    }
                },
            );
        });
    }

    page
}

// ---------------------------------------------------------------------------
//  Conversion
// ---------------------------------------------------------------------------

struct ConversionWidgets<'a> {
    toast_overlay: &'a adw::ToastOverlay,
    save_btn: &'a gtk::Button,
    progress_box: &'a gtk::Box,
    progress_spinner: &'a gtk::Spinner,
    progress_label: &'a gtk::Label,
    error_group: &'a adw::PreferencesGroup,
    error_group_children: &'a Rc<RefCell<Vec<gtk::Widget>>>,
}

fn start_conversion(
    state: &Rc<RefCell<AppState>>,
    input_path: &Path,
    output_path: &Path,
    target: &str,
    widgets: &ConversionWidgets,
) {
    let s = state.borrow();
    let is_plugin = s.plugin_targets.contains_key(target);
    let route = s.current_route.clone();
    let mime = s.current_mime.clone().unwrap_or_default();
    let plugin = if is_plugin {
        s.plugin_targets.get(target).cloned()
    } else {
        None
    };
    drop(s);

    widgets.error_group.set_visible(false);
    widgets.save_btn.set_sensitive(false);
    widgets.progress_label.set_label(&format!(
        "Converting {}…",
        input_path.file_name().unwrap_or_default().to_string_lossy()
    ));
    widgets.progress_box.set_visible(true);
    widgets.progress_spinner.start();

    let input_owned = input_path.to_path_buf();
    let output_owned = output_path.to_path_buf();

    let (tx, rx) = async_channel::unbounded::<ConversionResult>();

    // Worker thread
    thread::spawn(move || {
        let result = if is_plugin {
            if let Some(plugin) = plugin {
                // Execute the first pipeline in the plugin
                let ctx = FileContext::from_paths(&input_owned, &output_owned, &mime);
                if let Ok(steps) = plugin.default_pipeline() {
                    match execute_pipeline(steps, &ctx, &HashMap::new()) {
                        Ok(()) => ConversionResult {
                            success: true,
                            output_path: output_owned.to_string_lossy().into_owned(),
                            command: vec![],
                            stdout: String::new(),
                            stderr: String::new(),
                            returncode: 0,
                        },
                        Err(e) => ConversionResult {
                            success: false,
                            output_path: output_owned.to_string_lossy().into_owned(),
                            command: vec![],
                            stdout: String::new(),
                            stderr: e,
                            returncode: 1,
                        },
                    }
                } else {
                    ConversionResult {
                        success: false,
                        output_path: output_owned.to_string_lossy().into_owned(),
                        command: vec![],
                        stdout: String::new(),
                        stderr: plugin
                            .default_pipeline()
                            .err()
                            .unwrap_or_else(|| "Plugin has no pipeline defined.".to_string()),
                        returncode: 1,
                    }
                }
            } else {
                ConversionResult {
                    success: false,
                    output_path: output_owned.to_string_lossy().into_owned(),
                    command: vec![],
                    stdout: String::new(),
                    stderr: "Plugin not found.".into(),
                    returncode: 1,
                }
            }
        } else if let Some(route) = route {
            run_conversion(&route, &input_owned, &output_owned)
        } else {
            ConversionResult {
                success: false,
                output_path: output_owned.to_string_lossy().into_owned(),
                command: vec![],
                stdout: String::new(),
                stderr: "No conversion route available.".into(),
                returncode: 1,
            }
        };
        let _ = tx.send_blocking(result);
    });

    // Receive result on main thread
    let toast_c = widgets.toast_overlay.clone();
    let save_c = widgets.save_btn.clone();
    let pbox_c = widgets.progress_box.clone();
    let pspinner_c = widgets.progress_spinner.clone();
    let egrp_c = widgets.error_group.clone();
    let state_c = Rc::clone(state);
    let egrp_children_c = Rc::clone(widgets.error_group_children);

    gtk::glib::MainContext::default().spawn_local(async move {
        if let Ok(result) = rx.recv().await {
            pspinner_c.stop();
            pbox_c.set_visible(false);

            // Re-enable save button
            let s = state_c.borrow();
            let is_p = s
                .selected_target
                .as_ref()
                .map(|t| s.plugin_targets.contains_key(t))
                .unwrap_or(false);
            let ready = s.current_file.is_some()
                && s.selected_target.is_some()
                && (s.binary_error.is_none() || is_p);
            save_c.set_sensitive(ready);
            drop(s);

            if result.success {
                let toast = adw::Toast::new(&format!("Saved to {}", result.output_path));
                toast_c.add_toast(toast);
            } else {
                // Show error
                let mut children = egrp_children_c.borrow_mut();
                for child in children.drain(..) {
                    egrp_c.remove(&child);
                }
                let expander = adw::ExpanderRow::new();
                expander.set_title("Conversion failed");
                expander.set_subtitle(&format!("Exit code {}", result.returncode));
                expander.add_prefix(&gtk::Image::from_icon_name("dialog-error-symbolic"));

                if !result.command.is_empty() {
                    let cmd_row = adw::ActionRow::new();
                    cmd_row.set_title("Command");
                    cmd_row.set_subtitle(&result.command.join(" "));
                    cmd_row.set_subtitle_selectable(true);
                    expander.add_row(&cmd_row);
                }

                let stderr = if result.stderr.trim().is_empty() {
                    "(no error output)".to_string()
                } else {
                    result.stderr.trim().to_string()
                };

                let err_label = adw::ActionRow::new();
                err_label.set_title("Error");
                err_label.set_subtitle(&stderr);
                err_label.set_subtitle_selectable(true);
                expander.add_row(&err_label);

                egrp_c.add(&expander);
                children.push(expander.upcast::<gtk::Widget>());
                egrp_c.set_visible(true);
            }
        }
    });
}

// ---------------------------------------------------------------------------
//  Dynamic UI Generator (Plugin Tab)
// ---------------------------------------------------------------------------

fn parse_dimensions(s: &str) -> Option<(f64, f64)> {
    let target = if let (Some(start), Some(end)) = (s.find('{'), s.find('}')) {
        if start < end { &s[start + 1..end] } else { s }
    } else {
        s
    };

    for sep in ['x', 'X', ':', '/'] {
        let parts: Vec<&str> = target.split(sep).collect();
        if parts.len() == 2
            && let (Ok(w), Ok(h)) = (
                parts[0].trim().parse::<f64>(),
                parts[1].trim().parse::<f64>(),
            )
            && w > 0.0
            && h > 0.0
        {
            return Some((w, h));
        }
    }
    None
}

fn parse_ratio_str(s: &str) -> Option<f64> {
    if let Some((w, h)) = parse_dimensions(s) {
        return Some(w / h);
    }

    let target = if let (Some(start), Some(end)) = (s.find('{'), s.find('}')) {
        if start < end { &s[start + 1..end] } else { s }
    } else {
        s
    };
    if let Ok(val) = target.trim().parse::<f64>()
        && val > 0.0
    {
        return Some(val);
    }

    None
}

fn detect_image_dimensions(path_str: &str) -> Option<(f64, f64)> {
    let path = std::path::Path::new(path_str);
    if path.exists() {
        if let Ok((w, h)) = image::image_dimensions(path)
            && w > 0
            && h > 0
        {
            return Some((w as f64, h as f64));
        }
        let gio_file = gtk::gio::File::for_path(path);
        if let Ok(texture) = gtk::gdk::Texture::from_file(&gio_file) {
            let w = texture.width() as f64;
            let h = texture.height() as f64;
            if w > 0.0 && h > 0.0 {
                return Some((w, h));
            }
        }
    }
    None
}

fn get_active_aspect_ratio(
    aspect_ratio_source_expr: Option<&str>,
    state: &std::collections::HashMap<String, String>,
    picture: &gtk::Picture,
) -> f64 {
    // 1. If an explicit preset/aspect ratio source is provided
    if let Some(expr) = aspect_ratio_source_expr {
        let key = expr.trim_matches(|c| c == '{' || c == '}');
        if let Some(v) = state.get(key)
            && !v.to_lowercase().contains("original")
            && let Some(ratio) = parse_ratio_str(v)
        {
            return ratio;
        }
        let mut resolved = expr.to_string();
        for (k, val) in state.iter() {
            resolved = resolved.replace(&format!("{{{}}}", k), val);
        }
        if !resolved.to_lowercase().contains("original")
            && let Some(ratio) = parse_ratio_str(&resolved)
        {
            return ratio;
        }
    }

    // 2. Check if the original image properties / dimensions were detected
    if let Some(ar_str) = state
        .get("image.aspect_ratio")
        .or_else(|| state.get("file.aspect_ratio"))
        && let Ok(ar) = ar_str.parse::<f64>()
        && ar > 0.0
    {
        return ar;
    }

    if let (Some(w_str), Some(h_str)) = (
        state.get("image.width").or_else(|| state.get("file.width")),
        state
            .get("image.height")
            .or_else(|| state.get("file.height")),
    ) && let (Ok(w), Ok(h)) = (w_str.parse::<f64>(), h_str.parse::<f64>())
        && w > 0.0
        && h > 0.0
    {
        return w / h;
    }

    // 3. Check directly from file.path if present in state
    if let Some(file_path) = state.get("file.path")
        && let Some((w, h)) = detect_image_dimensions(file_path)
        && w > 0.0
        && h > 0.0
    {
        return w / h;
    }

    // 4. Check from picture paintable intrinsic dimensions
    if let Some(paintable) = picture.paintable() {
        let iw = paintable.intrinsic_width() as f64;
        let ih = paintable.intrinsic_height() as f64;
        if iw > 0.0 && ih > 0.0 {
            return iw / ih;
        }
    }

    // 5. Fallback to width / height text inputs if present
    if let (Some(w_str), Some(h_str)) = (state.get("width"), state.get("height"))
        && let (Ok(w), Ok(h)) = (w_str.parse::<f64>(), h_str.parse::<f64>())
        && w > 0.0
        && h > 0.0
    {
        return w / h;
    }

    1.0
}

fn get_image_bounds(
    picture: &gtk::Picture,
    drawing_area: &gtk::DrawingArea,
    state: Option<&std::collections::HashMap<String, String>>,
) -> Option<(f64, f64, f64, f64)> {
    let (mut img_w, mut img_h) = (0.0, 0.0);
    if let Some(s) = state
        && let (Some(w_s), Some(h_s)) = (
            s.get("image.width").or_else(|| s.get("file.width")),
            s.get("image.height").or_else(|| s.get("file.height")),
        )
        && let (Ok(w), Ok(h)) = (w_s.parse::<f64>(), h_s.parse::<f64>())
    {
        img_w = w;
        img_h = h;
    }
    if (img_w <= 0.0 || img_h <= 0.0)
        && let Some(paintable) = picture.paintable()
    {
        img_w = paintable.intrinsic_width() as f64;
        img_h = paintable.intrinsic_height() as f64;
    }

    let widget_w = drawing_area.width() as f64;
    let widget_h = drawing_area.height() as f64;

    if img_w > 0.0 && img_h > 0.0 && widget_w > 0.0 && widget_h > 0.0 {
        let scale = (widget_w / img_w).min(widget_h / img_h);
        let d_w = img_w * scale;
        let d_h = img_h * scale;
        let o_x = (widget_w - d_w) / 2.0;
        let o_y = (widget_h - d_h) / 2.0;
        return Some((o_x, o_y, d_w, d_h));
    }
    None
}

fn is_locked_inside_image(
    lock_inside_val: Option<&fm_core::types::BoolOrString>,
    state: &std::collections::HashMap<String, String>,
) -> bool {
    if let Some(val) = lock_inside_val {
        match val {
            fm_core::types::BoolOrString::Bool(b) => return *b,
            fm_core::types::BoolOrString::String(s) => {
                let key = s.trim_matches(|c| c == '{' || c == '}');
                if let Some(v) = state.get(key) {
                    let lower = v.trim().to_lowercase();
                    return lower == "true"
                        || lower == "1"
                        || lower.contains("lock")
                        || lower.contains("yes")
                        || lower.contains("on");
                }
                let lower = s.trim().to_lowercase();
                return lower == "true"
                    || lower == "1"
                    || lower.contains("lock")
                    || lower.contains("yes")
                    || lower.contains("on");
            }
        }
    }
    if let Some(v) = state.get("lock_inside_image") {
        let lower = v.trim().to_lowercase();
        return lower == "true" || lower == "1" || lower.contains("yes") || lower.contains("on");
    }
    true
}

fn is_aspect_ratio_locked(
    lock_aspect_ratio_expr: Option<&str>,
    state: &std::collections::HashMap<String, String>,
) -> bool {
    if let Some(expr) = lock_aspect_ratio_expr {
        let key = expr.trim_matches(|c| c == '{' || c == '}');
        if let Some(v) = state.get(key) {
            let lower = v.trim().to_lowercase();
            return lower.contains("locked")
                || lower.contains("ratio")
                || lower == "true"
                || lower == "1";
        }
    }
    false
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum DragAction {
    New(f64, f64),
    Translate(f64, f64, f64, f64),
    Resize(f64, f64, f64, f64, i32, i32),
}

/// Colour a plugin widget with the role its `color` names (see roles.css).
fn tint(widget: &impl IsA<gtk::Widget>, color: &Option<String>) {
    if let Some(role) = color {
        widget.add_css_class(&format!("role-{role}"));
    }
}

fn build_plugin_tab(
    _state: &Rc<RefCell<AppState>>,
    toast_overlay: &adw::ToastOverlay,
    plugin: &fm_core::types::Plugin,
) -> gtk::Widget {
    let root_box = gtk::Box::new(gtk::Orientation::Vertical, 12);
    root_box.set_margin_top(24);
    root_box.set_margin_bottom(24);
    root_box.set_margin_start(24);
    root_box.set_margin_end(24);

    let title = gtk::Label::new(Some(&plugin.plugin.name));
    title.add_css_class("title-1");
    title.set_halign(gtk::Align::Start);
    root_box.append(&title);

    let desc = gtk::Label::new(Some(&plugin.plugin.description));
    desc.set_halign(gtk::Align::Start);
    desc.add_css_class("dim-label");
    root_box.append(&desc);

    let mut pref_group: Option<adw::PreferencesGroup> = None;
    let tab_state = Rc::new(RefCell::new(
        std::collections::HashMap::<String, String>::new(),
    ));
    let canvases = Rc::new(RefCell::new(Vec::<gtk::Picture>::new()));
    let on_state_change = Rc::new(RefCell::new(Vec::<Box<dyn Fn()>>::new()));

    // Iter over widgets and render them
    for widget in &plugin.ui {
        let w_id = widget.id.clone().unwrap_or_default();
        match widget.widget_type.as_str() {
            "file_picker" => {
                let btn = gtk::Button::builder()
                    .label("Choose File...")
                    .valign(gtk::Align::Center)
                    .build();

                let file_label = gtk::Label::new(Some("No file selected"));
                file_label.add_css_class("dim-label");
                file_label.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
                file_label.set_hexpand(true);
                file_label.set_halign(gtk::Align::Start);

                let row = adw::ActionRow::builder()
                    .title(widget.label.as_deref().unwrap_or("File"))
                    .build();
                row.add_suffix(&file_label);
                row.add_suffix(&btn);

                if pref_group.is_none() {
                    let pg = adw::PreferencesGroup::new();
                    root_box.append(&pg);
                    pref_group = Some(pg);
                }
                tint(&row, &widget.color);
                pref_group.as_ref().unwrap().add(&row);

                let tab_state_c = tab_state.clone();
                let file_label_c = file_label.clone();
                let w_id_c = w_id.clone();
                let canvases_c = canvases.clone();
                let on_state_change_c = on_state_change.clone();
                let filter_patterns = widget.filter.clone();
                btn.connect_clicked(move |_| {
                    let dialog = gtk::FileDialog::new();
                    if let Some(patterns) = &filter_patterns
                        && !patterns.is_empty()
                    {
                        let filter = gtk::FileFilter::new();
                        for pattern in patterns {
                            if pattern.contains('/') {
                                filter.add_mime_type(pattern);
                            } else {
                                filter.add_pattern(pattern);
                            }
                        }
                        filter.set_name(Some("Supported files"));
                        let filters = gtk::gio::ListStore::new::<gtk::FileFilter>();
                        filters.append(&filter);
                        dialog.set_filters(Some(&filters));
                        dialog.set_default_filter(Some(&filter));
                    }
                    let file_label_c2 = file_label_c.clone();
                    let tab_state_c2 = tab_state_c.clone();
                    let w_id_c2 = w_id_c.clone();
                    let canvases_c2 = canvases_c.clone();
                    let on_state_change_c2 = on_state_change_c.clone();
                    dialog.open(
                        None::<&gtk::Window>,
                        None::<&gtk::gio::Cancellable>,
                        move |res| {
                            if let Ok(file) = res
                                && let Some(path) = file.path()
                            {
                                let path_str = path.to_string_lossy().to_string();
                                file_label_c2.set_label(&path_str);
                                tab_state_c2
                                    .borrow_mut()
                                    .insert(w_id_c2.clone(), path_str.clone());
                                tab_state_c2
                                    .borrow_mut()
                                    .insert("file.path".to_string(), path_str.clone());

                                // Detect image dimensions directly from file properties
                                if let Some((img_w, img_h)) = detect_image_dimensions(&path_str) {
                                    let ar = img_w / img_h;
                                    tab_state_c2
                                        .borrow_mut()
                                        .insert("image.width".to_string(), img_w.to_string());
                                    tab_state_c2
                                        .borrow_mut()
                                        .insert("image.height".to_string(), img_h.to_string());
                                    tab_state_c2
                                        .borrow_mut()
                                        .insert("image.aspect_ratio".to_string(), ar.to_string());
                                    tab_state_c2
                                        .borrow_mut()
                                        .insert("file.width".to_string(), img_w.to_string());
                                    tab_state_c2
                                        .borrow_mut()
                                        .insert("file.height".to_string(), img_h.to_string());
                                    tab_state_c2
                                        .borrow_mut()
                                        .insert("file.aspect_ratio".to_string(), ar.to_string());
                                    tab_state_c2.borrow_mut().insert(
                                        "preview_canvas.crop_data".to_string(),
                                        format!(
                                            "{}x{}+0+0",
                                            img_w.round() as i32,
                                            img_h.round() as i32
                                        ),
                                    );
                                }

                                let gio_file = gtk::gio::File::for_path(&path_str);
                                for pic in canvases_c2.borrow().iter() {
                                    pic.set_file(Some(&gio_file));
                                }
                                for cb in on_state_change_c2.borrow().iter() {
                                    cb();
                                }
                            }
                        },
                    );
                });
            }
            "image_canvas" => {
                let canvas_frame = gtk::Frame::new(None);
                canvas_frame.add_css_class("card");
                canvas_frame.set_vexpand(true);
                canvas_frame.set_hexpand(true);
                canvas_frame.set_height_request(280);

                // Initialize default crop_data so pipeline doesn't crash if user never crops
                let initial_crop = {
                    let st = tab_state.borrow();
                    if let (Some(w), Some(h)) = (st.get("image.width"), st.get("image.height")) {
                        format!("{}x{}+0+0", w, h)
                    } else {
                        "0x0+0+0".to_string()
                    }
                };
                tab_state
                    .borrow_mut()
                    .insert(format!("{}.crop_data", w_id), initial_crop);

                let overlay = gtk::Overlay::new();
                let picture = gtk::Picture::new();
                picture.set_content_fit(gtk::ContentFit::Contain);
                canvases.borrow_mut().push(picture.clone());
                overlay.set_child(Some(&picture));

                let drawing_area = gtk::DrawingArea::new();
                let crop_state = Rc::new(RefCell::new(None::<(f64, f64, f64, f64)>));
                let drag_action = Rc::new(RefCell::new(DragAction::New(0.0, 0.0)));

                let drag = gtk::GestureDrag::new();
                let lock_inside_val = widget.lock_inside_image.clone();
                let lock_aspect_ratio_expr = widget.lock_aspect_ratio.clone();
                let aspect_ratio_source_expr = widget.aspect_ratio_source.clone();
                let tab_state_c1 = tab_state.clone();

                let crop_state_c = crop_state.clone();
                let da_c = drawing_area.clone();
                let pic_c_begin = picture.clone();
                let lock_inside_begin = lock_inside_val.clone();
                let tab_state_begin = tab_state.clone();
                let show_handles = widget.show_handles.unwrap_or(false);
                let draggable = widget.draggable_selection.unwrap_or(false);
                let drag_action_c1 = drag_action.clone();

                drag.connect_drag_begin(move |_, x, y| {
                    let state = tab_state_begin.borrow();
                    let lock_inside = is_locked_inside_image(lock_inside_begin.as_ref(), &state);
                    let (start_x, start_y) = if lock_inside
                        && let Some((o_x, o_y, d_w, d_h)) =
                            get_image_bounds(&pic_c_begin, &da_c, Some(&state))
                    {
                        (x.clamp(o_x, o_x + d_w), y.clamp(o_y, o_y + d_h))
                    } else {
                        (x, y)
                    };

                    let mut action = DragAction::New(start_x, start_y);
                    if let Some(rect) = *crop_state_c.borrow() {
                        let cx = if rect.2 < 0.0 {
                            rect.0 + rect.2
                        } else {
                            rect.0
                        };
                        let cy = if rect.3 < 0.0 {
                            rect.1 + rect.3
                        } else {
                            rect.1
                        };
                        let cw = rect.2.abs();
                        let ch = rect.3.abs();

                        if cw > 2.0 && ch > 2.0 {
                            let mut dx = 0;
                            let mut dy = 0;
                            let r = 15.0; // handle radius

                            if show_handles {
                                if (start_x - cx).abs() < r && (start_y - cy).abs() < r {
                                    dx = -1;
                                    dy = -1;
                                } else if (start_x - (cx + cw)).abs() < r
                                    && (start_y - cy).abs() < r
                                {
                                    dx = 1;
                                    dy = -1;
                                } else if (start_x - cx).abs() < r
                                    && (start_y - (cy + ch)).abs() < r
                                {
                                    dx = -1;
                                    dy = 1;
                                } else if (start_x - (cx + cw)).abs() < r
                                    && (start_y - (cy + ch)).abs() < r
                                {
                                    dx = 1;
                                    dy = 1;
                                }
                            }

                            if dx != 0 || dy != 0 {
                                action = DragAction::Resize(cx, cy, cw, ch, dx, dy);
                            } else if draggable
                                && start_x >= cx
                                && start_x <= cx + cw
                                && start_y >= cy
                                && start_y <= cy + ch
                            {
                                action = DragAction::Translate(cx, cy, cw, ch);
                            }
                        }
                    }

                    *drag_action_c1.borrow_mut() = action;
                    if let DragAction::New(sx, sy) = action {
                        *crop_state_c.borrow_mut() = Some((sx, sy, 0.0, 0.0));
                    }
                });

                let crop_state_c = crop_state.clone();
                let da_c = drawing_area.clone();
                let pic_c_update = picture.clone();
                let lock_expr_c = lock_aspect_ratio_expr.clone();
                let ar_expr_c = aspect_ratio_source_expr.clone();
                let lock_inside_update = lock_inside_val.clone();
                let drag_action_c2 = drag_action.clone();
                drag.connect_drag_update(move |_, offset_x, offset_y| {
                    if let Some(rect) = crop_state_c.borrow_mut().as_mut() {
                        let state = tab_state_c1.borrow();
                        let lock_inside =
                            is_locked_inside_image(lock_inside_update.as_ref(), &state);
                        let locked = is_aspect_ratio_locked(lock_expr_c.as_deref(), &state);
                        let bounds = if lock_inside {
                            get_image_bounds(&pic_c_update, &da_c, Some(&state))
                        } else {
                            None
                        };

                        let action = *drag_action_c2.borrow();
                        match action {
                            DragAction::New(start_x, start_y) => {
                                let sign_x = if offset_x < 0.0 { -1.0 } else { 1.0 };
                                let sign_y = if offset_y < 0.0 { -1.0 } else { 1.0 };

                                let (max_w, max_h) = if let Some((o_x, o_y, d_w, d_h)) = bounds {
                                    let mw = if sign_x > 0.0 {
                                        (o_x + d_w - start_x).max(0.0)
                                    } else {
                                        (start_x - o_x).max(0.0)
                                    };
                                    let mh = if sign_y > 0.0 {
                                        (o_y + d_h - start_y).max(0.0)
                                    } else {
                                        (start_y - o_y).max(0.0)
                                    };
                                    (Some(mw), Some(mh))
                                } else {
                                    (None, None)
                                };

                                let (ox, oy) = if locked {
                                    let ar = get_active_aspect_ratio(
                                        ar_expr_c.as_deref(),
                                        &state,
                                        &pic_c_update,
                                    );
                                    let mut w = offset_x.abs();
                                    let mut h = offset_y.abs();
                                    if ar > 0.0 {
                                        if w / ar > h {
                                            h = w / ar;
                                        } else {
                                            w = h * ar;
                                        }
                                        if let Some(mw) = max_w
                                            && w > mw
                                        {
                                            w = mw;
                                            h = w / ar;
                                        }
                                        if let Some(mh) = max_h
                                            && h > mh
                                        {
                                            h = mh;
                                            w = h * ar;
                                        }
                                        (w * sign_x, h * sign_y)
                                    } else {
                                        (offset_x, offset_y)
                                    }
                                } else {
                                    let mut w = offset_x.abs();
                                    let mut h = offset_y.abs();
                                    if let Some(mw) = max_w {
                                        w = w.min(mw);
                                    }
                                    if let Some(mh) = max_h {
                                        h = h.min(mh);
                                    }
                                    (w * sign_x, h * sign_y)
                                };

                                rect.0 = start_x;
                                rect.1 = start_y;
                                rect.2 = ox;
                                rect.3 = oy;
                            }
                            DragAction::Translate(cx, cy, cw, ch) => {
                                let mut new_x = cx + offset_x;
                                let mut new_y = cy + offset_y;

                                if let Some((o_x, o_y, d_w, d_h)) = bounds {
                                    new_x = new_x.clamp(o_x, (o_x + d_w - cw).max(o_x));
                                    new_y = new_y.clamp(o_y, (o_y + d_h - ch).max(o_y));
                                }

                                rect.0 = new_x;
                                rect.1 = new_y;
                                rect.2 = cw;
                                rect.3 = ch;
                            }
                            DragAction::Resize(cx, cy, cw, ch, dx, dy) => {
                                let fixed_x = if dx < 0 { cx + cw } else { cx };
                                let fixed_y = if dy < 0 { cy + ch } else { cy };

                                let mut move_x = (if dx < 0 { cx } else { cx + cw }) + offset_x;
                                let mut move_y = (if dy < 0 { cy } else { cy + ch }) + offset_y;

                                if let Some((o_x, o_y, d_w, d_h)) = bounds {
                                    move_x = move_x.clamp(o_x, o_x + d_w);
                                    move_y = move_y.clamp(o_y, o_y + d_h);
                                }

                                let mut new_w = (move_x - fixed_x).abs();
                                let mut new_h = (move_y - fixed_y).abs();

                                if let Some((o_x, o_y, d_w, d_h)) = bounds {
                                    let max_w = if dx < 0 {
                                        (fixed_x - o_x).max(0.0)
                                    } else {
                                        (o_x + d_w - fixed_x).max(0.0)
                                    };
                                    let max_h = if dy < 0 {
                                        (fixed_y - o_y).max(0.0)
                                    } else {
                                        (o_y + d_h - fixed_y).max(0.0)
                                    };
                                    if locked {
                                        let ar = get_active_aspect_ratio(
                                            ar_expr_c.as_deref(),
                                            &state,
                                            &pic_c_update,
                                        );
                                        if ar > 0.0 {
                                            if new_w / ar > new_h {
                                                new_h = new_w / ar;
                                            } else {
                                                new_w = new_h * ar;
                                            }
                                            if new_w > max_w {
                                                new_w = max_w;
                                                new_h = new_w / ar;
                                            }
                                            if new_h > max_h {
                                                new_h = max_h;
                                                new_w = new_h * ar;
                                            }
                                        }
                                    } else {
                                        new_w = new_w.min(max_w);
                                        new_h = new_h.min(max_h);
                                    }
                                } else if locked {
                                    let ar = get_active_aspect_ratio(
                                        ar_expr_c.as_deref(),
                                        &state,
                                        &pic_c_update,
                                    );
                                    if ar > 0.0 {
                                        if new_w / ar > new_h {
                                            new_h = new_w / ar;
                                        } else {
                                            new_w = new_h * ar;
                                        }
                                    }
                                }

                                rect.0 = if dx < 0 { fixed_x - new_w } else { fixed_x };
                                rect.1 = if dy < 0 { fixed_y - new_h } else { fixed_y };
                                rect.2 = new_w;
                                rect.3 = new_h;
                            }
                        }
                    }
                    da_c.queue_draw();
                });

                let crop_state_c = crop_state.clone();
                let tab_state_c = tab_state.clone();
                let w_id_c = w_id.clone();
                let pic_c = picture.clone();
                let da_c = drawing_area.clone();
                drag.connect_drag_end(move |_, _offset_x, _offset_y| {
                    if let Some(rect) = crop_state_c.borrow().as_ref() {
                        let x = if rect.2 < 0.0 {
                            rect.0 + rect.2
                        } else {
                            rect.0
                        };
                        let y = if rect.3 < 0.0 {
                            rect.1 + rect.3
                        } else {
                            rect.1
                        };
                        let w = rect.2.abs();
                        let h = rect.3.abs();

                        let state = tab_state_c.borrow();
                        let (mut img_w, mut img_h) = (0.0, 0.0);
                        if let (Some(w_s), Some(h_s)) = (
                            state.get("image.width").or_else(|| state.get("file.width")),
                            state
                                .get("image.height")
                                .or_else(|| state.get("file.height")),
                        ) && let (Ok(w_val), Ok(h_val)) =
                            (w_s.parse::<f64>(), h_s.parse::<f64>())
                        {
                            img_w = w_val;
                            img_h = h_val;
                        }
                        if (img_w <= 0.0 || img_h <= 0.0)
                            && let Some(paintable) = pic_c.paintable()
                        {
                            img_w = paintable.intrinsic_width() as f64;
                            img_h = paintable.intrinsic_height() as f64;
                        }

                        let widget_w = da_c.width() as f64;
                        let widget_h = da_c.height() as f64;

                        if img_w > 0.0 && img_h > 0.0 && widget_w > 0.0 && widget_h > 0.0 {
                            let scale = (widget_w / img_w).min(widget_h / img_h);
                            let d_w = img_w * scale;
                            let d_h = img_h * scale;
                            let o_x = (widget_w - d_w) / 2.0;
                            let o_y = (widget_h - d_h) / 2.0;

                            let real_x = ((x - o_x) / scale).clamp(0.0, img_w);
                            let real_y = ((y - o_y) / scale).clamp(0.0, img_h);
                            let real_w = (w / scale).clamp(0.0, img_w - real_x);
                            let real_h = (h / scale).clamp(0.0, img_h - real_y);

                            if real_w > 1.0 && real_h > 1.0 {
                                let crop_str = format!(
                                    "{}x{}+{}+{}",
                                    real_w.round() as i32,
                                    real_h.round() as i32,
                                    real_x.round() as i32,
                                    real_y.round() as i32
                                );
                                drop(state);
                                tab_state_c
                                    .borrow_mut()
                                    .insert(format!("{}.crop_data", w_id_c), crop_str);
                            }
                        }
                    }
                    da_c.queue_draw();
                });
                drawing_area.add_controller(drag);

                let crop_state_c = crop_state.clone();
                let show_handles_draw = widget.show_handles.unwrap_or(false);
                let show_grid_draw = widget.show_grid.unwrap_or(true);
                let shade_alpha = widget.shade_outside.unwrap_or(0.45).clamp(0.0, 1.0);
                drawing_area.set_draw_func(move |_, cr, width, height| {
                    if let Some(rect) = crop_state_c.borrow().as_ref() {
                        let x = if rect.2 < 0.0 {
                            rect.0 + rect.2
                        } else {
                            rect.0
                        };
                        let y = if rect.3 < 0.0 {
                            rect.1 + rect.3
                        } else {
                            rect.1
                        };
                        let w = rect.2.abs();
                        let h = rect.3.abs();

                        if w > 2.0 && h > 2.0 {
                            // Semi-transparent shade outside crop box
                            cr.set_source_rgba(0.0, 0.0, 0.0, shade_alpha);
                            if y > 0.0 {
                                cr.rectangle(0.0, 0.0, width as f64, y);
                                let _ = cr.fill();
                            }
                            if y + h < height as f64 {
                                cr.rectangle(0.0, y + h, width as f64, height as f64 - (y + h));
                                let _ = cr.fill();
                            }
                            if x > 0.0 {
                                cr.rectangle(0.0, y, x, h);
                                let _ = cr.fill();
                            }
                            if x + w < width as f64 {
                                cr.rectangle(x + w, y, width as f64 - (x + w), h);
                                let _ = cr.fill();
                            }

                            // Crop box fill
                            cr.set_source_rgba(0.2, 0.6, 1.0, 0.15);
                            cr.rectangle(x, y, w, h);
                            let _ = cr.fill();

                            // Crop box border
                            cr.set_source_rgba(0.2, 0.6, 1.0, 0.95);
                            cr.set_line_width(2.0);
                            cr.rectangle(x, y, w, h);
                            let _ = cr.stroke();

                            // Rule of thirds grid lines
                            if show_grid_draw {
                                cr.set_source_rgba(1.0, 1.0, 1.0, 0.35);
                                cr.set_line_width(1.0);
                                cr.move_to(x + w / 3.0, y);
                                cr.line_to(x + w / 3.0, y + h);
                                cr.move_to(x + 2.0 * w / 3.0, y);
                                cr.line_to(x + 2.0 * w / 3.0, y + h);
                                cr.move_to(x, y + h / 3.0);
                                cr.line_to(x + w, y + h / 3.0);
                                cr.move_to(x, y + 2.0 * h / 3.0);
                                cr.line_to(x + w, y + 2.0 * h / 3.0);
                                let _ = cr.stroke();
                            }

                            if show_handles_draw {
                                cr.set_source_rgba(0.2, 0.6, 1.0, 1.0);
                                let hw = 6.0; // handle half-width

                                // top left
                                cr.rectangle(x - hw, y - hw, hw * 2.0, hw * 2.0);
                                // top right
                                cr.rectangle(x + w - hw, y - hw, hw * 2.0, hw * 2.0);
                                // bottom left
                                cr.rectangle(x - hw, y + h - hw, hw * 2.0, hw * 2.0);
                                // bottom right
                                cr.rectangle(x + w - hw, y + h - hw, hw * 2.0, hw * 2.0);

                                let _ = cr.fill();
                            }
                        }
                    }
                });
                let lock_aspect_ratio_expr = widget.lock_aspect_ratio.clone();
                let aspect_ratio_source_expr = widget.aspect_ratio_source.clone();
                let tab_state_c2 = tab_state.clone();
                let crop_state_c2 = crop_state.clone();
                let da_c2 = drawing_area.clone();
                let w_id_c2 = w_id.clone();
                let pic_c_state = picture.clone();
                on_state_change.borrow_mut().push(Box::new(move || {
                    let state = tab_state_c2.borrow().clone();
                    let mut mode = "Freeform".to_string();
                    if let Some(expr) = &lock_aspect_ratio_expr {
                        let key = expr.trim_matches(|c| c == '{' || c == '}');
                        if let Some(v) = state.get(key) {
                            mode = v.clone();
                        }
                    }
                    if mode.to_lowercase().contains("reset") {
                        *crop_state_c2.borrow_mut() = None;
                        let (mut img_w, mut img_h) = (0.0, 0.0);
                        if let (Some(w_s), Some(h_s)) = (
                            state.get("image.width").or_else(|| state.get("file.width")),
                            state
                                .get("image.height")
                                .or_else(|| state.get("file.height")),
                        ) && let (Ok(w_val), Ok(h_val)) =
                            (w_s.parse::<f64>(), h_s.parse::<f64>())
                        {
                            img_w = w_val;
                            img_h = h_val;
                        }
                        let reset_str = if img_w > 0.0 && img_h > 0.0 {
                            format!("{}x{}+0+0", img_w.round() as i32, img_h.round() as i32)
                        } else {
                            "0x0+0+0".to_string()
                        };
                        tab_state_c2
                            .borrow_mut()
                            .insert(format!("{}.crop_data", w_id_c2), reset_str);
                        da_c2.queue_draw();
                    } else if is_aspect_ratio_locked(lock_aspect_ratio_expr.as_deref(), &state) {
                        let (mut img_w, mut img_h) = (0.0, 0.0);
                        if let (Some(w_s), Some(h_s)) = (
                            state.get("image.width").or_else(|| state.get("file.width")),
                            state
                                .get("image.height")
                                .or_else(|| state.get("file.height")),
                        ) && let (Ok(w_val), Ok(h_val)) =
                            (w_s.parse::<f64>(), h_s.parse::<f64>())
                        {
                            img_w = w_val;
                            img_h = h_val;
                        }
                        if (img_w <= 0.0 || img_h <= 0.0)
                            && let Some(paintable) = pic_c_state.paintable()
                        {
                            img_w = paintable.intrinsic_width() as f64;
                            img_h = paintable.intrinsic_height() as f64;
                        }
                        let widget_w = da_c2.width() as f64;
                        let widget_h = da_c2.height() as f64;

                        if img_w > 0.0 && img_h > 0.0 && widget_w > 0.0 && widget_h > 0.0 {
                            let scale = (widget_w / img_w).min(widget_h / img_h);
                            let d_w = img_w * scale;
                            let d_h = img_h * scale;
                            let o_x = (widget_w - d_w) / 2.0;
                            let o_y = (widget_h - d_h) / 2.0;

                            let ar = get_active_aspect_ratio(
                                aspect_ratio_source_expr.as_deref(),
                                &state,
                                &pic_c_state,
                            );
                            let img_ar = img_w / img_h;

                            let (cw, ch) = if ar <= img_ar {
                                let ch = d_h * 0.85;
                                (ch * ar, ch)
                            } else {
                                let cw = d_w * 0.85;
                                (cw, cw / ar)
                            };

                            let cx = o_x + (d_w - cw) / 2.0;
                            let cy = o_y + (d_h - ch) / 2.0;

                            *crop_state_c2.borrow_mut() = Some((cx, cy, cw, ch));

                            let real_x = ((cx - o_x) / scale).clamp(0.0, img_w);
                            let real_y = ((cy - o_y) / scale).clamp(0.0, img_h);
                            let real_w = (cw / scale).clamp(0.0, img_w - real_x);
                            let real_h = (ch / scale).clamp(0.0, img_h - real_y);

                            let crop_str = format!(
                                "{}x{}+{}+{}",
                                real_w.round() as i32,
                                real_h.round() as i32,
                                real_x.round() as i32,
                                real_y.round() as i32
                            );
                            tab_state_c2
                                .borrow_mut()
                                .insert(format!("{}.crop_data", w_id_c2), crop_str);

                            da_c2.queue_draw();
                        }
                    }
                }));

                overlay.add_overlay(&drawing_area);
                canvas_frame.set_child(Some(&overlay));

                root_box.append(&canvas_frame);
                pref_group = None; // Reset so next prefs go below
            }
            "picker" => {
                let empty_opts = vec![];
                let opts_ref = widget.options.as_ref().unwrap_or(&empty_opts);
                let options: Vec<&str> = opts_ref.iter().map(|s| s.as_str()).collect();
                let dropdown = gtk::DropDown::from_strings(&options);
                dropdown.set_valign(gtk::Align::Center);

                let default_val = widget
                    .default
                    .as_ref()
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                if let Some(idx) = options.iter().position(|&s| s == default_val) {
                    dropdown.set_selected(idx as u32);
                }

                let tab_state_c = tab_state.clone();
                let w_id_c = w_id.clone();
                let opts = widget.options.clone().unwrap_or_default();
                let overwrites = widget.overwrites.clone();

                if let Some(opt) = opts.get(dropdown.selected() as usize) {
                    tab_state.borrow_mut().insert(w_id.clone(), opt.clone());
                }

                let on_state_change_c = on_state_change.clone();
                dropdown.connect_selected_notify(move |d| {
                    if let Some(opt) = opts.get(d.selected() as usize) {
                        tab_state_c.borrow_mut().insert(w_id_c.clone(), opt.clone());
                        if let Some(ow) = &overwrites
                            && ow.len() == 2
                            && let Some((w, h)) = parse_dimensions(opt)
                        {
                            tab_state_c
                                .borrow_mut()
                                .insert(ow[0].clone(), w.to_string());
                            tab_state_c
                                .borrow_mut()
                                .insert(ow[1].clone(), h.to_string());
                        }
                        for cb in on_state_change_c.borrow().iter() {
                            cb();
                        }
                    }
                });

                let row = adw::ActionRow::builder()
                    .title(widget.label.as_deref().unwrap_or("Picker"))
                    .build();
                row.add_suffix(&dropdown);

                if pref_group.is_none() {
                    let pg = adw::PreferencesGroup::new();
                    root_box.append(&pg);
                    pref_group = Some(pg);
                }
                tint(&row, &widget.color);
                pref_group.as_ref().unwrap().add(&row);
            }
            "radio_group" => {
                let empty_opts = vec![];
                let opts_ref = widget.options.as_ref().unwrap_or(&empty_opts);

                let box_ = gtk::Box::new(gtk::Orientation::Horizontal, 12);
                box_.set_valign(gtk::Align::Center);

                let mut first: Option<gtk::CheckButton> = None;
                let default_val = widget
                    .default
                    .as_ref()
                    .and_then(|v| v.as_str())
                    .unwrap_or("");

                for (i, opt) in opts_ref.iter().enumerate() {
                    let btn = gtk::CheckButton::builder().label(opt.as_str()).build();
                    if let Some(f) = &first {
                        btn.set_group(Some(f));
                    } else {
                        first = Some(btn.clone());
                    }

                    if opt == default_val || (default_val.is_empty() && i == 0) {
                        btn.set_active(true);
                        tab_state.borrow_mut().insert(w_id.clone(), opt.clone());
                    }

                    let tab_state_c = tab_state.clone();
                    let w_id_c = w_id.clone();
                    let opt_c = opt.clone();
                    let on_state_change_c = on_state_change.clone();
                    btn.connect_toggled(move |b| {
                        if b.is_active() {
                            tab_state_c
                                .borrow_mut()
                                .insert(w_id_c.clone(), opt_c.clone());
                            for cb in on_state_change_c.borrow().iter() {
                                cb();
                            }
                        }
                    });

                    box_.append(&btn);
                }

                let row = adw::ActionRow::builder()
                    .title(widget.label.as_deref().unwrap_or("Radio Group"))
                    .build();
                row.add_suffix(&box_);

                if pref_group.is_none() {
                    let pg = adw::PreferencesGroup::new();
                    root_box.append(&pg);
                    pref_group = Some(pg);
                }
                tint(&row, &widget.color);
                pref_group.as_ref().unwrap().add(&row);
            }
            "slider" => {
                let min = widget.min.unwrap_or(0.0);
                let max = widget.max.unwrap_or(100.0);
                let scale = gtk::Scale::with_range(gtk::Orientation::Horizontal, min, max, 1.0);
                let def = widget
                    .default
                    .as_ref()
                    .and_then(|v| v.as_float())
                    .unwrap_or(min);
                scale.set_value(def);
                scale.set_draw_value(true);
                scale.set_value_pos(gtk::PositionType::Left);
                scale.set_digits(0);
                scale.set_width_request(200);
                scale.set_valign(gtk::Align::Center);

                tab_state.borrow_mut().insert(w_id.clone(), def.to_string());
                let tab_state_c = tab_state.clone();
                let w_id_c = w_id.clone();
                let on_state_change_c = on_state_change.clone();
                scale.connect_value_changed(move |s| {
                    tab_state_c
                        .borrow_mut()
                        .insert(w_id_c.clone(), s.value().to_string());
                    for cb in on_state_change_c.borrow().iter() {
                        cb();
                    }
                });

                let row = adw::ActionRow::builder()
                    .title(widget.label.as_deref().unwrap_or("Slider"))
                    .build();
                row.add_suffix(&scale);

                if pref_group.is_none() {
                    let pg = adw::PreferencesGroup::new();
                    root_box.append(&pg);
                    pref_group = Some(pg);
                }
                tint(&row, &widget.color);
                pref_group.as_ref().unwrap().add(&row);
            }
            "text_input" => {
                let entry_row = adw::EntryRow::builder()
                    .title(widget.label.as_deref().unwrap_or("Text Input"))
                    .build();
                let def = widget
                    .default
                    .as_ref()
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                entry_row.set_text(def);
                tab_state.borrow_mut().insert(w_id.clone(), def.to_string());

                let tab_state_c = tab_state.clone();
                let w_id_c = w_id.clone();
                let entry_row_c = entry_row.clone();
                on_state_change.borrow_mut().push(Box::new(move || {
                    let val_opt = {
                        let s = tab_state_c.borrow();
                        s.get(&w_id_c).cloned()
                    };
                    if let Some(val) = val_opt
                        && entry_row_c.text() != val.as_str()
                    {
                        entry_row_c.set_text(&val);
                    }
                }));

                let tab_state_c = tab_state.clone();
                let w_id_c = w_id.clone();
                let on_state_change_c = on_state_change.clone();
                entry_row.connect_changed(move |e| {
                    let text = e.text().to_string();
                    let mut needs_update = false;
                    {
                        let mut s = tab_state_c.borrow_mut();
                        if s.get(&w_id_c) != Some(&text) {
                            s.insert(w_id_c.clone(), text);
                            needs_update = true;
                        }
                    }
                    if needs_update {
                        for cb in on_state_change_c.borrow().iter() {
                            cb();
                        }
                    }
                });

                if pref_group.is_none() {
                    let pg = adw::PreferencesGroup::new();
                    root_box.append(&pg);
                    pref_group = Some(pg);
                }
                tint(&entry_row, &widget.color);
                pref_group.as_ref().unwrap().add(&entry_row);
            }
            "button" => {
                let btn = gtk::Button::builder()
                    .label(widget.label.as_deref().unwrap_or("Action"))
                    .build();
                if widget.color.is_some() {
                    tint(&btn, &widget.color);
                } else {
                    btn.add_css_class("suggested-action");
                }
                btn.add_css_class("pill");
                btn.set_halign(gtk::Align::Center);
                btn.set_margin_top(12);
                btn.set_margin_bottom(12);
                btn.set_width_request(250);

                let action = widget.on_click.clone();
                let plugin_c = plugin.clone();
                let tab_state_c = tab_state.clone();
                let toast_overlay_c = toast_overlay.clone();

                btn.connect_clicked(move |_| {
                    if action.as_deref() == Some("execute_pipeline") {
                        let state_map = tab_state_c.borrow().clone();
                        let input_path = match state_map.get("file.path") {
                            Some(p) => std::path::PathBuf::from(p),
                            None => return, // No input file chosen
                        };

                        let dialog = gtk::FileDialog::new();
                        dialog.set_title("Save Output As");
                        let input_name = input_path
                            .file_name()
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_default();
                        let suggested =
                            input_path.with_file_name(plugin_c.output_name(&input_name));
                        if let Some(parent) = suggested.parent() {
                            dialog.set_initial_folder(Some(&gtk::gio::File::for_path(parent)));
                        }
                        if let Some(name) = suggested.file_name() {
                            dialog.set_initial_name(Some(&name.to_string_lossy()));
                        }

                        let plugin_c2 = plugin_c.clone();
                        let state_map_c = state_map.clone();
                        let toast_overlay_c2 = toast_overlay_c.clone();
                        dialog.save(
                            None::<&gtk::Window>,
                            None::<&gtk::gio::Cancellable>,
                            move |res| {
                                if let Ok(file) = res
                                    && let Some(out_path) = file.path()
                                {
                                    // Execute pipeline!
                                    let mime = ""; // Usually not needed by plugins with hardcoded pipelines
                                    let ctx = fm_core::types::FileContext::from_paths(
                                        &input_path,
                                        &out_path,
                                        mime,
                                    );

                                    match plugin_c2.default_pipeline() {
                                        Ok(steps) => {
                                            // Execute in a separate thread so we don't block GTK main loop
                                            let steps_c = steps.to_vec();
                                            let out_name = out_path.to_string_lossy().into_owned();
                                            let (tx, rx) =
                                                async_channel::bounded::<Result<(), String>>(1);
                                            std::thread::spawn(move || {
                                                let result = fm_core::engine::execute_pipeline(
                                                    &steps_c,
                                                    &ctx,
                                                    &state_map_c,
                                                );
                                                let _ = tx.send_blocking(result);
                                            });
                                            let toast_overlay_c3 = toast_overlay_c2.clone();
                                            gtk::glib::MainContext::default().spawn_local(
                                                async move {
                                                    let message = match rx.recv().await {
                                                        Ok(Ok(())) => {
                                                            format!("Saved to {}", out_name)
                                                        }
                                                        Ok(Err(e)) => {
                                                            format!("Plugin failed: {}", e)
                                                        }
                                                        Err(_) => {
                                                            "Plugin worker stopped unexpectedly"
                                                                .to_string()
                                                        }
                                                    };
                                                    toast_overlay_c3
                                                        .add_toast(adw::Toast::new(&message));
                                                },
                                            );
                                        }
                                        Err(e) => {
                                            toast_overlay_c2.add_toast(adw::Toast::new(&e));
                                        }
                                    }
                                }
                            },
                        );
                    }
                });

                root_box.append(&btn);
                pref_group = None;
            }
            "notice" => {
                let icon = if widget.color.as_deref() == Some("error") {
                    "dialog-warning-symbolic"
                } else {
                    "dialog-information-symbolic"
                };
                let text = gtk::Label::builder()
                    .label(widget.text.as_deref().unwrap_or_default())
                    .wrap(true)
                    .xalign(0.0)
                    .hexpand(true)
                    .build();
                let notice = gtk::Box::new(gtk::Orientation::Horizontal, 12);
                notice.add_css_class("notice");
                tint(&notice, &widget.color);
                notice.append(&gtk::Image::from_icon_name(icon));
                notice.append(&text);
                root_box.append(&notice);
                pref_group = None;
            }
            "html_view" => {
                let view = phone_preview::build(
                    widget.source.as_deref().unwrap_or_default(),
                    tab_state.clone(),
                    &on_state_change,
                );
                root_box.append(&view);
                pref_group = None;
            }
            other => {
                let label = gtk::Label::new(Some(&format!(
                    "This plugin uses a widget type this version cannot show: '{}'.\nSupported types: {}.",
                    other,
                    fm_core::validate::KNOWN_WIDGETS.join(", ")
                )));
                label.add_css_class("dim-label");
                label.set_wrap(true);
                root_box.append(&label);
            }
        }
    }

    let clamp = adw::Clamp::new();
    clamp.set_maximum_size(600);
    clamp.set_child(Some(&root_box));

    let scroller = gtk::ScrolledWindow::builder()
        .child(&clamp)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .build();

    scroller.upcast::<gtk::Widget>()
}
