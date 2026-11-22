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
        ],
    }
}

pub fn apply_break_speed(module: &Module, event: &mut PlayerBreakSpeedEvent) {
    if !module.enabled {
        return;
    }
    let mult = module.float("multiplier", 1.4) as f32;
    let cap = module.float("max_speed", 25.0) as f32;
    if mult > 1.0 && event.new_speed > 0.0 {
        event.new_speed = (event.new_speed * mult).min(cap.max(event.original_speed));
    }
}
