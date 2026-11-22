//! Render / Visual modules: ESP, Tracers, Nametags, No Fog, Zoom, and Camera.

use mod_api::{
    CameraDistanceEvent, CameraShakeEvent, ClientHost, DrawContext, EntityRef, RenderFogEvent,
    RenderNameTagEvent, WorldDepth, WorldDrawContext, WorldLine, WorldQuad, WorldRenderView, Zoom,
};

use crate::entity_filter::{EntityFilter, classify_entity_id, pretty_entity_id};
use crate::font::FontConfig;
use crate::module::{Category, Module, ModuleId, ModuleManager};
use crate::settings::Setting;
use crate::theme::{Theme, with_alpha};

// ─────────────────────────────────────────────────────────────────────────────
// Module definitions
// ─────────────────────────────────────────────────────────────────────────────

pub fn build_esp_module() -> Module {
    Module {
        id: ModuleId::Esp,
        key: "esp",
        name: "ESP",
        description: "Highlights selected entities with 3D/2D boxes and health bars through walls.",
        category: Category::Render,
        enabled: false,
        keybind: None,
        expanded: false,
        show_in_arraylist: true,
        settings: vec![
            Setting::entity_filter(
                "entities",
                "Entities",
                "Select the exact entity types highlighted by ESP.",
                EntityFilter::players_and_hostile(),
            )
            .group("Targets"),
            Setting::bool(
                "ignore_invisible",
                "Ignore Invisible",
                "Hide invisible entities from ESP.",
                false,
            ),
            Setting::float(
                "range",
                "Range",
                "Maximum distance in blocks to render ESP boxes.",
                8.0,
                128.0,
                2.0,
                64.0,
            )
            .suffix("m"),
            Setting::mode(
                "style",
                "Style",
                "3D world-space boxes or crisp 2D screen-space overlays.",
                &["3D Box", "3D Fill + Box", "2D Box", "2D Corners"],
                1,
            )
            .group("Appearance"),
            Setting::bool(
                "through_walls",
                "Through Walls",
                "Draw 3D ESP boxes visible through solid terrain.",
                true,
            ),
            Setting::float(
                "line_width",
                "Line Width",
                "Outline thickness in pixels.",
                1.0,
                4.0,
                0.2,
                1.6,
            )
            .suffix("px"),
            Setting::float(
                "fill_opacity",
                "Fill Opacity",
                "Translucent fill opacity for 3D Fill + Box and 2D Box styles.",
                0.0,
                0.60,
                0.05,
                0.16,
            ),
            Setting::bool(
                "health_bar",
                "Health Bar",
                "Show a live vertical health bar beside each living entity.",
                true,
            ),
            Setting::mode(
                "color_mode",
                "Color Mode",
                "Color by entity category, distance gradient, or custom color.",
                &["Category", "Custom", "Distance"],
                0,
            )
            .group("Color"),
            Setting::color(
                "custom_color",
                "Custom Color",
                "Box color used when Color Mode is set to Custom.",
                0xFF91_3DE8,
            ),
        ],
    }
}

pub fn build_tracers_module() -> Module {
    Module {
        id: ModuleId::Tracers,
        key: "tracers",
        name: "Tracers",
        description: "Draws 3D lines from your view directly to selected entities.",
        category: Category::Render,
        enabled: false,
        keybind: None,
        expanded: false,
        show_in_arraylist: true,
        settings: vec![
            Setting::entity_filter(
                "entities",
                "Entities",
                "Select which entity types have tracer lines drawn to them.",
                EntityFilter::players_only(),
            )
            .group("Targets"),
            Setting::bool(
                "ignore_invisible",
                "Ignore Invisible",
                "Skip invisible entities.",
                false,
            ),
            Setting::float(
                "range",
                "Range",
                "Maximum distance in blocks for tracers.",
                8.0,
                128.0,
                2.0,
                64.0,
            )
            .suffix("m"),
            Setting::mode(
                "target_point",
                "Target Point",
                "Where on each entity the tracer line ends.",
                &["Feet", "Center", "Eyes"],
                1,
            )
            .group("Appearance"),
            Setting::float(
                "line_width",
                "Line Width",
                "Tracer line thickness in pixels.",
                1.0,
                4.0,
                0.2,
                1.4,
            )
            .suffix("px"),
            Setting::mode(
                "color_mode",
                "Color Mode",
                "Color tracers by entity category, custom color, or distance.",
                &["Category", "Custom", "Distance"],
                0,
            )
            .group("Color"),
            Setting::color(
                "custom_color",
                "Custom Color",
                "Tracer color when Color Mode is Custom.",
                0xFF91_3DE8,
            ),
        ],
    }
}

pub fn build_nametags_module() -> Module {
    Module {
        id: ModuleId::Nametags,
        key: "nametags",
        name: "Nametags",
        description: "Renders clean floating name, health, and distance tags above selected entities.",
        category: Category::Render,
        enabled: false,
        keybind: None,
        expanded: false,
        show_in_arraylist: true,
        settings: vec![
            Setting::entity_filter(
                "entities",
                "Entities",
                "Select which entity types display custom Star nametags.",
                EntityFilter::players_only(),
            )
            .group("Targets"),
            Setting::bool(
                "ignore_invisible",
                "Ignore Invisible",
                "Skip invisible entities.",
                false,
            ),
            Setting::float(
                "range",
                "Range",
                "Maximum distance in blocks to display nametags.",
                8.0,
                128.0,
                2.0,
                64.0,
            )
            .suffix("m"),
            Setting::bool(
                "show_health",
                "Show Health",
                "Display current HP next to the entity's name.",
                true,
            )
            .group("Display"),
            Setting::bool(
                "show_distance",
                "Show Distance",
                "Display distance in blocks beside the name.",
                true,
            ),
            Setting::float(
                "scale",
                "Tag Scale",
                "Size multiplier for floating nametags.",
                0.65,
                1.35,
                0.05,
                0.90,
            )
            .suffix("x"),
            Setting::float(
                "bg_opacity",
                "Background",
                "Opacity of the nametag pill background.",
                0.0,
                0.95,
                0.05,
                0.65,
            ),
            Setting::bool(
                "hide_vanilla",
                "Hide Vanilla Tag",
                "Hide Minecraft's built-in nametag for entities shown by Star Nametags.",
                true,
            ),
        ],
    }
}

pub fn build_no_fog_module() -> Module {
    Module {
        id: ModuleId::NoFog,
        key: "no_fog",
        name: "No Fog",
        description: "Clears atmospheric, underwater, lava, and powder snow fog.",
        category: Category::Render,
        enabled: false,
        keybind: None,
        expanded: false,
        show_in_arraylist: true,
        settings: vec![
            Setting::bool("atmospheric", "Atmospheric", "Remove distance/nether fog.", true)
                .group("Fog Types"),
            Setting::bool("water", "Water", "Remove underwater fog.", true),
            Setting::bool("lava", "Lava", "Remove lava vision fog.", true),
            Setting::bool("powder_snow", "Powder Snow", "Remove powder snow fog.", true),
        ],
    }
}

pub fn build_zoom_module() -> Module {
    Module {
        id: ModuleId::Zoom,
        key: "zoom",
        name: "Zoom",
        description: "Smooth camera zoom with reduced mouse sensitivity.",
        category: Category::Render,
        enabled: false,
        keybind: Some("key.keyboard.c".to_owned()),
        expanded: false,
        show_in_arraylist: true,
        settings: vec![
            Setting::float(
                "factor",
                "Zoom Factor",
                "Camera magnification multiplier (higher = closer).",
                1.5,
                10.0,
                0.5,
                4.0,
            )
            .suffix("x")
            .group("Camera"),
            Setting::bool(
                "smooth",
                "Smooth Camera",
                "Ease into zoom smoothly like cinematic camera.",
                true,
            ),
            Setting::float(
                "turn_scale",
                "Mouse Scale",
                "Mouse sensitivity multiplier while zoomed in.",
                0.10,
                1.00,
                0.05,
                0.35,
            )
            .suffix("x"),
        ],
    }
}

pub fn build_camera_module() -> Module {
    Module {
        id: ModuleId::Camera,
        key: "camera",
        name: "Camera",
        description: "Removes hurt/camera shake and customizes third-person distance.",
        category: Category::Render,
        enabled: false,
        keybind: None,
        expanded: false,
        show_in_arraylist: true,
        settings: vec![
            Setting::bool(
                "no_shake",
                "No Camera Shake",
                "Suppress camera shake effects while playing.",
                true,
            )
            .group("View"),
            Setting::float(
                "third_person_dist",
                "3rd Person Dist",
                "Camera distance behind the player in third-person view.",
                2.0,
                14.0,
                0.5,
                5.5,
            )
            .suffix("m"),
        ],
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// 3D World Geometry (ESP + Tracers)
// ─────────────────────────────────────────────────────────────────────────────

pub fn render_world_geometry(
    host: &dyn ClientHost,
    view: &WorldRenderView,
    draw: &mut dyn WorldDrawContext,
    manager: &ModuleManager,
) {
    if !host.playing() {
        return;
    }
    let esp = manager.get(ModuleId::Esp);
    let tracers = manager.get(ModuleId::Tracers);
    let esp_3d = esp.enabled && matches!(esp.mode("style", 1), 0 | 1);
    if !esp_3d && !tracers.enabled {
        return;
    }

    let Some(player) = host.camera_entity() else {
        return;
    };
    let Some(level) = player.level(host) else {
        return;
    };
    let theme = Theme::from_manager(manager, host.millis());

    let max_range = if esp_3d && tracers.enabled {
        esp.float("range", 64.0).max(tracers.float("range", 64.0))
    } else if esp_3d {
        esp.float("range", 64.0)
    } else {
        tracers.float("range", 64.0)
    };

    let cam = view.camera;
    let pad = max_range + 2.0;
    let Some(entities) = level.entities_in(
        host,
        [cam[0] - pad, cam[1] - pad, cam[2] - pad],
        [cam[0] + pad, cam[1] + pad, cam[2] + pad],
    ) else {
        return;
    };

    let usage = draw.usage();
    let max_lines = usage.line_capacity.saturating_sub(usage.lines) as usize;
    let max_quads = usage.quad_capacity.saturating_sub(usage.quads) as usize;

    let mut lines: Vec<WorldLine> = Vec::new();
    let mut quads: Vec<WorldQuad> = Vec::new();

    // Compute tracer start point slightly ahead of the camera along the look direction
    // so lines remain centered on screen without clipping the near plane.
    let tracer_origin = if tracers.enabled {
        let dir = player.view_vector(host).unwrap_or_else(|| {
            let [yaw, pitch] = player.rotation(host).unwrap_or([0.0, 0.0]);
            look_dir(yaw, pitch)
        });
        [
            cam[0] + dir[0] * 0.35,
            cam[1] + dir[1] * 0.35,
            cam[2] + dir[2] * 0.35,
        ]
    } else {
        cam
    };

    let esp_filter = esp.entity_filter("entities");
    let tracer_filter = tracers.entity_filter("entities");

    let esp_range = esp.float("range", 64.0);
    let esp_depth = if esp.bool("through_walls", true) {
        WorldDepth::ThroughWalls
    } else {
        WorldDepth::Tested
    };
    let esp_width = (esp.float("line_width", 1.6) as f32).clamp(0.5, 6.0);
    let esp_fill = esp.mode("style", 1) == 1;
    let esp_fill_alpha = ((esp.float("fill_opacity", 0.16) * 255.0).round() as u32).clamp(0, 200) as u8;
    let esp_color_mode = esp.mode("color_mode", 0);
    let esp_custom_color = esp.color("custom_color", theme.accent);
    let esp_ignore_invis = esp.bool("ignore_invisible", false);

    let tr_range = tracers.float("range", 64.0);
    let tr_width = (tracers.float("line_width", 1.4) as f32).clamp(0.5, 6.0);
    let tr_point = tracers.mode("target_point", 1);
    let tr_color_mode = tracers.mode("color_mode", 0);
    let tr_custom_color = tracers.color("custom_color", theme.accent);
    let tr_ignore_invis = tracers.bool("ignore_invisible", false);

    for entity in entities {
        if entity.id() == player.id() || entity.spectator(host) == Some(true) {
            continue;
        }
        let Some(kind) = entity.kind(host) else {
            continue;
        };
        let kind_id = kind.id(host);
        let invisible = entity.is_invisible(host) == Some(true);

        let want_esp = esp_3d
            && !(esp_ignore_invis && invisible)
            && esp_filter.is_some_and(|f| f.contains(&kind_id));
        let want_tracer = tracers.enabled
            && !(tr_ignore_invis && invisible)
            && tracer_filter.is_some_and(|f| f.contains(&kind_id));

        if !want_esp && !want_tracer {
            continue;
        }

        let Some([mut lo, mut hi]) = entity.bounding_box(host) else {
            continue;
        };
        // Smooth sub-tick interpolation for fluid 3D boxes.
        if let Some(vel) = entity.velocity(host) {
            let pt = view.partial_tick as f64;
            for i in 0..3 {
                let d = vel[i] * pt * 0.5;
                if d.is_finite() && d.abs() < 2.0 {
                    lo[i] += d;
                    hi[i] += d;
                }
            }
        }
        let center = [
            (lo[0] + hi[0]) * 0.5,
            (lo[1] + hi[1]) * 0.5,
            (lo[2] + hi[2]) * 0.5,
        ];
        let d = dist3(cam, center);

        if want_esp && d <= esp_range {
            let color = entity_color(
                &kind_id,
                d,
                esp_range,
                esp_color_mode,
                esp_custom_color,
                theme.accent,
            );
            if lines.len() + 12 <= max_lines {
                push_box_lines(&mut lines, lo, hi, color, esp_width, esp_depth);
            }
            if esp_fill && esp_fill_alpha > 0 && quads.len() + 6 <= max_quads {
                push_box_quads(
                    &mut quads,
                    lo,
                    hi,
                    with_alpha(color, esp_fill_alpha),
                    esp_depth,
                );
            }
        }

        if want_tracer && d <= tr_range && lines.len() < max_lines {
            let target = match tr_point {
                0 => [center[0], lo[1] + 0.05, center[2]],
                2 => entity.eye_position(host).unwrap_or([center[0], hi[1] - 0.15, center[2]]),
                _ => center,
            };
            let color = entity_color(
                &kind_id,
                d,
                tr_range,
                tr_color_mode,
                tr_custom_color,
                theme.accent,
            );
            lines.push(WorldLine::new(
                tracer_origin,
                target,
                with_alpha(color, 0xD8),
                tr_width,
                WorldDepth::ThroughWalls,
            ));
        }
    }

    if !quads.is_empty() {
        let _ = draw.quads(&quads);
    }
    if !lines.is_empty() {
        let _ = draw.lines(&lines);
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// 2D Screen Overlay (2D ESP, Health Bars, Nametags)
// ─────────────────────────────────────────────────────────────────────────────

pub fn render_screen_overlays(
    host: &dyn ClientHost,
    draw: &mut dyn DrawContext,
    manager: &ModuleManager,
    theme: &Theme,
    font: FontConfig,
) {
    if !host.playing() {
        return;
    }
    let esp = manager.get(ModuleId::Esp);
    let nametags = manager.get(ModuleId::Nametags);

    let esp_style = esp.mode("style", 1);
    let esp_2d_box = esp.enabled && matches!(esp_style, 2 | 3);
    let esp_health_bar = esp.enabled && esp.bool("health_bar", true);
    if !esp_2d_box && !esp_health_bar && !nametags.enabled {
        return;
    }

    let Some(player) = host.camera_entity() else {
        return;
    };
    let Some(eyes) = player.eye_position(host) else {
        return;
    };
    let Some(level) = player.level(host) else {
        return;
    };

    let esp_range = esp.float("range", 64.0);
    let nt_range = nametags.float("range", 64.0);
    let max_range = if (esp_2d_box || esp_health_bar) && nametags.enabled {
        esp_range.max(nt_range)
    } else if esp_2d_box || esp_health_bar {
        esp_range
    } else {
        nt_range
    };

    let pad = max_range + 2.0;
    let Some(entities) = level.entities_in(
        host,
        [eyes[0] - pad, eyes[1] - pad, eyes[2] - pad],
        [eyes[0] + pad, eyes[1] + pad, eyes[2] + pad],
    ) else {
        return;
    };

    let size = draw.size();
    let esp_filter = esp.entity_filter("entities");
    let nt_filter = nametags.entity_filter("entities");

    let esp_ignore_invis = esp.bool("ignore_invisible", false);
    let esp_line_w = (esp.float("line_width", 1.6) as f32).clamp(1.0, 3.0);
    let esp_fill_alpha =
        ((esp.float("fill_opacity", 0.16) * 255.0).round() as u32).clamp(0, 180) as u8;
    let esp_color_mode = esp.mode("color_mode", 0);
    let esp_custom_color = esp.color("custom_color", theme.accent);

    let nt_ignore_invis = nametags.bool("ignore_invisible", false);
    let nt_show_hp = nametags.bool("show_health", true);
    let nt_show_dist = nametags.bool("show_distance", true);
    let nt_scale = nametags.float("scale", 0.90) as f32;
    let nt_bg_alpha =
        ((nametags.float("bg_opacity", 0.65) * 255.0).round() as u32).clamp(0, 240) as u8;
    let nt_font = font.scaled(nt_scale);

    // Sort far-to-near so closer nametags draw on top.
    let mut list: Vec<(f64, EntityRef, String, bool)> = Vec::new();
    for entity in entities {
        if entity.id() == player.id() || entity.spectator(host) == Some(true) {
            continue;
        }
        let Some(kind) = entity.kind(host) else {
            continue;
        };
        let kind_id = kind.id(host);
        let invis = entity.is_invisible(host) == Some(true);
        let pos = entity.position(host).unwrap_or(eyes);
        let d = dist3(eyes, pos);
        list.push((d, entity, kind_id, invis));
    }
    list.sort_by(|a, b| b.0.total_cmp(&a.0));

    for (d, entity, kind_id, invis) in list {
        let want_esp = (esp_2d_box || esp_health_bar)
            && d <= esp_range
            && !(esp_ignore_invis && invis)
            && esp_filter.is_some_and(|f| f.contains(&kind_id));
        let want_nt = nametags.enabled
            && d <= nt_range
            && !(nt_ignore_invis && invis)
            && nt_filter.is_some_and(|f| f.contains(&kind_id));

        if !want_esp && !want_nt {
            continue;
        }

        let Some([lo, hi]) = entity.bounding_box(host) else {
            continue;
        };

        if want_esp
            && let Some([x0, y0, x1, y1]) = project_aabb(host, size, lo, hi)
        {
            let w = (x1 - x0).max(2.0);
            let h = (y1 - y0).max(2.0);
            let color = entity_color(
                &kind_id,
                d,
                esp_range,
                esp_color_mode,
                esp_custom_color,
                theme.accent,
            );

            if esp_2d_box {
                if esp_style == 2 {
                    if esp_fill_alpha > 0 {
                        draw.fill([x0, y0, w, h], with_alpha(color, esp_fill_alpha));
                    }
                    draw_rect_outline(draw, [x0, y0, w, h], esp_line_w, color);
                } else {
                    draw_corner_box(draw, [x0, y0, w, h], esp_line_w, color);
                }
            }

            if esp_health_bar
                && let Some(living) = host.living(entity)
                && let (Some(hp), Some(max_hp)) = (living.health(host), living.max_health(host))
                && max_hp > 0.0
            {
                let frac = (hp / max_hp).clamp(0.0, 1.0);
                let bar_w = 2.5;
                let bar_x = x0 - bar_w - 2.0;
                let fill_h = h * frac;
                draw.fill([bar_x - 0.5, y0 - 0.5, bar_w + 1.0, h + 1.0], 0xB000_0000);
                draw.fill(
                    [bar_x, y0 + (h - fill_h), bar_w, fill_h],
                    health_color(frac),
                );
            }
        }

        if want_nt {
            let anchor = [(lo[0] + hi[0]) * 0.5, hi[1] + 0.35, (lo[2] + hi[2]) * 0.5];
            if let Some([sx, sy]) = project_point(host, size, anchor) {
                let name = entity
                    .name(host)
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| pretty_entity_id(&kind_id));
                let hp_part = if nt_show_hp
                    && let Some(living) = host.living(entity)
                    && let (Some(hp), Some(max_hp)) = (living.health(host), living.max_health(host))
                    && max_hp > 0.0
                {
                    let frac = (hp / max_hp).clamp(0.0, 1.0);
                    Some((format!(" {:.1}", hp), health_color(frac)))
                } else {
                    None
                };
                let dist_part = if nt_show_dist {
                    Some(format!(" {:.0}m", d))
                } else {
                    None
                };

                let name_w = nt_font.width(draw, &name, false);
                let hp_w = hp_part
                    .as_ref()
                    .map_or(0.0, |(t, _)| nt_font.width(draw, t, false));
                let dist_w = dist_part
                    .as_ref()
                    .map_or(0.0, |t| nt_font.width(draw, t, false));

                let pad_x = 4.0;
                let box_w = name_w + hp_w + dist_w + pad_x * 2.0;
                let box_h = nt_font.line_height() + 4.5;
                let bx = sx - box_w * 0.5;
                let by = sy - box_h;

                if nt_bg_alpha > 0 {
                    draw.fill([bx, by, box_w, box_h], with_alpha(theme.panel_bg, nt_bg_alpha));
                }
                let cat_color = classify_entity_id(&kind_id).color(theme.accent);
                draw.fill([bx, by, box_w, 1.2], with_alpha(cat_color, 0xCC));

                let ty = nt_font.center_y(by, box_h);
                let mut cx = bx + pad_x;
                nt_font.draw_body(draw, &name, [cx, ty], theme.text);
                cx += name_w;
                if let Some((hp_txt, hp_col)) = hp_part {
                    nt_font.draw_body(draw, &hp_txt, [cx, ty], hp_col);
                    cx += hp_w;
                }
                if let Some(dist_txt) = dist_part {
                    nt_font.draw_body(draw, &dist_txt, [cx, ty], theme.text_muted);
                }
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Hook handlers: NoFog, Zoom, Camera, Nametags vanilla suppression
// ─────────────────────────────────────────────────────────────────────────────

pub fn apply_no_fog(module: &Module, event: &mut RenderFogEvent<'_>) {
    if !module.enabled {
        return;
    }
    let clear = match event.fog_type {
        mod_api::client::fog::FogType::Atmospheric => module.bool("atmospheric", true),
        mod_api::client::fog::FogType::Water => module.bool("water", true),
        mod_api::client::fog::FogType::Lava => module.bool("lava", true),
        mod_api::client::fog::FogType::PowderSnow => module.bool("powder_snow", true),
    };
    if clear {
        event.fog.environmental_start = 10_000.0;
        event.fog.environmental_end = 20_000.0;
        event.fog.render_distance_start = 10_000.0;
        event.fog.render_distance_end = 20_000.0;
        event.fog.sky_end = 20_000.0;
        event.fog.cloud_end = 20_000.0;
    }
}

pub fn apply_zoom(module: &Module) -> Option<Zoom> {
    if !module.enabled {
        return None;
    }
    Some(Zoom {
        factor: (module.float("factor", 4.0) as f32).clamp(1.2, 16.0),
        smooth: module.bool("smooth", true),
        turn: (module.float("turn_scale", 0.35) as f32).clamp(0.05, 1.0),
    })
}

pub fn apply_camera_shake(module: &Module, event: &mut CameraShakeEvent<'_>) {
    if !module.enabled || !module.bool("no_shake", true) {
        return;
    }
    event.shake.pitch = 0.0;
    event.shake.yaw = 0.0;
    event.shake.roll = 0.0;
    event.shake.offset = [0.0, 0.0, 0.0];
}

pub fn apply_camera_distance(module: &Module, event: &mut CameraDistanceEvent<'_>) {
    if !module.enabled {
        return;
    }
    event.distance = (module.float("third_person_dist", 5.5) as f32).clamp(1.0, 20.0);
}

pub fn apply_nametag_hide(module: &Module, event: &mut RenderNameTagEvent<'_>) {
    if !module.enabled || !module.bool("hide_vanilla", true) {
        return;
    }
    if module
        .entity_filter("entities")
        .is_some_and(|f| f.contains(event.kind))
    {
        event.visible = false;
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Geometry & projection helpers
// ─────────────────────────────────────────────────────────────────────────────

fn entity_color(
    kind_id: &str,
    distance: f64,
    max_range: f64,
    mode: usize,
    custom: u32,
    accent: u32,
) -> u32 {
    match mode {
        1 => custom,
        2 => {
            let frac = (distance / max_range.max(1.0)).clamp(0.0, 1.0) as f32;
            health_color(frac)
        }
        _ => classify_entity_id(kind_id).color(accent),
    }
}

fn health_color(frac: f32) -> u32 {
    let f = frac.clamp(0.0, 1.0);
    let (r, g, b) = if f < 0.5 {
        let t = f * 2.0;
        (235.0, 65.0 + 160.0 * t, 60.0)
    } else {
        let t = (f - 0.5) * 2.0;
        (235.0 - 160.0 * t, 225.0, 60.0 + 70.0 * t)
    };
    0xFF00_0000 | ((r as u32) << 16) | ((g as u32) << 8) | (b as u32)
}

fn push_box_lines(
    out: &mut Vec<WorldLine>,
    lo: [f64; 3],
    hi: [f64; 3],
    color: u32,
    width: f32,
    depth: WorldDepth,
) {
    let c = [
        [lo[0], lo[1], lo[2]],
        [hi[0], lo[1], lo[2]],
        [hi[0], lo[1], hi[2]],
        [lo[0], lo[1], hi[2]],
        [lo[0], hi[1], lo[2]],
        [hi[0], hi[1], lo[2]],
        [hi[0], hi[1], hi[2]],
        [lo[0], hi[1], hi[2]],
    ];
    let edges = [
        (0, 1),
        (1, 2),
        (2, 3),
        (3, 0),
        (4, 5),
        (5, 6),
        (6, 7),
        (7, 4),
        (0, 4),
        (1, 5),
        (2, 6),
        (3, 7),
    ];
    for (a, b) in edges {
        out.push(WorldLine::new(c[a], c[b], color, width, depth));
    }
}

fn push_box_quads(
    out: &mut Vec<WorldQuad>,
    lo: [f64; 3],
    hi: [f64; 3],
    color: u32,
    depth: WorldDepth,
) {
    let [x0, y0, z0] = lo;
    let [x1, y1, z1] = hi;
    let faces = [
        // Bottom & Top
        [[x0, y0, z0], [x1, y0, z0], [x1, y0, z1], [x0, y0, z1]],
        [[x0, y1, z1], [x1, y1, z1], [x1, y1, z0], [x0, y1, z0]],
        // North & South
        [[x0, y1, z0], [x0, y0, z0], [x1, y0, z0], [x1, y1, z0]],
        [[x1, y1, z1], [x1, y0, z1], [x0, y0, z1], [x0, y1, z1]],
        // West & East
        [[x0, y1, z1], [x0, y0, z1], [x0, y0, z0], [x0, y1, z0]],
        [[x1, y1, z0], [x1, y0, z0], [x1, y0, z1], [x1, y1, z1]],
    ];
    for corners in faces {
        out.push(WorldQuad::new(corners, color, depth));
    }
}

fn project_point(host: &dyn ClientHost, size: [f32; 2], world: [f64; 3]) -> Option<[f32; 2]> {
    let [ndc_x, ndc_y, ndc_z] = host.project(world)?;
    if !ndc_x.is_finite() || !ndc_y.is_finite() || !ndc_z.is_finite() || ndc_z > 1.0 {
        return None;
    }
    let sx = (ndc_x + 1.0) * 0.5 * size[0];
    let sy = (1.0 - ndc_y) * 0.5 * size[1];
    if sx < -80.0 || sx > size[0] + 80.0 || sy < -80.0 || sy > size[1] + 80.0 {
        return None;
    }
    Some([sx, sy])
}

fn project_aabb(
    host: &dyn ClientHost,
    size: [f32; 2],
    lo: [f64; 3],
    hi: [f64; 3],
) -> Option<[f32; 4]> {
    let corners = [
        [lo[0], lo[1], lo[2]],
        [hi[0], lo[1], lo[2]],
        [hi[0], lo[1], hi[2]],
        [lo[0], lo[1], hi[2]],
        [lo[0], hi[1], lo[2]],
        [hi[0], hi[1], lo[2]],
        [hi[0], hi[1], hi[2]],
        [lo[0], hi[1], hi[2]],
    ];
    let mut min_x = f32::INFINITY;
    let mut min_y = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut max_y = f32::NEG_INFINITY;
    let mut visible = 0usize;

    for corner in corners {
        if let Some([sx, sy]) = project_point(host, size, corner) {
            min_x = min_x.min(sx);
            min_y = min_y.min(sy);
            max_x = max_x.max(sx);
            max_y = max_y.max(sy);
            visible += 1;
        }
    }
    if visible == 0 || max_x < 0.0 || min_x > size[0] || max_y < 0.0 || min_y > size[1] {
        return None;
    }
    Some([min_x, min_y, max_x, max_y])
}

fn draw_rect_outline(draw: &mut dyn DrawContext, rect: [f32; 4], t: f32, color: u32) {
    let [x, y, w, h] = rect;
    draw.fill([x, y, w, t], color);
    draw.fill([x, y + h - t, w, t], color);
    draw.fill([x, y, t, h], color);
    draw.fill([x + w - t, y, t, h], color);
}

fn draw_corner_box(draw: &mut dyn DrawContext, rect: [f32; 4], t: f32, color: u32) {
    let [x, y, w, h] = rect;
    let cw = (w * 0.28).clamp(3.0, 14.0);
    let ch = (h * 0.24).clamp(3.0, 14.0);
    // Top-left
    draw.fill([x, y, cw, t], color);
    draw.fill([x, y, t, ch], color);
    // Top-right
    draw.fill([x + w - cw, y, cw, t], color);
    draw.fill([x + w - t, y, t, ch], color);
    // Bottom-left
    draw.fill([x, y + h - t, cw, t], color);
    draw.fill([x, y + h - ch, t, ch], color);
    // Bottom-right
    draw.fill([x + w - cw, y + h - t, cw, t], color);
    draw.fill([x + w - t, y + h - ch, t, ch], color);
}

fn dist3(a: [f64; 3], b: [f64; 3]) -> f64 {
    let dx = a[0] - b[0];
    let dy = a[1] - b[1];
    let dz = a[2] - b[2];
    (dx * dx + dy * dy + dz * dz).sqrt()
}

fn look_dir(yaw_deg: f32, pitch_deg: f32) -> [f64; 3] {
    let yaw = (yaw_deg as f64).to_radians();
    let pitch = (pitch_deg as f64).to_radians();
    let cp = pitch.cos();
    [-yaw.sin() * cp, -pitch.sin(), yaw.cos() * cp]
}
