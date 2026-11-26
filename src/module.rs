//! Categories, module definitions, and the central `ModuleManager`.

use std::collections::{BTreeMap, HashSet};

use crate::entity_filter::EntityFilter;
use crate::font::{DEFAULT_FONT_INDEX, FONT_PRESETS};
use crate::modules::{aim_assist, extras, fast_mine, lock_visuals, movement, vis_settings, visuals};
use crate::settings::Setting;
use crate::theme::THEME_PRESETS;

/// Top-level categories. `MODULE_PANELS` are shown as draggable columns in the
/// `Modules` tab; `Client` modules (`ClickGUI` and `HUD`) live in the top bar tabs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Category {
    Combat,
    Player,
    Movement,
    Render,
    Client,
    Visuals,
}

impl Category {
    pub const ALL: [Category; 6] = [
        Category::Combat,
        Category::Player,
        Category::Movement,
        Category::Render,
        Category::Client,
        Category::Visuals,
    ];

    /// Categories displayed as draggable panels on the main `Modules` tab.
    pub const MODULE_PANELS: [Category; 4] = [
        Category::Combat,
        Category::Player,
        Category::Movement,
        Category::Render,
    ];

    pub fn index(self) -> usize {
        match self {
            Category::Combat => 0,
            Category::Player => 1,
            Category::Movement => 2,
            Category::Render => 3,
            Category::Client => 4,
            Category::Visuals => 5,
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            Category::Combat => "combat",
            Category::Player => "player",
            Category::Movement => "movement",
            Category::Render => "render",
            Category::Client => "client",
            Category::Visuals => "visuals",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Category::Combat => "Combat",
            Category::Player => "Player",
            Category::Movement => "Movement",
            Category::Render => "Render",
            Category::Client => "Client",
            Category::Visuals => "Visuals",
        }
    }

    pub fn badge(self) -> &'static str {
        match self {
            Category::Combat => "C",
            Category::Player => "P",
            Category::Movement => "M",
            Category::Render => "R",
            Category::Client => "S",
            Category::Visuals => "V",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|c| c.id().eq_ignore_ascii_case(id))
    }
}

/// Unique compile-time identifier for every registered module.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ModuleId {
    AimAssist,
    LockVisuals,
    FastMine,
    Esp,
    Tracers,
    Nametags,
    NoFog,
    Zoom,
    Camera,
    ItemEsp,
    StorageEsp,
    Trajectories,
    Breadcrumbs,
    DamageIndicators,
    Radar,
    Sprint,
    ToggleSneak,
    AutoWalk,
    VisPreset,
    VisSky,
    VisWorld,
    VisCamera,
    VisScreen,
    VisHit,
    VisAmbient,
    VisTrip,
    Hud,
    ClickGui,
}

impl ModuleId {
    /// Every module id, in registration order.
    pub const ALL: [ModuleId; 28] = [
        ModuleId::AimAssist,
        ModuleId::LockVisuals,
        ModuleId::FastMine,
        ModuleId::Esp,
        ModuleId::Tracers,
        ModuleId::Nametags,
        ModuleId::NoFog,
        ModuleId::Zoom,
        ModuleId::Camera,
        ModuleId::ItemEsp,
        ModuleId::StorageEsp,
        ModuleId::Trajectories,
        ModuleId::Breadcrumbs,
        ModuleId::DamageIndicators,
        ModuleId::Radar,
        ModuleId::Sprint,
        ModuleId::ToggleSneak,
        ModuleId::AutoWalk,
        ModuleId::VisPreset,
        ModuleId::VisSky,
        ModuleId::VisWorld,
        ModuleId::VisCamera,
        ModuleId::VisScreen,
        ModuleId::VisHit,
        ModuleId::VisAmbient,
        ModuleId::VisTrip,
        ModuleId::Hud,
        ModuleId::ClickGui,
    ];
}

/// How a module's keybind drives its enabled state.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum BindMode {
    /// Each press flips the module on or off.
    #[default]
    Toggle,
    /// The module is on only while the key is held.
    Hold,
}

impl BindMode {
    pub fn id(self) -> &'static str {
        match self {
            BindMode::Toggle => "toggle",
            BindMode::Hold => "hold",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            BindMode::Toggle => "Toggle",
            BindMode::Hold => "Hold",
        }
    }

    pub fn from_id(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "toggle" => Some(BindMode::Toggle),
            "hold" => Some(BindMode::Hold),
            _ => None,
        }
    }

    pub fn flipped(self) -> Self {
        match self {
            BindMode::Toggle => BindMode::Hold,
            BindMode::Hold => BindMode::Toggle,
        }
    }
}

/// One toggleable feature/module displayed in its category window or top tab.
#[derive(Clone, Debug)]
pub struct Module {
    pub id: ModuleId,
    pub key: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub category: Category,
    pub enabled: bool,
    pub keybind: Option<String>,
    pub expanded: bool,
    /// When false, the module is a configuration-only module (like ClickGUI/HUD) and does not show in the HUD ArrayList.
    pub show_in_arraylist: bool,
    pub settings: Vec<Setting>,
}

impl Module {
    pub fn setting(&self, id: &str) -> Option<&Setting> {
        self.settings.iter().find(|s| s.id == id)
    }

    pub fn setting_mut(&mut self, id: &str) -> Option<&mut Setting> {
        self.settings.iter_mut().find(|s| s.id == id)
    }

    pub fn bool(&self, id: &str, default: bool) -> bool {
        self.setting(id).and_then(Setting::as_bool).unwrap_or(default)
    }

    pub fn float(&self, id: &str, default: f64) -> f64 {
        self.setting(id).and_then(Setting::as_float).unwrap_or(default)
    }

    pub fn int(&self, id: &str, default: i64) -> i64 {
        self.setting(id).and_then(Setting::as_int).unwrap_or(default)
    }

    pub fn mode(&self, id: &str, default: usize) -> usize {
        self.setting(id).and_then(Setting::as_mode).unwrap_or(default)
    }

    pub fn mode_str(&self, id: &str, default: &'static str) -> &'static str {
        self.setting(id)
            .and_then(Setting::as_mode_str)
            .unwrap_or(default)
    }

    pub fn color(&self, id: &str, default: u32) -> u32 {
        self.setting(id).and_then(Setting::as_color).unwrap_or(default)
    }

    pub fn keybind_setting(&self, id: &str) -> Option<&str> {
        self.setting(id).and_then(Setting::as_keybind).flatten()
    }

    pub fn entity_filter(&self, id: &str) -> Option<&EntityFilter> {
        self.setting(id).and_then(Setting::as_entity_filter)
    }

    /// Short live info tag rendered beside the module name in the HUD ArrayList (Meteor style).
    pub fn hud_tag(&self) -> Option<String> {
        match self.id {
            ModuleId::AimAssist => {
                let prio = self.mode_str("priority", "Closest");
                let range = self.float("range", 4.5);
                Some(format!("{prio} {range:.1}"))
            }
            ModuleId::FastMine => {
                let mult = self.float("multiplier", 1.4);
                Some(format!("{mult:.2}x"))
            }
            ModuleId::Esp => Some(self.mode_str("style", "3D Fill + Box").to_owned()),
            ModuleId::Tracers => Some(self.mode_str("target_point", "Center").to_owned()),
            ModuleId::Nametags => Some(format!("{:.0}m", self.float("range", 64.0))),
            ModuleId::Zoom => Some(format!("{:.1}x", self.float("factor", 4.0))),
            ModuleId::ItemEsp => Some(format!("{:.0}m", self.float("range", 48.0))),
            ModuleId::StorageEsp => Some(format!("{:.0}m", self.float("range", 48.0))),
            ModuleId::Trajectories => Some(self.mode_str("style", "Line").to_owned()),
            ModuleId::Breadcrumbs => Some(self.mode_str("source", "Self").to_owned()),
            ModuleId::DamageIndicators => Some(format!("{:.0}m", self.float("range", 32.0))),
            ModuleId::Radar => Some(format!("{:.0}m", self.float("range", 48.0))),
            ModuleId::Sprint => Some(self.mode_str("mode", "Forward").to_owned()),
            ModuleId::LockVisuals
            | ModuleId::NoFog
            | ModuleId::Camera
            | ModuleId::ToggleSneak
            | ModuleId::AutoWalk
            | ModuleId::VisPreset
            | ModuleId::VisSky
            | ModuleId::VisWorld
            | ModuleId::VisCamera
            | ModuleId::VisScreen
            | ModuleId::VisHit
            | ModuleId::VisAmbient
            | ModuleId::VisTrip
            | ModuleId::Hud
            | ModuleId::ClickGui => None,
        }
    }
}

/// Persisted position and collapse state for a single category panel.
#[derive(Clone, Copy, Debug)]
pub struct PanelState {
    pub x: f32,
    pub y: f32,
    pub collapsed: bool,
    pub scroll: f32,
}

/// A named snapshot of modules, keybinds, settings, and panel layout.
#[derive(Clone, Debug)]
pub struct SavedProfile {
    pub name: String,
    pub body: String,
}

/// Central registry of all modules, panel layouts, profiles, and config dirty state.
pub struct ModuleManager {
    pub modules: Vec<Module>,
    pub panels: [PanelState; 6],
    pub config_loaded: bool,
    pub dirty: bool,
    /// Profile last loaded or saved. Empty when the live setup doesn't match a snapshot.
    pub active_profile: String,
    pub profiles: Vec<SavedProfile>,
    pub profiles_dirty: bool,
    /// Modules whose keybind is held rather than toggled.
    pub hold_binds: HashSet<ModuleId>,
    /// Modules pinned to the top of their panel.
    pub favorites: HashSet<ModuleId>,
    /// Dragged HUD widget positions (top-left, GUI pixels), keyed by widget id.
    pub hud_positions: BTreeMap<String, [f32; 2]>,
    /// Millisecond timestamp of the last successful config write (for save throttling).
    pub last_save_ms: u64,
    /// Whether the previous session's config has been copied to the backup file this session.
    pub backup_written: bool,
    /// Inert module returned when a lookup misses, so hooks never panic.
    fallback: Module,
}

impl Default for ModuleManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ModuleManager {
    pub fn new() -> Self {
        let modules = vec![
            aim_assist::build_module(),
            lock_visuals::build_module(),
            fast_mine::build_module(),
            visuals::build_esp_module(),
            visuals::build_tracers_module(),
            visuals::build_nametags_module(),
            visuals::build_no_fog_module(),
            visuals::build_zoom_module(),
            visuals::build_camera_module(),
            extras::build_item_esp_module(),
            extras::build_storage_esp_module(),
            extras::build_trajectories_module(),
            extras::build_breadcrumbs_module(),
            extras::build_damage_indicators_module(),
            extras::build_radar_module(),
            movement::build_sprint_module(),
            movement::build_toggle_sneak_module(),
            movement::build_auto_walk_module(),
            vis_settings::build_preset_module(),
            vis_settings::build_sky_module(),
            vis_settings::build_world_module(),
            vis_settings::build_camera_module(),
            vis_settings::build_screen_module(),
            vis_settings::build_hit_module(),
            vis_settings::build_ambient_module(),
            vis_settings::build_trip_module(),
            build_hud_module(),
            build_click_gui_module(),
        ];
        Self {
            modules,
            panels: default_panels(154.0),
            config_loaded: false,
            dirty: false,
            active_profile: String::new(),
            profiles: Vec::new(),
            profiles_dirty: false,
            hold_binds: HashSet::new(),
            favorites: HashSet::new(),
            hud_positions: BTreeMap::new(),
            last_save_ms: 0,
            backup_written: false,
            fallback: Module {
                id: ModuleId::ClickGui,
                key: "__fallback",
                name: "Unavailable",
                description: "",
                category: Category::Client,
                enabled: false,
                keybind: None,
                expanded: false,
                show_in_arraylist: false,
                settings: Vec::new(),
            },
        }
    }

    pub fn bind_mode(&self, id: ModuleId) -> BindMode {
        if self.hold_binds.contains(&id) {
            BindMode::Hold
        } else {
            BindMode::Toggle
        }
    }

    pub fn set_bind_mode(&mut self, id: ModuleId, mode: BindMode) {
        let changed = match mode {
            BindMode::Hold => self.hold_binds.insert(id),
            BindMode::Toggle => self.hold_binds.remove(&id),
        };
        if changed {
            self.dirty = true;
        }
    }

    pub fn is_favorite(&self, id: ModuleId) -> bool {
        self.favorites.contains(&id)
    }

    pub fn toggle_favorite(&mut self, id: ModuleId) {
        if !self.favorites.remove(&id) {
            self.favorites.insert(id);
        }
        self.dirty = true;
    }

    /// Restores every setting of one module to its default. Returns true if anything changed.
    pub fn reset_module(&mut self, id: ModuleId) -> bool {
        let Some(m) = self.modules.iter_mut().find(|m| m.id == id) else {
            return false;
        };
        let mut changed = false;
        for setting in &mut m.settings {
            // Keybinds belong to the user, not to the module's tuning.
            if setting.as_keybind().is_some() {
                continue;
            }
            changed |= setting.reset();
        }
        if changed {
            self.dirty = true;
        }
        changed
    }

    pub fn reset_panel_layout(&mut self, screen_width: f32) {
        let panel_w = self.get(ModuleId::ClickGui).int("panel_width", 154) as f32;
        let gap = 12.0;
        let count = Category::MODULE_PANELS.len();
        let total = count as f32 * panel_w + (count - 1) as f32 * gap;
        let start_x = ((screen_width - total) * 0.5).max(12.0);
        for (i, cat) in Category::MODULE_PANELS.into_iter().enumerate() {
            let panel = &mut self.panels[cat.index()];
            panel.x = start_x + i as f32 * (panel_w + gap);
            panel.y = 38.0;
            panel.collapsed = false;
            panel.scroll = 0.0;
        }
        self.dirty = true;
    }

    /// Looks a module up by id. Every `ModuleId` is registered in `new()` (covered by a
    /// unit test); a miss yields an inert, disabled module instead of panicking in a hook.
    pub fn get(&self, id: ModuleId) -> &Module {
        self.modules
            .iter()
            .find(|m| m.id == id)
            .unwrap_or(&self.fallback)
    }

    pub fn index_of(&self, id: ModuleId) -> usize {
        self.modules.iter().position(|m| m.id == id).unwrap_or(0)
    }

    #[allow(dead_code)]
    pub fn get_mut(&mut self, id: ModuleId) -> &mut Module {
        match self.modules.iter().position(|m| m.id == id) {
            Some(i) => &mut self.modules[i],
            None => &mut self.fallback,
        }
    }

    pub fn by_key_mut(&mut self, key: &str) -> Option<&mut Module> {
        self.modules.iter_mut().find(|m| m.key == key)
    }

    #[allow(dead_code)]
    pub fn is_enabled(&self, id: ModuleId) -> bool {
        self.get(id).enabled
    }

    #[allow(dead_code)]
    pub fn toggle(&mut self, id: ModuleId) -> bool {
        let m = self.get_mut(id);
        m.enabled = !m.enabled;
        let now = m.enabled;
        self.dirty = true;
        now
    }

    /// Handles a key press: toggle-bound modules flip, hold-bound modules switch on.
    /// Returns how many modules changed state.
    pub fn trigger_keybind(&mut self, key: &str) -> usize {
        let mut count = 0;
        for m in &mut self.modules {
            if m.id == ModuleId::ClickGui || m.keybind.as_deref() != Some(key) {
                continue;
            }
            if self.hold_binds.contains(&m.id) {
                if !m.enabled {
                    m.enabled = true;
                    count += 1;
                }
            } else {
                m.enabled = !m.enabled;
                count += 1;
            }
        }
        if count > 0 {
            self.dirty = true;
        }
        count
    }

    /// Handles a key release: hold-bound modules switch off. Returns how many changed.
    pub fn release_keybind(&mut self, key: &str) -> usize {
        let mut count = 0;
        for m in &mut self.modules {
            if m.id == ModuleId::ClickGui
                || !self.hold_binds.contains(&m.id)
                || m.keybind.as_deref() != Some(key)
            {
                continue;
            }
            if m.enabled {
                m.enabled = false;
                count += 1;
            }
        }
        if count > 0 {
            self.dirty = true;
        }
        count
    }

    /// Module indices in `category` matching `filter`, favorites first.
    pub fn sorted_indices_in_category(&self, category: Category, filter: &str) -> Vec<usize> {
        let mut out = self.indices_in_category(category, filter);
        out.sort_by_key(|&i| !self.favorites.contains(&self.modules[i].id));
        out
    }

    /// Configured ClickGUI open/close keybind (defaults to `key.keyboard.right.shift`).
    pub fn gui_key(&self) -> Option<&str> {
        self.get(ModuleId::ClickGui).keybind_setting("gui_key")
    }

    pub fn indices_in_category(&self, category: Category, filter: &str) -> Vec<usize> {
        let q = filter.trim().to_ascii_lowercase();
        self.modules
            .iter()
            .enumerate()
            .filter(|(_, m)| {
                if m.category != category {
                    return false;
                }
                if q.is_empty() {
                    return true;
                }
                m.name.to_ascii_lowercase().contains(&q)
                    || m.description.to_ascii_lowercase().contains(&q)
                    || m.settings
                        .iter()
                        .any(|s| s.name.to_ascii_lowercase().contains(&q))
            })
            .map(|(i, _)| i)
            .collect()
    }
}

fn default_panels(panel_width: f32) -> [PanelState; 6] {
    let gap = 12.0;
    let start_x = 18.0;
    let mut out = [PanelState {
        x: start_x,
        y: 38.0,
        collapsed: false,
        scroll: 0.0,
    }; 6];
    for (i, p) in out.iter_mut().enumerate() {
        p.x = start_x + i as f32 * (panel_width + gap);
    }
    out
}

fn build_click_gui_module() -> Module {
    Module {
        id: ModuleId::ClickGui,
        key: "click_gui",
        name: "Client Settings",
        description: "Customize Star's theme, font, typography, colors, layout, blur, and keybind.",
        category: Category::Client,
        enabled: true,
        keybind: None,
        expanded: true,
        show_in_arraylist: false,
        settings: vec![
            Setting::keybind(
                "gui_key",
                "Open Key",
                "Key used to open and close the ClickGUI while playing.",
                Some("key.keyboard.right.shift"),
            )
            .group("Controls"),
            Setting::mode(
                "font_face",
                "Font",
                "Font style used across Star's ClickGUI, modals, HUD, and Nametags.",
                FONT_PRESETS,
                DEFAULT_FONT_INDEX,
            )
            .group("Font & Typography"),
            Setting::float(
                "font_scale",
                "Font Scale",
                "Global text size multiplier (1.00x is native 1:1 sharpness).",
                0.70,
                1.25,
                0.05,
                1.00,
            )
            .suffix("x"),
            Setting::bool(
                "font_shadow",
                "Text Shadow",
                "Render drop shadows behind UI and HUD text.",
                false,
            ),
            Setting::bool(
                "bold_headers",
                "Bold Headers",
                "Use bold weight on panel titles, tabs, and section headers.",
                true,
            ),
            Setting::bool(
                "italic_values",
                "Italic Values",
                "Use italic style for numeric and mode values.",
                false,
            ),
            Setting::mode(
                "preset",
                "Theme",
                "Select a built-in color theme or switch to Custom to use the pickers below.",
                THEME_PRESETS,
                0,
            )
            .group("Appearance"),
            Setting::bool(
                "rainbow",
                "Rainbow Accent",
                "Smoothly cycle the accent color across the spectrum.",
                false,
            ),
            Setting::bool(
                "blur",
                "Menu Blur",
                "Apply Horizon's translucent menu blur behind the ClickGUI.",
                true,
            ),
            Setting::float(
                "backdrop_dim",
                "Backdrop Dim",
                "Darkness of the tint behind the ClickGUI panels.",
                0.0,
                0.85,
                0.05,
                0.45,
            ),
            Setting::float(
                "panel_opacity",
                "Panel Opacity",
                "Opacity of the category window backgrounds.",
                0.40,
                1.0,
                0.05,
                0.90,
            ),
            Setting::int(
                "panel_width",
                "Panel Width",
                "Width of each category window in GUI pixels.",
                124,
                220,
                4,
                154,
            )
            .suffix("px")
            .group("Layout"),
            Setting::int(
                "row_height",
                "Row Height",
                "Height of each module row in GUI pixels.",
                14,
                22,
                1,
                17,
            )
            .suffix("px"),
            Setting::bool(
                "descriptions",
                "Tooltips",
                "Show description tooltips when hovering modules and settings.",
                true,
            ),
            Setting::bool(
                "show_keybinds",
                "Show Binds",
                "Display assigned keybind badges next to modules.",
                true,
            ),
            Setting::color(
                "accent_color",
                "Accent Color",
                "Primary highlight color (used when Theme is set to Custom).",
                0xFF91_3DE8,
            )
            .group("Custom Colors"),
            Setting::color(
                "panel_color",
                "Panel Color",
                "Panel background color (used when Theme is set to Custom).",
                0xE614_141C,
            ),
            Setting::color(
                "header_color",
                "Header Color",
                "Category header bar color (used when Theme is set to Custom).",
                0xF51D_1A29,
            ),
            Setting::color(
                "text_color",
                "Text Color",
                "Primary label color (used when Theme is set to Custom).",
                0xFFF2_F2F7,
            ),
        ],
    }
}

fn build_hud_module() -> Module {
    Module {
        id: ModuleId::Hud,
        key: "hud",
        name: "HUD",
        description: "In-game watermark, coordinates, active module ArrayList, and Aim Assist overlays.",
        category: Category::Client,
        enabled: true,
        keybind: None,
        expanded: true,
        show_in_arraylist: false,
        settings: vec![
            Setting::bool(
                "watermark",
                "Watermark",
                "Show the Star title badge in the top-left corner.",
                true,
            )
            .group("Elements"),
            Setting::bool(
                "coords",
                "Coordinates",
                "Display live X / Y / Z coordinates in the bottom-left corner.",
                true,
            ),
            Setting::bool(
                "arraylist",
                "Active Modules",
                "List currently enabled modules along the selected screen corner.",
                true,
            ),
            Setting::bool(
                "show_tags",
                "Module Tags",
                "Show live mode/range metadata beside each active module.",
                true,
            ),
            Setting::mode(
                "corner",
                "List Corner",
                "Corner of the screen where the active modules list is anchored.",
                &["Top Right", "Top Left", "Bottom Right", "Bottom Left"],
                0,
            )
            .group("ArrayList"),
            Setting::mode(
                "sort",
                "Sort Order",
                "Order active modules by text width or alphabetically.",
                &["Width", "A-Z"],
                0,
            ),
            Setting::float(
                "bg_opacity",
                "HUD Opacity",
                "Background opacity for HUD boxes.",
                0.0,
                0.95,
                0.05,
                0.60,
            ),
            Setting::bool(
                "accent_bar",
                "Side Accent Bar",
                "Draw a colored accent bar along the edge of each active module.",
                true,
            ),
            Setting::bool(
                "show_fps",
                "FPS",
                "Show the current frame rate in the info panel.",
                false,
            )
            .group("Info Panel (drag to move)"),
            Setting::bool(
                "show_ping",
                "Ping",
                "Show your latency to the server in the info panel.",
                false,
            ),
            Setting::bool(
                "show_speed",
                "Speed",
                "Show your horizontal speed in blocks per second.",
                false,
            ),
            Setting::bool(
                "show_armor",
                "Armor",
                "Show the average remaining durability of your worn armor.",
                false,
            ),
            Setting::bool(
                "show_compass",
                "Compass",
                "Show the compass direction and heading you are facing.",
                false,
            ),
            Setting::bool(
                "show_session",
                "Session Timer",
                "Show how long you have been playing this session.",
                false,
            ),
            Setting::bool(
                "keystrokes",
                "Keystrokes",
                "Show WASD, jump and mouse buttons with live clicks per second.",
                false,
            )
            .group("Widgets (drag to move)"),
            Setting::bool(
                "toasts",
                "Notifications",
                "Show brief messages when modules toggle, profiles load, or errors are recovered.",
                true,
            )
            .group("Notifications"),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_module_id_is_registered_exactly_once() {
        let mgr = ModuleManager::new();
        for id in ModuleId::ALL {
            let count = mgr.modules.iter().filter(|m| m.id == id).count();
            assert_eq!(count, 1, "{id:?} registered {count} times");
        }
        assert_eq!(mgr.modules.len(), ModuleId::ALL.len());
    }

    #[test]
    fn module_keys_and_setting_ids_are_unique() {
        let mgr = ModuleManager::new();
        let mut keys = HashSet::new();
        for m in &mgr.modules {
            assert!(keys.insert(m.key), "duplicate module key {}", m.key);
            let mut ids = HashSet::new();
            for s in &m.settings {
                assert!(ids.insert(s.id), "{}: duplicate setting id {}", m.key, s.id);
            }
        }
    }

    #[test]
    fn lookups_never_panic() {
        let mut mgr = ModuleManager::new();
        mgr.modules.clear();
        assert!(!mgr.get(ModuleId::Radar).enabled);
        assert!(!mgr.is_enabled(ModuleId::Sprint));
        // A miss on the mutable path hands back the inert fallback instead of panicking.
        let _ = mgr.get_mut(ModuleId::Sprint);
        assert!(mgr.by_key_mut("sprint").is_none());
        assert!(!mgr.reset_module(ModuleId::Radar));
    }

    #[test]
    fn toggle_bind_flips_and_hold_bind_follows_the_key() {
        let mut mgr = ModuleManager::new();
        mgr.get_mut(ModuleId::Radar).keybind = Some("key.keyboard.g".to_owned());
        mgr.get_mut(ModuleId::Sprint).keybind = Some("key.keyboard.g".to_owned());
        mgr.set_bind_mode(ModuleId::Sprint, BindMode::Hold);

        assert_eq!(mgr.trigger_keybind("key.keyboard.g"), 2);
        assert!(mgr.is_enabled(ModuleId::Radar) && mgr.is_enabled(ModuleId::Sprint));
        // Key repeat / double press: hold stays on, toggle flips back.
        assert_eq!(mgr.trigger_keybind("key.keyboard.g"), 1);
        assert!(!mgr.is_enabled(ModuleId::Radar) && mgr.is_enabled(ModuleId::Sprint));

        assert_eq!(mgr.release_keybind("key.keyboard.g"), 1);
        assert!(!mgr.is_enabled(ModuleId::Sprint));
        // Releasing never touches toggle-bound modules.
        mgr.trigger_keybind("key.keyboard.g");
        assert!(mgr.is_enabled(ModuleId::Radar));
        mgr.release_keybind("key.keyboard.g");
        assert!(mgr.is_enabled(ModuleId::Radar));
        assert_eq!(mgr.trigger_keybind("key.keyboard.unbound"), 0);
    }

    #[test]
    fn bind_mode_helpers_round_trip() {
        assert_eq!(BindMode::from_id(BindMode::Hold.id()), Some(BindMode::Hold));
        assert_eq!(BindMode::from_id("bogus"), None);
        assert_eq!(BindMode::Toggle.flipped(), BindMode::Hold);
        assert_eq!(BindMode::Hold.flipped(), BindMode::Toggle);
    }

    #[test]
    fn favorites_sort_first_and_search_still_filters() {
        let mut mgr = ModuleManager::new();
        let render: Vec<ModuleId> = mgr
            .indices_in_category(Category::Render, "")
            .iter()
            .map(|&i| mgr.modules[i].id)
            .collect();
        assert!(render.len() >= 2);
        let last = *render.last().expect("render panel is not empty");
        mgr.toggle_favorite(last);
        let sorted = mgr.sorted_indices_in_category(Category::Render, "");
        assert_eq!(mgr.modules[sorted[0]].id, last);
        assert_eq!(sorted.len(), render.len());

        let filtered = mgr.sorted_indices_in_category(Category::Render, "zzzz-no-such-module");
        assert!(filtered.is_empty());
        mgr.toggle_favorite(last);
        assert!(!mgr.is_favorite(last));
    }

    #[test]
    fn reset_module_restores_settings_but_keeps_keybind() {
        let mut mgr = ModuleManager::new();
        mgr.get_mut(ModuleId::AimAssist).keybind = Some("key.keyboard.r".to_owned());
        let default_range = mgr.get(ModuleId::AimAssist).float("range", 0.0);
        mgr.get_mut(ModuleId::AimAssist)
            .setting_mut("range")
            .expect("aim assist has a range")
            .deserialize_value("3.0");
        assert!((mgr.get(ModuleId::AimAssist).float("range", 0.0) - 3.0).abs() < 1e-6);
        mgr.dirty = false;

        assert!(mgr.reset_module(ModuleId::AimAssist));
        assert!(mgr.dirty);
        assert!((mgr.get(ModuleId::AimAssist).float("range", 0.0) - default_range).abs() < 1e-6);
        assert_eq!(
            mgr.get(ModuleId::AimAssist).keybind.as_deref(),
            Some("key.keyboard.r")
        );
        assert!(!mgr.reset_module(ModuleId::AimAssist), "already at defaults");
    }
}
