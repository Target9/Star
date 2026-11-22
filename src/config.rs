//! Zero-dependency persistent configuration (`config/star.cfg`).
//!
//! Saves and restores every module's enabled state, keybind, settings, and ClickGUI
//! panel layout through Horizon's checked `Host::config_files()` API.

use mod_api::{ClientHost, ConfigPath};

use crate::module::{Category, ModuleManager};

const CONFIG_FILE: &str = "star.cfg";

pub fn load(host: &dyn ClientHost, manager: &mut ModuleManager) {
    manager.config_loaded = true;
    let Ok(path) = ConfigPath::new(CONFIG_FILE) else {
        return;
    };
    let Ok(bytes) = host.config_files().read(&path) else {
        return;
    };
    let Ok(text) = std::str::from_utf8(&bytes) else {
        return;
    };
    let migrated = deserialize(text, manager);
    manager.dirty = migrated;
}

pub fn save(host: &dyn ClientHost, manager: &mut ModuleManager) {
    if !manager.dirty {
        return;
    }
    let Ok(path) = ConfigPath::new(CONFIG_FILE) else {
        return;
    };
    let payload = serialize(manager);
    if host.config_files().write(&path, payload.as_bytes()).is_ok() {
        manager.dirty = false;
    }
}

pub fn serialize(manager: &ModuleManager) -> String {
    let mut out = String::from("# Star configuration\n# format=2\n\n");

    for cat in Category::ALL {
        let p = manager.panels[cat.index()];
        out.push_str(&format!(
            "panel.{}.x={:.1}\npanel.{}.y={:.1}\npanel.{}.collapsed={}\n",
            cat.id(),
            p.x,
            cat.id(),
            p.y,
            cat.id(),
            p.collapsed
        ));
    }

    for module in &manager.modules {
        out.push('\n');
        out.push_str(&format!("mod.{}.enabled={}\n", module.key, module.enabled));
        if let Some(bind) = &module.keybind {
            out.push_str(&format!("mod.{}.keybind={bind}\n", module.key));
        } else {
            out.push_str(&format!("mod.{}.keybind=\n", module.key));
        }
        for setting in &module.settings {
            out.push_str(&format!(
                "set.{}.{}={}\n",
                module.key,
                setting.id,
                setting.serialize_value()
            ));
        }
    }

    out
}

/// Loads configuration text into `manager`. Returns `true` if a legacy config
/// was migrated so callers can persist the upgraded file.
pub fn deserialize(text: &str, manager: &mut ModuleManager) -> bool {
    let mut format_version: u32 = 1;
    for raw_line in text.lines() {
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(comment) = line.strip_prefix('#') {
            if let Some(ver) = comment.trim().strip_prefix("format=")
                && let Ok(v) = ver.trim().parse::<u32>()
            {
                format_version = v;
            }
            continue;
        }
        let Some((key, val)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        let val = val.trim();

        if let Some(rest) = key.strip_prefix("panel.") {
            if let Some((cat_id, prop)) = rest.split_once('.')
                && let Some(cat) = Category::from_id(cat_id)
            {
                let panel = &mut manager.panels[cat.index()];
                match prop {
                    "x" => {
                        if let Ok(x) = val.parse::<f32>()
                            && x.is_finite()
                        {
                            panel.x = x.clamp(0.0, 4000.0);
                        }
                    }
                    "y" => {
                        if let Ok(y) = val.parse::<f32>()
                            && y.is_finite()
                        {
                            panel.y = y.clamp(0.0, 4000.0);
                        }
                    }
                    "collapsed" => {
                        if let Ok(c) = val.parse::<bool>() {
                            panel.collapsed = c;
                        }
                    }
                    _ => {}
                }
            }
            continue;
        }

        if let Some(rest) = key.strip_prefix("mod.") {
            if let Some((mod_key, prop)) = rest.split_once('.')
                && let Some(module) = manager.by_key_mut(mod_key)
            {
                match prop {
                    "enabled" => {
                        if let Ok(on) = val.parse::<bool>() {
                            module.enabled = on;
                        }
                    }
                    "keybind" => {
                        module.keybind = if val.is_empty() {
                            None
                        } else {
                            Some(val.to_owned())
                        };
                    }
                    _ => {}
                }
            }
            continue;
        }

        if let Some(rest) = key.strip_prefix("set.")
            && let Some((mod_key, setting_id)) = rest.split_once('.')
            && let Some(module) = manager.by_key_mut(mod_key)
            && let Some(setting) = module.setting_mut(setting_id)
        {
            setting.deserialize_value(val);
        }
    }

    // Migrate old v1 default `font_scale=0.95` to native crisp `1.00` once.
    let mut migrated = false;
    if format_version < 2
        && let Some(gui) = manager.by_key_mut("click_gui")
        && (gui.float("font_scale", 1.0) - 0.95).abs() < 1e-4
        && let Some(scale_setting) = gui.setting_mut("font_scale")
    {
        scale_setting.deserialize_value("1.0");
        migrated = true;
    }
    migrated
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::module::ModuleId;

    #[test]
    fn config_round_trips_modules_settings_and_panels() {
        let mut mgr = ModuleManager::new();
        mgr.toggle(ModuleId::AimAssist);
        mgr.get_mut(ModuleId::AimAssist).keybind = Some("key.keyboard.r".to_owned());
        mgr.get_mut(ModuleId::AimAssist)
            .setting_mut("range")
            .unwrap()
            .deserialize_value("5.4");
        mgr.get_mut(ModuleId::AimAssist)
            .setting_mut("priority")
            .unwrap()
            .deserialize_value("Lowest Angle");
        mgr.panels[Category::Combat.index()].x = 222.5;
        mgr.panels[Category::Combat.index()].collapsed = true;

        let encoded = serialize(&mgr);
        let mut restored = ModuleManager::new();
        deserialize(&encoded, &mut restored);

        assert!(restored.is_enabled(ModuleId::AimAssist));
        assert_eq!(
            restored.get(ModuleId::AimAssist).keybind.as_deref(),
            Some("key.keyboard.r")
        );
        assert!((restored.get(ModuleId::AimAssist).float("range", 0.0) - 5.4).abs() < 1e-6);
        assert_eq!(
            restored.get(ModuleId::AimAssist).mode_str("priority", ""),
            "Lowest Angle"
        );
        assert!((restored.panels[Category::Combat.index()].x - 222.5).abs() < 0.2);
        assert!(restored.panels[Category::Combat.index()].collapsed);
    }
}
