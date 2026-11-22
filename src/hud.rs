//! In-game HUD overlay: Watermark, Active Modules ArrayList, Aim Assist Target Info,
//! and Aim Assist FOV ring.

use mod_api::{ClientHost, DrawContext};

use crate::module::{ModuleId, ModuleManager};
use crate::modules::aim_assist::AimTargetInfo;
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
    let size = draw.size();
    let bg_alpha = ((hud.float("bg_opacity", 0.60) * 255.0).round() as u32).clamp(0, 245) as u8;

    if hud.enabled {
        if hud.bool("watermark", true) {
            draw_watermark(draw, &theme, bg_alpha);
        }
        if hud.bool("arraylist", true) {
            draw_arraylist(host, draw, manager, hud, &theme, size, bg_alpha);
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
            draw_target_hud(draw, size, info, &theme, bg_alpha);
        }
    }
}

fn draw_watermark(draw: &mut dyn DrawContext, theme: &Theme, bg_alpha: u8) {
    let title = "Star";
    let hint = " [RSHIFT]";
    let tw = draw.text_width(title);
    let hw = draw.text_width(hint);
    let w = tw + hw + 12.0;
    let h = 15.0;
    let x = 6.0;
    let y = 6.0;

    if bg_alpha > 0 {
        draw.fill([x, y, w, h], with_alpha(theme.panel_bg, bg_alpha));
    }
    draw.fill([x, y, 2.0, h], theme.accent);
    draw.text(title, [x + 6.0, y + 3.5], theme.accent, true);
    draw.text(hint, [x + 6.0 + tw, y + 3.5], theme.text_muted, false);
}

fn draw_arraylist(
    host: &dyn ClientHost,
    draw: &mut dyn DrawContext,
    manager: &ModuleManager,
    hud: &crate::module::Module,
    theme: &Theme,
    size: [f32; 2],
    bg_alpha: u8,
) {
    let show_tags = hud.bool("show_tags", true);
    let corner = hud.mode("corner", 0); // 0=TR, 1=TL, 2=BR, 3=BL
    let sort_mode = hud.mode("sort", 0); // 0=Width, 1=A-Z
    let accent_bar = hud.bool("accent_bar", true);

    let mut entries: Vec<(String, Option<String>, f32)> = manager
        .modules
        .iter()
        .filter(|m| m.enabled && m.show_in_arraylist)
        .map(|m| {
            let tag = if show_tags { m.hud_tag() } else { None };
            let name = m.name.to_owned();
            let width = draw.text_width(&name)
                + tag
                    .as_deref()
                    .map_or(0.0, |t| draw.text_width(" ") + draw.text_width(t));
            (name, tag, width)
        })
        .collect();

    if entries.is_empty() {
        return;
    }

    match sort_mode {
        0 => entries.sort_by(|a, b| b.2.total_cmp(&a.2).then_with(|| a.0.cmp(&b.0))),
        _ => entries.sort_by(|a, b| a.0.cmp(&b.0)),
    }

    let right = matches!(corner, 0 | 2);
    let bottom = matches!(corner, 2 | 3);
    let row_h = 12.0;
    let margin = 6.0;
    let start_y = if !bottom {
        if !right && hud.bool("watermark", true) {
            25.0
        } else {
            margin
        }
    } else {
        size[1] - margin - entries.len() as f32 * row_h
    };

    let base_hue = (host.millis() as f32 * 0.04) % 360.0;

    for (i, (name, tag, text_w)) in entries.iter().enumerate() {
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
        let ty = y + 2.0;
        draw.text(name, [tx, ty], row_accent, true);
        if let Some(tag) = tag {
            let nw = draw.text_width(name);
            draw.text(
                &format!(" {tag}"),
                [tx + nw, ty],
                theme.text_muted,
                false,
            );
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
    let Some([ndc_x, ndc_y, _]) = host.project(world_point) else {
        return;
    };
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
    bg_alpha: u8,
) {
    let header = &info.name;
    let stats = format!("{:.1}m  |  {:.1}°", info.distance, info.angle_deg);
    let content_w = draw.text_width(header).max(draw.text_width(&stats));
    let w = (content_w + 18.0).max(112.0);
    let has_hp = info.health.is_some() && info.max_health.is_some_and(|m| m > 0.0);
    let h = if has_hp { 34.0 } else { 25.0 };

    let x = size[0] * 0.5 + 16.0;
    let y = size[1] * 0.5 + 10.0;

    draw.fill(
        [x, y, w, h],
        with_alpha(theme.panel_bg, bg_alpha.max(0xA8)),
    );
    draw.fill([x, y, 2.0, h], theme.accent);
    draw.text(header, [x + 6.0, y + 4.0], theme.text, true);
    draw.text(&stats, [x + 6.0, y + 14.0], theme.text_muted, false);

    if let (Some(hp), Some(max_hp)) = (info.health, info.max_health)
        && max_hp > 0.0
    {
        let frac = (hp / max_hp).clamp(0.0, 1.0);
        let bar_x = x + 6.0;
        let bar_y = y + 25.0;
        let bar_w = w - 12.0;
        draw.fill([bar_x, bar_y, bar_w, 4.0], theme.slider_track);
        draw.fill([bar_x, bar_y, bar_w * frac, 4.0], theme.accent);
    }
}

fn draw_fov_ring(draw: &mut dyn DrawContext, size: [f32; 2], fov_deg: f32, color: u32) {
    let cx = size[0] * 0.5;
    let cy = size[1] * 0.5;
    // Approximate screen-space radius from half-FOV relative to a standard ~70° vertical FOV.
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
