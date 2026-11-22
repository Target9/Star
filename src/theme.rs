//! ClickGUI and HUD color theme presets and ARGB utilities.

use crate::module::{ModuleId, ModuleManager};

pub const THEME_PRESETS: &[&str] = &[
    "Meteor Purple",
    "Midnight Cyan",
    "Crimson",
    "Emerald",
    "Amber Gold",
    "Monochrome",
    "Custom",
];

/// Resolved visual palette and sizing parameters read each frame by the ClickGUI and HUD.
#[derive(Clone, Copy, Debug)]
pub struct Theme {
    pub accent: u32,
    pub accent_soft: u32,
    pub panel_bg: u32,
    pub header_bg: u32,
    pub module_bg: u32,
    pub module_hover: u32,
    pub module_enabled_bg: u32,
    pub setting_bg: u32,
    pub slider_track: u32,
    pub border: u32,
    pub text: u32,
    pub text_muted: u32,
    pub backdrop_dim: u32,
    pub panel_width: f32,
    pub header_height: f32,
    pub row_height: f32,
    pub setting_height: f32,
    pub blur_background: bool,
    pub show_descriptions: bool,
    pub show_keybinds: bool,
    pub rainbow_accent: bool,
}

impl Theme {
    /// Builds the active theme from the `ClickGUI` module's settings.
    pub fn from_manager(manager: &ModuleManager, time_millis: u64) -> Self {
        let m = manager.get(ModuleId::ClickGui);
        let preset_idx = m.mode("preset", 0);
        let rainbow = m.bool("rainbow", false);

        let (base_accent, base_panel, base_header, base_text) = match preset_idx {
            0 => (0xFF91_3DE8, 0xE614_141C, 0xF51D_1A29, 0xFFF2_F2F7), // Meteor Purple
            1 => (0xFF1F_B6FF, 0xE60F_1720, 0xF515_2230, 0xFFE8_F4FA), // Midnight Cyan
            2 => (0xFFE5_3950, 0xE61A_1215, 0xF527_181E, 0xFFFA_EEEE), // Crimson
            3 => (0xFF28_C76F, 0xE611_1A16, 0xF518_2921, 0xFFEE_FAF3), // Emerald
            4 => (0xFFF5_A623, 0xE61B_1710, 0xF529_2216, 0xFFFB_F6EC), // Amber Gold
            5 => (0xFFD8_DCE6, 0xE612_1316, 0xF51C_1E24, 0xFFF5_F6FA), // Monochrome
            _ => (
                m.color("accent_color", 0xFF91_3DE8),
                m.color("panel_color", 0xE614_141C),
                m.color("header_color", 0xF51D_1A29),
                m.color("text_color", 0xFFF2_F2F7),
            ),
        };

        let accent = if rainbow {
            let hue = ((time_millis as f32 * 0.04) % 360.0) / 360.0;
            hsv_to_argb(hue, 0.72, 0.95, 0xFF)
        } else {
            base_accent
        };

        let panel_alpha = ((m.float("panel_opacity", 0.90) * 255.0).round() as u32).clamp(40, 255);
        let dim_alpha = ((m.float("backdrop_dim", 0.45) * 255.0).round() as u32).clamp(0, 230);

        let panel_bg = with_alpha(base_panel, panel_alpha as u8);
        let header_bg = with_alpha(base_header, panel_alpha.saturating_add(16).min(255) as u8);

        let panel_width = m.int("panel_width", 136) as f32;
        let row_height = m.int("row_height", 16) as f32;

        Self {
            accent,
            accent_soft: with_alpha(accent, 0x48),
            panel_bg,
            header_bg,
            module_bg: with_alpha(lighten(base_panel, 6), panel_alpha as u8),
            module_hover: with_alpha(lighten(base_panel, 20), panel_alpha as u8),
            module_enabled_bg: blend_rgb(panel_bg, accent, 0.24),
            setting_bg: with_alpha(darken(base_panel, 6), panel_alpha.saturating_add(8).min(255) as u8),
            slider_track: with_alpha(lighten(base_panel, 26), 0xCC),
            border: with_alpha(lighten(base_header, 28), 0xB8),
            text: base_text,
            text_muted: with_alpha(darken(base_text, 64), 0xE0),
            backdrop_dim: dim_alpha << 24,
            panel_width,
            header_height: (row_height + 4.0).max(18.0),
            row_height,
            setting_height: (row_height - 1.0).max(13.0),
            blur_background: m.bool("blur", true),
            show_descriptions: m.bool("descriptions", true),
            show_keybinds: m.bool("show_keybinds", true),
            rainbow_accent: rainbow,
        }
    }
}

pub fn argb_parts(argb: u32) -> (u8, u8, u8, u8) {
    (
        ((argb >> 24) & 0xFF) as u8,
        ((argb >> 16) & 0xFF) as u8,
        ((argb >> 8) & 0xFF) as u8,
        (argb & 0xFF) as u8,
    )
}

pub fn pack_argb(a: u8, r: u8, g: u8, b: u8) -> u32 {
    ((a as u32) << 24) | ((r as u32) << 16) | ((g as u32) << 8) | (b as u32)
}

pub fn with_alpha(argb: u32, a: u8) -> u32 {
    (argb & 0x00FF_FFFF) | ((a as u32) << 24)
}

pub fn lighten(argb: u32, delta: u8) -> u32 {
    let (a, r, g, b) = argb_parts(argb);
    pack_argb(
        a,
        r.saturating_add(delta),
        g.saturating_add(delta),
        b.saturating_add(delta),
    )
}

pub fn darken(argb: u32, delta: u8) -> u32 {
    let (a, r, g, b) = argb_parts(argb);
    pack_argb(
        a,
        r.saturating_sub(delta),
        g.saturating_sub(delta),
        b.saturating_sub(delta),
    )
}

pub fn blend_rgb(base: u32, over: u32, t: f32) -> u32 {
    let t = t.clamp(0.0, 1.0);
    let (a, r0, g0, b0) = argb_parts(base);
    let (_, r1, g1, b1) = argb_parts(over);
    let mix = |c0: u8, c1: u8| -> u8 {
        ((c0 as f32) * (1.0 - t) + (c1 as f32) * t).round().clamp(0.0, 255.0) as u8
    };
    pack_argb(a, mix(r0, r1), mix(g0, g1), mix(b0, b1))
}

pub fn hsv_to_argb(h: f32, s: f32, v: f32, alpha: u8) -> u32 {
    let h = h.rem_euclid(1.0) * 6.0;
    let i = h.floor() as i32;
    let f = h - i as f32;
    let p = v * (1.0 - s);
    let q = v * (1.0 - s * f);
    let t = v * (1.0 - s * (1.0 - f));
    let (r, g, b) = match i.rem_euclid(6) {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    };
    pack_argb(
        alpha,
        (r * 255.0).round().clamp(0.0, 255.0) as u8,
        (g * 255.0).round().clamp(0.0, 255.0) as u8,
        (b * 255.0).round().clamp(0.0, 255.0) as u8,
    )
}
