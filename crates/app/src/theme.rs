//! Videleum's theme layer.
//!
//! The product uses GPUI Kit's semantic colours and adds a brand accent derived
//! from the logo's pink-to-violet gradient. Raw colour values live only here;
//! every view reads roles from the active theme.

use gpui_kit::component::{Theme, ThemeMode};
use gpui_kit::{App, WindowAppearance, hsla};

use videleum_core::ThemePreference;

/// HSL for the logo's dominant pink (`#FF1A6B`).
const BRAND_HUE: f32 = 339.0;

pub fn mode_for(preference: ThemePreference, appearance: WindowAppearance) -> ThemeMode {
    match preference {
        ThemePreference::System => match appearance {
            WindowAppearance::Dark | WindowAppearance::VibrantDark => ThemeMode::Dark,
            WindowAppearance::Light | WindowAppearance::VibrantLight => ThemeMode::Light,
        },
        ThemePreference::Light => ThemeMode::Light,
        ThemePreference::Dark => ThemeMode::Dark,
    }
}

/// Load the requested mode and apply Videleum's accent without disturbing the
/// rest of the registered theme.
pub fn apply(cx: &mut App, preference: ThemePreference, appearance: WindowAppearance) {
    let mode = mode_for(preference, appearance);
    Theme::change(mode, None, cx);
    Theme::update(cx, apply_accent);
}

fn apply_accent(theme: &mut Theme) {
    let dark = theme.mode.is_dark();
    let primary = if dark {
        hsla(BRAND_HUE / 360.0, 0.95, 0.63, 1.0)
    } else {
        hsla(BRAND_HUE / 360.0, 0.90, 0.53, 1.0)
    };
    let hover = if dark {
        hsla(BRAND_HUE / 360.0, 0.95, 0.69, 1.0)
    } else {
        hsla(BRAND_HUE / 360.0, 0.90, 0.60, 1.0)
    };
    let active = if dark {
        hsla(BRAND_HUE / 360.0, 0.95, 0.56, 1.0)
    } else {
        hsla(BRAND_HUE / 360.0, 0.90, 0.46, 1.0)
    };
    let foreground = hsla(0.0, 0.0, 1.0, 1.0);

    theme.primary = primary;
    theme.primary_hover = hover;
    theme.primary_active = active;
    theme.primary_foreground = foreground;
    theme.button_primary = primary;
    theme.button_primary_hover = hover;
    theme.button_primary_active = active;
    theme.button_primary_foreground = foreground;
    theme.ring = primary;
    theme.progress_bar = primary;
    theme.selection = primary.opacity(if dark { 0.32 } else { 0.22 });
    theme.link = primary;
    theme.link_hover = hover;
    theme.link_active = active;
    theme.sidebar_primary = primary;
    theme.sidebar_primary_foreground = foreground;
    theme.sidebar_accent = primary.opacity(if dark { 0.18 } else { 0.10 });
    theme.sidebar_accent_foreground = theme.sidebar_foreground;
    theme.radius = gpui_kit::px(8.0);
}
