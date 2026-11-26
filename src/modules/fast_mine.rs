//! Fast Mine module (Player category).
//!
//! Scales the player's block break speed through Horizon's `player_break_speed`
//! hook. Works on singleplayer/integrated worlds and any server that permits the
//! mod's speed adjustment.

use mod_api::PlayerBreakSpeedEvent;

use crate::module::{Category, Module, ModuleId};
use crate::settings::Setting;

pub fn build_module() -> Module {
    Module {
        id: ModuleId::FastMine,
        key: "fast_mine",
        name: "Fast Mine",
        description: "Multiplies block mining speed via Horizon's break speed hook.",
        category: Category::Player,
        enabled: false,
        keybind: None,
        expanded: false,
        show_in_arraylist: true,
        settings: vec![
            Setting::float(
                "multiplier",
                "Speed Multiplier",
                "Factor applied to your vanilla block breaking speed.",
                1.0,
                5.0,
                0.1,
                1.4,
            )
            .suffix("x")
            .group("Mining"),
            Setting::float(
                "max_speed",
                "Max Speed Cap",
                "Upper cap on the resulting break speed value.",
                1.0,
                60.0,
                1.0,
                25.0,
            ),
            Setting::bool(
                "effective_only",
                "Effective Tools Only",
                "Only speed up blocks your held tool already mines faster than bare hands.",
                false,
            )
            .group("Filters"),
            Setting::bool(
                "skip_instant",
                "Skip Instant Mining",
                "Leave blocks you already break instantly alone, so the cap never slows them.",
                true,
            ),
        ],
    }
}

/// Speed at or above which vanilla already breaks most blocks in a single tick.
const INSTANT_SPEED: f32 = 30.0;

pub fn apply_break_speed(module: &Module, event: &mut PlayerBreakSpeedEvent) {
    if !module.enabled {
        return;
    }
    if let Some(speed) = scaled_speed(
        event.original_speed,
        event.new_speed,
        module.float("multiplier", 1.4) as f32,
        module.float("max_speed", 25.0) as f32,
        module.bool("effective_only", false),
        module.bool("skip_instant", true),
    ) {
        event.new_speed = speed;
    }
}

/// The new break speed, or `None` to leave the event untouched.
pub fn scaled_speed(
    original: f32,
    current: f32,
    mult: f32,
    cap: f32,
    effective_only: bool,
    skip_instant: bool,
) -> Option<f32> {
    if !(mult > 1.0 && current > 0.0 && mult.is_finite() && current.is_finite()) {
        return None;
    }
    if effective_only && original <= 1.0 {
        return None;
    }
    if skip_instant && current >= INSTANT_SPEED {
        return None;
    }
    Some((current * mult).min(cap.max(original)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn multiplies_and_respects_cap() {
        assert_eq!(scaled_speed(4.0, 4.0, 2.0, 25.0, false, true), Some(8.0));
        assert_eq!(scaled_speed(4.0, 20.0, 2.0, 25.0, false, true), Some(25.0));
    }

    #[test]
    fn ignores_invalid_or_neutral_multipliers() {
        assert_eq!(scaled_speed(4.0, 4.0, 1.0, 25.0, false, true), None);
        assert_eq!(scaled_speed(4.0, 0.0, 2.0, 25.0, false, true), None);
        assert_eq!(scaled_speed(4.0, f32::NAN, 2.0, 25.0, false, true), None);
        assert_eq!(scaled_speed(4.0, 4.0, f32::INFINITY, 25.0, false, true), None);
    }

    #[test]
    fn effective_only_skips_bare_hand_speed() {
        assert_eq!(scaled_speed(1.0, 1.0, 2.0, 25.0, true, true), None);
        assert_eq!(scaled_speed(6.0, 6.0, 2.0, 25.0, true, true), Some(12.0));
    }

    #[test]
    fn instant_mining_is_left_alone_when_requested() {
        assert_eq!(scaled_speed(40.0, 40.0, 2.0, 25.0, false, true), None);
        assert_eq!(scaled_speed(40.0, 40.0, 2.0, 60.0, false, false), Some(60.0));
    }
}
