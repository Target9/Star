//! Categories, module definitions, and the central `ModuleManager`.

use crate::modules::{aim_assist, fast_mine};
use crate::settings::Setting;
use crate::theme::THEME_PRESETS;

/// Top-level categories shown as draggable windows in the ClickGUI.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Category {
    Combat,
    Player,
    Movement,
    Render,
    Client,
}

impl Category {
    pub const ALL: [Category; 5] = [
        Category::Combat,
        Category::Player,
        Category::Movement,
        Category::Render,
        Category::Client,
    ];

    pub fn index(self) -> usize {
        match self {
            Category::Combat => 0,
            Category::Player => 1,
            Category::Movement => 2,
            Category::Render => 3,
            Category::Client => 4,
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            Category::Combat => "combat",
            Category::Player => "player",
            Category::Movement => "movement",
            Category::Render => "render",
            Category::Client => "client",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Category::Combat => "Combat",
            Category::Player => "Player",
            Category::Movement => "Movement",
            Category::Render => "Render",
            Category::Client => "Client",
        }
    }

    pub fn badge(self) -> &'static str {
        match self {
            Category::Combat => "C",
            Category::Player => "P",
            Category::Movement => "M",
            Category::Render => "R",
            Category::Client => "G",
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
    FastMine,
    Hud,
    ClickGui,
}

/// One toggleable feature/module displayed in its category window.
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
    /// When false, the module is a configuration-only module (like ClickGUI) and does not show in the HUD ArrayList.
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
            ModuleId::Hud | ModuleId::ClickGui => None,
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

/// Central registry of all modules, panel layouts, and config dirty state.
pub struct ModuleManager {
    pub modules: Vec<Module>,
    pub panels: [PanelState; 5],
    pub config_loaded: bool,
    pub dirty: bool,
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
            fast_mine::build_module(),
            build_hud_module(),
            build_click_gui_module(),
        ];
        Self {
            modules,
            panels: default_panels(136.0),
            config_loaded: false,
            dirty: false,
        }
    }

    pub fn reset_panel_layout(&mut self, screen_width: f32) {
        let panel_w = self.get(ModuleId::ClickGui).int("panel_width", 136) as f32;
        let gap = 10.0;
        let total = Category::ALL.len() as f32 * panel_w + (Category::ALL.len() - 1) as f32 * gap;
        let start_x = ((screen_width - total) * 0.5).max(12.0);
        for (i, panel) in self.panels.iter_mut().enumerate() {
            panel.x = start_x + i as f32 * (panel_w + gap);
            panel.y = 36.0;
            panel.collapsed = false;
            panel.scroll = 0.0;
        }
        self.dirty = true;
    }

    pub fn get(&self, id: ModuleId) -> &Module {
        self.modules
            .iter()
            .find(|m| m.id == id)
            .expect("registered module exists")
    }

    #[allow(dead_code)]
    pub fn get_mut(&mut self, id: ModuleId) -> &mut Module {
        self.modules
            .iter_mut()
            .find(|m| m.id == id)
            .expect("registered module exists")
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

    /// Toggles every module whose keybind matches `key` (returns how many toggled).
    pub fn trigger_keybind(&mut self, key: &str) -> usize {
        let mut count = 0;
        for m in &mut self.modules {
            if m.id == ModuleId::ClickGui {
                continue;
            }
            if m.keybind.as_deref() == Some(key) {
                m.enabled = !m.enabled;
                count += 1;
            }
        }
        if count > 0 {
            self.dirty = true;
        }
        count
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

fn default_panels(panel_width: f32) -> [PanelState; 5] {
    let gap = 10.0;
    let start_x = 14.0;
    let mut out = [PanelState {
        x: start_x,
        y: 36.0,
        collapsed: false,
        scroll: 0.0,
    }; 5];
    for (i, p) in out.iter_mut().enumerate() {
        p.x = start_x + i as f32 * (panel_width + gap);
    }
    out
}

fn build_click_gui_module() -> Module {
    Module {
        id: ModuleId::ClickGui,
        key: "click_gui",
        name: "ClickGUI",
        description: "Customize the module menu theme, colors, layout, blur, and keybind.",
        category: Category::Client,
        enabled: true,
        keybind: None,
        expanded: false,
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
                110,
                210,
                4,
                136,
            )
            .suffix("px")
            .group("Layout"),
            Setting::int(
                "row_height",
                "Row Height",
                "Height of each module row in GUI pixels.",
                13,
                22,
                1,
                16,
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
        description: "In-game watermark, active module ArrayList, and Aim Assist overlays.",
        category: Category::Client,
        enabled: true,
        keybind: None,
        expanded: false,
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
        ],
    }
}
