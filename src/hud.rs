//! In-game HUD overlay: Watermark, Coordinates, Active Modules ArrayList,
//! Aim Assist Target Info / FOV ring, and 2D ESP / Nametags overlays.

use mod_api::{ClientHost, DrawContext};

use crate::font::FontConfig;
use crate::module::{ModuleId, ModuleManager};
use crate::modules::aim_assist::AimTargetInfo;
use crate::modules::trackers::Trackers;
use crate::modules::{extras, visuals};
use crate::theme::{Theme, hsv_to_argb, with_alpha};
use crate::toast::Toasts;

/// Draggable HUD widgets. Their positions are stored in `ModuleManager::hud_positions`.
pub const WIDGET_IDS: [&str; 3] = ["info", "keystrokes", "radar"];

/// One draggable HUD widget and where it currently sits on screen.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WidgetRect {
    pub id: &'static str,
    pub label: &'static str,
    pub rect: [f32; 4],
}

const KEY_CELL: f32 = 18.0;
const KEY_GAP: f32 = 2.0;
const KEYSTROKES_W: f32 = KEY_CELL * 3.0 + KEY_GAP * 2.0;
/// W row, ASD row, thin space bar, and mouse row, separated by three gaps.
const KEYSTROKES_H: f32 = KEY_CELL * 3.0 + 12.0 + KEY_GAP * 3.0;

pub fn render(
    host: &dyn ClientHost,
    draw: &mut dyn DrawContext,
    manager: &ModuleManager,
    interp: &visuals::EntityInterpCache,
    target: Option<&AimTargetInfo>,
    trackers: &Trackers,
    toasts: &mut Toasts,
) {
    let hud = manager.get(ModuleId::Hud);
    let aim = manager.get(ModuleId::AimAssist);
    let now_ms = host.millis();
    let theme = Theme::from_manager(manager, now_ms);
    let font = FontConfig::from_manager(manager);
    let size = draw.size();
    let bg_alpha = ((hud.float("bg_opacity", 0.60) * 255.0).round() as u32).clamp(0, 245) as u8;

    // 2D ESP boxes, health bars, and floating Nametags draw underneath HUD chrome.
    visuals::render_screen_overlays(host, draw, manager, interp, trackers, &theme, font);
    extras::render_overlays(host, draw, manager, interp, trackers, &theme, font);

    if hud.enabled {
        if hud.bool("watermark", true) {
            draw_watermark(draw, &theme, font, bg_alpha);
        }
        if hud.bool("coords", true) {
            draw_coords(host, draw, &theme, font, size, bg_alpha);
        }
        if hud.bool("arraylist", true) {
            draw_arraylist(host, draw, manager, hud, &theme, font, size, bg_alpha);
        }
    }
    draw_widgets(host, draw, manager, trackers, &theme, font, bg_alpha);

    toasts.update(now_ms);
    if hud.bool("toasts", true) {
        toasts.render(draw, &theme, font, now_ms);
    } else {
        toasts.clear();
    }

    if aim.enabled {
        if aim.bool("show_fov_circle", false) {
            let aim_fov = aim.int("fov", 90) as f32;
            if aim_fov < 360.0 {
                let camera_fov = host
                    .camera()
                    .map(|c| c.fov_degrees)
                    .filter(|f| f.is_finite() && *f > 1.0)
                    .unwrap_or_else(|| host.graphics_options().fov.max(30.0));
                draw_fov_ring(
                    draw,
                    size,
                    aim_fov,
                    camera_fov,
                    with_alpha(theme.accent, 0x88),
                );
            }
        }
        if aim.bool("show_target_hud", true)
            && let Some(info) = target
        {
            draw_target_reticle(host, draw, size, info.aim_point, theme.accent);
            draw_target_hud(draw, size, info, aim, &theme, font, bg_alpha);
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Draggable widgets: info panel, keystrokes, radar
// ─────────────────────────────────────────────────────────────────────────────

/// Text of the compass line for a Minecraft `yaw` (0 = south, 90 = west).
pub fn compass_label(yaw_deg: f32) -> String {
    const NAMES: [&str; 8] = ["N", "NE", "E", "SE", "S", "SW", "W", "NW"];
    let heading = (yaw_deg + 180.0).rem_euclid(360.0);
    let index = ((heading + 22.5) / 45.0).floor() as usize % 8;
    format!("{} {:.0}\u{b0}", NAMES[index], heading.round() % 360.0)
}

/// `65` -> `1:05`, `3725` -> `1:02:05`.
pub fn format_duration(total_secs: u64) -> String {
    let h = total_secs / 3600;
    let m = (total_secs % 3600) / 60;
    let s = total_secs % 60;
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

/// Label/value rows of the info panel, in display order.
fn info_lines(
    host: &dyn ClientHost,
    manager: &ModuleManager,
    trackers: &Trackers,
) -> Vec<(&'static str, String)> {
    let hud = manager.get(ModuleId::Hud);
    if !hud.enabled {
        return Vec::new();
    }
    let player = host.camera_entity();
    let mut lines = Vec::new();
    if hud.bool("show_fps", false) {
        lines.push(("FPS", trackers.fps().to_string()));
    }
    if hud.bool("show_ping", false) {
        let ping = player
            .and_then(|p| p.name(host))
            .and_then(|name| trackers.ping_of(&name));
        lines.push((
            "Ping",
            ping.map_or_else(|| "--".to_owned(), |ms| format!("{ms}ms")),
        ));
    }
    if hud.bool("show_speed", false) {
        lines.push(("Speed", format!("{:.1} b/s", trackers.speed_bps())));
    }
    if hud.bool("show_armor", false) {
        let armor = player.and_then(|p| extras::armor_percent(host, p));
        lines.push((
            "Armor",
            armor.map_or_else(|| "None".to_owned(), |pct| format!("{pct:.0}%")),
        ));
    }
    if hud.bool("show_compass", false)
        && let Some([yaw, _]) = player.and_then(|p| p.rotation(host))
    {
        lines.push(("Facing", compass_label(yaw)));
    }
    if hud.bool("show_session", false) {
        lines.push(("Session", format_duration(trackers.session_secs(host.millis()))));
    }
    lines
}

fn info_panel_size(
    draw: &dyn DrawContext,
    font: FontConfig,
    lines: &[(&'static str, String)],
) -> [f32; 2] {
    let label_w = lines
        .iter()
        .map(|(l, _)| font.width(draw, l, font.bold_headers))
        .fold(0.0_f32, f32::max);
    let value_w = lines
        .iter()
        .map(|(_, v)| font.width(draw, v, false))
        .fold(0.0_f32, f32::max);
    let row_h = (font.line_height() + 3.0).max(11.5);
    [label_w + value_w + 22.0, lines.len() as f32 * row_h + 6.0]
}

/// Where a widget sits when the player has not dragged it yet.
pub fn default_widget_origin(id: &str, screen: [f32; 2], widget: [f32; 2]) -> [f32; 2] {
    match id {
        "info" => [6.0, (screen[1] * 0.32).max(40.0)],
        "keystrokes" => [6.0, (screen[1] - widget[1] - 34.0).max(40.0)],
        _ => [(screen[0] - widget[0] - 8.0).max(0.0), (screen[1] * 0.30).max(40.0)],
    }
}

/// Keeps a widget fully on screen.
pub fn clamp_widget_origin(origin: [f32; 2], screen: [f32; 2], widget: [f32; 2]) -> [f32; 2] {
    [
        origin[0].clamp(0.0, (screen[0] - widget[0]).max(0.0)),
        origin[1].clamp(0.0, (screen[1] - widget[1]).max(0.0)),
    ]
}

/// The widgets that are currently switched on, with their on-screen rectangles.
pub fn widget_rects(
    host: &dyn ClientHost,
    draw: &dyn DrawContext,
    manager: &ModuleManager,
    trackers: &Trackers,
) -> Vec<WidgetRect> {
    let screen = draw.size();
    let font = FontConfig::from_manager(manager);
    let hud = manager.get(ModuleId::Hud);
    let radar = manager.get(ModuleId::Radar);
    let mut out = Vec::new();

    let place = |id: &'static str, label: &'static str, wsize: [f32; 2]| {
        let origin = manager
            .hud_positions
            .get(id)
            .copied()
            .unwrap_or_else(|| default_widget_origin(id, screen, wsize));
        let [x, y] = clamp_widget_origin(origin, screen, wsize);
        WidgetRect {
            id,
            label,
            rect: [x, y, wsize[0], wsize[1]],
        }
    };

    let lines = info_lines(host, manager, trackers);
    if !lines.is_empty() {
        out.push(place("info", "Info", info_panel_size(draw, font, &lines)));
    }
    if hud.enabled && hud.bool("keystrokes", false) {
        out.push(place("keystrokes", "Keystrokes", [KEYSTROKES_W, KEYSTROKES_H]));
    }
    if radar.enabled {
        let s = extras::radar_size(radar);
        out.push(place("radar", "Radar", [s, s]));
    }
    out
}

fn draw_widgets(
    host: &dyn ClientHost,
    draw: &mut dyn DrawContext,
    manager: &ModuleManager,
    trackers: &Trackers,
    theme: &Theme,
    font: FontConfig,
    bg_alpha: u8,
) {
    for widget in widget_rects(host, &*draw, manager, trackers) {
        let [x, y, _, _] = widget.rect;
        match widget.id {
            "info" => {
                let lines = info_lines(host, manager, trackers);
                draw_info_panel(draw, &lines, [x, y], widget.rect[2], theme, font, bg_alpha);
            }
            "keystrokes" => draw_keystrokes(host, draw, trackers, [x, y], theme, font, bg_alpha),
            "radar" => extras::render_radar(host, draw, manager.get(ModuleId::Radar), [x, y], theme, font),
            _ => {}
        }
    }
}

fn draw_info_panel(
    draw: &mut dyn DrawContext,
    lines: &[(&'static str, String)],
    origin: [f32; 2],
    width: f32,
    theme: &Theme,
    font: FontConfig,
    bg_alpha: u8,
) {
    let [x, y] = origin;
    let row_h = (font.line_height() + 3.0).max(11.5);
    let h = lines.len() as f32 * row_h + 6.0;
    if bg_alpha > 0 {
        draw.fill([x, y, width, h], with_alpha(theme.panel_bg, bg_alpha));
    }
    draw.fill([x, y, 2.0, h], theme.accent);
    for (i, (label, value)) in lines.iter().enumerate() {
        let ty = font.center_y(y + 3.0 + i as f32 * row_h, row_h);
        font.draw_header(draw, label, [x + 7.0, ty], theme.accent);
        let vw = font.width(&*draw, value, false);
        font.draw_body(draw, value, [x + width - 6.0 - vw, ty], theme.text);
    }
}

fn draw_keystrokes(
    host: &dyn ClientHost,
    draw: &mut dyn DrawContext,
    trackers: &Trackers,
    origin: [f32; 2],
    theme: &Theme,
    font: FontConfig,
    bg_alpha: u8,
) {
    let down = |name: &str| {
        host.mapping(name)
            .ok()
            .flatten()
            .is_some_and(|m| m.down)
    };
    let cps = trackers.clicks_per_second();
    let [x, y] = origin;
    let step = KEY_CELL + KEY_GAP;
    let fill_bg = bg_alpha.max(0x50);

    let key = |draw: &mut dyn DrawContext, rect: [f32; 4], label: &str, pressed: bool| {
        let bg = if pressed {
            with_alpha(theme.accent, 0xB8)
        } else {
            with_alpha(theme.panel_bg, fill_bg)
        };
        draw.fill(rect, bg);
        let tw = font.width(&*draw, label, false);
        font.draw_body(
            draw,
            label,
            [rect[0] + (rect[2] - tw) * 0.5, font.center_y(rect[1], rect[3])],
            if pressed { 0xFFFF_FFFF } else { theme.text },
        );
    };

    key(draw, [x + step, y, KEY_CELL, KEY_CELL], "W", down("key.forward"));
    key(draw, [x, y + step, KEY_CELL, KEY_CELL], "A", down("key.left"));
    key(draw, [x + step, y + step, KEY_CELL, KEY_CELL], "S", down("key.back"));
    key(draw, [x + step * 2.0, y + step, KEY_CELL, KEY_CELL], "D", down("key.right"));
    key(
        draw,
        [x, y + step * 2.0, KEYSTROKES_W, 12.0],
        "SPACE",
        down("key.jump"),
    );
    let mouse_y = y + step * 2.0 + 12.0 + KEY_GAP;
    let half_w = (KEYSTROKES_W - KEY_GAP) * 0.5;
    key(
        draw,
        [x, mouse_y, half_w, KEY_CELL],
        &format!("LMB {}", cps.left),
        down("key.attack"),
    );
    key(
        draw,
        [x + half_w + KEY_GAP, mouse_y, half_w, KEY_CELL],
        &format!("RMB {}", cps.right),
        down("key.use"),
    );
}

fn draw_watermark(draw: &mut dyn DrawContext, theme: &Theme, font: FontConfig, bg_alpha: u8) {
    let title = "STAR";
    let hint = " [RSHIFT]";
    let tw = font.width(draw, title, font.bold_headers);
    let hw = font.width(draw, hint, false);
    let w = tw + hw + 13.0;
    let h = (font.line_height() + 6.5).max(14.0);
    let x = 6.0;
    let y = 6.0;

    if bg_alpha > 0 {
        draw.fill([x, y, w, h], with_alpha(theme.panel_bg, bg_alpha));
    }
    draw.fill([x, y, 2.0, h], theme.accent);
    let ty = font.center_y(y, h);
    font.draw_header(draw, title, [x + 6.0, ty], theme.accent);
    font.draw_body(draw, hint, [x + 6.0 + tw, ty], theme.text_muted);
}

fn draw_coords(
    host: &dyn ClientHost,
    draw: &mut dyn DrawContext,
    theme: &Theme,
    font: FontConfig,
    size: [f32; 2],
    bg_alpha: u8,
) {
    let Some(player) = host.camera_entity() else {
        return;
    };
    let Some([px, py, pz]) = player.position(host) else {
        return;
    };
    let label = "XYZ ";
    let coords = format!("{px:.1}, {py:.1}, {pz:.1}");
    let lw = font.width(draw, label, font.bold_headers);
    let cw = font.width(draw, &coords, false);
    let w = lw + cw + 12.0;
    let h = (font.line_height() + 5.5).max(13.0);
    let x = 6.0;
    let y = size[1] - h - 6.0;

    if bg_alpha > 0 {
        draw.fill([x, y, w, h], with_alpha(theme.panel_bg, bg_alpha));
    }
    draw.fill([x, y, 2.0, h], theme.accent);
    let ty = font.center_y(y, h);
    font.draw_header(draw, label, [x + 5.5, ty], theme.accent);
    font.draw_body(draw, &coords, [x + 5.5 + lw, ty], theme.text);
}

#[allow(clippy::too_many_arguments)]
fn draw_arraylist(
    host: &dyn ClientHost,
    draw: &mut dyn DrawContext,
    manager: &ModuleManager,
    hud: &crate::module::Module,
    theme: &Theme,
    font: FontConfig,
    size: [f32; 2],
    bg_alpha: u8,
) {
    let show_tags = hud.bool("show_tags", true);
    let corner = hud.mode("corner", 0); // 0=TR, 1=TL, 2=BR, 3=BL
    let sort_mode = hud.mode("sort", 0); // 0=Width, 1=A-Z
    let accent_bar = hud.bool("accent_bar", true);

    let mut entries: Vec<(String, Option<String>, f32, f32)> = manager
        .modules
        .iter()
        .filter(|m| m.enabled && m.show_in_arraylist)
        .map(|m| {
            let tag = if show_tags { m.hud_tag() } else { None };
            let name = m.name.to_owned();
            let name_w = font.width(draw, &name, false);
            let tag_w = tag
                .as_deref()
                .map_or(0.0, |t| font.width(draw, &format!(" {t}"), false));
            (name, tag, name_w, name_w + tag_w)
        })
        .collect();

    if entries.is_empty() {
        return;
    }

    match sort_mode {
        0 => entries.sort_by(|a, b| b.3.total_cmp(&a.3).then_with(|| a.0.cmp(&b.0))),
        _ => entries.sort_by(|a, b| a.0.cmp(&b.0)),
    }

    let right = matches!(corner, 0 | 2);
    let bottom = matches!(corner, 2 | 3);
    let row_h = (font.line_height() + 4.0).max(11.5);
    let margin = 6.0;
    let start_y = if !bottom {
        if !right && hud.bool("watermark", true) {
            25.0
        } else {
            margin
        }
    } else {
        let bottom_pad = if !right && hud.bool("coords", true) {
            24.0
        } else {
            margin
        };
        size[1] - bottom_pad - entries.len() as f32 * row_h
    };

    let base_hue = (host.millis() as f32 * 0.04) % 360.0;

    for (i, (name, tag, name_w, text_w)) in entries.iter().enumerate() {
        let box_w = *text_w + 10.0;
        let x = if right {
            size[0] - margin - box_w
        } else {
            margin
        };
        let y = start_y + i as f32 * row_h;

        let row_accent = if theme.rainbow_accent {
            let hue = ((base_hue + i as f32 * 18.0) % 360.0) / 360.0;
            hsv_to_argb(hue, 0.72, 0.95, 0xFF)
        } else {
            theme.accent
        };

        if bg_alpha > 0 {
            draw.fill([x, y, box_w, row_h], with_alpha(theme.panel_bg, bg_alpha));
        }
        if accent_bar {
            let bar_x = if right { x + box_w - 2.0 } else { x };
            draw.fill([bar_x, y, 2.0, row_h], row_accent);
        }

        let tx = if right { x + 4.0 } else { x + 6.0 };
        let ty = font.center_y(y, row_h);
        font.draw_body(draw, name, [tx, ty], row_accent);
        if let Some(tag) = tag {
            font.draw_value(draw, &format!(" {tag}"), [tx + *name_w, ty], theme.text_muted);
        }
    }
}

fn draw_target_reticle(
    host: &dyn ClientHost,
    draw: &mut dyn DrawContext,
    size: [f32; 2],
    world_point: [f64; 3],
    color: u32,
) {
    let Some([ndc_x, ndc_y, ndc_z]) = host.project(world_point) else {
        return;
    };
    if ndc_z > 1.0 {
        return;
    }
    let sx = (ndc_x + 1.0) * 0.5 * size[0];
    let sy = (1.0 - ndc_y) * 0.5 * size[1];
    if sx < 0.0 || sx > size[0] || sy < 0.0 || sy > size[1] {
        return;
    }
    let r = 4.0;
    let t = 1.2;
    draw.fill([sx - r, sy - t * 0.5, r * 2.0, t], color);
    draw.fill([sx - t * 0.5, sy - r, t, r * 2.0], color);
}

#[allow(clippy::too_many_arguments)]
fn draw_target_hud(
    draw: &mut dyn DrawContext,
    size: [f32; 2],
    info: &AimTargetInfo,
    aim: &crate::module::Module,
    theme: &Theme,
    font: FontConfig,
    bg_alpha: u8,
) {
    let show_name = aim.bool("hud_name", true);
    let show_health = aim.bool("hud_health", true);
    let show_distance = aim.bool("hud_distance", true);
    let show_angle = aim.bool("hud_angle", true);
    let show_ping = aim.bool("hud_ping", true);

    let mut stats = String::new();
    if show_distance {
        stats.push_str(&format!("{:.1}m", info.distance));
    }
    if show_angle {
        if !stats.is_empty() {
            stats.push_str("  |  ");
        }
        stats.push_str(&format!("{:.1}°", info.angle_deg));
    }
    if show_ping {
        if !stats.is_empty() {
            stats.push_str("  |  ");
        }
        match info.ping_ms {
            Some(ms) => stats.push_str(&format!("{ms}ms")),
            None => stats.push_str("--ms"),
        }
    }

    let header = if show_name { info.name.as_str() } else { "" };
    if header.is_empty() && stats.is_empty() && !show_health {
        return;
    }

    let content_w = font
        .width(draw, header, font.bold_headers)
        .max(font.width(draw, &stats, false))
        .max(48.0);
    let w = (content_w + 18.0).max(108.0);
    let has_hp = show_health && info.health.is_some() && info.max_health.is_some_and(|m| m > 0.0);
    let mut h: f32 = 8.0;
    if show_name {
        h += 10.0;
    }
    if !stats.is_empty() {
        h += 10.0;
    }
    if has_hp {
        h += 9.0;
    }
    h = h.max(18.0);

    let x = size[0] * 0.5 + 16.0;
    let y = size[1] * 0.5 + 10.0;

    draw.fill(
        [x, y, w, h],
        with_alpha(theme.panel_bg, bg_alpha.max(0xA8)),
    );
    draw.fill([x, y, 2.0, h], theme.accent);

    let mut cursor_y = y + 4.0;
    if show_name {
        font.draw_header(draw, header, [x + 6.0, cursor_y], theme.text);
        cursor_y += 10.0;
    }
    if !stats.is_empty() {
        font.draw_body(draw, &stats, [x + 6.0, cursor_y], theme.text_muted);
        cursor_y += 10.0;
    }

    if let (true, Some(hp), Some(max_hp)) = (show_health, info.health, info.max_health)
        && max_hp > 0.0
    {
        let frac = (hp / max_hp).clamp(0.0, 1.0);
        let bar_x = x + 6.0;
        let bar_y = cursor_y + 1.0;
        let bar_w = w - 12.0;
        draw.fill([bar_x, bar_y, bar_w, 3.5], theme.slider_track);
        draw.fill([bar_x, bar_y, bar_w * frac, 3.5], theme.accent);
    }
}

/// Screen-space FOV ring sized so the Aim Assist angular cone matches the live camera FOV.
fn draw_fov_ring(
    draw: &mut dyn DrawContext,
    size: [f32; 2],
    aim_fov_deg: f32,
    camera_fov_deg: f32,
    color: u32,
) {
    let cx = size[0] * 0.5;
    let cy = size[1] * 0.5;
    let aim_half = (aim_fov_deg * 0.5).clamp(0.5, 89.0).to_radians();
    let cam_half = (camera_fov_deg * 0.5).clamp(1.0, 89.0).to_radians();
    // Project the Aim Assist angular radius through the camera's vertical FOV frustum.
    let radius = ((aim_half.tan() / cam_half.tan()) * (size[1] * 0.5))
        .clamp(8.0, size[1].min(size[0]) * 0.55);

    let segments = 48usize;
    let thickness = 1.15_f32;
    for i in 0..segments {
        let a0 = (i as f32 / segments as f32) * std::f32::consts::TAU;
        let a1 = ((i + 1) as f32 / segments as f32) * std::f32::consts::TAU;
        let (s0, c0) = a0.sin_cos();
        let (s1, c1) = a1.sin_cos();
        let r_in = (radius - thickness * 0.5).max(1.0);
        let r_out = radius + thickness * 0.5;
        draw.quad(
            [
                [cx + c0 * r_in, cy + s0 * r_in],
                [cx + c0 * r_out, cy + s0 * r_out],
                [cx + c1 * r_out, cy + s1 * r_out],
                [cx + c1 * r_in, cy + s1 * r_in],
            ],
            color,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compass_matches_minecraft_yaw_convention() {
        assert_eq!(compass_label(0.0), "S 180\u{b0}");
        assert_eq!(compass_label(90.0), "W 270\u{b0}");
        assert_eq!(compass_label(180.0), "N 0\u{b0}");
        assert_eq!(compass_label(-180.0), "N 0\u{b0}");
        assert_eq!(compass_label(-90.0), "E 90\u{b0}");
        assert!(compass_label(-135.0).starts_with("NE"));
        assert!(compass_label(720.0).starts_with('S'));
    }

    #[test]
    fn durations_format_with_and_without_hours() {
        assert_eq!(format_duration(0), "0:00");
        assert_eq!(format_duration(65), "1:05");
        assert_eq!(format_duration(3725), "1:02:05");
    }

    #[test]
    fn widgets_stay_on_screen() {
        let screen = [320.0, 180.0];
        let widget = [60.0, 40.0];
        assert_eq!(clamp_widget_origin([-20.0, 500.0], screen, widget), [0.0, 140.0]);
        assert_eq!(clamp_widget_origin([100.0, 50.0], screen, widget), [100.0, 50.0]);
        // A widget larger than the screen pins to the origin instead of panicking.
        assert_eq!(clamp_widget_origin([30.0, 30.0], [50.0, 50.0], [80.0, 80.0]), [0.0, 0.0]);
        for id in WIDGET_IDS {
            let o = default_widget_origin(id, screen, widget);
            assert!(o[0] >= 0.0 && o[1] >= 0.0, "{id}: {o:?}");
        }
    }

    #[test]
    fn fov_ring_grows_with_aim_fov_and_shrinks_with_camera_fov() {
        let aim_half = 45.0_f32.to_radians();
        let cam_70 = 35.0_f32.to_radians();
        let cam_90 = 45.0_f32.to_radians();
        let h = 360.0_f32;
        let r70 = (aim_half.tan() / cam_70.tan()) * (h * 0.5);
        let r90 = (aim_half.tan() / cam_90.tan()) * (h * 0.5);
        assert!(r70 > r90, "wider camera FOV must shrink the on-screen ring");
        let aim_small_half = (20.0_f32 * 0.5).to_radians();
        let r_small = (aim_small_half.tan() / cam_90.tan()) * (h * 0.5);
        assert!(r90 > r_small, "smaller Aim Assist FOV must shrink the ring");
    }
}
