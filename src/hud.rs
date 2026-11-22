//! In-game HUD overlay: Watermark, Coordinates, Active Modules ArrayList,
//! Aim Assist Target Info / FOV ring, and 2D ESP / Nametags overlays.

use mod_api::{ClientHost, DrawContext};

use crate::font::FontConfig;
use crate::module::{ModuleId, ModuleManager};
use crate::modules::aim_assist::AimTargetInfo;
use crate::modules::visuals;
use crate::theme::{Theme, hsv_to_argb, with_alpha};

pub fn render(
    host: &dyn ClientHost,
    draw: &mut dyn DrawContext,
    manager: &ModuleManager,
    target: Option<&AimTargetInfo>,
) {
    let hud = manager.get(ModuleId::Hud);
    let aim = manager.get(ModuleId::AimAssist);
    let theme = Theme::from_manager(manager, host.millis());
    let font = FontConfig::from_manager(manager);
    let size = draw.size();
    let bg_alpha = ((hud.float("bg_opacity", 0.60) * 255.0).round() as u32).clamp(0, 245) as u8;

    // 2D ESP boxes, health bars, and floating Nametags draw underneath HUD chrome.
    visuals::render_screen_overlays(host, draw, manager, &theme, font);

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

    if aim.enabled {
        if aim.bool("show_fov_circle", false) {
            let fov = aim.int("fov", 90) as f32;
            if fov < 360.0 {
                draw_fov_ring(draw, size, fov, with_alpha(theme.accent, 0x88));
            }
        }
        if aim.bool("show_target_hud", true)
            && let Some(info) = target
        {
            draw_target_reticle(host, draw, size, info.aim_point, theme.accent);
            draw_target_hud(draw, size, info, &theme, font, bg_alpha);
        }
    }
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

fn draw_target_hud(
    draw: &mut dyn DrawContext,
    size: [f32; 2],
    info: &AimTargetInfo,
    theme: &Theme,
    font: FontConfig,
    bg_alpha: u8,
) {
    let header = &info.name;
    let stats = format!("{:.1}m  |  {:.1}°", info.distance, info.angle_deg);
    let content_w = font
        .width(draw, header, font.bold_headers)
        .max(font.width(draw, &stats, false));
    let w = (content_w + 18.0).max(108.0);
    let has_hp = info.health.is_some() && info.max_health.is_some_and(|m| m > 0.0);
    let h = if has_hp { 33.0 } else { 24.0 };

    let x = size[0] * 0.5 + 16.0;
    let y = size[1] * 0.5 + 10.0;

    draw.fill(
        [x, y, w, h],
        with_alpha(theme.panel_bg, bg_alpha.max(0xA8)),
    );
    draw.fill([x, y, 2.0, h], theme.accent);
    font.draw_header(draw, header, [x + 6.0, y + 4.0], theme.text);
    font.draw_body(draw, &stats, [x + 6.0, y + 13.5], theme.text_muted);

    if let (Some(hp), Some(max_hp)) = (info.health, info.max_health)
        && max_hp > 0.0
    {
        let frac = (hp / max_hp).clamp(0.0, 1.0);
        let bar_x = x + 6.0;
        let bar_y = y + 24.5;
        let bar_w = w - 12.0;
        draw.fill([bar_x, bar_y, bar_w, 3.5], theme.slider_track);
        draw.fill([bar_x, bar_y, bar_w * frac, 3.5], theme.accent);
    }
}

fn draw_fov_ring(draw: &mut dyn DrawContext, size: [f32; 2], fov_deg: f32, color: u32) {
    let cx = size[0] * 0.5;
    let cy = size[1] * 0.5;
    let half_rad = (fov_deg * 0.5).clamp(1.0, 85.0).to_radians();
    let ref_rad = 35.0_f32.to_radians();
    let radius = ((half_rad.tan() / ref_rad.tan()) * (size[1] * 0.5))
        .clamp(12.0, size[1].min(size[0]) * 0.48);

    let segments = 36usize;
    let thickness = 1.0_f32;
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
