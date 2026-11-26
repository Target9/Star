//! Settings builders for the Visuals tab modules.

use crate::module::{Category, Module, ModuleId};
use crate::settings::Setting;

pub const PRESET_OPTIONS: &[&str] = &[
    "Vanilla",
    "PVP",
    "LSD",
    "Synthwave",
    "Dreamy",
    "Void",
    "Custom",
];

pub const COLOUR_SOURCE_OPTIONS: &[&str] =
    &["Theme Accent", "Theme Complement", "Rainbow", "Custom A→B"];

pub const SKY_STYLE_OPTIONS: &[&str] =
    &["Vanilla", "Gradient", "Aurora", "Galaxy", "Psychedelic"];

pub const HIT_STYLE_OPTIONS: &[&str] =
    &["Burst", "Sparks", "Shock Ring", "Stardust", "Fireworks", "Nova"];

pub const AMBIENT_STYLE_OPTIONS: &[&str] =
    &["None", "Embers", "Sparkles", "Dust", "Geometric"];

pub fn build_preset_module() -> Module {
    Module {
        id: ModuleId::VisPreset,
        key: "vis_preset",
        name: "Visuals",
        description: "Master switch, preset pack, colour source and dose for the Visuals tab.",
        category: Category::Visuals,
        enabled: true,
        keybind: None,
        expanded: true,
        show_in_arraylist: false,
        settings: vec![
            Setting::mode(
                "preset",
                "Preset",
                "Apply a ready-made look. Custom keeps your current values.",
                PRESET_OPTIONS,
                0,
            )
            .group("Presets"),
            Setting::mode(
                "colour_source",
                "Colour Source",
                "Where accent colours for hit FX, sky tint, lights and overlays come from.",
                COLOUR_SOURCE_OPTIONS,
                0,
            ),
            Setting::float(
                "dose",
                "Dose",
                "Global intensity multiplier for trip-style effects (0–150%).",
                0.0,
                1.5,
                0.05,
                0.0,
            )
            .suffix("x")
            .group("Intensity"),
            Setting::color(
                "custom_a",
                "Custom A",
                "First colour when Colour Source is Custom A→B.",
                0xFFFF_4AD8,
            )
            .group("Custom Colours"),
            Setting::color(
                "custom_b",
                "Custom B",
                "Second colour when Colour Source is Custom A→B.",
                0xFF4A_E8FF,
            ),
        ],
    }
}

pub fn build_sky_module() -> Module {
    Module {
        id: ModuleId::VisSky,
        key: "vis_sky",
        name: "Sky & Fog",
        description: "Sky dome style, fog colour, density and breathing.",
        category: Category::Visuals,
        enabled: true,
        keybind: None,
        expanded: true,
        show_in_arraylist: false,
        settings: vec![
            Setting::mode(
                "style",
                "Sky Style",
                "Backdrop drawn behind the world after the vanilla sky.",
                SKY_STYLE_OPTIONS,
                0,
            )
            .group("Sky"),
            Setting::float(
                "brightness",
                "Sky Brightness",
                "How strong the custom sky dome is.",
                0.0,
                1.0,
                0.05,
                0.55,
            ),
            Setting::bool(
                "tint_fog",
                "Tint Fog",
                "Replace atmospheric fog colour with the Visuals colour source.",
                false,
            )
            .group("Fog"),
            Setting::float(
                "fog_density",
                "Fog Density",
                "1.0 is vanilla. Lower clears the view; higher pulls fog closer.",
                0.05,
                2.5,
                0.05,
                1.0,
            )
            .suffix("x"),
            Setting::bool(
                "fog_breathing",
                "Fog Breathing",
                "Slowly pulse fog distance in and out.",
                false,
            ),
            Setting::float(
                "fog_breath_amount",
                "Breath Amount",
                "How far fog distance swings while breathing.",
                0.0,
                0.6,
                0.05,
                0.2,
            ),
            Setting::bool(
                "clear_water",
                "Clear Water Fog",
                "Push water fog out so underwater looks clearer.",
                false,
            ),
            Setting::bool(
                "clear_lava",
                "Clear Lava Fog",
                "Push lava fog out.",
                false,
            ),
            Setting::float(
                "night_boost",
                "Night Boost",
                "Extra sky/fog brightness at night (0 = none).",
                0.0,
                1.0,
                0.05,
                0.0,
            )
            .group("Grading"),
        ],
    }
}

pub fn build_world_module() -> Module {
    Module {
        id: ModuleId::VisWorld,
        key: "vis_world",
        name: "World",
        description: "Biome tint overrides, glow rims and coloured lights.",
        category: Category::Visuals,
        enabled: true,
        keybind: None,
        expanded: true,
        show_in_arraylist: false,
        settings: vec![
            Setting::bool(
                "tint_biomes",
                "Tint Biomes",
                "Override grass, leaf and water colours. Rebuilds terrain when changed.",
                false,
            )
            .group("Biome Colours"),
            Setting::bool(
                "rainbow_biomes",
                "Rainbow Biomes",
                "Cycle biome tints through the spectrum (varies per biome).",
                false,
            ),
            Setting::color(
                "grass",
                "Grass",
                "Grass tint when Rainbow Biomes is off (0xRRGGBB).",
                0xFF55_C96A,
            ),
            Setting::color(
                "foliage",
                "Foliage",
                "Leaf / vine tint when Rainbow Biomes is off.",
                0xFF3D_A85A,
            ),
            Setting::color(
                "water",
                "Water",
                "Water tint when Rainbow Biomes is off.",
                0xFF3A_8CFF,
            ),
            Setting::float(
                "saturation",
                "Saturation",
                "How vivid biome overrides are.",
                0.0,
                2.0,
                0.05,
                1.0,
            )
            .suffix("x"),
            Setting::bool(
                "glow_rims",
                "Glow Rims",
                "Draw coloured outlines around nearby solid blocks under your feet / target.",
                false,
            )
            .group("Glow & Light"),
            Setting::float(
                "rim_width",
                "Rim Width",
                "Outline thickness in pixels.",
                0.5,
                4.0,
                0.5,
                1.5,
            )
            .suffix("px"),
            Setting::bool(
                "player_light",
                "Player Light",
                "Add a coloured light around you.",
                false,
            ),
            Setting::float(
                "light_radius",
                "Light Radius",
                "How far the coloured light reaches.",
                2.0,
                16.0,
                0.5,
                6.0,
            )
            .suffix("m"),
            Setting::float(
                "light_intensity",
                "Light Intensity",
                "Brightness of the coloured light.",
                0.05,
                1.0,
                0.05,
                0.45,
            ),
        ],
    }
}

pub fn build_camera_module() -> Module {
    Module {
        id: ModuleId::VisCamera,
        key: "vis_camera",
        name: "Camera FX",
        description: "FOV offset, idle sway, roll drift and hit punch.",
        category: Category::Visuals,
        enabled: true,
        keybind: None,
        expanded: true,
        show_in_arraylist: false,
        settings: vec![
            Setting::float(
                "fov_offset",
                "FOV Offset",
                "Degrees added to the world FOV.",
                -20.0,
                30.0,
                0.5,
                0.0,
            )
            .suffix("°")
            .group("Field of View"),
            Setting::bool(
                "speed_fov",
                "Speed FOV",
                "Widen FOV slightly when moving fast.",
                false,
            ),
            Setting::float(
                "speed_fov_amount",
                "Speed FOV Amount",
                "Extra degrees at full sprint speed.",
                0.0,
                12.0,
                0.5,
                4.0,
            )
            .suffix("°"),
            Setting::float(
                "sway",
                "Idle Sway",
                "Gentle camera bob while standing still.",
                0.0,
                4.0,
                0.05,
                0.0,
            )
            .group("Motion"),
            Setting::float(
                "roll",
                "Roll Amount",
                "Degrees of camera roll drift.",
                0.0,
                28.0,
                0.5,
                0.0,
            )
            .suffix("°"),
            Setting::float(
                "hit_punch",
                "Hit Punch",
                "Camera kick when you land a hit.",
                0.0,
                4.0,
                0.1,
                0.0,
            ),
        ],
    }
}

pub fn build_screen_module() -> Module {
    Module {
        id: ModuleId::VisScreen,
        key: "vis_screen",
        name: "Screen",
        description: "Vignette, colour wash, scanlines, edge glow and hit flash overlays.",
        category: Category::Visuals,
        enabled: true,
        keybind: None,
        expanded: true,
        show_in_arraylist: false,
        settings: vec![
            Setting::float(
                "vignette",
                "Vignette",
                "Dark / coloured edge falloff.",
                0.0,
                1.0,
                0.05,
                0.0,
            )
            .group("Overlays"),
            Setting::bool(
                "vignette_pulse",
                "Vignette Pulse",
                "Breathe the vignette in and out.",
                false,
            ),
            Setting::float(
                "wash",
                "Colour Wash",
                "Full-screen tint strength.",
                0.0,
                0.75,
                0.05,
                0.0,
            ),
            Setting::float(
                "scanlines",
                "Scanlines",
                "Horizontal line shimmer.",
                0.0,
                0.4,
                0.05,
                0.0,
            ),
            Setting::float(
                "edge_glow",
                "Edge Glow",
                "Accent glow along the screen edges.",
                0.0,
                0.8,
                0.05,
                0.0,
            ),
            Setting::bool(
                "hit_flash",
                "Hit Flash",
                "Brief edge flash when you land a hit.",
                false,
            )
            .group("Feedback"),
            Setting::bool(
                "low_hp_pulse",
                "Low HP Pulse",
                "Pulse a red vignette when your health is low.",
                false,
            ),
            Setting::float(
                "chroma_bars",
                "Chroma Bars",
                "Fake chromatic aberration bars on the left / right edges.",
                0.0,
                0.85,
                0.05,
                0.0,
            ),
        ],
    }
}

pub fn build_hit_module() -> Module {
    Module {
        id: ModuleId::VisHit,
        key: "vis_hit",
        name: "Hit FX",
        description: "Particle bursts, fireworks and feedback when you hit something.",
        category: Category::Visuals,
        enabled: true,
        keybind: None,
        expanded: true,
        show_in_arraylist: false,
        settings: vec![
            Setting::bool(
                "enabled_fx",
                "Enable Hit FX",
                "Spawn particles and feedback when damage lands.",
                false,
            )
            .group("Hit Effects"),
            Setting::mode(
                "style",
                "Style",
                "Particle look when you land a hit.",
                HIT_STYLE_OPTIONS,
                2,
            ),
            Setting::int(
                "count",
                "Count",
                "How many particles per hit.",
                2,
                48,
                1,
                16,
            ),
            Setting::float(
                "speed",
                "Speed",
                "Particle velocity multiplier.",
                0.2,
                2.5,
                0.1,
                1.0,
            )
            .suffix("x"),
            Setting::float(
                "spread",
                "Spread",
                "How wide the burst fans out.",
                0.1,
                2.0,
                0.1,
                0.8,
            ),
            Setting::float(
                "size",
                "Size",
                "Dust / spark scale.",
                0.3,
                3.0,
                0.1,
                1.1,
            )
            .suffix("x"),
            Setting::bool(
                "crit_boost",
                "Crit Boost",
                "Bigger burst and fireworks on critical-looking hits.",
                true,
            )
            .group("Extras"),
            Setting::bool(
                "kill_burst",
                "Kill Burst",
                "Extra Nova / fireworks when a target dies.",
                true,
            ),
            Setting::bool(
                "ding",
                "Hit Ding",
                "Play a short local sound on hit.",
                false,
            ),
            Setting::bool(
                "only_mine",
                "Only My Hits",
                "Only react when you were the last attacker.",
                true,
            ),
            Setting::float(
                "range",
                "Range",
                "How far away hits are still considered.",
                4.0,
                48.0,
                1.0,
                12.0,
            )
            .suffix("m"),
        ],
    }
}

pub fn build_ambient_module() -> Module {
    Module {
        id: ModuleId::VisAmbient,
        key: "vis_ambient",
        name: "Ambient",
        description: "Floating ambient particles around you.",
        category: Category::Visuals,
        enabled: true,
        keybind: None,
        expanded: true,
        show_in_arraylist: false,
        settings: vec![
            Setting::mode(
                "style",
                "Style",
                "Ambient particle look.",
                AMBIENT_STYLE_OPTIONS,
                0,
            )
            .group("Ambient Particles"),
            Setting::float(
                "density",
                "Density",
                "How many ambient particles spawn per second.",
                0.0,
                36.0,
                0.5,
                4.0,
            )
            .suffix("/s"),
            Setting::float(
                "radius",
                "Radius",
                "Sphere around you where particles appear.",
                2.0,
                24.0,
                0.5,
                8.0,
            )
            .suffix("m"),
        ],
    }
}

pub fn build_trip_module() -> Module {
    Module {
        id: ModuleId::VisTrip,
        key: "vis_trip",
        name: "Trip",
        description: "LSD-style effects scaled by Dose, with onset and wave timing.",
        category: Category::Visuals,
        enabled: true,
        keybind: None,
        expanded: true,
        show_in_arraylist: false,
        settings: vec![
            Setting::float(
                "onset",
                "Onset",
                "Seconds for trip effects to fade in after Dose rises.",
                0.0,
                30.0,
                0.5,
                8.0,
            )
            .suffix("s")
            .group("Timing"),
            Setting::float(
                "wave_period",
                "Wave Period",
                "Seconds per slow breathe / hue wave.",
                2.0,
                40.0,
                0.5,
                12.0,
            )
            .suffix("s"),
            Setting::bool(
                "fov_breathe",
                "FOV Breathing",
                "Slow FOV pulse plus a faint heartbeat.",
                true,
            )
            .group("Effects"),
            Setting::bool(
                "roll_drift",
                "Roll Drift",
                "Gentle rolling of the camera.",
                true,
            ),
            Setting::bool(
                "rainbow_sky",
                "Rainbow Sky",
                "Force the psychedelic sky and cycling fog.",
                true,
            ),
            Setting::bool(
                "rainbow_biomes",
                "Rainbow Biomes",
                "Force rainbow grass / leaves / water while tripping.",
                true,
            ),
            Setting::bool(
                "colour_wash",
                "Colour Enhancement",
                "Soft vivid tint that slowly shifts — not a neon overlay.",
                true,
            ),
            Setting::bool(
                "morph",
                "Breathing / Melting",
                "Slow FOV breathe, gentle roll, and drifting depth — like the world melting.",
                true,
            ),
            Setting::bool(
                "kaleidoscope",
                "Afterimages / Tracers",
                "Soft delayed colour ghosts and drifting peripheral clouds.",
                true,
            ),
            Setting::bool(
                "particles",
                "Visual Snow",
                "Sparse drifting sparkles in the air.",
                true,
            ),
            Setting::bool(
                "audio_warp",
                "Audio Warp",
                "Periodic low-pass muffle that wobbles with the wave.",
                false,
            ),
            Setting::bool(
                "shader_pack",
                "Shader Pack Params",
                "Write Dose / warp / chroma into frame.mods for a native-star shader pack.",
                true,
            ),
            Setting::float(
                "strength",
                "Effect Strength",
                "Extra multiplier on top of Dose for trip-only effects.",
                0.25,
                2.5,
                0.05,
                1.0,
            )
            .suffix("x")
            .group("Tuning"),
        ],
    }
}

/// Every Visuals-tab module, in registration order.
pub fn all_modules() -> [Module; 8] {
    [
        build_preset_module(),
        build_sky_module(),
        build_world_module(),
        build_camera_module(),
        build_screen_module(),
        build_hit_module(),
        build_ambient_module(),
        build_trip_module(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn every_visuals_module_has_unique_setting_ids() {
        for m in all_modules() {
            let mut ids = HashSet::new();
            for s in &m.settings {
                assert!(ids.insert(s.id), "{} duplicate {}", m.key, s.id);
            }
            assert_eq!(m.category, Category::Visuals);
            assert!(!m.show_in_arraylist);
        }
    }
}
