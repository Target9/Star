//! Additional Render modules: Item ESP, Storage ESP, Trajectories, Breadcrumbs,
//! Damage Indicators, and the Radar.
//!
//! World geometry (boxes, paths, trails) is submitted from `submit_custom_geometry`;
//! 2D elements (item labels, damage numbers, radar) are drawn from the GUI layer.
//! Slow world queries (storage blocks, projectile paths, damage tracking) are sampled
//! once per tick in [`super::trackers`], so rendering only reads finished data.

use mod_api::{
    ClientHost, DrawContext, EntityRef, WorldDepth, WorldDrawContext, WorldLine, WorldQuad,
    WorldRenderView,
};

use crate::entity_filter::{EntityFilter, classify_entity_id, pretty_entity_id};
use crate::font::FontConfig;
use crate::module::{Category, Module, ModuleId, ModuleManager};
use crate::modules::trackers::{
    MAX_STORAGE_RADIUS, POPUP_LIFETIME_MS, StorageKind, Trackers,
};
use crate::modules::visuals::{
    EntityInterpCache, dist3, draw_rect_outline, project_point, push_box_lines, push_box_quads,
};
use crate::settings::Setting;
use crate::theme::{Theme, with_alpha};

const ITEM_KIND: &str = "minecraft:item";
const MAX_ITEM_LABELS: usize = 40;

// ─────────────────────────────────────────────────────────────────────────────
// Module definitions
// ─────────────────────────────────────────────────────────────────────────────

pub fn build_item_esp_module() -> Module {
    Module {
        id: ModuleId::ItemEsp,
        key: "item_esp",
        name: "Item ESP",
        description: "Highlights dropped items with boxes and name / count labels.",
        category: Category::Render,
        enabled: false,
        keybind: None,
        expanded: false,
        show_in_arraylist: true,
        settings: vec![
            Setting::float(
                "range",
                "Range",
                "Maximum distance in blocks to highlight dropped items.",
                4.0,
                128.0,
                2.0,
                48.0,
            )
            .suffix("m")
            .group("Targets"),
            Setting::int(
                "min_count",
                "Min Stack",
                "Ignore stacks smaller than this many items.",
                1,
                64,
                1,
                1,
            ),
            Setting::mode(
                "style",
                "Style",
                "Outline only, or outline with a translucent fill.",
                &["3D Box", "3D Fill + Box"],
                1,
            )
            .group("Appearance"),
            Setting::bool(
                "through_walls",
                "Through Walls",
                "Show boxes through solid terrain.",
                true,
            ),
            Setting::float(
                "line_width",
                "Line Width",
                "Outline thickness in pixels.",
                1.0,
                4.0,
                0.2,
                1.4,
            )
            .suffix("px"),
            Setting::float(
                "fill_opacity",
                "Fill Opacity",
                "Translucent fill opacity.",
                0.0,
                0.60,
                0.05,
                0.18,
            ),
            Setting::bool(
                "labels",
                "Labels",
                "Show the item name and stack size above each drop.",
                true,
            ),
            Setting::color(
                "color",
                "Color",
                "Highlight color for dropped items.",
                0xFFF5_C84D,
            ),
        ],
    }
}

pub fn build_storage_esp_module() -> Module {
    Module {
        id: ModuleId::StorageEsp,
        key: "storage_esp",
        name: "Storage ESP",
        description: "Highlights chests, barrels, shulker boxes, furnaces and other containers.",
        category: Category::Render,
        enabled: false,
        keybind: None,
        expanded: false,
        show_in_arraylist: true,
        settings: vec![
            Setting::float(
                "range",
                "Range",
                "Maximum distance in blocks to scan for containers.",
                8.0,
                MAX_STORAGE_RADIUS,
                2.0,
                40.0,
            )
            .suffix("m")
            .group("Targets"),
            Setting::bool("chests", "Chests", "Chests and trapped chests.", true),
            Setting::bool("ender_chests", "Ender Chests", "Ender chests.", true),
            Setting::bool("shulkers", "Shulker Boxes", "Shulker boxes of any color.", true),
            Setting::bool("barrels", "Barrels", "Barrels.", true),
            Setting::bool(
                "furnaces",
                "Furnaces",
                "Furnaces, blast furnaces and smokers.",
                false,
            ),
            Setting::bool(
                "utility",
                "Hoppers & Droppers",
                "Hoppers, droppers, dispensers and crafters.",
                false,
            ),
            Setting::bool(
                "other",
                "Other",
                "Brewing stands, decorated pots and chiseled bookshelves.",
                false,
            ),
            Setting::mode(
                "style",
                "Style",
                "Outline only, or outline with a translucent fill.",
                &["3D Box", "3D Fill + Box"],
                1,
            )
            .group("Appearance"),
            Setting::bool(
                "through_walls",
                "Through Walls",
                "Show boxes through solid terrain.",
                true,
            ),
            Setting::float(
                "line_width",
                "Line Width",
                "Outline thickness in pixels.",
                1.0,
                4.0,
                0.2,
                1.4,
            )
            .suffix("px"),
            Setting::float(
                "fill_opacity",
                "Fill Opacity",
                "Translucent fill opacity.",
                0.0,
                0.60,
                0.05,
                0.16,
            ),
        ],
    }
}

pub fn build_trajectories_module() -> Module {
    Module {
        id: ModuleId::Trajectories,
        key: "trajectories",
        name: "Trajectories",
        description: "Predicts the flight path and landing spot of arrows, tridents, pearls and thrown items.",
        category: Category::Render,
        enabled: false,
        keybind: None,
        expanded: false,
        show_in_arraylist: true,
        settings: vec![
            Setting::mode(
                "owner",
                "Projectiles",
                "Which projectiles to predict: everyone's, only yours, or only other players'. Yours is estimated from projectiles that appear right next to you.",
                &["All", "Mine", "Others"],
                0,
            )
            .group("Targets"),
            Setting::float(
                "range",
                "Range",
                "Maximum distance in blocks from you to track projectiles.",
                8.0,
                128.0,
                4.0,
                64.0,
            )
            .suffix("m"),
            Setting::mode(
                "style",
                "Style",
                "A plain path, or a path with a marker where it lands.",
                &["Line", "Line + Landing"],
                1,
            )
            .group("Appearance"),
            Setting::float(
                "line_width",
                "Line Width",
                "Path thickness in pixels.",
                1.0,
                4.0,
                0.2,
                1.6,
            )
            .suffix("px"),
            Setting::bool(
                "through_walls",
                "Through Walls",
                "Show paths through solid terrain.",
                true,
            ),
            Setting::color(
                "own_color",
                "Your Color",
                "Path color for projectiles you launched.",
                0xFF4C_E08A,
            )
            .group("Color"),
            Setting::color(
                "other_color",
                "Other Color",
                "Path color for everyone else's projectiles.",
                0xFFFF_4D5E,
            ),
        ],
    }
}

pub fn build_breadcrumbs_module() -> Module {
    Module {
        id: ModuleId::Breadcrumbs,
        key: "breadcrumbs",
        name: "Breadcrumbs",
        description: "Leaves a fading trail behind you or behind selected entities.",
        category: Category::Render,
        enabled: false,
        keybind: None,
        expanded: false,
        show_in_arraylist: true,
        settings: vec![
            Setting::mode(
                "source",
                "Source",
                "Trail behind yourself, or behind the entities selected below.",
                &["Self", "Selected"],
                0,
            )
            .group("Targets"),
            Setting::entity_filter(
                "entities",
                "Entities",
                "Entity types that leave trails when Source is Selected.",
                EntityFilter::players_only(),
            ),
            Setting::float(
                "range",
                "Range",
                "Maximum distance in blocks to trail selected entities.",
                8.0,
                128.0,
                4.0,
                48.0,
            )
            .suffix("m"),
            Setting::int(
                "length",
                "Length",
                "How many ticks of movement each trail remembers (20 ticks = 1 second).",
                20,
                400,
                10,
                100,
            )
            .suffix("t")
            .group("Appearance"),
            Setting::float(
                "line_width",
                "Line Width",
                "Trail thickness in pixels.",
                1.0,
                4.0,
                0.2,
                1.6,
            )
            .suffix("px"),
            Setting::mode(
                "color_mode",
                "Color Mode",
                "Theme accent, entity category color, or a custom color.",
                &["Accent", "Category", "Custom"],
                0,
            ),
            Setting::color(
                "custom_color",
                "Custom Color",
                "Trail color when Color Mode is Custom.",
                0xFF91_3DE8,
            ),
            Setting::bool(
                "through_walls",
                "Through Walls",
                "Show trails through solid terrain.",
                false,
            ),
        ],
    }
}

pub fn build_damage_indicators_module() -> Module {
    Module {
        id: ModuleId::DamageIndicators,
        key: "damage_indicators",
        name: "Damage Indicators",
        description: "Floats the amount of damage dealt above entities as it happens.",
        category: Category::Render,
        enabled: false,
        keybind: None,
        expanded: false,
        show_in_arraylist: true,
        settings: vec![
            Setting::entity_filter(
                "entities",
                "Entities",
                "Entity types that show damage numbers.",
                EntityFilter::players_and_hostile(),
            )
            .group("Targets"),
            Setting::float(
                "range",
                "Range",
                "Maximum distance in blocks to track damage.",
                4.0,
                96.0,
                2.0,
                32.0,
            )
            .suffix("m"),
            Setting::bool(
                "only_mine",
                "Only My Hits",
                "Only show damage from entities you were the last to hit.",
                false,
            ),
            Setting::bool(
                "show_healing",
                "Show Healing",
                "Also show green numbers when an entity regains health.",
                false,
            ),
            Setting::float(
                "scale",
                "Text Scale",
                "Size multiplier for the floating numbers.",
                0.7,
                1.6,
                0.05,
                1.0,
            )
            .suffix("x")
            .group("Appearance"),
        ],
    }
}

pub fn build_radar_module() -> Module {
    Module {
        id: ModuleId::Radar,
        key: "radar",
        name: "Radar",
        description: "A top-down mini-map of nearby entities. Drag it in the HUD tab.",
        category: Category::Render,
        enabled: false,
        keybind: None,
        expanded: false,
        show_in_arraylist: true,
        settings: vec![
            Setting::entity_filter(
                "entities",
                "Entities",
                "Entity types shown on the radar.",
                EntityFilter::players_and_hostile(),
            )
            .group("Targets"),
            Setting::bool(
                "ignore_invisible",
                "Ignore Invisible",
                "Hide invisible entities from the radar.",
                false,
            ),
            Setting::float(
                "range",
                "Range",
                "Distance in blocks from the center to the edge of the radar.",
                8.0,
                128.0,
                4.0,
                48.0,
            )
            .suffix("m"),
            Setting::int(
                "size",
                "Size",
                "Radar width and height in GUI pixels.",
                64,
                180,
                4,
                96,
            )
            .suffix("px")
            .group("Appearance"),
            Setting::bool(
                "rotate",
                "Rotate With View",
                "Keep the direction you are facing at the top; off keeps north at the top.",
                true,
            ),
            Setting::int(
                "dot_size",
                "Dot Size",
                "Size of each entity marker in pixels.",
                2,
                6,
                1,
                3,
            ),
            Setting::bool(
                "edge_dots",
                "Edge Markers",
                "Pin entities beyond the range to the radar edge (dimmed).",
                true,
            ),
            Setting::float(
                "bg_opacity",
                "Background",
                "Opacity of the radar background.",
                0.0,
                0.95,
                0.05,
                0.55,
            ),
        ],
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// World geometry
// ─────────────────────────────────────────────────────────────────────────────

/// Remaining primitive budget for one frame of custom geometry.
struct Budget {
    lines: usize,
    quads: usize,
}

fn depth(through_walls: bool) -> WorldDepth {
    if through_walls {
        WorldDepth::ThroughWalls
    } else {
        WorldDepth::Tested
    }
}

/// Submits Item ESP, Storage ESP, Trajectories and Breadcrumbs geometry.
pub fn render_world(
    host: &dyn ClientHost,
    view: &WorldRenderView,
    draw: &mut dyn WorldDrawContext,
    manager: &ModuleManager,
    interp: &EntityInterpCache,
    trackers: &Trackers,
) {
    if !host.playing() {
        return;
    }
    let item = manager.get(ModuleId::ItemEsp);
    let storage = manager.get(ModuleId::StorageEsp);
    let paths = manager.get(ModuleId::Trajectories);
    let crumbs = manager.get(ModuleId::Breadcrumbs);
    if !(item.enabled || storage.enabled || paths.enabled || crumbs.enabled) {
        return;
    }

    let usage = draw.usage();
    let mut budget = Budget {
        lines: usage.line_capacity.saturating_sub(usage.lines) as usize,
        quads: usage.quad_capacity.saturating_sub(usage.quads) as usize,
    };
    let mut lines: Vec<WorldLine> = Vec::new();
    let mut quads: Vec<WorldQuad> = Vec::new();
    let theme = Theme::from_manager(manager, host.millis());

    if item.enabled {
        push_item_boxes(host, view, manager, interp, item, &mut lines, &mut quads, &mut budget);
    }
    if storage.enabled {
        push_storage_boxes(view, storage, trackers, &mut lines, &mut quads, &mut budget);
    }
    if paths.enabled {
        push_paths(paths, trackers, &mut lines, &mut budget);
    }
    if crumbs.enabled {
        push_trails(crumbs, trackers, theme.accent, &mut lines, &mut budget);
    }

    if !quads.is_empty() {
        let _ = draw.quads(&quads);
    }
    if !lines.is_empty() {
        let _ = draw.lines(&lines);
    }
}

#[allow(clippy::too_many_arguments)]
fn push_item_boxes(
    host: &dyn ClientHost,
    view: &WorldRenderView,
    _manager: &ModuleManager,
    interp: &EntityInterpCache,
    module: &Module,
    lines: &mut Vec<WorldLine>,
    quads: &mut Vec<WorldQuad>,
    budget: &mut Budget,
) {
    let Some(player) = host.camera_entity() else {
        return;
    };
    let range = module.float("range", 48.0);
    let min_count = module.int("min_count", 1).max(1) as u32;
    let fill = module.mode("style", 1) == 1;
    let fill_alpha = ((module.float("fill_opacity", 0.18) * 255.0).round() as u32).clamp(0, 200) as u8;
    let width = (module.float("line_width", 1.4) as f32).clamp(0.5, 6.0);
    let depth = depth(module.bool("through_walls", true));
    let color = module.color("color", 0xFFF5_C84D);

    for entity in nearby_items(host, player, view.camera, range, min_count) {
        let Some(raw) = entity.bounding_box(host) else {
            continue;
        };
        let [lo, hi] = interp.interpolate_box(entity.id(), raw, view.partial_tick);
        if budget.lines >= 12 {
            push_box_lines(lines, lo, hi, color, width, depth);
            budget.lines -= 12;
        }
        if fill && fill_alpha > 0 && budget.quads >= 6 {
            push_box_quads(quads, lo, hi, with_alpha(color, fill_alpha), depth);
            budget.quads -= 6;
        }
    }
}

/// Dropped items within `range` of `center`, nearest first, filtered by stack size.
fn nearby_items(
    host: &dyn ClientHost,
    player: EntityRef,
    center: [f64; 3],
    range: f64,
    min_count: u32,
) -> Vec<EntityRef> {
    let Some(level) = player.level(host) else {
        return Vec::new();
    };
    let pad = range + 1.0;
    let Some(entities) = level.entities_in(
        host,
        [center[0] - pad, center[1] - pad, center[2] - pad],
        [center[0] + pad, center[1] + pad, center[2] + pad],
    ) else {
        return Vec::new();
    };
    let mut items: Vec<(f64, EntityRef)> = Vec::new();
    for entity in entities {
        let Some(kind) = entity.kind(host) else {
            continue;
        };
        if kind.id(host) != ITEM_KIND {
            continue;
        }
        let Some(pos) = entity.position(host) else {
            continue;
        };
        let d = dist3(center, pos);
        if d > range {
            continue;
        }
        if min_count > 1 {
            let count = host
                .item_entity(entity)
                .and_then(|i| i.stack(host))
                .map_or(1, |s| s.count());
            if count < min_count {
                continue;
            }
        }
        items.push((d, entity));
    }
    items.sort_by(|a, b| a.0.total_cmp(&b.0));
    items.into_iter().map(|(_, e)| e).collect()
}

fn push_storage_boxes(
    view: &WorldRenderView,
    module: &Module,
    trackers: &Trackers,
    lines: &mut Vec<WorldLine>,
    quads: &mut Vec<WorldQuad>,
    budget: &mut Budget,
) {
    let range = module.float("range", 40.0);
    let fill = module.mode("style", 1) == 1;
    let fill_alpha = ((module.float("fill_opacity", 0.16) * 255.0).round() as u32).clamp(0, 200) as u8;
    let width = (module.float("line_width", 1.4) as f32).clamp(0.5, 6.0);
    let depth = depth(module.bool("through_walls", true));

    for block in trackers.storage() {
        if !storage_kind_enabled(module, block.kind) {
            continue;
        }
        let lo = [
            block.pos[0] as f64 + 0.03,
            block.pos[1] as f64,
            block.pos[2] as f64 + 0.03,
        ];
        let hi = [
            block.pos[0] as f64 + 0.97,
            block.pos[1] as f64 + 0.94,
            block.pos[2] as f64 + 0.97,
        ];
        let center = [
            (lo[0] + hi[0]) * 0.5,
            (lo[1] + hi[1]) * 0.5,
            (lo[2] + hi[2]) * 0.5,
        ];
        if dist3(view.camera, center) > range {
            continue;
        }
        let color = block.kind.color();
        if budget.lines >= 12 {
            push_box_lines(lines, lo, hi, color, width, depth);
            budget.lines -= 12;
        }
        if fill && fill_alpha > 0 && budget.quads >= 6 {
            push_box_quads(quads, lo, hi, with_alpha(color, fill_alpha), depth);
            budget.quads -= 6;
        }
    }
}

pub fn storage_kind_enabled(module: &Module, kind: StorageKind) -> bool {
    match kind {
        StorageKind::Chest => module.bool("chests", true),
        StorageKind::EnderChest => module.bool("ender_chests", true),
        StorageKind::Shulker => module.bool("shulkers", true),
        StorageKind::Barrel => module.bool("barrels", true),
        StorageKind::Furnace => module.bool("furnaces", false),
        StorageKind::Hopper | StorageKind::Dispenser => module.bool("utility", false),
        StorageKind::Other => module.bool("other", false),
    }
}

fn push_paths(
    module: &Module,
    trackers: &Trackers,
    lines: &mut Vec<WorldLine>,
    budget: &mut Budget,
) {
    let width = (module.float("line_width", 1.6) as f32).clamp(0.5, 6.0);
    let depth = depth(module.bool("through_walls", true));
    let own_color = module.color("own_color", 0xFF4C_E08A);
    let other_color = module.color("other_color", 0xFFFF_4D5E);
    let landing_marker = module.mode("style", 1) == 1;

    for path in trackers.paths() {
        let color = if path.own { own_color } else { other_color };
        let count = path.points.len();
        for (i, pair) in path.points.windows(2).enumerate() {
            if budget.lines == 0 {
                return;
            }
            // Fade the far end of the path so the prediction reads as uncertain.
            let t = i as f32 / count.max(2) as f32;
            let alpha = (0xF0 as f32 * (1.0 - 0.55 * t)) as u8;
            lines.push(WorldLine::new(
                pair[0],
                pair[1],
                with_alpha(color, alpha),
                width,
                depth,
            ));
            budget.lines -= 1;
        }
        if landing_marker
            && let Some(hit) = path.landing
            && budget.lines >= 12
        {
            let r = 0.2;
            push_box_lines(
                lines,
                [hit[0] - r, hit[1] - 0.02, hit[2] - r],
                [hit[0] + r, hit[1] + 2.0 * r, hit[2] + r],
                with_alpha(color, 0xFF),
                width + 0.4,
                depth,
            );
            budget.lines -= 12;
        }
    }
}

fn push_trails(
    module: &Module,
    trackers: &Trackers,
    accent: u32,
    lines: &mut Vec<WorldLine>,
    budget: &mut Budget,
) {
    let width = (module.float("line_width", 1.6) as f32).clamp(0.5, 6.0);
    let depth = depth(module.bool("through_walls", false));
    let color_mode = module.mode("color_mode", 0);
    let custom = module.color("custom_color", accent);

    let mut emit = |points: &std::collections::VecDeque<[f64; 3]>, base: u32| {
        let n = points.len();
        if n < 2 {
            return;
        }
        for (i, (a, b)) in points.iter().zip(points.iter().skip(1)).enumerate() {
            if budget.lines == 0 {
                return;
            }
            let alpha = trail_alpha(i + 1, n);
            lines.push(WorldLine::new(*a, *b, with_alpha(base, alpha), width, depth));
            budget.lines -= 1;
        }
    };

    if module.mode("source", 0) == 0 {
        let base = match color_mode {
            2 => custom,
            _ => accent,
        };
        emit(trackers.self_trail(), base);
    } else {
        for trail in trackers.trails() {
            let base = match color_mode {
                1 => trail.group.color(accent),
                2 => custom,
                _ => accent,
            };
            emit(&trail.points, base);
        }
    }
}

/// Opacity of the trail segment at `index` of `len` points (newest = most opaque).
pub fn trail_alpha(index: usize, len: usize) -> u8 {
    if len <= 1 {
        return 0;
    }
    let t = (index as f32 / (len - 1) as f32).clamp(0.0, 1.0);
    (20.0 + 215.0 * t).round().clamp(0.0, 255.0) as u8
}

// ─────────────────────────────────────────────────────────────────────────────
// 2D overlays: item labels and damage numbers
// ─────────────────────────────────────────────────────────────────────────────

pub fn render_overlays(
    host: &dyn ClientHost,
    draw: &mut dyn DrawContext,
    manager: &ModuleManager,
    interp: &EntityInterpCache,
    trackers: &Trackers,
    theme: &Theme,
    font: FontConfig,
) {
    if !host.playing() {
        return;
    }
    let item = manager.get(ModuleId::ItemEsp);
    if item.enabled && item.bool("labels", true) {
        draw_item_labels(host, draw, item, interp, theme, font);
    }
    let damage = manager.get(ModuleId::DamageIndicators);
    if damage.enabled {
        draw_damage_popups(host, draw, damage, trackers, font);
    }
}

fn draw_item_labels(
    host: &dyn ClientHost,
    draw: &mut dyn DrawContext,
    module: &Module,
    interp: &EntityInterpCache,
    theme: &Theme,
    font: FontConfig,
) {
    let Some(player) = host.camera_entity() else {
        return;
    };
    let Some(eyes) = player.eye_position(host) else {
        return;
    };
    let range = module.float("range", 48.0);
    let min_count = module.int("min_count", 1).max(1) as u32;
    let color = module.color("color", 0xFFF5_C84D);
    let size = draw.size();
    let label_font = font.scaled(0.85);

    // Nearest first, so the cap keeps the closest labels; draw far-to-near afterwards.
    let items = nearby_items(host, player, eyes, range, min_count);
    for entity in items.iter().take(MAX_ITEM_LABELS).rev() {
        let Some(raw) = entity.bounding_box(host) else {
            continue;
        };
        let [lo, hi] = interp.interpolate_box(entity.id(), raw, interp.last_partial_tick);
        let anchor = [(lo[0] + hi[0]) * 0.5, hi[1] + 0.25, (lo[2] + hi[2]) * 0.5];
        let Some([sx, sy]) = project_point(host, size, anchor) else {
            continue;
        };
        let Some(stack) = host.item_entity(*entity).and_then(|i| i.stack(host)) else {
            continue;
        };
        let name = pretty_entity_id(&stack.item().id(host));
        let label = format_item_label(&name, stack.count());
        let w = label_font.width(draw, &label, false) + 8.0;
        let h = label_font.line_height() + 4.0;
        let x = sx - w * 0.5;
        let y = sy - h;
        draw.fill([x, y, w, h], with_alpha(theme.panel_bg, 0xB0));
        draw.fill([x, y, w, 1.2], with_alpha(color, 0xCC));
        label_font.draw_body(draw, &label, [x + 4.0, label_font.center_y(y, h)], theme.text);
    }
}

/// `Diamond` / 1 -> `Diamond`, `Cobblestone` / 32 -> `Cobblestone x32`.
pub fn format_item_label(name: &str, count: u32) -> String {
    if count > 1 {
        format!("{name} x{count}")
    } else {
        name.to_owned()
    }
}

fn draw_damage_popups(
    host: &dyn ClientHost,
    draw: &mut dyn DrawContext,
    module: &Module,
    trackers: &Trackers,
    font: FontConfig,
) {
    let now = host.millis();
    let size = draw.size();
    let scale = module.float("scale", 1.0) as f32;
    let pop_font = font.scaled(scale);

    for popup in trackers.popups() {
        let age = now.saturating_sub(popup.born_ms);
        if age >= POPUP_LIFETIME_MS {
            continue;
        }
        let (rise, alpha) = popup_motion(age);
        let world = [popup.pos[0], popup.pos[1] + rise, popup.pos[2]];
        let Some([sx, sy]) = project_point(host, size, world) else {
            continue;
        };
        let text = format_damage(popup.amount, popup.heal);
        let base = if popup.heal { 0xFF4C_E08A } else { 0xFFFF_4D5E };
        let a8 = (alpha * 255.0).round() as u8;
        let w = pop_font.width(draw, &text, true);
        let y = sy - pop_font.line_height() * 0.5;
        // Soft shadow for readability on bright backgrounds.
        pop_font.draw_header(draw, &text, [sx - w * 0.5 + 1.0, y + 1.0], with_alpha(0xFF00_0000, a8 / 2));
        pop_font.draw_header(draw, &text, [sx - w * 0.5, y], with_alpha(base, a8));
    }
}

/// Vertical rise in blocks and opacity of a damage number `age_ms` old.
pub fn popup_motion(age_ms: u64) -> (f64, f32) {
    let t = (age_ms as f32 / POPUP_LIFETIME_MS as f32).clamp(0.0, 1.0);
    let ease = 1.0 - (1.0 - t) * (1.0 - t);
    let alpha = if t < 0.65 { 1.0 } else { (1.0 - (t - 0.65) / 0.35).clamp(0.0, 1.0) };
    (ease as f64 * 0.9, alpha)
}

/// `-4` for whole numbers, `-4.5` otherwise; `+` for healing.
pub fn format_damage(amount: f32, heal: bool) -> String {
    let sign = if heal { '+' } else { '-' };
    if (amount - amount.round()).abs() < 0.05 || amount >= 10.0 {
        format!("{sign}{:.0}", amount)
    } else {
        format!("{sign}{:.1}", amount)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Shared equipment helpers (Nametags, HUD)
// ─────────────────────────────────────────────────────────────────────────────

/// Remaining durability in percent for an item with `damage` taken of `max` uses.
pub fn durability_percent(damage: u32, max: u32) -> f32 {
    if max == 0 {
        return 100.0;
    }
    (100.0 - damage as f32 * 100.0 / max as f32).clamp(0.0, 100.0)
}

/// Mean remaining durability (0-100) over the worn armor pieces that can break, or `None`
/// when the entity wears no breakable armor.
pub fn armor_percent(host: &dyn ClientHost, entity: EntityRef) -> Option<f32> {
    let living = host.living(entity)?;
    let mut total = 0.0;
    let mut pieces = 0u32;
    for slot in [
        mod_api::EquipmentSlot::Head,
        mod_api::EquipmentSlot::Chest,
        mod_api::EquipmentSlot::Legs,
        mod_api::EquipmentSlot::Feet,
    ] {
        let Some(stack) = living.equipment(host, slot) else {
            continue;
        };
        let max = host.max_damage(stack.item()).unwrap_or(0);
        if max == 0 {
            continue;
        }
        total += durability_percent(stack.damage(), max);
        pieces += 1;
    }
    (pieces > 0).then(|| total / pieces as f32)
}

/// Pretty name of the item in the entity's main hand (`None` for an empty hand).
pub fn held_item_name(host: &dyn ClientHost, entity: EntityRef) -> Option<String> {
    let stack = host.living(entity)?.hand_item(host, mod_api::Hand::Main)?;
    Some(pretty_entity_id(&stack.item().id(host)))
}

// ─────────────────────────────────────────────────────────────────────────────
// Radar
// ─────────────────────────────────────────────────────────────────────────────

/// Width and height of the radar widget in GUI pixels.
pub fn radar_size(module: &Module) -> f32 {
    module.int("size", 96).clamp(64, 180) as f32
}

/// Position of a world offset `(dx, dz)` on the radar, in blocks to the right of and below
/// the center. With `rotate`, the direction the player faces (`yaw_deg`, Minecraft
/// convention: 0 = south, 90 = west) points up; otherwise north is up.
pub fn radar_offset(dx: f64, dz: f64, yaw_deg: f32, rotate: bool) -> [f64; 2] {
    if !rotate {
        return [dx, dz];
    }
    let yaw = (yaw_deg as f64).to_radians();
    let (sin, cos) = yaw.sin_cos();
    let forward = dx * -sin + dz * cos;
    let right = dx * -cos + dz * -sin;
    [right, -forward]
}

/// Scales a radar-space point so its distance from the center is at most `max`, returning
/// the point and whether it was pulled in.
pub fn clamp_to_radius(point: [f64; 2], max: f64) -> ([f64; 2], bool) {
    let len = (point[0] * point[0] + point[1] * point[1]).sqrt();
    if len <= max || len <= 1e-9 {
        (point, false)
    } else {
        let k = max / len;
        ([point[0] * k, point[1] * k], true)
    }
}

pub fn render_radar(
    host: &dyn ClientHost,
    draw: &mut dyn DrawContext,
    module: &Module,
    origin: [f32; 2],
    theme: &Theme,
    font: FontConfig,
) {
    if !host.playing() {
        return;
    }
    let size = radar_size(module);
    let range = module.float("range", 48.0).max(1.0);
    let rotate = module.bool("rotate", true);
    let dot = module.int("dot_size", 3).clamp(2, 6) as f32;
    let edge_dots = module.bool("edge_dots", true);
    let bg_alpha = ((module.float("bg_opacity", 0.55) * 255.0).round() as u32).clamp(0, 245) as u8;
    let ignore_invisible = module.bool("ignore_invisible", false);

    let [x, y] = origin;
    let cx = x + size * 0.5;
    let cy = y + size * 0.5;
    let half = size * 0.5 - dot * 0.5 - 1.0;

    if bg_alpha > 0 {
        draw.fill([x, y, size, size], with_alpha(theme.panel_bg, bg_alpha));
    }
    draw.fill([cx - 0.5, y + 1.0, 1.0, size - 2.0], with_alpha(theme.border, 0x70));
    draw.fill([x + 1.0, cy - 0.5, size - 2.0, 1.0], with_alpha(theme.border, 0x70));
    draw_rect_outline(draw, [x, y, size, size], 1.0, with_alpha(theme.accent, 0xA0));
    if !rotate {
        let nw = font.width(draw, "N", true);
        font.draw_header(draw, "N", [cx - nw * 0.5, y + 2.0], theme.accent);
    }

    let Some(player) = host.camera_entity() else {
        return;
    };
    let (Some(origin_pos), Some([yaw, _])) = (player.position(host), player.rotation(host)) else {
        return;
    };
    let Some(filter) = module.entity_filter("entities") else {
        return;
    };
    let Some(level) = player.level(host) else {
        return;
    };
    let pad = range + 1.0;
    let Some(entities) = level.entities_in(
        host,
        [origin_pos[0] - pad, origin_pos[1] - pad, origin_pos[2] - pad],
        [origin_pos[0] + pad, origin_pos[1] + pad, origin_pos[2] + pad],
    ) else {
        return;
    };

    let scale = half as f64 / range;
    for entity in entities {
        if entity.id() == player.id() {
            continue;
        }
        let Some(kind) = entity.kind(host) else {
            continue;
        };
        let kind_id = kind.id(host);
        if !filter.contains(&kind_id) {
            continue;
        }
        if ignore_invisible && entity.is_invisible(host) == Some(true) {
            continue;
        }
        let Some(pos) = entity.position(host) else {
            continue;
        };
        let rel = radar_offset(pos[0] - origin_pos[0], pos[2] - origin_pos[2], yaw, rotate);
        let scaled = [rel[0] * scale, rel[1] * scale];
        let (pt, clamped) = clamp_to_radius(scaled, half as f64);
        if clamped && !edge_dots {
            continue;
        }
        let mut color = classify_entity_id(&kind_id).color(theme.accent);
        if clamped {
            color = with_alpha(color, 0x70);
        }
        // Higher entities read slightly brighter, lower ones dimmer.
        let dy = pos[1] - origin_pos[1];
        if !clamped && dy.abs() > 3.0 {
            color = with_alpha(color, if dy > 0.0 { 0xFF } else { 0xA0 });
        }
        draw.fill(
            [cx + pt[0] as f32 - dot * 0.5, cy + pt[1] as f32 - dot * 0.5, dot, dot],
            color,
        );
    }

    // Rotating radars always have you facing up; north-up radars turn the arrow with your yaw.
    let angle = if rotate { 0.0 } else { self_marker_angle(yaw) };
    draw_self_marker(draw, cx, cy, angle, theme.text);
}

/// Clockwise rotation in degrees that makes an up-pointing arrow face `yaw_deg` on a
/// north-up radar (Minecraft yaw 0 faces south, i.e. down the radar).
pub fn self_marker_angle(yaw_deg: f32) -> f32 {
    yaw_deg + 180.0
}

/// A small arrow at the radar center, rotated clockwise by `angle_deg` from pointing up.
fn draw_self_marker(draw: &mut dyn DrawContext, cx: f32, cy: f32, angle_deg: f32, color: u32) {
    // Local points: tip, right wing, tail notch, left wing (y grows downward).
    let local = [[0.0f32, -4.5], [3.2, 3.2], [0.0, 1.4], [-3.2, 3.2]];
    let (s, c) = angle_deg.to_radians().sin_cos();
    let mut pts = [[0.0f32; 2]; 4];
    for (out, p) in pts.iter_mut().zip(local) {
        *out = [cx + p[0] * c - p[1] * s, cy + p[0] * s + p[1] * c];
    }
    draw.quad(pts, color);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: [f64; 2], b: [f64; 2]) -> bool {
        (a[0] - b[0]).abs() < 1e-6 && (a[1] - b[1]).abs() < 1e-6
    }

    #[test]
    fn radar_north_up_maps_world_axes_directly() {
        assert!(approx(radar_offset(3.0, -5.0, 123.0, false), [3.0, -5.0]));
    }

    #[test]
    fn radar_rotated_puts_things_ahead_at_the_top() {
        // Facing south (yaw 0): +z is forward, so an entity 10 blocks south is straight up.
        assert!(approx(radar_offset(0.0, 10.0, 0.0, true), [0.0, -10.0]));
        // Facing west (yaw 90): -x is forward.
        assert!(approx(radar_offset(-10.0, 0.0, 90.0, true), [0.0, -10.0]));
        // Facing north (yaw 180): -z is forward.
        assert!(approx(radar_offset(0.0, -10.0, 180.0, true), [0.0, -10.0]));
    }

    #[test]
    fn radar_rotated_puts_things_to_the_right_on_the_right() {
        // Facing south, your right-hand side is west (-x).
        let off = radar_offset(-6.0, 0.0, 0.0, true);
        assert!(off[0] > 5.9 && off[1].abs() < 1e-6, "{off:?}");
        // Facing west, your right-hand side is north (-z).
        let off = radar_offset(0.0, -6.0, 90.0, true);
        assert!(off[0] > 5.9 && off[1].abs() < 1e-6, "{off:?}");
    }

    #[test]
    fn clamp_to_radius_pins_far_points_to_the_edge() {
        let (p, clamped) = clamp_to_radius([30.0, 40.0], 10.0);
        assert!(clamped);
        assert!(((p[0] * p[0] + p[1] * p[1]).sqrt() - 10.0).abs() < 1e-9);
        let (p, clamped) = clamp_to_radius([3.0, 4.0], 10.0);
        assert!(!clamped);
        assert!(approx(p, [3.0, 4.0]));
        let (p, clamped) = clamp_to_radius([0.0, 0.0], 10.0);
        assert!(!clamped && approx(p, [0.0, 0.0]));
    }

    #[test]
    fn trail_alpha_fades_toward_the_oldest_point() {
        assert!(trail_alpha(1, 100) < trail_alpha(50, 100));
        assert!(trail_alpha(50, 100) < trail_alpha(99, 100));
        assert_eq!(trail_alpha(0, 1), 0);
    }

    #[test]
    fn damage_numbers_format_cleanly() {
        assert_eq!(format_damage(4.0, false), "-4");
        assert_eq!(format_damage(4.5, false), "-4.5");
        assert_eq!(format_damage(12.3, false), "-12");
        assert_eq!(format_damage(2.0, true), "+2");
    }

    #[test]
    fn popups_rise_then_fade() {
        let (rise0, a0) = popup_motion(0);
        let (rise_mid, a_mid) = popup_motion(POPUP_LIFETIME_MS / 2);
        let (rise_end, a_end) = popup_motion(POPUP_LIFETIME_MS);
        assert!(rise0 < rise_mid && rise_mid < rise_end);
        assert_eq!(a0, 1.0);
        assert_eq!(a_mid, 1.0);
        assert!(a_end < 0.01);
    }

    #[test]
    fn durability_percent_is_clamped_and_safe_for_zero_max() {
        assert_eq!(durability_percent(0, 100), 100.0);
        assert_eq!(durability_percent(25, 100), 75.0);
        assert_eq!(durability_percent(500, 100), 0.0);
        assert_eq!(durability_percent(5, 0), 100.0);
    }

    #[test]
    fn north_up_marker_faces_the_players_yaw() {
        // Facing south (yaw 0) points the arrow down the radar: a half turn from "up".
        assert_eq!(self_marker_angle(0.0), 180.0);
        // Facing north (yaw 180) points it up (a full turn is the same as no turn).
        assert_eq!(self_marker_angle(180.0), 360.0);
    }

    #[test]
    fn item_labels_show_stack_size_only_when_stacked() {
        assert_eq!(format_item_label("Diamond", 1), "Diamond");
        assert_eq!(format_item_label("Cobblestone", 32), "Cobblestone x32");
    }

    #[test]
    fn new_modules_build_with_unique_setting_ids() {
        for module in [
            build_item_esp_module(),
            build_storage_esp_module(),
            build_trajectories_module(),
            build_breadcrumbs_module(),
            build_damage_indicators_module(),
            build_radar_module(),
        ] {
            let mut ids: Vec<_> = module.settings.iter().map(|s| s.id).collect();
            let before = ids.len();
            ids.sort_unstable();
            ids.dedup();
            assert_eq!(ids.len(), before, "duplicate setting id in {}", module.key);
        }
    }

    #[test]
    fn storage_toggles_respect_module_settings() {
        let mut m = build_storage_esp_module();
        assert!(storage_kind_enabled(&m, StorageKind::Chest));
        assert!(!storage_kind_enabled(&m, StorageKind::Furnace));
        m.setting_mut("furnaces").unwrap().deserialize_value("true");
        assert!(storage_kind_enabled(&m, StorageKind::Furnace));
    }
}
