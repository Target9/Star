//! Typed settings primitives shared by every module and rendered by the ClickGUI.

/// The interactive value and bounds of a module setting.
#[derive(Clone, Debug)]
pub enum SettingKind {
    /// On/off toggle switch.
    Bool { value: bool, default: bool },
    /// Floating-point slider with snapping step and optional display suffix (e.g. `"°"` or `"x"`).
    Float {
        value: f64,
        default: f64,
        min: f64,
        max: f64,
        step: f64,
        suffix: &'static str,
    },
    /// Integer slider with snapping step and optional display suffix.
    Int {
        value: i64,
        default: i64,
        min: i64,
        max: i64,
        step: i64,
        suffix: &'static str,
    },
    /// Named enum/mode selector.
    Mode {
        index: usize,
        default: usize,
        options: &'static [&'static str],
    },
    /// ARGB color (`0xAARRGGBB`) with an expandable RGBA slider drawer in the GUI.
    Color {
        value: u32,
        default: u32,
        expanded: bool,
    },
    /// Optional vanilla key name (e.g. `"key.keyboard.right.shift"`).
    Keybind {
        key: Option<String>,
        default: Option<&'static str>,
    },
}

/// One configurable parameter owned by a module.
#[derive(Clone, Debug)]
pub struct Setting {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub group: Option<&'static str>,
    pub kind: SettingKind,
}

impl Setting {
    pub fn bool(id: &'static str, name: &'static str, description: &'static str, default: bool) -> Self {
        Self {
            id,
            name,
            description,
            group: None,
            kind: SettingKind::Bool {
                value: default,
                default,
            },
        }
    }

    pub fn float(
        id: &'static str,
        name: &'static str,
        description: &'static str,
        min: f64,
        max: f64,
        step: f64,
        default: f64,
    ) -> Self {
        Self {
            id,
            name,
            description,
            group: None,
            kind: SettingKind::Float {
                value: snap_float(default, min, max, step),
                default: snap_float(default, min, max, step),
                min,
                max,
                step,
                suffix: "",
            },
        }
    }

    pub fn int(
        id: &'static str,
        name: &'static str,
        description: &'static str,
        min: i64,
        max: i64,
        step: i64,
        default: i64,
    ) -> Self {
        let clamped = snap_int(default, min, max, step);
        Self {
            id,
            name,
            description,
            group: None,
            kind: SettingKind::Int {
                value: clamped,
                default: clamped,
                min,
                max,
                step,
                suffix: "",
            },
        }
    }

    pub fn suffix(mut self, unit: &'static str) -> Self {
        match &mut self.kind {
            SettingKind::Float { suffix, .. } | SettingKind::Int { suffix, .. } => {
                *suffix = unit;
            }
            _ => {}
        }
        self
    }

    pub fn mode(
        id: &'static str,
        name: &'static str,
        description: &'static str,
        options: &'static [&'static str],
        default: usize,
    ) -> Self {
        let idx = default.min(options.len().saturating_sub(1));
        Self {
            id,
            name,
            description,
            group: None,
            kind: SettingKind::Mode {
                index: idx,
                default: idx,
                options,
            },
        }
    }

    pub fn color(
        id: &'static str,
        name: &'static str,
        description: &'static str,
        default: u32,
    ) -> Self {
        Self {
            id,
            name,
            description,
            group: None,
            kind: SettingKind::Color {
                value: default,
                default,
                expanded: false,
            },
        }
    }

    pub fn keybind(
        id: &'static str,
        name: &'static str,
        description: &'static str,
        default: Option<&'static str>,
    ) -> Self {
        Self {
            id,
            name,
            description,
            group: None,
            kind: SettingKind::Keybind {
                key: default.map(str::to_owned),
                default,
            },
        }
    }

    pub fn group(mut self, group: &'static str) -> Self {
        self.group = Some(group);
        self
    }

    pub fn as_bool(&self) -> Option<bool> {
        match &self.kind {
            SettingKind::Bool { value, .. } => Some(*value),
            _ => None,
        }
    }

    pub fn as_float(&self) -> Option<f64> {
        match &self.kind {
            SettingKind::Float { value, .. } => Some(*value),
            _ => None,
        }
    }

    pub fn as_int(&self) -> Option<i64> {
        match &self.kind {
            SettingKind::Int { value, .. } => Some(*value),
            _ => None,
        }
    }

    pub fn as_mode(&self) -> Option<usize> {
        match &self.kind {
            SettingKind::Mode { index, .. } => Some(*index),
            _ => None,
        }
    }

    pub fn as_mode_str(&self) -> Option<&'static str> {
        match &self.kind {
            SettingKind::Mode { index, options, .. } => options.get(*index).copied(),
            _ => None,
        }
    }

    pub fn as_color(&self) -> Option<u32> {
        match &self.kind {
            SettingKind::Color { value, .. } => Some(*value),
            _ => None,
        }
    }

    pub fn as_keybind(&self) -> Option<Option<&str>> {
        match &self.kind {
            SettingKind::Keybind { key, .. } => Some(key.as_deref()),
            _ => None,
        }
    }

    /// Resets this setting back to its default value. Returns true if it changed.
    pub fn reset(&mut self) -> bool {
        match &mut self.kind {
            SettingKind::Bool { value, default } => {
                let changed = *value != *default;
                *value = *default;
                changed
            }
            SettingKind::Float { value, default, .. } => {
                let changed = (*value - *default).abs() > 1e-9;
                *value = *default;
                changed
            }
            SettingKind::Int { value, default, .. } => {
                let changed = *value != *default;
                *value = *default;
                changed
            }
            SettingKind::Mode { index, default, .. } => {
                let changed = *index != *default;
                *index = *default;
                changed
            }
            SettingKind::Color { value, default, .. } => {
                let changed = *value != *default;
                *value = *default;
                changed
            }
            SettingKind::Keybind { key, default } => {
                let target = default.map(str::to_owned);
                let changed = *key != target;
                *key = target;
                changed
            }
        }
    }

    /// Normalized slider fraction in `0.0..=1.0`, if this setting is numeric.
    pub fn slider_fraction(&self) -> Option<f32> {
        match &self.kind {
            SettingKind::Float {
                value, min, max, ..
            } => {
                let span = (max - min).max(1e-9);
                Some(((*value - *min) / span).clamp(0.0, 1.0) as f32)
            }
            SettingKind::Int {
                value, min, max, ..
            } => {
                let span = (*max - *min).max(1) as f64;
                Some(((*value - *min) as f64 / span).clamp(0.0, 1.0) as f32)
            }
            _ => None,
        }
    }

    /// Updates a numeric slider from a normalized fraction in `0.0..=1.0`.
    pub fn set_slider_fraction(&mut self, t: f32) -> bool {
        let t = (t as f64).clamp(0.0, 1.0);
        match &mut self.kind {
            SettingKind::Float {
                value,
                min,
                max,
                step,
                ..
            } => {
                let next = snap_float(*min + (*max - *min) * t, *min, *max, *step);
                let changed = (*value - next).abs() > 1e-9;
                *value = next;
                changed
            }
            SettingKind::Int {
                value,
                min,
                max,
                step,
                ..
            } => {
                let raw = *min + ((*max - *min) as f64 * t).round() as i64;
                let next = snap_int(raw, *min, *max, *step);
                let changed = *value != next;
                *value = next;
                changed
            }
            _ => false,
        }
    }

    /// Adjusts a numeric slider by `steps` increments (positive or negative).
    pub fn nudge(&mut self, steps: i64) -> bool {
        if steps == 0 {
            return false;
        }
        match &mut self.kind {
            SettingKind::Float {
                value,
                min,
                max,
                step,
                ..
            } => {
                let next = snap_float(*value + *step * steps as f64, *min, *max, *step);
                let changed = (*value - next).abs() > 1e-9;
                *value = next;
                changed
            }
            SettingKind::Int {
                value,
                min,
                max,
                step,
                ..
            } => {
                let next = snap_int(*value + *step * steps, *min, *max, *step);
                let changed = *value != next;
                *value = next;
                changed
            }
            _ => false,
        }
    }

    /// Formats the current setting value for compact display in the GUI.
    pub fn display_value(&self) -> String {
        match &self.kind {
            SettingKind::Bool { value, .. } => {
                if *value { "ON".into() } else { "OFF".into() }
            }
            SettingKind::Float {
                value,
                step,
                suffix,
                ..
            } => {
                let decimals = decimals_for_step(*step);
                format!("{value:.decimals$}{suffix}")
            }
            SettingKind::Int { value, suffix, .. } => format!("{value}{suffix}"),
            SettingKind::Mode { index, options, .. } => {
                options.get(*index).copied().unwrap_or("?").to_owned()
            }
            SettingKind::Color { value, .. } => format!("#{:08X}", value),
            SettingKind::Keybind { key, .. } => format_key_name(key.as_deref()),
        }
    }

    /// Encodes the value as a single config line token.
    pub fn serialize_value(&self) -> String {
        match &self.kind {
            SettingKind::Bool { value, .. } => {
                if *value { "true".into() } else { "false".into() }
            }
            SettingKind::Float { value, .. } => format!("{value:.6}"),
            SettingKind::Int { value, .. } => value.to_string(),
            SettingKind::Mode { index, options, .. } => {
                options.get(*index).copied().unwrap_or("").to_owned()
            }
            SettingKind::Color { value, .. } => format!("{value:08X}"),
            SettingKind::Keybind { key, .. } => key.clone().unwrap_or_default(),
        }
    }

    /// Decodes a config line token back into this setting, preserving bounds.
    pub fn deserialize_value(&mut self, raw: &str) {
        let raw = raw.trim();
        match &mut self.kind {
            SettingKind::Bool { value, .. } => {
                if let Ok(parsed) = raw.parse::<bool>() {
                    *value = parsed;
                }
            }
            SettingKind::Float {
                value,
                min,
                max,
                step,
                ..
            } => {
                if let Ok(parsed) = raw.parse::<f64>()
                    && parsed.is_finite()
                {
                    *value = snap_float(parsed, *min, *max, *step);
                }
            }
            SettingKind::Int {
                value,
                min,
                max,
                step,
                ..
            } => {
                if let Ok(parsed) = raw.parse::<i64>() {
                    *value = snap_int(parsed, *min, *max, *step);
                }
            }
            SettingKind::Mode { index, options, .. } => {
                if let Some(pos) = options
                    .iter()
                    .position(|opt| opt.eq_ignore_ascii_case(raw))
                {
                    *index = pos;
                } else if let Ok(pos) = raw.parse::<usize>()
                    && pos < options.len()
                {
                    *index = pos;
                }
            }
            SettingKind::Color { value, .. } => {
                let hex = raw.trim_start_matches('#');
                if let Ok(parsed) = u32::from_str_radix(hex, 16) {
                    *value = if hex.len() == 6 {
                        0xFF00_0000 | parsed
                    } else {
                        parsed
                    };
                }
            }
            SettingKind::Keybind { key, .. } => {
                if raw.is_empty() || raw.eq_ignore_ascii_case("none") {
                    *key = None;
                } else {
                    *key = Some(raw.to_owned());
                }
            }
        }
    }
}

pub fn snap_float(v: f64, min: f64, max: f64, step: f64) -> f64 {
    let clamped = v.clamp(min, max);
    if step <= 0.0 || !step.is_finite() {
        return clamped;
    }
    let steps = ((clamped - min) / step).round();
    (min + steps * step).clamp(min, max)
}

pub fn snap_int(v: i64, min: i64, max: i64, step: i64) -> i64 {
    let clamped = v.clamp(min, max);
    if step <= 1 {
        return clamped;
    }
    let offset = clamped - min;
    let snapped = ((offset + step / 2) / step) * step;
    (min + snapped).clamp(min, max)
}

fn decimals_for_step(step: f64) -> usize {
    if step >= 1.0 {
        0
    } else if step >= 0.1 - 1e-6 {
        1
    } else {
        2
    }
}

/// Converts a vanilla GLFW key identifier (`key.keyboard.right.shift`) into a compact label (`RSHIFT`).
pub fn format_key_name(key: Option<&str>) -> String {
    let Some(key) = key else {
        return "NONE".to_owned();
    };
    let tail = key.strip_prefix("key.keyboard.").unwrap_or(key);
    match tail {
        "right.shift" => "RSHIFT".into(),
        "left.shift" => "LSHIFT".into(),
        "right.control" => "RCTRL".into(),
        "left.control" => "LCTRL".into(),
        "right.alt" => "RALT".into(),
        "left.alt" => "LALT".into(),
        "grave.accent" => "`".into(),
        "space" => "SPACE".into(),
        "tab" => "TAB".into(),
        "enter" => "ENTER".into(),
        "escape" => "ESC".into(),
        "backspace" => "BKSP".into(),
        "delete" => "DEL".into(),
        "insert" => "INS".into(),
        "caps.lock" => "CAPS".into(),
        other => other.replace('.', "_").to_ascii_uppercase(),
    }
}
