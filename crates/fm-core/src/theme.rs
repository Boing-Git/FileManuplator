//! Material 3 colour schemes shared by every frontend.
//!
//! A scheme is a seed colour plus light or dark. The desktop app turns the
//! roles into GTK `@define-color`s and the mobile app into CSS custom
//! properties, so `color = "error"` in a plugin means the same on both.

use material_colors::{color::Rgb, theme::ThemeBuilder};

/// Seed colours offered in the scheme picker: (id, label, 0xRRGGBB).
pub const PRESETS: &[(&str, &str, u32)] = &[
    ("lavender", "Lavender", 0x6750A4), // the M3 baseline
    ("ocean", "Ocean", 0x0061A4),
    ("forest", "Forest", 0x386A20),
    ("ember", "Ember", 0xB33B15),
    ("rose", "Rose", 0x9C4146),
    ("sand", "Sand", 0x7D5700),
];

/// Colour roles a plugin may name with `color = "..."`. Each comes with its
/// `on_`, `_container` and `on_..._container` partners in [`roles`].
pub const PLUGIN_ROLES: &[&str] = &["primary", "secondary", "tertiary", "error"];

/// Every M3 role of the scheme grown from `seed`, as (`snake_case` role, `#rrggbb`).
pub fn roles(seed: u32, dark: bool) -> Vec<(&'static str, String)> {
    let theme = ThemeBuilder::with_source(Rgb::from_u32(seed)).build();
    let scheme = if dark {
        theme.schemes.dark
    } else {
        theme.schemes.light
    };
    scheme
        .into_iter()
        .map(|(role, rgb)| (role, format!("#{}", rgb.as_hex())))
        .collect()
}

/// Seed for a scheme id: a preset id, or a custom `#rrggbb`.
pub fn seed_for(id: &str) -> Option<u32> {
    if let Some((_, _, seed)) = PRESETS.iter().find(|(p, _, _)| *p == id) {
        return Some(*seed);
    }
    let hex = id.strip_prefix('#')?;
    if hex.len() != 6 {
        return None;
    }
    u32::from_str_radix(hex, 16).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn baseline_seed_gives_a_full_tonal_spot_scheme() {
        let roles = roles(0x6750A4, true);
        let primary = roles.iter().find(|(r, _)| *r == "primary").unwrap();
        // Tone 80 of the generated palette (M3's hand-tuned baseline is #d0bcff).
        assert_eq!(primary.1, "#cfbdfe");
        for role in PLUGIN_ROLES {
            for name in [
                role.to_string(),
                format!("on_{role}"),
                format!("{role}_container"),
                format!("on_{role}_container"),
            ] {
                assert!(roles.iter().any(|(r, _)| *r == name), "missing {name}");
            }
        }
    }

    #[test]
    fn seeds_resolve_from_presets_and_hex() {
        assert_eq!(seed_for("ocean"), Some(0x0061A4));
        assert_eq!(seed_for("#ff8800"), Some(0xFF8800));
        assert_eq!(seed_for("#f80"), None);
        assert_eq!(seed_for("plaid"), None);
    }
}
