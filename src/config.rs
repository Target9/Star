//! Zero-dependency persistent configuration (`config/star.cfg`).
//!
//! Saves and restores every module's enabled state, keybind, settings, and ClickGUI
//! panel layout through Horizon's checked `Host::config_files()` API.
//! Named profiles live beside it in `config/star-profiles.cfg`.

use mod_api::{ClientHost, ConfigPath};

use crate::module::{BindMode, Category, ModuleId, ModuleManager, SavedProfile};

const CONFIG_FILE: &str = "star.cfg";
const CONFIG_BACKUP_FILE: &str = "star.cfg.bak";
const PROFILES_FILE: &str = "star-profiles.cfg";
const MAX_PROFILES: usize = 32;
/// Current on-disk format. v3 adds bind modes, favorites, HUD positions and a trailing
/// `# end` marker used to detect truncated writes.
const FORMAT_VERSION: u32 = 3;
const END_MARKER: &str = "# end";
const HEADER: &str = "# Star configuration";
/// Minimum gap between automatic saves while the player is dragging sliders and so on.
const SAVE_INTERVAL_MS: u64 = 1_000;
/// Hard cap on config size so a corrupt or hostile file cannot balloon memory use.
const MAX_CONFIG_BYTES: usize = 512 * 1024;

pub fn load(host: &dyn ClientHost, manager: &mut ModuleManager) {
    manager.config_loaded = true;

    let main = read_config(host, CONFIG_FILE);
    let mut restored_from_backup = false;
    let text = match main {
        Some(text) if is_valid_config(&text) => Some(text),
        other => {
            // Missing is normal on first run; present-but-invalid means corruption.
            let had_file = other.is_some();
            match read_config(host, CONFIG_BACKUP_FILE) {
                Some(bak) if is_valid_config(&bak) => {
                    restored_from_backup = had_file;
                    Some(bak)
                }
                _ => None,
            }
        }
    };
    if let Some(text) = text {
        let migrated = deserialize(&text, manager);
        manager.dirty = migrated || restored_from_backup;
        if restored_from_backup {
            crate::toast::notify(
                crate::toast::ToastKind::Warning,
                "Config was damaged; restored the last backup",
            );
        }
    }
    if let Some(text) = read_config(host, PROFILES_FILE) {
        deserialize_profiles(&text, manager);
    }
    ensure_default_profile(manager);
}

/// Writes pending changes now.
pub fn save(host: &dyn ClientHost, manager: &mut ModuleManager) {
    if manager.dirty && write_main(host, manager) {
        manager.dirty = false;
    }
    if manager.profiles_dirty
        && let Ok(path) = ConfigPath::new(PROFILES_FILE)
    {
        let payload = serialize_profiles(manager);
        if host.config_files().write(&path, payload.as_bytes()).is_ok() {
            manager.profiles_dirty = false;
        }
    }
    manager.last_save_ms = host.millis();
}

/// Writes pending changes unless a save already happened within the last second, so
/// dragging a slider does not hit the disk on every event. Dirty state is kept, so the
/// next call (or the forced save on GUI close) persists the latest values.
pub fn save_throttled(host: &dyn ClientHost, manager: &mut ModuleManager) {
    if !(manager.dirty || manager.profiles_dirty) {
        return;
    }
    if host.millis().saturating_sub(manager.last_save_ms) >= SAVE_INTERVAL_MS {
        save(host, manager);
    }
}

fn write_main(host: &dyn ClientHost, manager: &mut ModuleManager) -> bool {
    let Ok(path) = ConfigPath::new(CONFIG_FILE) else {
        return false;
    };
    // Once per session, keep the previous run's good config as a backup before replacing it.
    if !manager.backup_written {
        manager.backup_written = true;
        if let Some(previous) = read_config(host, CONFIG_FILE)
            && is_valid_config(&previous)
            && let Ok(bak_path) = ConfigPath::new(CONFIG_BACKUP_FILE)
        {
            let _ = host.config_files().write(&bak_path, previous.as_bytes());
        }
    }
    let payload = serialize(manager);
    host.config_files().write(&path, payload.as_bytes()).is_ok()
}

fn read_config(host: &dyn ClientHost, file: &str) -> Option<String> {
    let path = ConfigPath::new(file).ok()?;
    let bytes = host.config_files().read(&path).ok()?;
    if bytes.len() > MAX_CONFIG_BYTES {
        return None;
    }
    String::from_utf8(bytes).ok()
}

/// Whether `text` looks like a complete Star config: right header, no stray control
/// characters, and (for v3+) the trailing end marker that proves the write finished.
pub fn is_valid_config(text: &str) -> bool {
    if text.is_empty() || text.len() > MAX_CONFIG_BYTES || text.contains('\0') {
        return false;
    }
    if !text.lines().any(|l| l.trim() == HEADER) {
        return false;
    }
    let version = text
        .lines()
        .filter_map(|l| l.trim().strip_prefix('#'))
        .find_map(|c| c.trim().strip_prefix("format=")?.trim().parse::<u32>().ok())
        .unwrap_or(1);
    if version >= 3 {
        return text
            .lines()
            .rev()
            .find(|l| !l.trim().is_empty())
            .is_some_and(|l| l.trim() == END_MARKER);
    }
    true
}

/// The whole setup as one shareable text blob (same format as `star.cfg`).
pub fn export_text(manager: &ModuleManager) -> String {
    serialize(manager)
}

/// Replaces the live setup with a blob produced by [`export_text`]. Saved profiles and
/// the loaded flag are kept; the setup is only touched if the blob validates.
pub fn import_text(manager: &mut ModuleManager, text: &str) -> Result<(), &'static str> {
    let text = text.trim();
    if text.is_empty() {
        return Err("Clipboard is empty.");
    }
    if !is_valid_config(&format!("{text}\n")) {
        return Err("Clipboard does not contain a Star config.");
    }
    apply_snapshot(manager, text);
    manager.active_profile.clear();
    manager.profiles_dirty = true;
    Ok(())
}

pub fn serialize(manager: &ModuleManager) -> String {
    let mut out = format!("{HEADER}\n# format={FORMAT_VERSION}\n\n");

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
        out.push_str(&format!(
            "mod.{}.bindmode={}\n",
            module.key,
            manager.bind_mode(module.id).id()
        ));
        if manager.is_favorite(module.id) {
            out.push_str(&format!("mod.{}.favorite=true\n", module.key));
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

    if !manager.hud_positions.is_empty() {
        out.push('\n');
        for (id, [x, y]) in &manager.hud_positions {
            out.push_str(&format!("hud.{id}.x={x:.1}\nhud.{id}.y={y:.1}\n"));
        }
    }

    out.push('\n');
    out.push_str(END_MARKER);
    out.push('\n');
    out
}

/// Loads configuration text into `manager`. Returns `true` if a legacy config
/// was migrated so callers can persist the upgraded file.
pub fn deserialize(text: &str, manager: &mut ModuleManager) -> bool {
    let mut format_version: u32 = 1;
    let mut legacy_visuals_on = false;
    let mut visuals_enabled_in_file = false;
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
                            if mod_key == "aim_visuals" {
                                visuals_enabled_in_file = true;
                            }
                        }
                    }
                    "keybind" => {
                        module.keybind = if val.is_empty() {
                            None
                        } else {
                            Some(val.to_owned())
                        };
                    }
                    "bindmode" => {
                        if let Some(mode) = BindMode::from_id(val) {
                            let id = module.id;
                            manager.set_bind_mode(id, mode);
                        }
                    }
                    "favorite" => {
                        if let Ok(fav) = val.parse::<bool>() {
                            let id = module.id;
                            if fav {
                                manager.favorites.insert(id);
                            } else {
                                manager.favorites.remove(&id);
                            }
                        }
                    }
                    _ => {}
                }
            }
            continue;
        }

        if let Some(rest) = key.strip_prefix("hud.") {
            if let Some((widget, axis)) = rest.rsplit_once('.')
                && is_widget_id(widget)
                && let Ok(v) = val.parse::<f32>()
                && v.is_finite()
            {
                let entry = manager
                    .hud_positions
                    .entry(widget.to_owned())
                    .or_insert([0.0, 0.0]);
                match axis {
                    "x" => entry[0] = v.clamp(0.0, 8000.0),
                    "y" => entry[1] = v.clamp(0.0, 8000.0),
                    _ => {}
                }
            }
            continue;
        }

        if let Some(rest) = key.strip_prefix("set.")
            && let Some((mut mod_key, setting_id)) = rest.split_once('.')
        {
            // Aim Assist's visuals became their own module; carry old values over.
            if mod_key == "aim_assist"
                && crate::modules::aim_assist::MOVED_VISUAL_SETTINGS.contains(&setting_id)
            {
                mod_key = "aim_visuals";
                if setting_id.starts_with("show_") && val == "true" {
                    legacy_visuals_on = true;
                }
            }
            if let Some(module) = manager.by_key_mut(mod_key)
                && let Some(setting) = module.setting_mut(setting_id)
            {
                setting.deserialize_value(val);
            }
        }
    }

    let mut migrated = false;
    // Old configs showed the visuals whenever Aim Assist was on; keep that behavior once.
    if legacy_visuals_on
        && !visuals_enabled_in_file
        && manager.get(ModuleId::AimAssist).enabled
    {
        manager.get_mut(ModuleId::AimVisuals).enabled = true;
        migrated = true;
    }

    // Migrate old v1 default `font_scale=0.95` to native crisp `1.00` once.
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

fn is_widget_id(id: &str) -> bool {
    crate::hud::WIDGET_IDS.contains(&id)
}

pub fn normalize_profile_name(raw: &str) -> Result<String, &'static str> {
    let name = raw.trim();
    if name.is_empty() {
        return Err("Enter a profile name.");
    }
    if name.chars().count() > 24 {
        return Err("Name must be 24 characters or fewer.");
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, ' ' | '-' | '_'))
    {
        return Err("Use letters, numbers, spaces, - or _.");
    }
    Ok(name.to_owned())
}

/// Saves the live setup as a new profile and makes it active.
pub fn create_profile(manager: &mut ModuleManager, raw: &str) -> Result<(), &'static str> {
    let name = normalize_profile_name(raw)?;
    if manager
        .profiles
        .iter()
        .any(|p| p.name.eq_ignore_ascii_case(&name))
    {
        return Err("That profile already exists. Use Save on its row.");
    }
    if manager.profiles.len() >= MAX_PROFILES {
        return Err("Profile list is full (32).");
    }
    let body = serialize(manager);
    manager.profiles.push(SavedProfile {
        name: name.clone(),
        body,
    });
    manager.active_profile = name;
    manager.profiles_dirty = true;
    Ok(())
}

/// Overwrites an existing profile with the live setup and makes it active.
pub fn save_profile(manager: &mut ModuleManager, name: &str) -> bool {
    let body = serialize(manager);
    let Some(profile) = manager
        .profiles
        .iter_mut()
        .find(|p| p.name.eq_ignore_ascii_case(name))
    else {
        return false;
    };
    profile.body = body;
    manager.active_profile = profile.name.clone();
    manager.profiles_dirty = true;
    true
}

pub fn delete_profile(manager: &mut ModuleManager, name: &str) -> bool {
    let before = manager.profiles.len();
    manager
        .profiles
        .retain(|p| !p.name.eq_ignore_ascii_case(name));
    if manager.profiles.len() == before {
        return false;
    }
    if manager.active_profile.eq_ignore_ascii_case(name) {
        manager.active_profile.clear();
    }
    manager.profiles_dirty = true;
    true
}

/// Replaces the live setup with a saved profile. Missing settings fall back to defaults.
pub fn load_profile(manager: &mut ModuleManager, name: &str) -> bool {
    let Some(index) = manager
        .profiles
        .iter()
        .position(|p| p.name.eq_ignore_ascii_case(name))
    else {
        return false;
    };
    let canonical = manager.profiles[index].name.clone();
    let body = manager.profiles[index].body.clone();
    apply_snapshot(manager, &body);
    manager.active_profile = canonical;
    manager.profiles_dirty = true;
    manager.dirty = true;
    true
}

/// Restores modules, keybinds, settings, and panel layout. Saved profiles stay.
pub fn reset_everything(manager: &mut ModuleManager, screen_width: f32) {
    let (profiles, loaded, backup, last_save) = take_session_state(manager);
    *manager = ModuleManager::new();
    manager.profiles = profiles;
    manager.active_profile.clear();
    manager.config_loaded = loaded;
    manager.backup_written = backup;
    manager.last_save_ms = last_save;
    manager.profiles_dirty = true;
    manager.reset_panel_layout(screen_width);
}

fn apply_snapshot(manager: &mut ModuleManager, body: &str) {
    let (profiles, loaded, backup, last_save) = take_session_state(manager);
    *manager = ModuleManager::new();
    deserialize(body, manager);
    manager.profiles = profiles;
    manager.config_loaded = loaded;
    manager.backup_written = backup;
    manager.last_save_ms = last_save;
    manager.dirty = true;
}

/// State that must survive replacing the module set wholesale.
fn take_session_state(manager: &mut ModuleManager) -> (Vec<SavedProfile>, bool, bool, u64) {
    (
        std::mem::take(&mut manager.profiles),
        manager.config_loaded,
        manager.backup_written,
        manager.last_save_ms,
    )
}

fn ensure_default_profile(manager: &mut ModuleManager) {
    if manager.profiles.is_empty() {
        let body = serialize(manager);
        manager.profiles.push(SavedProfile {
            name: "default".to_owned(),
            body,
        });
        manager.active_profile = "default".to_owned();
        manager.profiles_dirty = true;
        return;
    }
    if !manager
        .profiles
        .iter()
        .any(|p| p.name == manager.active_profile)
    {
        manager.active_profile = manager.profiles[0].name.clone();
        manager.profiles_dirty = true;
    }
}

pub fn serialize_profiles(manager: &ModuleManager) -> String {
    let mut out = format!(
        "# Star profiles\n# format=1\nactive={}\n",
        manager.active_profile
    );
    for profile in &manager.profiles {
        out.push_str("\n:::");
        out.push_str(&profile.name);
        out.push('\n');
        out.push_str(&profile.body);
        if !profile.body.ends_with('\n') {
            out.push('\n');
        }
    }
    out
}

pub fn deserialize_profiles(text: &str, manager: &mut ModuleManager) {
    let mut active = String::new();
    let mut profiles: Vec<SavedProfile> = Vec::new();
    let mut current: Option<SavedProfile> = None;
    for raw_line in text.lines() {
        if let Some(name) = raw_line.strip_prefix(":::") {
            if let Some(profile) = current.take() {
                profiles.push(profile);
            }
            let name = name.trim();
            if normalize_profile_name(name).is_ok() {
                current = Some(SavedProfile {
                    name: name.to_owned(),
                    body: String::new(),
                });
            }
            continue;
        }
        if let Some(profile) = current.as_mut() {
            profile.body.push_str(raw_line);
            profile.body.push('\n');
            continue;
        }
        if let Some(value) = raw_line.trim().strip_prefix("active=") {
            active = value.trim().to_owned();
        }
    }
    if let Some(profile) = current.take() {
        profiles.push(profile);
    }
    profiles.truncate(MAX_PROFILES);
    manager.profiles = profiles;
    manager.active_profile = active;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::module::{BindMode, ModuleId};

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

    #[test]
    fn profiles_round_trip_load_and_reset_keep_snapshots() {
        let mut mgr = ModuleManager::new();
        mgr.toggle(ModuleId::AimAssist);
        mgr.get_mut(ModuleId::AimAssist).keybind = Some("key.keyboard.r".to_owned());
        create_profile(&mut mgr, "pvp").unwrap();
        assert!(create_profile(&mut mgr, "pvp").is_err());
        assert!(create_profile(&mut mgr, "bad/name").is_err());

        mgr.get_mut(ModuleId::AimAssist).keybind = Some("key.keyboard.f".to_owned());
        assert!(save_profile(&mut mgr, "pvp"));
        assert_eq!(mgr.active_profile, "pvp");

        let encoded = serialize_profiles(&mgr);
        let mut restored = ModuleManager::new();
        deserialize_profiles(&encoded, &mut restored);
        assert_eq!(restored.profiles.len(), 1);
        assert_eq!(restored.active_profile, "pvp");

        mgr.toggle(ModuleId::AimAssist);
        assert!(!mgr.is_enabled(ModuleId::AimAssist));
        assert!(load_profile(&mut mgr, "pvp"));
        assert!(mgr.is_enabled(ModuleId::AimAssist));
        assert_eq!(
            mgr.get(ModuleId::AimAssist).keybind.as_deref(),
            Some("key.keyboard.f")
        );
        assert_eq!(mgr.profiles.len(), 1);

        reset_everything(&mut mgr, 800.0);
        assert!(!mgr.is_enabled(ModuleId::AimAssist));
        assert!(mgr.get(ModuleId::AimAssist).keybind.is_none());
        assert!(mgr.active_profile.is_empty());
        assert_eq!(mgr.profiles.len(), 1);
        assert!(mgr.profiles[0].body.contains("key.keyboard.f"));
    }

    #[test]
    fn v3_round_trips_bind_modes_favorites_and_widget_positions() {
        let mut mgr = ModuleManager::new();
        mgr.set_bind_mode(ModuleId::Sprint, BindMode::Hold);
        mgr.toggle_favorite(ModuleId::Radar);
        mgr.hud_positions.insert("radar".to_owned(), [120.5, 44.0]);

        let encoded = serialize(&mgr);
        assert!(is_valid_config(&encoded));
        let mut restored = ModuleManager::new();
        assert!(!deserialize(&encoded, &mut restored), "current format needs no migration");
        assert_eq!(restored.bind_mode(ModuleId::Sprint), BindMode::Hold);
        assert_eq!(restored.bind_mode(ModuleId::Radar), BindMode::Toggle);
        assert!(restored.is_favorite(ModuleId::Radar));
        let pos = restored.hud_positions.get("radar").copied().unwrap_or_default();
        assert!((pos[0] - 120.5).abs() < 0.2 && (pos[1] - 44.0).abs() < 0.2);
    }

    #[test]
    fn old_aim_assist_visual_settings_move_to_aim_visuals() {
        let old = format!(
            "{HEADER}\n# format=3\nmod.aim_assist.enabled=true\n\
             set.aim_assist.show_target_hud=false\nset.aim_assist.hud_ping=false\n\
             set.aim_assist.show_range_circle=true\nset.aim_assist.range=5.0\n{END_MARKER}\n"
        );
        let mut mgr = ModuleManager::new();
        assert!(deserialize(&old, &mut mgr), "migration asks for a re-save");
        let visuals = mgr.get(ModuleId::AimVisuals);
        assert!(visuals.enabled, "visuals stay on for users who had them on");
        assert!(!visuals.bool("show_target_hud", true));
        assert!(!visuals.bool("hud_ping", true));
        assert!(visuals.bool("show_range_circle", false));
        assert!((mgr.get(ModuleId::AimAssist).float("range", 0.0) - 5.0).abs() < 1e-6);

        // Saved again, the file carries the new module and migrates nothing further.
        let mut again = ModuleManager::new();
        assert!(!deserialize(&serialize(&mgr), &mut again));
        assert!(again.is_enabled(ModuleId::AimVisuals));
        assert!(!again.get(ModuleId::AimVisuals).bool("show_target_hud", true));

        // Nothing was on before: the new module stays off.
        let quiet = format!(
            "{HEADER}\n# format=3\nmod.aim_assist.enabled=true\nset.aim_assist.show_target_hud=false\n{END_MARKER}\n"
        );
        let mut off = ModuleManager::new();
        deserialize(&quiet, &mut off);
        assert!(!off.is_enabled(ModuleId::AimVisuals));
    }

    #[test]
    fn truncated_or_foreign_files_are_rejected() {
        let mgr = ModuleManager::new();
        let good = serialize(&mgr);
        assert!(is_valid_config(&good));

        // A write cut off before the end marker must not be trusted.
        let cut = &good[..good.len() / 2];
        assert!(!is_valid_config(cut));
        assert!(!is_valid_config(""));
        assert!(!is_valid_config("hello world"));
        assert!(!is_valid_config(&format!("{good}\0")));
        assert!(!is_valid_config(&"x".repeat(MAX_CONFIG_BYTES + 1)));
    }

    #[test]
    fn hostile_values_are_clamped_not_trusted() {
        let mut mgr = ModuleManager::new();
        let text = format!(
            "{HEADER}\n# format=3\nmod.aim_assist.enabled=maybe\nmod.aim_assist.range=99999\n\
             hud.radar.x=-5000\nhud.radar.y=nan\nhud.bogus.x=3\nmod.nonexistent.enabled=true\n{END_MARKER}\n"
        );
        assert!(is_valid_config(&text));
        deserialize(&text, &mut mgr);
        let range = mgr.get(ModuleId::AimAssist).float("range", 0.0);
        assert!(range.is_finite() && range < 99999.0);
        assert!(!mgr.hud_positions.contains_key("bogus"));
        if let Some(p) = mgr.hud_positions.get("radar") {
            assert!(p[0].is_finite() && p[1].is_finite());
        }
    }

    #[test]
    fn import_export_round_trip_and_rejects_garbage() {
        let mut source = ModuleManager::new();
        source.toggle(ModuleId::Radar);
        source.toggle_favorite(ModuleId::Sprint);
        let blob = export_text(&source);

        let mut target = ModuleManager::new();
        assert!(import_text(&mut target, "").is_err());
        assert!(import_text(&mut target, "not a config").is_err());
        assert!(!target.is_enabled(ModuleId::Radar), "failed imports change nothing");

        assert!(import_text(&mut target, &blob).is_ok());
        assert!(target.is_enabled(ModuleId::Radar));
        assert!(target.is_favorite(ModuleId::Sprint));
    }
}
