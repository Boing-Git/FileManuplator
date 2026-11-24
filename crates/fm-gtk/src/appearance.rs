//! Colour scheme and light/dark choice: saved per user, applied as GTK CSS.
//!
//! Every colour in `style.css` and `roles.css` is an M3 role (`@m3_primary`,
//! `@m3_error_container`, …). This module defines those roles for the chosen
//! scheme, so switching schemes re-colours the whole app, plugin tabs included.

use adw::prelude::*;
use gtk4 as gtk;
use libadwaita as adw;
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::fmt::Write as _;
use std::path::PathBuf;
use std::rc::Rc;

/// Fallback seed when the desktop has no accent colour to follow.
const LAVENDER: u32 = 0x6750A4;

/// What the user picked. `scheme` is `"system"`, a preset id from
/// [`fm_core::theme::PRESETS`] or `"#rrggbb"`; `mode` is `"auto"`, `"light"`
/// or `"dark"`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Appearance {
    pub scheme: String,
    pub mode: String,
}

impl Default for Appearance {
    fn default() -> Self {
        // Dark was the only look before schemes existed; keep it on upgrade.
        Self {
            scheme: "system".into(),
            mode: "dark".into(),
        }
    }
}

fn settings_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_default()
        .join("filemanuplator")
        .join("appearance.toml")
}

impl Appearance {
    fn load() -> Self {
        std::fs::read_to_string(settings_path())
            .ok()
            .and_then(|text| toml::from_str(&text).ok())
            .unwrap_or_default()
    }

    fn save(&self) {
        let path = settings_path();
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let result = toml::to_string(self)
            .map_err(|e| e.to_string())
            .and_then(|text| std::fs::write(&path, text).map_err(|e| e.to_string()));
        if let Err(e) = result {
            eprintln!("Could not save appearance to {}: {e}", path.display());
        }
    }
}

/// libadwaita's named colours, mapped onto M3 roles.
const ADWAITA_COLORS: &[(&str, &str)] = &[
    ("accent_color", "primary"),
    ("accent_bg_color", "primary"),
    ("accent_fg_color", "on_primary"),
    ("destructive_color", "error"),
    ("destructive_bg_color", "error_container"),
    ("destructive_fg_color", "on_error_container"),
    ("error_color", "error"),
    ("error_bg_color", "error_container"),
    ("error_fg_color", "on_error_container"),
    ("window_bg_color", "surface"),
    ("window_fg_color", "on_surface"),
    ("view_bg_color", "surface"),
    ("view_fg_color", "on_surface"),
    ("headerbar_bg_color", "surface"),
    ("headerbar_fg_color", "on_surface"),
    ("headerbar_backdrop_color", "surface"),
    ("sidebar_bg_color", "surface_container_low"),
    ("sidebar_fg_color", "on_surface"),
    ("card_bg_color", "surface_container"),
    ("card_fg_color", "on_surface"),
    ("popover_bg_color", "surface_container_high"),
    ("popover_fg_color", "on_surface"),
    ("dialog_bg_color", "surface_container_high"),
    ("dialog_fg_color", "on_surface"),
    ("borders", "outline_variant"),
];

/// The scheme choices in picker order: (id, label). `custom` opens a colour dialog.
fn choices() -> Vec<(&'static str, &'static str)> {
    let mut list = vec![("system", "Desktop accent")];
    list.extend(
        fm_core::theme::PRESETS
            .iter()
            .map(|(id, label, _)| (*id, *label)),
    );
    list.push(("custom", "Custom colour…"));
    list
}

pub struct Theme {
    provider: gtk::CssProvider,
    system_seed: u32,
    appearance: RefCell<Appearance>,
}

impl Theme {
    /// Read the desktop accent from `probe`, then take over colours for the
    /// display. Call before any other app CSS is added, or the accent read is
    /// our own.
    pub fn install(probe: &impl IsA<gtk::Widget>) -> Rc<Self> {
        let theme = Rc::new(Self {
            provider: gtk::CssProvider::new(),
            system_seed: desktop_accent(probe).unwrap_or(LAVENDER),
            appearance: RefCell::new(Appearance::load()),
        });
        // Above USER so a desktop-wide gtk.css (e.g. one generated from the
        // wallpaper) cannot half-override the scheme the user picked here.
        gtk::style_context_add_provider_for_display(
            &probe.display(),
            &theme.provider,
            gtk::STYLE_PROVIDER_PRIORITY_USER + 1,
        );
        let t = theme.clone();
        adw::StyleManager::default().connect_dark_notify(move |_| t.refresh_css());
        theme.apply();
        theme
    }

    fn appearance(&self) -> Appearance {
        self.appearance.borrow().clone()
    }

    fn set(&self, appearance: Appearance) {
        appearance.save();
        *self.appearance.borrow_mut() = appearance;
        self.apply();
    }

    fn seed_of(&self, scheme: &str) -> u32 {
        match scheme {
            "system" => self.system_seed,
            other => fm_core::theme::seed_for(other).unwrap_or(LAVENDER),
        }
    }

    fn apply(&self) {
        let scheme = match self.appearance().mode.as_str() {
            "light" => adw::ColorScheme::ForceLight,
            "dark" => adw::ColorScheme::ForceDark,
            _ => adw::ColorScheme::Default,
        };
        adw::StyleManager::default().set_color_scheme(scheme);
        // An unchanged light/dark emits no dark-notify, so re-colour here too.
        self.refresh_css();
    }

    fn refresh_css(&self) {
        let dark = adw::StyleManager::default().is_dark();
        let appearance = self.appearance();
        let roles = fm_core::theme::roles(self.seed_of(&appearance.scheme), dark);
        let mut css = String::new();
        for (role, hex) in &roles {
            let _ = writeln!(css, "@define-color m3_{role} {hex};");
        }
        for (name, role) in ADWAITA_COLORS {
            let _ = writeln!(css, "@define-color {name} @m3_{role};");
        }
        // libadwaita 1.6+ reads CSS variables; GTK before 4.16 cannot parse them.
        if gtk::check_version(4, 16, 0).is_none() {
            css.push_str(":root {\n");
            for (name, role) in ADWAITA_COLORS {
                if let Some((_, hex)) = roles.iter().find(|(r, _)| r == role) {
                    let _ = writeln!(css, "  --{}: {hex};", name.replace('_', "-"));
                }
            }
            css.push_str("}\n");
        }
        // Each swatch previews its own scheme's primary in the current mode.
        for (id, _) in choices() {
            let seed = match id {
                "custom" if appearance.scheme.starts_with('#') => self.seed_of(&appearance.scheme),
                "custom" => continue,
                other => self.seed_of(other),
            };
            if let Some((_, hex)) = fm_core::theme::roles(seed, dark)
                .into_iter()
                .find(|(role, _)| *role == "primary")
            {
                let _ = writeln!(css, ".swatch-{id} {{ background-color: {hex}; }}");
            }
        }
        self.provider.load_from_string(&css);
    }
}

/// The desktop theme's accent colour as 0xRRGGBB, if it defines one.
#[allow(deprecated)] // StyleContext is the only GTK 4.12 way to read a named theme colour.
fn desktop_accent(widget: &impl IsA<gtk::Widget>) -> Option<u32> {
    let c = widget.style_context().lookup_color("accent_bg_color")?;
    let byte = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u32;
    Some(byte(c.red()) << 16 | byte(c.green()) << 8 | byte(c.blue()))
}

/// The header-bar button that opens the scheme and light/dark picker.
pub fn picker_button(theme: &Rc<Theme>) -> gtk::MenuButton {
    let content = gtk::Box::new(gtk::Orientation::Vertical, 12);
    content.add_css_class("scheme-picker");

    let heading = |text: &str| {
        let label = gtk::Label::new(Some(text));
        label.set_halign(gtk::Align::Start);
        label.add_css_class("heading");
        label
    };

    // --- Scheme swatches -------------------------------------------------
    content.append(&heading("Colour scheme"));
    let grid = gtk::Grid::builder()
        .row_spacing(8)
        .column_spacing(8)
        .halign(gtk::Align::Center)
        .build();
    let current = theme.appearance();
    let mut group: Option<gtk::ToggleButton> = None;
    let swatches: Rc<RefCell<Vec<(&'static str, gtk::ToggleButton)>>> = Rc::default();
    for (i, (id, label)) in choices().into_iter().enumerate() {
        let swatch = gtk::ToggleButton::builder()
            .tooltip_text(label)
            .css_classes(["swatch", &format!("swatch-{id}")])
            .child(&gtk::Image::from_icon_name(if id == "custom" {
                "color-select-symbolic"
            } else {
                "object-select-symbolic"
            }))
            .build();
        swatch.update_property(&[gtk::accessible::Property::Label(label)]);
        let selected = current.scheme == id || (id == "custom" && current.scheme.starts_with('#'));
        match &group {
            Some(first) => swatch.set_group(Some(first)),
            None => group = Some(swatch.clone()),
        }
        swatch.set_active(selected);
        grid.attach(&swatch, (i % 4) as i32, (i / 4) as i32, 1, 1);
        swatches.borrow_mut().push((id, swatch));
    }
    for (id, swatch) in swatches.borrow().iter() {
        let (theme, id, swatches) = (theme.clone(), *id, swatches.clone());
        if id == "custom" {
            swatch.connect_clicked(move |button| {
                let previous = theme.appearance();
                let dialog = gtk::ColorDialog::builder()
                    .title("Pick a seed colour")
                    .with_alpha(false)
                    .build();
                let window = button.root().and_downcast::<gtk::Window>();
                let (theme, swatches) = (theme.clone(), swatches.clone());
                dialog.choose_rgba(
                    window.as_ref(),
                    None::<&gtk::gdk::RGBA>,
                    None::<&gtk::gio::Cancellable>,
                    move |result| match result {
                        Ok(rgba) => {
                            let byte = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
                            let hex = format!(
                                "#{:02x}{:02x}{:02x}",
                                byte(rgba.red()),
                                byte(rgba.green()),
                                byte(rgba.blue())
                            );
                            theme.set(Appearance {
                                scheme: hex,
                                ..theme.appearance()
                            });
                        }
                        // Cancelled: put the tick back where it was.
                        Err(_) => {
                            let back = if previous.scheme.starts_with('#') {
                                "custom"
                            } else {
                                previous.scheme.as_str()
                            };
                            if let Some((_, s)) = swatches.borrow().iter().find(|(i, _)| *i == back)
                            {
                                s.set_active(true);
                            }
                        }
                    },
                );
            });
        } else {
            swatch.connect_toggled(move |button| {
                if button.is_active() && theme.appearance().scheme != id {
                    theme.set(Appearance {
                        scheme: id.to_string(),
                        ..theme.appearance()
                    });
                }
            });
        }
    }
    content.append(&grid);

    // --- Light / dark: an M3 segmented button -------------------------------
    content.append(&heading("Mode"));
    let modes = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    modes.add_css_class("segmented");
    modes.set_homogeneous(true);
    let mut first: Option<gtk::ToggleButton> = None;
    for (id, label) in [("auto", "Auto"), ("light", "Light"), ("dark", "Dark")] {
        let check = gtk::Image::from_icon_name("object-select-symbolic");
        let inner = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        inner.set_halign(gtk::Align::Center);
        inner.append(&check);
        inner.append(&gtk::Label::new(Some(label)));
        let segment = gtk::ToggleButton::builder().child(&inner).build();
        match &first {
            Some(f) => segment.set_group(Some(f)),
            None => first = Some(segment.clone()),
        }
        let active = current.mode == id;
        segment.set_active(active);
        check.set_visible(active);
        let theme = theme.clone();
        segment.connect_toggled(move |button| {
            check.set_visible(button.is_active());
            if button.is_active() && theme.appearance().mode != id {
                theme.set(Appearance {
                    mode: id.to_string(),
                    ..theme.appearance()
                });
            }
        });
        modes.append(&segment);
    }
    content.append(&modes);

    let popover = gtk::Popover::builder().child(&content).build();
    gtk::MenuButton::builder()
        .icon_name("applications-graphics-symbolic")
        .tooltip_text("Colour scheme")
        .popover(&popover)
        .build()
}
