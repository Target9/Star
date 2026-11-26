//! Lock Visuals: one shared "locked onto a target" display for every module that locks.
//!
//! # How a module plugs in
//!
//! 1. Add a [`LockSource`] to [`LOCK_SOURCES`]. That gives Lock Visuals a "show for this
//!    module" switch automatically.
//! 2. While the module is locked onto something, build a [`LockTarget`] each tick or frame
//!    and call [`LockBoard::publish`] with it. Publish `None` when the lock is gone.
//!
//! Lock Visuals then draws the target HUD, reticle, 3D box and link line for whichever
//! source is locked, with its own configuration. Module-specific indicators (like Aim
//! Assist's FOV ring and range circle) stay on that module.
//!
//! The screen part is [`render_screen`] (called from the HUD hook) and the world part is
//! [`render_world`] (called from the custom geometry hook).

use mod_api::{
    ClientHost, DrawContext, EntityRef, WorldDepth, WorldDrawContext, WorldLine, WorldQuad,
    WorldRenderView,
};

use crate::font::FontConfig;
use crate::module::{Category, Module, ModuleId, ModuleManager};
use crate::modules::visuals::{
    EntityInterpCache, dist3, health_color, look_dir, project_point, push_box_lines,
    push_box_quads,
};
use crate::settings::Setting;
use crate::theme::{Theme, with_alpha};

/// How long the lock-on animation takes to settle.
const LOCK_ANIM_MS: u64 = 220;
/// A lock regained on the same target within this time counts as the same lock.
const REACQUIRE_MS: u64 = 250;

// ─────────────────────────────────────────────────────────────────────────────
// Sources: modules that can lock onto a target
// ─────────────────────────────────────────────────────────────────────────────

/// A module that can publish locks to the [`LockBoard`].
pub struct LockSource {
    pub id: ModuleId,
    /// Id of the on/off setting Lock Visuals shows for this source.
    pub setting: &'static str,
    /// Name shown in the target HUD and the settings list.
    pub label: &'static str,
    pub description: &'static str,
}

/// Every module that publishes locks. Add new lock-on modules here.
pub const LOCK_SOURCES: &[LockSource] = &[LockSource {
    id: ModuleId::AimAssist,
    setting: "src_aim_assist",
    label: "Aim Assist",
    description: "Show Lock Visuals while Aim Assist is locked onto a target.",
}];

fn source_label(id: ModuleId) -> &'static str {
    LOCK_SOURCES
        .iter()
        .find(|s| s.id == id)
        .map_or("Target", |s| s.label)
}

fn source_allowed(module: &Module, id: ModuleId) -> bool {
    LOCK_SOURCES
        .iter()
        .find(|s| s.id == id)
        .is_none_or(|s| module.bool(s.setting, true))
}

// ─────────────────────────────────────────────────────────────────────────────
// The shared lock state
// ─────────────────────────────────────────────────────────────────────────────

/// What a module reports while it is locked onto a target.
#[derive(Clone, Debug)]
pub struct LockTarget {
    pub source: ModuleId,
    pub entity: Option<EntityRef>,
    pub name: String,
    pub distance: f64,
    pub angle_deg: f32,
    pub health: Option<f32>,
    pub max_health: Option<f32>,
    /// Tab-list latency in ms when known (players only).
    pub ping_ms: Option<i32>,
    /// World point the source is aiming at.
    pub aim_point: [f64; 3],
}

impl LockTarget {
    /// Whether `other` is the same thing the source was already locked onto.
    fn same_target(&self, other: &LockTarget) -> bool {
        match (self.entity, other.entity) {
            (Some(a), Some(b)) => a.id() == b.id(),
            _ => self.name == other.name,
        }
    }
}

#[derive(Default)]
struct Slot {
    source: Option<ModuleId>,
    live: Option<LockTarget>,
    /// The most recent target, kept so it can fade out after the lock ends.
    last: Option<LockTarget>,
    since_ms: u64,
    lost_ms: Option<u64>,
    updated_ms: u64,
}

/// Locks currently reported by all sources.
#[derive(Default)]
pub struct LockBoard {
    slots: Vec<Slot>,
}

/// The lock Lock Visuals is currently showing.
pub struct LockView<'a> {
    pub target: &'a LockTarget,
    /// Milliseconds since this lock started (drives the lock-on animation).
    pub locked_ms: u64,
    /// `1.0` while locked, falling to `0.0` over the fade-out time afterwards.
    pub fade: f32,
    /// True while the source still holds the lock.
    pub live: bool,
}

impl LockBoard {
    /// Reports the current lock of `source`, or `None` when it has none.
    pub fn publish(&mut self, source: ModuleId, target: Option<LockTarget>, now_ms: u64) {
        let index = match self.slots.iter().position(|s| s.source == Some(source)) {
            Some(i) => i,
            None => {
                self.slots.push(Slot {
                    source: Some(source),
                    ..Slot::default()
                });
                self.slots.len() - 1
            }
        };
        let Some(slot) = self.slots.get_mut(index) else {
            return;
        };
        match target {
            Some(target) => {
                // A lock that drops for a moment and returns to the same target keeps its
                // timer, so the lock-on animation does not replay on every flicker.
                let recently = slot.live.is_some()
                    || slot
                        .lost_ms
                        .is_some_and(|lost| now_ms.saturating_sub(lost) < REACQUIRE_MS);
                let same =
                    recently && slot.last.as_ref().is_some_and(|cur| cur.same_target(&target));
                if !same {
                    slot.since_ms = now_ms;
                }
                slot.lost_ms = None;
                slot.updated_ms = now_ms;
                slot.last = Some(target.clone());
                slot.live = Some(target);
            }
            None => {
                if slot.live.take().is_some() {
                    slot.lost_ms = Some(now_ms);
                }
            }
        }
    }

    /// Forgets every lock (the mod was switched off or the world changed).
    pub fn clear(&mut self) {
        self.slots.clear();
    }

    /// The lock to show: the most recently updated live one from an allowed source, or
    /// else the most recently lost one still inside the `linger_ms` fade-out.
    pub fn active(
        &self,
        now_ms: u64,
        linger_ms: u64,
        allow: impl Fn(ModuleId) -> bool,
    ) -> Option<LockView<'_>> {
        let allowed = |slot: &Slot| slot.source.is_some_and(&allow);

        let live = self
            .slots
            .iter()
            .filter(|s| allowed(s) && s.live.is_some())
            .max_by_key(|s| s.updated_ms)
            .and_then(|s| {
                Some(LockView {
                    target: s.live.as_ref()?,
                    locked_ms: now_ms.saturating_sub(s.since_ms),
                    fade: 1.0,
                    live: true,
                })
            });
        if live.is_some() || linger_ms == 0 {
            return live;
        }

        self.slots
            .iter()
            .filter(|s| allowed(s))
            .filter_map(|s| Some((s, s.lost_ms?)))
            .filter(|(_, lost)| now_ms.saturating_sub(*lost) < linger_ms)
            .max_by_key(|(_, lost)| *lost)
            .and_then(|(s, lost)| {
                let elapsed = now_ms.saturating_sub(lost) as f32;
                Some(LockView {
                    target: s.last.as_ref()?,
                    locked_ms: lost.saturating_sub(s.since_ms),
                    fade: (1.0 - elapsed / linger_ms as f32).clamp(0.0, 1.0),
                    live: false,
                })
            })
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Module definition
// ─────────────────────────────────────────────────────────────────────────────

pub fn build_module() -> Module {
    let mut settings: Vec<Setting> = LOCK_SOURCES
        .iter()
        .map(|s| Setting::bool(s.setting, s.label, s.description, true))
        .collect();
    if let Some(first) = settings.first_mut() {
        first.group = Some("Sources");
    }

    settings.extend([
        Setting::float(
            "linger",
            "Fade Out",
            "How long the visuals fade away after a lock ends (0 hides them at once).",
            0.0,
            2000.0,
            50.0,
            300.0,
        )
        .suffix("ms")
        .group("Behavior"),
        Setting::bool(
            "lock_anim",
            "Lock-On Animation",
            "Make the reticle snap in from a larger size when a new lock starts.",
            true,
        ),
        // Target HUD
        Setting::bool(
            "show_target_hud",
            "Target HUD",
            "Show a compact info box for the locked target.",
            true,
        )
        .group("Target HUD"),
        Setting::mode(
            "hud_position",
            "Position",
            "Where the Target HUD sits on screen.",
            &[
                "Beside Crosshair",
                "Below Crosshair",
                "Above Crosshair",
                "Top Center",
                "Bottom Center",
            ],
            0,
        ),
        Setting::float(
            "hud_scale",
            "Scale",
            "Size multiplier for the Target HUD.",
            0.7,
            1.6,
            0.05,
            1.0,
        )
        .suffix("x"),
        Setting::float(
            "hud_opacity",
            "Background",
            "Opacity of the Target HUD background.",
            0.2,
            1.0,
            0.05,
            0.66,
        ),
        Setting::bool("hud_name", "Name", "Show the target's name.", true),
        Setting::bool("hud_health", "Health", "Show the target's health bar.", true),
        Setting::mode(
            "hud_health_color",
            "Health Bar Color",
            "Fill the health bar with the accent color or a red-to-green health gradient.",
            &["Accent", "Health Gradient"],
            0,
        ),
        Setting::bool(
            "hud_distance",
            "Distance",
            "Show the distance to the target.",
            true,
        ),
        Setting::bool(
            "hud_angle",
            "Angle",
            "Show the angular offset to the target.",
            true,
        ),
        Setting::bool(
            "hud_ping",
            "Ping",
            "Show the target's tab-list ping (players only).",
            true,
        ),
        Setting::bool(
            "hud_source",
            "Locked By",
            "Show which module holds the lock.",
            false,
        ),
        // Reticle
        Setting::bool(
            "show_reticle",
            "Reticle",
            "Mark the aim point of the locked target on screen.",
            true,
        )
        .group("Reticle"),
        Setting::mode(
            "reticle_style",
            "Style",
            "Shape of the reticle.",
            &["Cross", "Brackets", "Ring", "Diamond"],
            0,
        ),
        Setting::float(
            "reticle_size",
            "Size",
            "Reticle radius in pixels.",
            2.0,
            24.0,
            0.5,
            5.0,
        )
        .suffix("px"),
        Setting::float(
            "reticle_thickness",
            "Thickness",
            "Reticle line thickness in pixels.",
            0.6,
            3.0,
            0.1,
            1.2,
        )
        .suffix("px"),
        // 3D box
        Setting::bool(
            "show_box",
            "Target Box",
            "Outline the locked entity with a 3D box.",
            false,
        )
        .group("Target Box"),
        Setting::mode(
            "box_style",
            "Style",
            "Outline only, or outline with a translucent fill.",
            &["3D Box", "3D Fill + Box"],
            0,
        ),
        Setting::float(
            "box_fill",
            "Fill Opacity",
            "Opacity of the fill when the style includes one.",
            0.02,
            0.6,
            0.02,
            0.16,
        ),
        Setting::float(
            "box_width",
            "Line Width",
            "Box outline thickness in pixels.",
            0.5,
            5.0,
            0.1,
            1.8,
        )
        .suffix("px"),
        Setting::bool(
            "box_through_walls",
            "Through Walls",
            "Show the box through solid terrain.",
            true,
        ),
        // Link line
        Setting::bool(
            "show_link",
            "Link Line",
            "Draw a line from your view to the locked target.",
            false,
        )
        .group("Link Line"),
        Setting::float(
            "link_width",
            "Line Width",
            "Link line thickness in pixels.",
            0.5,
            4.0,
            0.1,
            1.4,
        )
        .suffix("px"),
        // Color
        Setting::mode(
            "color_mode",
            "Color Mode",
            "How reticle, box and link line are colored: theme accent, by the target's health, by its distance, or a custom color.",
            &["Accent", "Health", "Distance", "Custom"],
            0,
        )
        .group("Color"),
        Setting::color(
            "custom_color",
            "Custom Color",
            "Color used when Color Mode is Custom.",
            0xFFFF_4D5E,
        ),
    ]);

    Module {
        id: ModuleId::LockVisuals,
        key: "lock_visuals",
        name: "Lock Visuals",
        description: "Shared lock-on display: target HUD, reticle, box and link line for any module that locks onto a target. Module-specific indicators stay on that module.",
        category: Category::Render,
        enabled: false,
        keybind: None,
        expanded: false,
        show_in_arraylist: false,
        settings,
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Shared helpers (pure, unit-tested)
// ─────────────────────────────────────────────────────────────────────────────

/// `color` with its alpha multiplied by `fade` (`0.0..=1.0`).
pub fn fade_color(color: u32, fade: f32) -> u32 {
    let a = ((color >> 24) as f32 * fade.clamp(0.0, 1.0)).round() as u32;
    (color & 0x00FF_FFFF) | (a.min(255) << 24)
}

/// Fraction of `0..=1` for health, or `None` when the target has no usable health.
fn health_fraction(target: &LockTarget) -> Option<f32> {
    match (target.health, target.max_health) {
        (Some(hp), Some(max)) if max > 0.0 => Some((hp / max).clamp(0.0, 1.0)),
        _ => None,
    }
}

/// The color Lock Visuals uses for `target` under the module's Color Mode.
pub fn lock_color(module: &Module, target: &LockTarget, accent: u32) -> u32 {
    match module.mode("color_mode", 0) {
        1 => health_fraction(target).map_or(accent, health_color),
        2 => {
            // Distance mode is relative to a typical short-range lock (~6 blocks).
            let frac = (target.distance / 6.0).clamp(0.0, 1.0) as f32;
            health_color(1.0 - frac)
        }
        3 => module.color("custom_color", accent),
        _ => accent,
    }
}

/// Reticle scale for the lock-on animation: starts large and settles to `1.0`.
pub fn lock_anim_scale(locked_ms: u64, enabled: bool) -> f32 {
    if !enabled || locked_ms >= LOCK_ANIM_MS {
        return 1.0;
    }
    let t = locked_ms as f32 / LOCK_ANIM_MS as f32;
    let inv = 1.0 - t;
    1.0 + 1.4 * inv * inv * inv
}

/// Top-left corner of a `size` box for each Target HUD position mode, kept on screen.
pub fn hud_origin(mode: usize, screen: [f32; 2], size: [f32; 2]) -> [f32; 2] {
    let (cx, cy) = (screen[0] * 0.5, screen[1] * 0.5);
    let [x, y] = match mode {
        1 => [cx - size[0] * 0.5, cy + 22.0],
        2 => [cx - size[0] * 0.5, cy - size[1] - 22.0],
        3 => [cx - size[0] * 0.5, 30.0],
        4 => [cx - size[0] * 0.5, screen[1] - size[1] - 56.0],
        _ => [cx + 16.0, cy + 10.0],
    };
    [
        x.clamp(2.0, (screen[0] - size[0] - 2.0).max(2.0)),
        y.clamp(2.0, (screen[1] - size[1] - 2.0).max(2.0)),
    ]
}

fn current_lock<'a>(
    module: &Module,
    board: &'a LockBoard,
    now_ms: u64,
) -> Option<LockView<'a>> {
    let linger = module.float("linger", 300.0).clamp(0.0, 5000.0) as u64;
    board.active(now_ms, linger, |id| source_allowed(module, id))
}

// ─────────────────────────────────────────────────────────────────────────────
// Screen rendering
// ─────────────────────────────────────────────────────────────────────────────

/// Draws the screen-space parts: reticle and Target HUD.
pub fn render_screen(
    host: &dyn ClientHost,
    draw: &mut dyn DrawContext,
    manager: &ModuleManager,
    board: &LockBoard,
    theme: &Theme,
    font: FontConfig,
) {
    let module = manager.get(ModuleId::LockVisuals);
    if !module.enabled || !host.playing() {
        return;
    }
    let now_ms = host.millis();
    let size = draw.size();
    let Some(lock) = current_lock(module, board, now_ms) else {
        return;
    };
    let color = lock_color(module, lock.target, theme.accent);

    if module.bool("show_reticle", true)
        && let Some([sx, sy]) = project_point(host, size, lock.target.aim_point)
    {
        let scale = lock_anim_scale(lock.locked_ms, module.bool("lock_anim", true) && lock.live);
        draw_reticle(
            draw,
            [sx, sy],
            module.float("reticle_size", 5.0) as f32 * scale,
            module.float("reticle_thickness", 1.2) as f32,
            module.mode("reticle_style", 0),
            fade_color(color, lock.fade),
        );
    }

    if module.bool("show_target_hud", true) {
        draw_target_hud(draw, size, &lock, module, theme, font);
    }
}

fn draw_target_hud(
    draw: &mut dyn DrawContext,
    screen: [f32; 2],
    lock: &LockView<'_>,
    module: &Module,
    theme: &Theme,
    font: FontConfig,
) {
    let info = lock.target;
    let scale = module.float("hud_scale", 1.0).clamp(0.5, 2.0) as f32;
    let f = font.scaled(scale);
    let fade = lock.fade;
    let show_name = module.bool("hud_name", true);
    let show_health = module.bool("hud_health", true);
    let show_source = module.bool("hud_source", false);

    let mut stats = String::new();
    let mut push_stat = |text: String| {
        if !stats.is_empty() {
            stats.push_str("  |  ");
        }
        stats.push_str(&text);
    };
    if module.bool("hud_distance", true) {
        push_stat(format!("{:.1}m", info.distance));
    }
    if module.bool("hud_angle", true) {
        push_stat(format!("{:.1}\u{b0}", info.angle_deg));
    }
    if module.bool("hud_ping", true) {
        push_stat(match info.ping_ms {
            Some(ms) => format!("{ms}ms"),
            None => "--ms".to_owned(),
        });
    }

    let header = if show_name { info.name.as_str() } else { "" };
    let health = if show_health { health_fraction(info) } else { None };
    let source_text = format!("Locked by {}", source_label(info.source));
    if header.is_empty() && stats.is_empty() && health.is_none() && !show_source {
        return;
    }

    let mut content_w = f
        .width(draw, header, f.bold_headers)
        .max(f.width(draw, &stats, false))
        .max(48.0 * scale);
    if show_source {
        content_w = content_w.max(f.width(draw, &source_text, false));
    }
    let w = (content_w + 18.0 * scale).max(108.0 * scale);
    let row = 10.0 * scale;
    let mut h = 8.0 * scale;
    if !header.is_empty() {
        h += row;
    }
    if !stats.is_empty() {
        h += row;
    }
    if health.is_some() {
        h += 9.0 * scale;
    }
    if show_source {
        h += row;
    }
    h = h.max(18.0 * scale);

    let [x, y] = hud_origin(module.mode("hud_position", 0), screen, [w, h]);
    let bg = (module.float("hud_opacity", 0.66).clamp(0.0, 1.0) * 255.0).round() as u32;
    draw.fill(
        [x, y, w, h],
        fade_color(with_alpha(theme.panel_bg, bg.min(255) as u8), fade),
    );
    draw.fill([x, y, 2.0, h], fade_color(theme.accent, fade));

    let tx = x + 6.0 * scale;
    let mut cursor_y = y + 4.0 * scale;
    if !header.is_empty() {
        f.draw_header(draw, header, [tx, cursor_y], fade_color(theme.text, fade));
        cursor_y += row;
    }
    if !stats.is_empty() {
        f.draw_body(draw, &stats, [tx, cursor_y], fade_color(theme.text_muted, fade));
        cursor_y += row;
    }
    if let Some(frac) = health {
        let bar = [tx, cursor_y + scale, w - 12.0 * scale, 3.5 * scale];
        draw.fill(bar, fade_color(theme.slider_track, fade));
        let fill = if module.mode("hud_health_color", 0) == 1 {
            health_color(frac)
        } else {
            theme.accent
        };
        draw.fill([bar[0], bar[1], bar[2] * frac, bar[3]], fade_color(fill, fade));
        cursor_y += 9.0 * scale;
    }
    if show_source {
        f.draw_body(
            draw,
            &source_text,
            [tx, cursor_y],
            fade_color(theme.text_muted, fade),
        );
    }
}

/// A straight line of thickness `t` between two screen points.
fn draw_line(draw: &mut dyn DrawContext, a: [f32; 2], b: [f32; 2], t: f32, color: u32) {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let len = (dx * dx + dy * dy).sqrt();
    if len < 1e-3 {
        return;
    }
    let (nx, ny) = (-dy / len * t * 0.5, dx / len * t * 0.5);
    draw.quad(
        [
            [a[0] + nx, a[1] + ny],
            [b[0] + nx, b[1] + ny],
            [b[0] - nx, b[1] - ny],
            [a[0] - nx, a[1] - ny],
        ],
        color,
    );
}

fn draw_ring(
    draw: &mut dyn DrawContext,
    center: [f32; 2],
    radius: f32,
    thickness: f32,
    segments: usize,
    color: u32,
) {
    let r_in = (radius - thickness * 0.5).max(0.5);
    let r_out = radius + thickness * 0.5;
    for i in 0..segments {
        let a0 = (i as f32 / segments as f32) * std::f32::consts::TAU;
        let a1 = ((i + 1) as f32 / segments as f32) * std::f32::consts::TAU;
        let (s0, c0) = a0.sin_cos();
        let (s1, c1) = a1.sin_cos();
        draw.quad(
            [
                [center[0] + c0 * r_in, center[1] + s0 * r_in],
                [center[0] + c0 * r_out, center[1] + s0 * r_out],
                [center[0] + c1 * r_out, center[1] + s1 * r_out],
                [center[0] + c1 * r_in, center[1] + s1 * r_in],
            ],
            color,
        );
    }
}

/// Reticle at `center` with radius `r`: 0 cross, 1 corner brackets, 2 ring, 3 diamond.
fn draw_reticle(
    draw: &mut dyn DrawContext,
    center: [f32; 2],
    r: f32,
    t: f32,
    style: usize,
    color: u32,
) {
    let [cx, cy] = center;
    match style {
        1 => {
            let arm = (r * 0.7).max(2.0);
            for (sx, sy) in [(-1.0_f32, -1.0_f32), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
                let (px, py) = (cx + sx * r, cy + sy * r);
                let hx = if sx < 0.0 { px } else { px - arm };
                let vy = if sy < 0.0 { py } else { py - arm };
                draw.fill([hx, py - t * 0.5, arm, t], color);
                draw.fill([px - t * 0.5, vy, t, arm], color);
            }
        }
        2 => draw_ring(draw, center, r, t, 24, color),
        3 => {
            let pts = [[cx, cy - r], [cx + r, cy], [cx, cy + r], [cx - r, cy]];
            for i in 0..4 {
                draw_line(draw, pts[i], pts[(i + 1) % 4], t, color);
            }
        }
        _ => {
            draw.fill([cx - r, cy - t * 0.5, r * 2.0, t], color);
            draw.fill([cx - t * 0.5, cy - r, t, r * 2.0], color);
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// World rendering
// ─────────────────────────────────────────────────────────────────────────────

/// Draws the world-space parts: target box and link line.
pub fn render_world(
    host: &dyn ClientHost,
    view: &WorldRenderView,
    draw: &mut dyn WorldDrawContext,
    manager: &ModuleManager,
    board: &LockBoard,
    interp: &EntityInterpCache,
    accent: u32,
) {
    let module = manager.get(ModuleId::LockVisuals);
    if !module.enabled || !host.playing() {
        return;
    }
    let Some(player) = host.camera_entity() else {
        return;
    };
    let Some(lock) = current_lock(module, board, host.millis()).filter(|l| l.live) else {
        return;
    };

    let usage = draw.usage();
    let max_lines = usage.line_capacity.saturating_sub(usage.lines) as usize;
    let max_quads = usage.quad_capacity.saturating_sub(usage.quads) as usize;
    let mut lines: Vec<WorldLine> = Vec::new();
    let mut quads: Vec<WorldQuad> = Vec::new();
    let color = lock_color(module, lock.target, accent);

    if module.bool("show_box", false)
        && let Some(entity) = lock.target.entity
        && let Some(raw) = entity.bounding_box(host)
    {
        let [lo, hi] = interp.interpolate_box(entity.id(), raw, view.partial_tick);
        let depth = if module.bool("box_through_walls", true) {
            WorldDepth::ThroughWalls
        } else {
            WorldDepth::Tested
        };
        let width = module.float("box_width", 1.8).clamp(0.3, 8.0) as f32;
        if lines.len() + 12 <= max_lines {
            push_box_lines(&mut lines, lo, hi, color, width, depth);
        }
        if module.mode("box_style", 0) == 1 && quads.len() + 6 <= max_quads {
            let alpha = (module.float("box_fill", 0.16).clamp(0.0, 1.0) * 255.0).round() as u32;
            push_box_quads(&mut quads, lo, hi, with_alpha(color, alpha.min(255) as u8), depth);
        }
    }

    if module.bool("show_link", false) && lines.len() < max_lines {
        let cam = view.camera;
        let dir = player.view_vector(host).unwrap_or_else(|| {
            let [yaw, pitch] = player.rotation(host).unwrap_or([0.0, 0.0]);
            look_dir(yaw, pitch)
        });
        let origin = [cam[0] + dir[0] * 0.35, cam[1] + dir[1] * 0.35, cam[2] + dir[2] * 0.35];
        if dist3(origin, lock.target.aim_point) > 0.2 {
            lines.push(WorldLine::new(
                origin,
                lock.target.aim_point,
                with_alpha(color, 0xD8),
                module.float("link_width", 1.4).clamp(0.3, 6.0) as f32,
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
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn target(source: ModuleId, name: &str) -> LockTarget {
        LockTarget {
            source,
            entity: None,
            name: name.to_owned(),
            distance: 3.0,
            angle_deg: 4.0,
            health: Some(10.0),
            max_health: Some(20.0),
            ping_ms: None,
            aim_point: [0.0, 1.0, 0.0],
        }
    }

    fn any(_: ModuleId) -> bool {
        true
    }

    #[test]
    fn module_builds_with_unique_ids_and_a_switch_per_source() {
        let m = build_module();
        let mut ids = HashSet::new();
        for s in &m.settings {
            assert!(ids.insert(s.id), "duplicate setting id {}", s.id);
        }
        for source in LOCK_SOURCES {
            assert!(m.setting(source.setting).is_some(), "{} has no switch", source.label);
            assert!(m.bool(source.setting, false), "sources default to on");
        }
        assert_eq!(m.settings.first().and_then(|s| s.group), Some("Sources"));
        assert!(
            m.setting("show_fov_ring").is_none() && m.setting("show_range_circle").is_none(),
            "FOV/range indicators belong on the locking module, not Lock Visuals"
        );
    }

    #[test]
    fn publishing_makes_the_lock_active_and_clearing_ends_it() {
        let mut board = LockBoard::default();
        assert!(board.active(0, 0, any).is_none());
        board.publish(ModuleId::AimAssist, Some(target(ModuleId::AimAssist, "Zed")), 1000);
        let view = board.active(1100, 0, any).expect("locked");
        assert!(view.live);
        assert_eq!(view.target.name, "Zed");
        assert_eq!(view.locked_ms, 100);
        assert_eq!(view.fade, 1.0);

        board.publish(ModuleId::AimAssist, None, 1200);
        assert!(board.active(1250, 0, any).is_none(), "no linger, gone at once");
    }

    #[test]
    fn same_target_keeps_the_lock_timer_but_a_new_one_restarts_it() {
        let mut board = LockBoard::default();
        board.publish(ModuleId::AimAssist, Some(target(ModuleId::AimAssist, "A")), 0);
        board.publish(ModuleId::AimAssist, Some(target(ModuleId::AimAssist, "A")), 500);
        assert_eq!(board.active(600, 0, any).map(|v| v.locked_ms), Some(600));
        board.publish(ModuleId::AimAssist, Some(target(ModuleId::AimAssist, "B")), 700);
        assert_eq!(board.active(750, 0, any).map(|v| v.locked_ms), Some(50));
    }

    #[test]
    fn lost_locks_fade_out_over_the_linger_time() {
        let mut board = LockBoard::default();
        board.publish(ModuleId::AimAssist, Some(target(ModuleId::AimAssist, "A")), 0);
        board.publish(ModuleId::AimAssist, None, 1000);

        let mid = board.active(1150, 300, any).expect("still fading");
        assert!(!mid.live);
        assert_eq!(mid.target.name, "A");
        assert!((mid.fade - 0.5).abs() < 1e-3);
        assert!(board.active(1300, 300, any).is_none(), "fade finished");
        assert!(board.active(1100, 0, any).is_none(), "zero linger disables fading");

        // A new lock during the fade wins over the fading one.
        board.publish(ModuleId::AimAssist, Some(target(ModuleId::AimAssist, "B")), 1100);
        let now = board.active(1120, 300, any).expect("new lock");
        assert!(now.live && now.target.name == "B");
    }

    #[test]
    fn disallowed_sources_are_ignored() {
        let mut board = LockBoard::default();
        board.publish(ModuleId::AimAssist, Some(target(ModuleId::AimAssist, "A")), 0);
        assert!(board.active(10, 300, |id| id != ModuleId::AimAssist).is_none());
        board.clear();
        assert!(board.active(10, 300, any).is_none());
    }

    #[test]
    fn the_most_recently_updated_source_is_shown() {
        let mut board = LockBoard::default();
        board.publish(ModuleId::AimAssist, Some(target(ModuleId::AimAssist, "A")), 10);
        board.publish(ModuleId::Radar, Some(target(ModuleId::Radar, "R")), 20);
        assert_eq!(board.active(30, 0, any).map(|v| v.target.name.as_str()), Some("R"));
        board.publish(ModuleId::AimAssist, Some(target(ModuleId::AimAssist, "A")), 40);
        assert_eq!(board.active(50, 0, any).map(|v| v.target.name.as_str()), Some("A"));
    }

    #[test]
    fn color_modes_pick_the_expected_colors() {
        let mut m = build_module();
        let t = target(ModuleId::AimAssist, "A");
        let accent = 0xFF12_3456;
        assert_eq!(lock_color(&m, &t, accent), accent);

        m.setting_mut("color_mode").expect("setting").deserialize_value("Health");
        assert_eq!(lock_color(&m, &t, accent), health_color(0.5));
        let mut unknown = t.clone();
        unknown.health = None;
        assert_eq!(lock_color(&m, &unknown, accent), accent, "no health falls back to accent");

        m.setting_mut("color_mode").expect("setting").deserialize_value("Distance");
        let mut close = t.clone();
        close.distance = 0.0;
        let mut far = t.clone();
        far.distance = 99.0;
        assert_eq!(lock_color(&m, &close, accent), health_color(1.0));
        assert_eq!(lock_color(&m, &far, accent), health_color(0.0));

        m.setting_mut("color_mode").expect("setting").deserialize_value("Custom");
        m.setting_mut("custom_color").expect("setting").deserialize_value("FF00FF00");
        assert_eq!(lock_color(&m, &t, accent) >> 24, 0xFF);
    }

    #[test]
    fn fading_scales_only_the_alpha() {
        assert_eq!(fade_color(0xFF11_2233, 1.0), 0xFF11_2233);
        assert_eq!(fade_color(0xFF11_2233, 0.0), 0x0011_2233);
        assert_eq!(fade_color(0x8011_2233, 0.5) >> 24, 0x40);
        assert_eq!(fade_color(0xFF11_2233, 9.0), 0xFF11_2233);
    }

    #[test]
    fn lock_animation_starts_large_and_settles() {
        assert!(lock_anim_scale(0, true) > 2.0);
        assert!(lock_anim_scale(100, true) < lock_anim_scale(0, true));
        assert_eq!(lock_anim_scale(LOCK_ANIM_MS, true), 1.0);
        assert_eq!(lock_anim_scale(0, false), 1.0);
    }

    #[test]
    fn hud_positions_stay_on_screen() {
        let screen = [320.0, 180.0];
        let size = [100.0, 40.0];
        for mode in 0..5 {
            let [x, y] = hud_origin(mode, screen, size);
            assert!(x >= 0.0 && y >= 0.0, "mode {mode}: {x},{y}");
            assert!(x + size[0] <= screen[0] && y + size[1] <= screen[1], "mode {mode}: {x},{y}");
        }
        // Beside the crosshair is to the lower right of the center.
        let [x, y] = hud_origin(0, screen, size);
        assert!(x > 160.0 && y > 90.0);
        // A box bigger than the screen does not panic.
        let _ = hud_origin(3, [50.0, 50.0], [200.0, 200.0]);
    }

}
