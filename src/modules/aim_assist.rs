//! Aim Assist module (Combat category).
//!
//! Smoothly rotates the local player's view toward the best valid target inside
//! range and FOV, with full customization for targets, priority, aim point,
//! activation conditions, line-of-sight filtering, smoothing, deadzone, and
//! movement prediction.

use mod_api::{ClientHost, ClipBlock, ClipFluid, EntityRef, Hand};

use crate::entity_filter::{EntityFilter, pretty_entity_id};
use crate::module::{Category, Module, ModuleId};
use crate::settings::Setting;

/// Snapshot of the currently tracked target for HUD display.
#[derive(Clone, Debug)]
pub struct AimTargetInfo {
    pub name: String,
    pub distance: f64,
    pub angle_deg: f32,
    pub health: Option<f32>,
    pub max_health: Option<f32>,
    pub aim_point: [f64; 3],
}

/// Constructs the `Aim Assist` module definition with all customizable settings.
pub fn build_module() -> Module {
    Module {
        id: ModuleId::AimAssist,
        key: "aim_assist",
        name: "Aim Assist",
        description: "Smoothly guides your crosshair toward nearby players or mobs.",
        category: Category::Combat,
        enabled: false,
        keybind: None,
        expanded: false,
        show_in_arraylist: true,
        settings: vec![
            Setting::entity_filter(
                "entities",
                "Targets",
                "Open the Entity Selector to pick the exact entity types Aim Assist tracks.",
                EntityFilter::players_only(),
            )
            .group("Targets"),
            Setting::bool(
                "ignore_invisible",
                "Ignore Invisible",
                "Skip entities that are currently invisible.",
                true,
            ),
            Setting::mode(
                "activation",
                "Activation",
                "When Aim Assist actively turns your camera.",
                &["Always", "Holding Attack", "Holding Weapon", "Attack + Weapon"],
                0,
            )
            .group("Activation"),
            Setting::float(
                "range",
                "Range",
                "Maximum distance in blocks from your eyes to the target.",
                1.5,
                8.0,
                0.1,
                4.5,
            )
            .suffix("m")
            .group("Targeting"),
            Setting::int(
                "fov",
                "FOV",
                "Maximum angular cone around your crosshair (360 = all around).",
                10,
                360,
                5,
                90,
            )
            .suffix("°"),
            Setting::mode(
                "priority",
                "Priority",
                "How the best target is chosen when several are in range.",
                &["Closest", "Lowest Angle", "Lowest Health", "Combined"],
                0,
            ),
            Setting::mode(
                "aim_point",
                "Aim Point",
                "Point on the target's hitbox to aim toward.",
                &["Chest", "Eyes", "Feet", "Closest Box"],
                0,
            ),
            Setting::bool(
                "line_of_sight",
                "Line of Sight",
                "Only track targets not blocked by solid blocks.",
                true,
            ),
            Setting::mode(
                "update_rate",
                "Update Rate",
                "Run every render frame for smooth tracking or once per game tick.",
                &["Per Frame", "Per Tick"],
                0,
            )
            .group("Smoothing"),
            Setting::mode(
                "smoothing",
                "Curve",
                "Ease Out slows gently near the target; Linear turns at a constant rate.",
                &["Ease Out", "Linear"],
                0,
            ),
            Setting::float(
                "h_speed",
                "Horizontal Speed",
                "Maximum horizontal turn speed in degrees per tick (20 ticks/sec).",
                1.0,
                180.0,
                1.0,
                35.0,
            )
            .suffix("°"),
            Setting::float(
                "v_speed",
                "Vertical Speed",
                "Maximum vertical turn speed in degrees per tick (0 = horizontal only).",
                0.0,
                180.0,
                1.0,
                20.0,
            )
            .suffix("°"),
            Setting::float(
                "deadzone",
                "Deadzone",
                "Stop micro-adjusting once within this many degrees of the target.",
                0.0,
                10.0,
                0.25,
                1.5,
            )
            .suffix("°"),
            Setting::float(
                "predict",
                "Movement Lead",
                "Lead moving targets by this many ticks of their velocity.",
                0.0,
                2.0,
                0.1,
                0.0,
            )
            .suffix("t"),
            Setting::bool(
                "show_target_hud",
                "Target HUD",
                "Show a compact info box for the currently tracked target.",
                true,
            )
            .group("Visuals"),
            Setting::bool(
                "show_fov_circle",
                "FOV Indicator",
                "Draw a subtle ring around the crosshair representing the Aim Assist FOV.",
                false,
            ),
        ],
    }
}

/// Runs one step of Aim Assist. `tick_scale` is `1.0` for a 50ms client tick,
/// or `elapsed_secs * 20.0` for per-frame updates.
pub fn step(
    host: &mut dyn ClientHost,
    module: &Module,
    tick_scale: f32,
    is_frame_hook: bool,
) -> Option<AimTargetInfo> {
    if !module.enabled || !host.playing() {
        return None;
    }

    let update_rate = module.mode("update_rate", 0);
    // 0 = Per Frame, 1 = Per Tick
    if (update_rate == 0 && !is_frame_hook) || (update_rate == 1 && is_frame_hook) {
        return None;
    }

    let player = host.camera_entity()?;
    if !activation_satisfied(host, player, module.mode("activation", 0)) {
        return None;
    }

    let entities = module.entity_filter("entities")?;
    let ignore_invisible = module.bool("ignore_invisible", true);
    let range = module.float("range", 4.5);
    let fov = module.int("fov", 90) as f32;
    let priority = module.mode("priority", 0);
    let aim_point_mode = module.mode("aim_point", 0);
    let require_los = module.bool("line_of_sight", true);
    let predict = module.float("predict", 0.0);

    let (target, point, distance, angle_deg) = pick_target(
        host,
        player,
        TargetFilter {
            entities,
            ignore_invisible,
            range,
            fov,
            priority,
            aim_point_mode,
            require_los,
            predict,
        },
    )?;

    let scale = tick_scale.clamp(0.02, 2.0);
    let h_speed = module.float("h_speed", 35.0) as f32 * scale;
    let v_speed = module.float("v_speed", 20.0) as f32 * scale;
    let ease_out = module.mode("smoothing", 0) == 0;
    let deadzone = module.float("deadzone", 1.5) as f32;

    if let Some(eyes) = player.eye_position(host)
        && let Some([yaw, pitch]) = player.rotation(host)
        && dist(eyes, point) >= 1.0e-4
    {
        let (want_yaw, want_pitch) = look_at(eyes, point);
        let (new_yaw, new_pitch) = compute_step(
            yaw,
            pitch,
            want_yaw,
            want_pitch,
            h_speed,
            v_speed,
            deadzone,
            ease_out,
        );
        if (new_yaw - yaw).abs() > 1e-4 || (new_pitch - pitch).abs() > 1e-4 {
            let _ = host.set_local_rotation(new_yaw, new_pitch);
        }
    }

    let name = target
        .name(host)
        .filter(|s| !s.is_empty())
        .or_else(|| target.kind(host).map(|k| pretty_entity_id(&k.id(host))))
        .unwrap_or_else(|| "Target".to_owned());

    let (health, max_health) = match host.living(target) {
        Some(living) => (living.health(host), living.max_health(host)),
        None => (None, None),
    };

    Some(AimTargetInfo {
        name,
        distance,
        angle_deg,
        health,
        max_health,
        aim_point: point,
    })
}

struct TargetFilter<'a> {
    entities: &'a EntityFilter,
    ignore_invisible: bool,
    range: f64,
    fov: f32,
    priority: usize,
    aim_point_mode: usize,
    require_los: bool,
    predict: f64,
}

fn activation_satisfied(host: &dyn ClientHost, player: EntityRef, mode: usize) -> bool {
    let attack_held = || {
        host.mapping("key.attack")
            .ok()
            .flatten()
            .is_some_and(|m| m.down)
    };
    let weapon_held = || is_holding_weapon(host, player);

    match mode {
        0 => true,
        1 => attack_held(),
        2 => weapon_held(),
        3 => attack_held() && weapon_held(),
        _ => true,
    }
}

fn is_holding_weapon(host: &dyn ClientHost, player: EntityRef) -> bool {
    let Some(living) = host.living(player) else {
        return false;
    };
    let Some(stack) = living.hand_item(host, Hand::Main) else {
        return false;
    };
    let id = stack.item().id(host);
    let path = id.split_once(':').map_or(id.as_str(), |(_, p)| p);
    path.ends_with("_sword")
        || path.ends_with("_axe")
        || path.ends_with("_spear")
        || path == "mace"
        || path == "trident"
}

fn pick_target(
    host: &dyn ClientHost,
    player: EntityRef,
    filter: TargetFilter<'_>,
) -> Option<(EntityRef, [f64; 3], f64, f32)> {
    if filter.entities.is_empty() {
        return None;
    }

    let eyes = player.eye_position(host)?;
    let level = player.level(host)?;
    let [yaw, pitch] = player.rotation(host).unwrap_or([0.0, 0.0]);
    let pad = filter.range + 1.0;
    let candidates = level.entities_in(
        host,
        [eyes[0] - pad, eyes[1] - pad, eyes[2] - pad],
        [eyes[0] + pad, eyes[1] + pad, eyes[2] + pad],
    )?;

    let mut best: Option<(f64, EntityRef, [f64; 3], f64, f32)> = None;

    for entity in candidates {
        if entity.id() == player.id() || entity.spectator(host) == Some(true) {
            continue;
        }
        if filter.ignore_invisible && entity.is_invisible(host) == Some(true) {
            continue;
        }

        let Some(kind) = entity.kind(host) else {
            continue;
        };
        let kind_id = kind.id(host);
        if !filter.entities.contains(&kind_id) {
            continue;
        }

        if let Some(living) = host.living(entity)
            && (living.alive(host) == Some(false)
                || living.health(host).is_some_and(|h| h <= 0.0))
        {
            continue;
        }

        let mut aim = select_aim_point(host, entity, eyes, filter.aim_point_mode);
        if filter.predict > 0.0
            && let Some(vel) = entity.velocity(host)
        {
            aim[0] += vel[0] * filter.predict;
            aim[1] += vel[1] * filter.predict;
            aim[2] += vel[2] * filter.predict;
        }

        let box_dist = closest_on_box(host, entity, eyes)
            .map(|p| dist(eyes, p))
            .unwrap_or_else(|| dist(eyes, aim));
        if box_dist > filter.range {
            continue;
        }

        let (want_yaw, want_pitch) = look_at(eyes, aim);
        let yaw_off = wrap_degrees(want_yaw - yaw).abs();
        let pitch_off = (want_pitch - pitch).abs();
        let angle = yaw_off.hypot(pitch_off);
        if filter.fov < 360.0 && angle > filter.fov * 0.5 {
            continue;
        }

        if filter.require_los
            && let Some(hit) = level.clip(host, eyes, aim, ClipBlock::Collider, ClipFluid::None)
            && !hit.miss
            && dist(eyes, hit.location) + 0.15 < dist(eyes, aim)
        {
            continue;
        }

        let health = host
            .living(entity)
            .and_then(|l| l.health(host))
            .unwrap_or(20.0) as f64;

        let score = match filter.priority {
            0 => box_dist,                               // Closest
            1 => angle as f64,                           // Lowest Angle
            2 => health * 10.0 + box_dist,               // Lowest Health
            _ => box_dist * 0.6 + (angle as f64) * 0.05, // Combined
        };

        if best.as_ref().is_none_or(|(best_score, ..)| score < *best_score) {
            best = Some((score, entity, aim, box_dist, angle));
        }
    }

    best.map(|(_, e, p, d, a)| (e, p, d, a))
}

fn select_aim_point(
    host: &dyn ClientHost,
    entity: EntityRef,
    eyes: [f64; 3],
    mode: usize,
) -> [f64; 3] {
    match mode {
        1 => {
            // Eyes
            if let Some(ep) = entity.eye_position(host) {
                return ep;
            }
        }
        2 => {
            // Feet
            if let Some(feet) = entity.position(host) {
                return [feet[0], feet[1] + 0.25, feet[2]];
            }
        }
        3 => {
            // Closest point on bounding box
            if let Some(p) = closest_on_box(host, entity, eyes) {
                return p;
            }
        }
        _ => {
            // Chest (default): center of bounding box slightly above midpoint
            if let Some([min, max]) = entity.bounding_box(host) {
                return [
                    (min[0] + max[0]) * 0.5,
                    min[1] + (max[1] - min[1]) * 0.65,
                    (min[2] + max[2]) * 0.5,
                ];
            }
        }
    }

    if let Some(ep) = entity.eye_position(host) {
        return ep;
    }
    let feet = entity.position(host).unwrap_or([0.0; 3]);
    [feet[0], feet[1] + 1.2, feet[2]]
}

fn closest_on_box(host: &dyn ClientHost, entity: EntityRef, from: [f64; 3]) -> Option<[f64; 3]> {
    let [min, max] = entity.bounding_box(host)?;
    Some([
        from[0].clamp(min[0].min(max[0]), min[0].max(max[0])),
        from[1].clamp(min[1].min(max[1]), min[1].max(max[1])),
        from[2].clamp(min[2].min(max[2]), min[2].max(max[2])),
    ])
}

/// Computes the next `(yaw, pitch)` given current angles, desired angles,
/// per-axis speed caps, deadzone, and curve mode.
#[allow(clippy::too_many_arguments)]
pub fn compute_step(
    cur_yaw: f32,
    cur_pitch: f32,
    want_yaw: f32,
    want_pitch: f32,
    max_h_step: f32,
    max_v_step: f32,
    deadzone: f32,
    ease_out: bool,
) -> (f32, f32) {
    let dyaw = wrap_degrees(want_yaw - cur_yaw);
    let dpitch = want_pitch - cur_pitch;

    if dyaw.hypot(dpitch) <= deadzone {
        return (cur_yaw, cur_pitch);
    }

    let step_axis = |delta: f32, max_step: f32| -> f32 {
        if max_step <= 0.0 || delta.abs() <= 1e-4 {
            return 0.0;
        }
        if ease_out {
            // Smooth exponential-like approach: turns briskly when far off-target and eases in gently.
            let factor = (max_step / 45.0).clamp(0.05, 1.0);
            let eased = delta * factor;
            eased.clamp(-max_step, max_step)
        } else {
            delta.clamp(-max_step, max_step)
        }
    };

    let next_yaw = cur_yaw + step_axis(dyaw, max_h_step);
    let next_pitch = (cur_pitch + step_axis(dpitch, max_v_step)).clamp(-90.0, 90.0);
    (next_yaw, next_pitch)
}

pub fn look_at(from: [f64; 3], to: [f64; 3]) -> (f32, f32) {
    let dx = to[0] - from[0];
    let dy = to[1] - from[1];
    let dz = to[2] - from[2];
    let horizontal = (dx * dx + dz * dz).sqrt();
    let yaw = dz.atan2(dx).to_degrees() as f32 - 90.0;
    let pitch = -(dy.atan2(horizontal).to_degrees()) as f32;
    (yaw, pitch)
}

pub fn wrap_degrees(mut degrees: f32) -> f32 {
    degrees %= 360.0;
    if degrees >= 180.0 {
        degrees -= 360.0;
    } else if degrees < -180.0 {
        degrees += 360.0;
    }
    degrees
}

pub fn dist(a: [f64; 3], b: [f64; 3]) -> f64 {
    let dx = a[0] - b[0];
    let dy = a[1] - b[1];
    let dz = a[2] - b[2];
    (dx * dx + dy * dy + dz * dz).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrap_degrees_stays_within_half_turn() {
        assert!((wrap_degrees(190.0) - (-170.0)).abs() < 1e-4);
        assert!((wrap_degrees(-190.0) - 170.0).abs() < 1e-4);
        assert!((wrap_degrees(360.0)).abs() < 1e-4);
    }

    #[test]
    fn deadzone_stops_micro_jitter() {
        let (y, p) = compute_step(10.0, 5.0, 10.8, 5.4, 30.0, 20.0, 1.5, true);
        assert_eq!((y, p), (10.0, 5.0));
    }

    #[test]
    fn vertical_zero_keeps_pitch_locked() {
        let (y, p) = compute_step(0.0, 12.0, 40.0, -20.0, 10.0, 0.0, 0.5, false);
        assert!((y - 10.0).abs() < 1e-4);
        assert!((p - 12.0).abs() < 1e-4);
    }

    #[test]
    fn look_at_cardinal_directions_match_minecraft_yaw() {
        let (yaw_south, pitch_flat) = look_at([0.0, 64.0, 0.0], [0.0, 64.0, 5.0]);
        assert!(wrap_degrees(yaw_south - 0.0).abs() < 1e-3);
        assert!(pitch_flat.abs() < 1e-3);

        let (yaw_east, _) = look_at([0.0, 64.0, 0.0], [5.0, 64.0, 0.0]);
        assert!(wrap_degrees(yaw_east - (-90.0)).abs() < 1e-3);
    }
}
