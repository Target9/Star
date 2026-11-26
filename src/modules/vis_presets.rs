//! Named Visuals presets applied through existing setting deserialize paths.

use crate::module::ModuleManager;

/// One setting write: `module.key` + `setting.id` + serialized value token.
#[derive(Clone, Copy, Debug)]
pub struct PresetWrite {
    pub module: &'static str,
    pub setting: &'static str,
    pub value: &'static str,
}

/// Apply every write in `rows`. Returns how many settings changed.
pub fn apply_writes(manager: &mut ModuleManager, rows: &[PresetWrite]) -> usize {
    let mut changed = 0;
    for row in rows {
        let Some(module) = manager.by_key_mut(row.module) else {
            continue;
        };
        let Some(setting) = module.setting_mut(row.setting) else {
            continue;
        };
        let before = setting.serialize_value();
        setting.deserialize_value(row.value);
        if setting.serialize_value() != before {
            changed += 1;
        }
    }
    if changed > 0 {
        manager.dirty = true;
    }
    changed
}

/// Reset every Visuals module setting to its defaults, then leave preset on Vanilla.
pub fn apply_vanilla(manager: &mut ModuleManager) -> usize {
    let mut changed = 0;
    for key in [
        "vis_preset",
        "vis_sky",
        "vis_world",
        "vis_camera",
        "vis_screen",
        "vis_hit",
        "vis_ambient",
        "vis_trip",
    ] {
        if let Some(module) = manager.by_key_mut(key) {
            for setting in &mut module.settings {
                if setting.reset() {
                    changed += 1;
                }
            }
        }
    }
    // Ensure the selector itself lands on Vanilla.
    changed += apply_writes(
        manager,
        &[PresetWrite {
            module: "vis_preset",
            setting: "preset",
            value: "Vanilla",
        }],
    );
    if changed > 0 {
        manager.dirty = true;
    }
    changed
}

const PVP: &[PresetWrite] = &[
    PresetWrite {
        module: "vis_preset",
        setting: "preset",
        value: "PVP",
    },
    PresetWrite {
        module: "vis_preset",
        setting: "colour_source",
        value: "Theme Accent",
    },
    PresetWrite {
        module: "vis_preset",
        setting: "dose",
        value: "0.0",
    },
    PresetWrite {
        module: "vis_sky",
        setting: "style",
        value: "Aurora",
    },
    PresetWrite {
        module: "vis_sky",
        setting: "brightness",
        value: "0.70",
    },
    PresetWrite {
        module: "vis_sky",
        setting: "tint_fog",
        value: "true",
    },
    PresetWrite {
        module: "vis_sky",
        setting: "fog_density",
        value: "0.35",
    },
    PresetWrite {
        module: "vis_sky",
        setting: "fog_breathing",
        value: "false",
    },
    PresetWrite {
        module: "vis_sky",
        setting: "clear_water",
        value: "true",
    },
    PresetWrite {
        module: "vis_sky",
        setting: "night_boost",
        value: "0.25",
    },
    PresetWrite {
        module: "vis_world",
        setting: "tint_biomes",
        value: "false",
    },
    PresetWrite {
        module: "vis_world",
        setting: "rainbow_biomes",
        value: "false",
    },
    PresetWrite {
        module: "vis_world",
        setting: "glow_rims",
        value: "true",
    },
    PresetWrite {
        module: "vis_world",
        setting: "rim_width",
        value: "2.0",
    },
    PresetWrite {
        module: "vis_world",
        setting: "player_light",
        value: "true",
    },
    PresetWrite {
        module: "vis_world",
        setting: "light_radius",
        value: "5.0",
    },
    PresetWrite {
        module: "vis_world",
        setting: "light_intensity",
        value: "0.35",
    },
    PresetWrite {
        module: "vis_camera",
        setting: "fov_offset",
        value: "2.0",
    },
    PresetWrite {
        module: "vis_camera",
        setting: "speed_fov",
        value: "true",
    },
    PresetWrite {
        module: "vis_camera",
        setting: "speed_fov_amount",
        value: "3.0",
    },
    PresetWrite {
        module: "vis_camera",
        setting: "sway",
        value: "0.0",
    },
    PresetWrite {
        module: "vis_camera",
        setting: "roll",
        value: "0.0",
    },
    PresetWrite {
        module: "vis_camera",
        setting: "hit_punch",
        value: "1.4",
    },
    PresetWrite {
        module: "vis_screen",
        setting: "vignette",
        value: "0.25",
    },
    PresetWrite {
        module: "vis_screen",
        setting: "vignette_pulse",
        value: "false",
    },
    PresetWrite {
        module: "vis_screen",
        setting: "wash",
        value: "0.0",
    },
    PresetWrite {
        module: "vis_screen",
        setting: "scanlines",
        value: "0.0",
    },
    PresetWrite {
        module: "vis_screen",
        setting: "edge_glow",
        value: "0.35",
    },
    PresetWrite {
        module: "vis_screen",
        setting: "hit_flash",
        value: "true",
    },
    PresetWrite {
        module: "vis_screen",
        setting: "low_hp_pulse",
        value: "true",
    },
    PresetWrite {
        module: "vis_screen",
        setting: "chroma_bars",
        value: "0.0",
    },
    PresetWrite {
        module: "vis_hit",
        setting: "enabled_fx",
        value: "true",
    },
    PresetWrite {
        module: "vis_hit",
        setting: "style",
        value: "Shock Ring",
    },
    PresetWrite {
        module: "vis_hit",
        setting: "count",
        value: "28",
    },
    PresetWrite {
        module: "vis_hit",
        setting: "speed",
        value: "1.4",
    },
    PresetWrite {
        module: "vis_hit",
        setting: "spread",
        value: "1.1",
    },
    PresetWrite {
        module: "vis_hit",
        setting: "size",
        value: "1.4",
    },
    PresetWrite {
        module: "vis_hit",
        setting: "crit_boost",
        value: "true",
    },
    PresetWrite {
        module: "vis_hit",
        setting: "kill_burst",
        value: "true",
    },
    PresetWrite {
        module: "vis_hit",
        setting: "ding",
        value: "true",
    },
    PresetWrite {
        module: "vis_hit",
        setting: "only_mine",
        value: "true",
    },
    PresetWrite {
        module: "vis_ambient",
        setting: "style",
        value: "Embers",
    },
    PresetWrite {
        module: "vis_ambient",
        setting: "density",
        value: "3.0",
    },
    PresetWrite {
        module: "vis_trip",
        setting: "onset",
        value: "0.0",
    },
    PresetWrite {
        module: "vis_trip",
        setting: "fov_breathe",
        value: "false",
    },
    PresetWrite {
        module: "vis_trip",
        setting: "roll_drift",
        value: "false",
    },
    PresetWrite {
        module: "vis_trip",
        setting: "rainbow_sky",
        value: "false",
    },
    PresetWrite {
        module: "vis_trip",
        setting: "rainbow_biomes",
        value: "false",
    },
    PresetWrite {
        module: "vis_trip",
        setting: "colour_wash",
        value: "false",
    },
    PresetWrite {
        module: "vis_trip",
        setting: "morph",
        value: "false",
    },
    PresetWrite {
        module: "vis_trip",
        setting: "kaleidoscope",
        value: "false",
    },
    PresetWrite {
        module: "vis_trip",
        setting: "particles",
        value: "false",
    },
    PresetWrite {
        module: "vis_trip",
        setting: "audio_warp",
        value: "false",
    },
];

const LSD: &[PresetWrite] = &[
    PresetWrite {
        module: "vis_preset",
        setting: "preset",
        value: "LSD",
    },
    PresetWrite {
        module: "vis_preset",
        setting: "colour_source",
        value: "Rainbow",
    },
    PresetWrite {
        module: "vis_preset",
        setting: "dose",
        value: "1.15",
    },
    PresetWrite {
        module: "vis_sky",
        setting: "style",
        value: "Psychedelic",
    },
    PresetWrite {
        module: "vis_sky",
        setting: "brightness",
        value: "0.72",
    },
    PresetWrite {
        module: "vis_sky",
        setting: "tint_fog",
        value: "true",
    },
    PresetWrite {
        module: "vis_sky",
        setting: "fog_density",
        value: "0.95",
    },
    PresetWrite {
        module: "vis_sky",
        setting: "fog_breathing",
        value: "true",
    },
    PresetWrite {
        module: "vis_sky",
        setting: "fog_breath_amount",
        value: "0.40",
    },
    PresetWrite {
        module: "vis_sky",
        setting: "night_boost",
        value: "0.45",
    },
    PresetWrite {
        module: "vis_world",
        setting: "tint_biomes",
        value: "true",
    },
    PresetWrite {
        module: "vis_world",
        setting: "rainbow_biomes",
        value: "true",
    },
    PresetWrite {
        module: "vis_world",
        setting: "saturation",
        value: "1.55",
    },
    PresetWrite {
        module: "vis_world",
        setting: "glow_rims",
        value: "true",
    },
    PresetWrite {
        module: "vis_world",
        setting: "player_light",
        value: "true",
    },
    PresetWrite {
        module: "vis_world",
        setting: "light_radius",
        value: "8.0",
    },
    PresetWrite {
        module: "vis_world",
        setting: "light_intensity",
        value: "0.55",
    },
    PresetWrite {
        module: "vis_camera",
        setting: "fov_offset",
        value: "2.0",
    },
    PresetWrite {
        module: "vis_camera",
        setting: "speed_fov",
        value: "false",
    },
    PresetWrite {
        module: "vis_camera",
        setting: "sway",
        value: "1.1",
    },
    PresetWrite {
        module: "vis_camera",
        setting: "roll",
        value: "5.0",
    },
    PresetWrite {
        module: "vis_camera",
        setting: "hit_punch",
        value: "0.8",
    },
    PresetWrite {
        module: "vis_screen",
        setting: "vignette",
        value: "0.32",
    },
    PresetWrite {
        module: "vis_screen",
        setting: "vignette_pulse",
        value: "true",
    },
    PresetWrite {
        module: "vis_screen",
        setting: "wash",
        value: "0.08",
    },
    PresetWrite {
        module: "vis_screen",
        setting: "scanlines",
        value: "0.0",
    },
    PresetWrite {
        module: "vis_screen",
        setting: "edge_glow",
        value: "0.18",
    },
    PresetWrite {
        module: "vis_screen",
        setting: "hit_flash",
        value: "true",
    },
    PresetWrite {
        module: "vis_screen",
        setting: "chroma_bars",
        value: "0.0",
    },
    PresetWrite {
        module: "vis_hit",
        setting: "enabled_fx",
        value: "true",
    },
    PresetWrite {
        module: "vis_hit",
        setting: "style",
        value: "Stardust",
    },
    PresetWrite {
        module: "vis_hit",
        setting: "count",
        value: "20",
    },
    PresetWrite {
        module: "vis_hit",
        setting: "crit_boost",
        value: "true",
    },
    PresetWrite {
        module: "vis_hit",
        setting: "kill_burst",
        value: "true",
    },
    PresetWrite {
        module: "vis_ambient",
        setting: "style",
        value: "Sparkles",
    },
    PresetWrite {
        module: "vis_ambient",
        setting: "density",
        value: "7.0",
    },
    PresetWrite {
        module: "vis_ambient",
        setting: "radius",
        value: "9.0",
    },
    PresetWrite {
        module: "vis_trip",
        setting: "onset",
        value: "5.0",
    },
    PresetWrite {
        module: "vis_trip",
        setting: "wave_period",
        value: "9.0",
    },
    PresetWrite {
        module: "vis_trip",
        setting: "fov_breathe",
        value: "true",
    },
    PresetWrite {
        module: "vis_trip",
        setting: "roll_drift",
        value: "true",
    },
    PresetWrite {
        module: "vis_trip",
        setting: "rainbow_sky",
        value: "true",
    },
    PresetWrite {
        module: "vis_trip",
        setting: "rainbow_biomes",
        value: "true",
    },
    PresetWrite {
        module: "vis_trip",
        setting: "colour_wash",
        value: "true",
    },
    PresetWrite {
        module: "vis_trip",
        setting: "morph",
        value: "true",
    },
    PresetWrite {
        module: "vis_trip",
        setting: "kaleidoscope",
        value: "true",
    },
    PresetWrite {
        module: "vis_trip",
        setting: "particles",
        value: "true",
    },
    PresetWrite {
        module: "vis_trip",
        setting: "audio_warp",
        value: "false",
    },
    PresetWrite {
        module: "vis_trip",
        setting: "shader_pack",
        value: "true",
    },
    PresetWrite {
        module: "vis_trip",
        setting: "strength",
        value: "1.25",
    },
];

const SYNTHWAVE: &[PresetWrite] = &[
    PresetWrite {
        module: "vis_preset",
        setting: "preset",
        value: "Synthwave",
    },
    PresetWrite {
        module: "vis_preset",
        setting: "colour_source",
        value: "Custom A→B",
    },
    PresetWrite {
        module: "vis_preset",
        setting: "custom_a",
        value: "FFFF4AD8",
    },
    PresetWrite {
        module: "vis_preset",
        setting: "custom_b",
        value: "FF4AE8FF",
    },
    PresetWrite {
        module: "vis_preset",
        setting: "dose",
        value: "0.15",
    },
    PresetWrite {
        module: "vis_sky",
        setting: "style",
        value: "Galaxy",
    },
    PresetWrite {
        module: "vis_sky",
        setting: "brightness",
        value: "0.85",
    },
    PresetWrite {
        module: "vis_sky",
        setting: "tint_fog",
        value: "true",
    },
    PresetWrite {
        module: "vis_sky",
        setting: "fog_density",
        value: "0.55",
    },
    PresetWrite {
        module: "vis_sky",
        setting: "night_boost",
        value: "0.70",
    },
    PresetWrite {
        module: "vis_world",
        setting: "glow_rims",
        value: "true",
    },
    PresetWrite {
        module: "vis_world",
        setting: "player_light",
        value: "true",
    },
    PresetWrite {
        module: "vis_screen",
        setting: "vignette",
        value: "0.40",
    },
    PresetWrite {
        module: "vis_screen",
        setting: "edge_glow",
        value: "0.60",
    },
    PresetWrite {
        module: "vis_screen",
        setting: "scanlines",
        value: "0.18",
    },
    PresetWrite {
        module: "vis_screen",
        setting: "wash",
        value: "0.10",
    },
    PresetWrite {
        module: "vis_hit",
        setting: "enabled_fx",
        value: "true",
    },
    PresetWrite {
        module: "vis_hit",
        setting: "style",
        value: "Nova",
    },
    PresetWrite {
        module: "vis_ambient",
        setting: "style",
        value: "Sparkles",
    },
    PresetWrite {
        module: "vis_ambient",
        setting: "density",
        value: "6.0",
    },
    PresetWrite {
        module: "vis_trip",
        setting: "rainbow_sky",
        value: "false",
    },
    PresetWrite {
        module: "vis_trip",
        setting: "fov_breathe",
        value: "false",
    },
    PresetWrite {
        module: "vis_trip",
        setting: "roll_drift",
        value: "false",
    },
];

const DREAMY: &[PresetWrite] = &[
    PresetWrite {
        module: "vis_preset",
        setting: "preset",
        value: "Dreamy",
    },
    PresetWrite {
        module: "vis_preset",
        setting: "colour_source",
        value: "Custom A→B",
    },
    PresetWrite {
        module: "vis_preset",
        setting: "custom_a",
        value: "FFFFB7D5",
    },
    PresetWrite {
        module: "vis_preset",
        setting: "custom_b",
        value: "FFB7E0FF",
    },
    PresetWrite {
        module: "vis_preset",
        setting: "dose",
        value: "0.25",
    },
    PresetWrite {
        module: "vis_sky",
        setting: "style",
        value: "Gradient",
    },
    PresetWrite {
        module: "vis_sky",
        setting: "brightness",
        value: "0.60",
    },
    PresetWrite {
        module: "vis_sky",
        setting: "tint_fog",
        value: "true",
    },
    PresetWrite {
        module: "vis_sky",
        setting: "fog_density",
        value: "1.15",
    },
    PresetWrite {
        module: "vis_sky",
        setting: "fog_breathing",
        value: "true",
    },
    PresetWrite {
        module: "vis_sky",
        setting: "fog_breath_amount",
        value: "0.25",
    },
    PresetWrite {
        module: "vis_world",
        setting: "tint_biomes",
        value: "true",
    },
    PresetWrite {
        module: "vis_world",
        setting: "rainbow_biomes",
        value: "false",
    },
    PresetWrite {
        module: "vis_world",
        setting: "grass",
        value: "FFA8E6B8",
    },
    PresetWrite {
        module: "vis_world",
        setting: "foliage",
        value: "FF8FD4A4",
    },
    PresetWrite {
        module: "vis_world",
        setting: "water",
        value: "FF9BC8FF",
    },
    PresetWrite {
        module: "vis_camera",
        setting: "sway",
        value: "0.35",
    },
    PresetWrite {
        module: "vis_screen",
        setting: "vignette",
        value: "0.30",
    },
    PresetWrite {
        module: "vis_screen",
        setting: "wash",
        value: "0.12",
    },
    PresetWrite {
        module: "vis_ambient",
        setting: "style",
        value: "Dust",
    },
    PresetWrite {
        module: "vis_ambient",
        setting: "density",
        value: "5.0",
    },
    PresetWrite {
        module: "vis_hit",
        setting: "enabled_fx",
        value: "true",
    },
    PresetWrite {
        module: "vis_hit",
        setting: "style",
        value: "Burst",
    },
];

const VOID: &[PresetWrite] = &[
    PresetWrite {
        module: "vis_preset",
        setting: "preset",
        value: "Void",
    },
    PresetWrite {
        module: "vis_preset",
        setting: "colour_source",
        value: "Theme Accent",
    },
    PresetWrite {
        module: "vis_preset",
        setting: "dose",
        value: "0.0",
    },
    PresetWrite {
        module: "vis_sky",
        setting: "style",
        value: "Galaxy",
    },
    PresetWrite {
        module: "vis_sky",
        setting: "brightness",
        value: "0.40",
    },
    PresetWrite {
        module: "vis_sky",
        setting: "tint_fog",
        value: "true",
    },
    PresetWrite {
        module: "vis_sky",
        setting: "fog_density",
        value: "1.60",
    },
    PresetWrite {
        module: "vis_sky",
        setting: "night_boost",
        value: "0.0",
    },
    PresetWrite {
        module: "vis_world",
        setting: "glow_rims",
        value: "true",
    },
    PresetWrite {
        module: "vis_world",
        setting: "player_light",
        value: "true",
    },
    PresetWrite {
        module: "vis_world",
        setting: "light_intensity",
        value: "0.55",
    },
    PresetWrite {
        module: "vis_screen",
        setting: "vignette",
        value: "0.70",
    },
    PresetWrite {
        module: "vis_screen",
        setting: "edge_glow",
        value: "0.25",
    },
    PresetWrite {
        module: "vis_screen",
        setting: "wash",
        value: "0.08",
    },
    PresetWrite {
        module: "vis_hit",
        setting: "enabled_fx",
        value: "true",
    },
    PresetWrite {
        module: "vis_hit",
        setting: "style",
        value: "Sparks",
    },
    PresetWrite {
        module: "vis_ambient",
        setting: "style",
        value: "Sparkles",
    },
    PresetWrite {
        module: "vis_ambient",
        setting: "density",
        value: "2.0",
    },
];

/// Apply the preset named by `index` into `PRESET_OPTIONS`. Custom is a no-op.
pub fn apply_preset_index(manager: &mut ModuleManager, index: usize) -> usize {
    match index {
        0 => apply_vanilla(manager),
        1 => apply_writes(manager, PVP),
        2 => apply_writes(manager, LSD),
        3 => apply_writes(manager, SYNTHWAVE),
        4 => apply_writes(manager, DREAMY),
        5 => apply_writes(manager, VOID),
        _ => 0, // Custom
    }
}

/// Card metadata for the Visuals presets page.
#[derive(Clone, Copy, Debug)]
pub struct PresetCard {
    pub name: &'static str,
    pub blurb: &'static str,
    pub swatch_a: u32,
    pub swatch_b: u32,
}

pub const PRESET_CARDS: [PresetCard; 7] = [
    PresetCard {
        name: "Vanilla",
        blurb: "Reset every Visuals setting.",
        swatch_a: 0xFF9A_A0A8,
        swatch_b: 0xFF5A_6068,
    },
    PresetCard {
        name: "PVP",
        blurb: "Insane hit FX, clear fog, accent aurora sky.",
        swatch_a: 0xFFFF_4A6A,
        swatch_b: 0xFF91_3DE8,
    },
    PresetCard {
        name: "LSD",
        blurb: "Breathing world, colour shift, soft tracers.",
        swatch_a: 0xFFFF_4AD8,
        swatch_b: 0xFF4A_FF8A,
    },
    PresetCard {
        name: "Synthwave",
        blurb: "Magenta / cyan neon night drive.",
        swatch_a: 0xFFFF_4AD8,
        swatch_b: 0xFF4A_E8FF,
    },
    PresetCard {
        name: "Dreamy",
        blurb: "Soft pastel fog and gentle particles.",
        swatch_a: 0xFFFF_B7D5,
        swatch_b: 0xFFB7_E0FF,
    },
    PresetCard {
        name: "Void",
        blurb: "Dark moody sky, heavy vignette.",
        swatch_a: 0xFF1A_1020,
        swatch_b: 0xFF6A_3AE8,
    },
    PresetCard {
        name: "Custom",
        blurb: "Keep your current mix. No overwrite.",
        swatch_a: 0xFFD8_DCE6,
        swatch_b: 0xFF3A_3E48,
    },
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::module::{ModuleId, ModuleManager};
    use crate::modules::vis_settings::PRESET_OPTIONS as PRESET_NAMES;

    fn all_tables() -> [&'static [PresetWrite]; 5] {
        [PVP, LSD, SYNTHWAVE, DREAMY, VOID]
    }

    #[test]
    fn every_preset_write_targets_a_real_setting() {
        let mgr = ModuleManager::new();
        for table in all_tables() {
            for row in table {
                let module = mgr
                    .modules
                    .iter()
                    .find(|m| m.key == row.module)
                    .unwrap_or_else(|| panic!("missing module {}", row.module));
                assert!(
                    module.setting(row.setting).is_some(),
                    "missing {}.{}",
                    row.module,
                    row.setting
                );
            }
        }
        assert_eq!(PRESET_NAMES.len(), PRESET_CARDS.len());
    }

    #[test]
    fn applying_pvp_and_lsd_changes_settings() {
        let mut mgr = ModuleManager::new();
        assert!(apply_preset_index(&mut mgr, 1) > 0);
        assert_eq!(mgr.get(ModuleId::VisPreset).mode_str("preset", ""), "PVP");
        assert!(mgr.get(ModuleId::VisHit).bool("enabled_fx", false));

        assert!(apply_preset_index(&mut mgr, 2) > 0);
        assert_eq!(mgr.get(ModuleId::VisPreset).mode_str("preset", ""), "LSD");
        assert!((mgr.get(ModuleId::VisPreset).float("dose", 0.0) - 1.15).abs() < 1e-6);

        let before = mgr.get(ModuleId::VisHit).bool("enabled_fx", false);
        assert_eq!(apply_preset_index(&mut mgr, 6), 0, "Custom is a no-op");
        assert_eq!(mgr.get(ModuleId::VisHit).bool("enabled_fx", false), before);
    }

    #[test]
    fn vanilla_resets_dose() {
        let mut mgr = ModuleManager::new();
        apply_preset_index(&mut mgr, 2);
        apply_preset_index(&mut mgr, 0);
        assert!((mgr.get(ModuleId::VisPreset).float("dose", 1.0) - 0.0).abs() < 1e-6);
        assert_eq!(
            mgr.get(ModuleId::VisPreset).mode_str("preset", ""),
            "Vanilla"
        );
    }
}
