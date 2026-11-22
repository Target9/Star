//! Meteor-inspired ClickGUI rendered on Horizon's mod screen (`host.mod_screen_open`).
//!
//! Layout features:
//! - Top bar with live search box, Reset Layout button, and quick controls legend
//! - Draggable, collapsible, scrollable Category windows (Combat, Player, Movement, Render, Client)
//! - Left-click module to toggle, Right-click to expand settings, Middle-click to bind hotkey
//! - Draggable Float/Int sliders (with scroll-wheel fine adjustment and Right-click reset)
//! - Expandable RGBA Color pickers, Mode cyclers, Keybind listeners, and Hover Tooltips

use mod_api::{ClientHost, DrawContext};

use crate::module::{Category, ModuleId, ModuleManager};
use crate::settings::{SettingKind, format_key_name};
use crate::theme::{Theme, argb_parts, blend_rgb, lighten, pack_argb, with_alpha};

const TOP_BAR_H: f32 = 24.0;
const COLOR_DRAWER_H: f32 = 52.0;

#[derive(Clone, Copy, Debug)]
enum ActiveDrag {
    Panel {
        cat: Category,
        offset_x: f32,
        offset_y: f32,
    },
    Slider {
        mod_idx: usize,
        set_idx: usize,
        track_x: f32,
        track_w: f32,
    },
    ColorChannel {
        mod_idx: usize,
        set_idx: usize,
        channel: u8, // 0=R, 1=G, 2=B, 3=A
        track_x: f32,
        track_w: f32,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum KeybindTarget {
    Module(usize),
    Setting { mod_idx: usize, set_idx: usize },
}

#[derive(Clone, Debug)]
enum HitAction {
    SearchBox,
    ClearSearch,
    ResetLayout,
    PanelHeader(Category),
    PanelBody(Category),
    ModuleRow(usize),
    SettingBool { mod_idx: usize, set_idx: usize },
    SettingSlider {
        mod_idx: usize,
        set_idx: usize,
        track_x: f32,
        track_w: f32,
    },
    SettingMode { mod_idx: usize, set_idx: usize },
    SettingKeybind { mod_idx: usize, set_idx: usize },
    SettingColorHeader { mod_idx: usize, set_idx: usize },
    SettingColorChannel {
        mod_idx: usize,
        set_idx: usize,
        channel: u8,
        track_x: f32,
        track_w: f32,
    },
}

#[derive(Clone, Debug)]
struct HitItem {
    rect: [f32; 4],
    clip: Option<[f32; 4]>,
    action: HitAction,
    tooltip: Option<&'static str>,
}

impl HitItem {
    fn contains(&self, pt: [f32; 2]) -> bool {
        if let Some(c) = self.clip
            && !rect_contains(c, pt)
        {
            return false;
        }
        rect_contains(self.rect, pt)
    }
}

/// Interactive runtime state for the ClickGUI screen.
pub struct ClickGuiState {
    pub panel_order: [Category; 5],
    pub search: String,
    pub search_focused: bool,
    drag: Option<ActiveDrag>,
    listening: Option<KeybindTarget>,
    hits: Vec<HitItem>,
    last_screen_size: [f32; 2],
    laid_out_once: bool,
}

impl Default for ClickGuiState {
    fn default() -> Self {
        Self {
            panel_order: Category::ALL,
            search: String::new(),
            search_focused: false,
            drag: None,
            listening: None,
            hits: Vec::with_capacity(128),
            last_screen_size: [640.0, 360.0],
            laid_out_once: false,
        }
    }
}

impl ClickGuiState {
    pub fn on_close(&mut self) {
        self.drag = None;
        self.listening = None;
        self.search_focused = false;
    }

    pub fn is_listening(&self) -> bool {
        self.listening.is_some()
    }

    fn bring_to_front(&mut self, cat: Category) {
        if let Some(pos) = self.panel_order.iter().position(|c| *c == cat) {
            let item = self.panel_order[pos];
            for i in pos..self.panel_order.len() - 1 {
                self.panel_order[i] = self.panel_order[i + 1];
            }
            let last = self.panel_order.len() - 1;
            self.panel_order[last] = item;
        }
    }

    /// Renders the entire ClickGUI and refreshes hit-test regions for this frame.
    pub fn render(
        &mut self,
        host: &dyn ClientHost,
        draw: &mut dyn DrawContext,
        mouse: [f32; 2],
        manager: &mut ModuleManager,
    ) {
        let size = draw.size();
        self.last_screen_size = size;
        let theme = Theme::from_manager(manager, host.millis());

        if !self.laid_out_once && !manager.config_loaded {
            manager.reset_panel_layout(size[0]);
            manager.dirty = false;
        }
        self.laid_out_once = true;

        if theme.blur_background {
            let _ = host.apply_screen_blur();
        }
        if (theme.backdrop_dim >> 24) > 0 {
            draw.fill([0.0, 0.0, size[0], size[1]], theme.backdrop_dim);
        }

        self.hits.clear();

        // Draw panels back-to-front so frontmost panel draws on top.
        let order = self.panel_order;
        for cat in order {
            self.draw_category_panel(draw, size, mouse, manager, &theme, cat);
        }

        // Top bar drawn above panels so search and Reset Layout are always reachable.
        self.draw_top_bar(draw, size, mouse, &theme);

        // Hover tooltip drawn last so it sits above everything.
        if theme.show_descriptions
            && self.drag.is_none()
            && let Some(tip) = self.hovered_tooltip(mouse)
        {
            draw_tooltip(draw, size, mouse, tip, &theme);
        }
    }

    fn draw_top_bar(
        &mut self,
        draw: &mut dyn DrawContext,
        size: [f32; 2],
        mouse: [f32; 2],
        theme: &Theme,
    ) {
        let w = size[0];
        draw.fill_gradient(
            [0.0, 0.0, w, TOP_BAR_H],
            theme.header_bg,
            with_alpha(theme.panel_bg, 0xE8),
        );
        draw.fill([0.0, TOP_BAR_H - 1.5, w, 1.5], theme.accent);

        // Brand title on left
        let title = "STAR";
        draw.text(title, [10.0, 7.5], theme.accent, true);
        let title_w = draw.text_width(title);
        draw.text("v0.1", [14.0 + title_w, 7.5], theme.text_muted, false);

        // Centered search box
        let search_w = (w * 0.28).clamp(110.0, 220.0);
        let search_h = 15.0;
        let search_x = (w - search_w) * 0.5;
        let search_y = (TOP_BAR_H - search_h) * 0.5 - 0.5;
        let search_rect = [search_x, search_y, search_w, search_h];
        let search_hover = rect_contains(search_rect, mouse);

        let box_bg = if self.search_focused || search_hover {
            theme.module_hover
        } else {
            theme.setting_bg
        };
        let box_border = if self.search_focused {
            theme.accent
        } else {
            theme.border
        };
        draw_bordered_rect(draw, search_rect, box_bg, box_border);

        if self.search.is_empty() {
            draw.text(
                "Search modules...",
                [search_x + 5.0, search_y + 3.5],
                theme.text_muted,
                false,
            );
        } else {
            let caret = if self.search_focused { "_" } else { "" };
            let shown = format!("{}{caret}", self.search);
            let _ = draw.push_clip([search_x + 3.0, search_y, search_w - 18.0, search_h]);
            draw.text(&shown, [search_x + 5.0, search_y + 3.5], theme.text, false);
            let _ = draw.pop_clip();

            let clear_rect = [search_x + search_w - 14.0, search_y + 1.0, 13.0, search_h - 2.0];
            let clear_col = if rect_contains(clear_rect, mouse) {
                theme.accent
            } else {
                theme.text_muted
            };
            draw.text("x", [clear_rect[0] + 4.0, search_y + 3.0], clear_col, false);
            self.hits.push(HitItem {
                rect: clear_rect,
                clip: None,
                action: HitAction::ClearSearch,
                tooltip: Some("Clear search filter."),
            });
        }

        self.hits.push(HitItem {
            rect: [
                search_x,
                search_y,
                if self.search.is_empty() {
                    search_w
                } else {
                    search_w - 15.0
                },
                search_h,
            ],
            clip: None,
            action: HitAction::SearchBox,
            tooltip: Some("Type to filter modules or settings across all categories."),
        });

        // Reset Layout button on right
        let btn_label = "Reset Layout";
        let btn_w = draw.text_width(btn_label) + 12.0;
        let btn_h = 15.0;
        let btn_x = w - btn_w - 8.0;
        let btn_y = (TOP_BAR_H - btn_h) * 0.5 - 0.5;
        let btn_rect = [btn_x, btn_y, btn_w, btn_h];
        let btn_hover = rect_contains(btn_rect, mouse);
        draw_bordered_rect(
            draw,
            btn_rect,
            if btn_hover {
                theme.module_hover
            } else {
                theme.module_bg
            },
            if btn_hover { theme.accent } else { theme.border },
        );
        draw.text(btn_label, [btn_x + 6.0, btn_y + 3.5], theme.text, false);
        self.hits.push(HitItem {
            rect: btn_rect,
            clip: None,
            action: HitAction::ResetLayout,
            tooltip: Some("Arrange all category panels neatly across the screen."),
        });

        // Controls hint at bottom of screen
        let hint = if self.listening.is_some() {
            "Listening for key... Press any key to bind, ESC/Backspace to clear."
        } else {
            "LMB Toggle  |  RMB Settings / Reset Slider  |  MMB Keybind  |  Scroll Adjust  |  ESC Close"
        };
        let hint_w = draw.text_width(hint);
        let hint_x = ((w - hint_w) * 0.5).max(6.0);
        let hint_y = size[1] - 14.0;
        draw.fill(
            [hint_x - 6.0, hint_y - 2.0, hint_w + 12.0, 12.0],
            with_alpha(theme.panel_bg, 0xCC),
        );
        draw.text(
            hint,
            [hint_x, hint_y],
            if self.listening.is_some() {
                theme.accent
            } else {
                theme.text_muted
            },
            false,
        );
    }

    fn draw_category_panel(
        &mut self,
        draw: &mut dyn DrawContext,
        size: [f32; 2],
        mouse: [f32; 2],
        manager: &mut ModuleManager,
        theme: &Theme,
        cat: Category,
    ) {
        let w = theme.panel_width;
        let header_h = theme.header_height;

        // Clamp panel position to stay visible
        {
            let panel = &mut manager.panels[cat.index()];
            panel.x = panel.x.clamp(2.0, (size[0] - w - 2.0).max(2.0));
            panel.y = panel
                .y
                .clamp(TOP_BAR_H + 4.0, (size[1] - header_h - 16.0).max(TOP_BAR_H + 4.0));
        }

        let panel_state = manager.panels[cat.index()];
        let px = panel_state.x;
        let py = panel_state.y;
        let indices = manager.indices_in_category(cat, &self.search);

        // Header bar
        let header_rect = [px, py, w, header_h];
        let header_hover = rect_contains(header_rect, mouse);
        let hdr_top = if header_hover {
            lighten(theme.header_bg, 10)
        } else {
            theme.header_bg
        };
        draw.fill_gradient(header_rect, hdr_top, theme.panel_bg);
        // Top accent bar
        draw.fill([px, py, w, 2.0], theme.accent);
        // Subtle header bottom divider
        draw.fill([px, py + header_h - 1.0, w, 1.0], theme.border);

        // Category badge pill
        let badge_rect = [px + 5.0, py + (header_h - 11.0) * 0.5 + 0.5, 11.0, 11.0];
        draw.fill(badge_rect, theme.accent_soft);
        draw.text(
            cat.badge(),
            [badge_rect[0] + 3.0, badge_rect[1] + 1.5],
            theme.accent,
            false,
        );

        let text_y = py + (header_h - 8.0) * 0.5 + 0.5;
        draw.text(cat.name(), [px + 20.0, text_y], theme.text, true);

        let right_label = format!(
            "{} {}",
            indices.len(),
            if panel_state.collapsed { "+" } else { "-" }
        );
        let rl_w = draw.text_width(&right_label);
        draw.text(&right_label, [px + w - rl_w - 6.0, text_y], theme.text_muted, false);

        self.hits.push(HitItem {
            rect: header_rect,
            clip: None,
            action: HitAction::PanelHeader(cat),
            tooltip: Some("Drag to move panel. Right-click to collapse or expand."),
        });

        if panel_state.collapsed {
            draw_outline(draw, header_rect, theme.border);
            return;
        }

        // Measure total content height
        let content_h = self.measure_panel_content(manager, &indices, theme);
        let max_body_h = (size[1] - (py + header_h) - 22.0).max(40.0);
        let body_h = content_h.min(max_body_h).max(theme.row_height);
        let max_scroll = (content_h - body_h).max(0.0);

        let scroll = panel_state.scroll.clamp(0.0, max_scroll);
        manager.panels[cat.index()].scroll = scroll;

        let body_rect = [px, py + header_h, w, body_h];
        draw.fill(body_rect, theme.panel_bg);

        self.hits.push(HitItem {
            rect: body_rect,
            clip: None,
            action: HitAction::PanelBody(cat),
            tooltip: None,
        });

        let _ = draw.push_clip(body_rect);

        if indices.is_empty() {
            let empty_msg = if self.search.is_empty() {
                "No modules yet"
            } else {
                "No matches"
            };
            let tw = draw.text_width(empty_msg);
            draw.text(
                empty_msg,
                [
                    px + (w - tw) * 0.5,
                    py + header_h + (body_h - 8.0) * 0.5,
                ],
                theme.text_muted,
                false,
            );
        } else {
            let mut cy = py + header_h - scroll;
            let clip = Some(body_rect);
            for &mod_idx in &indices {
                cy = self.draw_module_block(
                    draw,
                    mouse,
                    manager,
                    theme,
                    mod_idx,
                    px,
                    cy,
                    w,
                    clip,
                );
            }
        }

        // Thin scrollbar indicator when overflowing
        if max_scroll > 0.5 {
            let bar_h = (body_h * (body_h / content_h)).clamp(14.0, body_h);
            let bar_y = py + header_h + (scroll / max_scroll) * (body_h - bar_h);
            draw.fill([px + w - 2.5, bar_y, 2.0, bar_h], theme.accent);
        }

        let _ = draw.pop_clip();

        // Outer frame border around header + body
        draw_outline(draw, [px, py, w, header_h + body_h], theme.border);
    }

    fn measure_panel_content(
        &self,
        manager: &ModuleManager,
        indices: &[usize],
        theme: &Theme,
    ) -> f32 {
        if indices.is_empty() {
            return theme.row_height + 4.0;
        }
        let mut h = 0.0;
        for &mod_idx in indices {
            h += theme.row_height;
            let module = &manager.modules[mod_idx];
            if module.expanded {
                for setting in &module.settings {
                    if setting.group.is_some() {
                        h += 12.0;
                    }
                    h += theme.setting_height;
                    if let SettingKind::Color { expanded: true, .. } = setting.kind {
                        h += COLOR_DRAWER_H;
                    }
                }
                h += 3.0;
            }
        }
        h
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_module_block(
        &mut self,
        draw: &mut dyn DrawContext,
        mouse: [f32; 2],
        manager: &ModuleManager,
        theme: &Theme,
        mod_idx: usize,
        x: f32,
        mut y: f32,
        w: f32,
        clip: Option<[f32; 4]>,
    ) -> f32 {
        let module = &manager.modules[mod_idx];
        let row_h = theme.row_height;
        let row_rect = [x, y, w, row_h];
        let hovered = clip.is_none_or(|c| rect_contains(c, mouse)) && rect_contains(row_rect, mouse);

        let is_config_only = module.id == ModuleId::ClickGui;
        let row_bg = if module.enabled && !is_config_only {
            if hovered {
                blend_rgb(theme.module_hover, theme.accent, 0.30)
            } else {
                theme.module_enabled_bg
            }
        } else if hovered {
            theme.module_hover
        } else {
            theme.module_bg
        };
        draw.fill(row_rect, row_bg);

        // Left active indicator bar
        if module.enabled && !is_config_only {
            draw.fill([x, y, 2.5, row_h], theme.accent);
        } else if is_config_only {
            draw.fill([x, y, 2.5, row_h], with_alpha(theme.accent, 0x80));
        }

        // Bottom subtle separator
        draw.fill(
            [x + 2.0, y + row_h - 1.0, w - 4.0, 1.0],
            with_alpha(theme.border, 0x45),
        );

        let text_y = y + (row_h - 8.0) * 0.5 + 0.5;
        let listening_this = self.listening == Some(KeybindTarget::Module(mod_idx));

        let label = if listening_this {
            "Press key..."
        } else {
            module.name
        };
        let label_col = if listening_this {
            theme.accent
        } else if module.enabled && !is_config_only {
            theme.text
        } else {
            theme.text_muted
        };
        draw.text(label, [x + 7.0, text_y], label_col, module.enabled && !is_config_only);

        // Right-side expand chevron and optional keybind tag
        let chevron = if module.expanded { "v" } else { ">" };
        let chev_x = x + w - 10.0;
        draw.text(
            chevron,
            [chev_x, text_y],
            if module.expanded {
                theme.accent
            } else {
                theme.text_muted
            },
            false,
        );

        if theme.show_keybinds
            && !listening_this
            && let Some(bind) = module.keybind.as_deref()
        {
            let bind_txt = format!("[{}]", format_key_name(Some(bind)));
            let bw = draw.text_width(&bind_txt);
            draw.text(
                &bind_txt,
                [chev_x - bw - 4.0, text_y],
                with_alpha(theme.accent, 0xD0),
                false,
            );
        }

        self.hits.push(HitItem {
            rect: row_rect,
            clip,
            action: HitAction::ModuleRow(mod_idx),
            tooltip: Some(module.description),
        });

        y += row_h;

        if !module.expanded {
            return y;
        }

        // Settings drawer background
        let drawer_start = y;
        let set_h = theme.setting_height;
        let pad_x = 6.0;
        let inner_x = x + pad_x;
        let inner_w = w - pad_x - 4.0;

        for (set_idx, setting) in module.settings.iter().enumerate() {
            if let Some(group) = setting.group {
                let grp_rect = [x, y, w, 12.0];
                draw.fill(grp_rect, theme.setting_bg);
                draw.fill([x, y, 2.0, 12.0], with_alpha(theme.accent, 0x55));
                draw.text(group, [inner_x, y + 2.5], theme.accent, false);
                let gw = draw.text_width(group);
                let line_x = inner_x + gw + 4.0;
                if line_x < x + w - 6.0 {
                    draw.fill(
                        [line_x, y + 6.5, (x + w - 6.0) - line_x, 1.0],
                        with_alpha(theme.accent, 0x45),
                    );
                }
                y += 12.0;
            }

            let row_rect = [x, y, w, set_h];
            let s_hover =
                clip.is_none_or(|c| rect_contains(c, mouse)) && rect_contains(row_rect, mouse);
            draw.fill(
                row_rect,
                if s_hover {
                    lighten(theme.setting_bg, 8)
                } else {
                    theme.setting_bg
                },
            );
            draw.fill([x, y, 2.0, set_h], with_alpha(theme.accent, 0x60));

            let sty = y + (set_h - 8.0) * 0.5 + 0.5;

            match &setting.kind {
                SettingKind::Bool { value, .. } => {
                    draw.text(
                        setting.name,
                        [inner_x, sty],
                        if *value { theme.text } else { theme.text_muted },
                        false,
                    );
                    let pill_w = 18.0;
                    let pill_h = 9.0;
                    let pill_x = x + w - pill_w - 6.0;
                    let pill_y = y + (set_h - pill_h) * 0.5;
                    draw.fill(
                        [pill_x, pill_y, pill_w, pill_h],
                        if *value {
                            theme.accent
                        } else {
                            theme.slider_track
                        },
                    );
                    let knob_x = if *value {
                        pill_x + pill_w - 8.0
                    } else {
                        pill_x + 1.0
                    };
                    draw.fill([knob_x, pill_y + 1.0, 7.0, 7.0], 0xFFFF_FFFF);

                    self.hits.push(HitItem {
                        rect: row_rect,
                        clip,
                        action: HitAction::SettingBool { mod_idx, set_idx },
                        tooltip: Some(setting.description),
                    });
                }
                SettingKind::Float { .. } | SettingKind::Int { .. } => {
                    let frac = setting.slider_fraction().unwrap_or(0.0);
                    let track_x = inner_x;
                    let track_w = inner_w;
                    let fill_rect = [track_x, y + 1.5, track_w, set_h - 3.0];
                    draw.fill(fill_rect, with_alpha(theme.slider_track, 0x70));
                    if frac > 0.001 {
                        draw.fill(
                            [track_x, y + 1.5, track_w * frac, set_h - 3.0],
                            with_alpha(theme.accent, 0xA8),
                        );
                        draw.fill(
                            [
                                (track_x + track_w * frac - 1.5).max(track_x),
                                y + 1.5,
                                1.5,
                                set_h - 3.0,
                            ],
                            theme.accent,
                        );
                    }
                    draw.text(setting.name, [track_x + 3.0, sty], theme.text, false);
                    let val_str = setting.display_value();
                    let vw = draw.text_width(&val_str);
                    draw.text(&val_str, [track_x + track_w - vw - 3.0, sty], theme.text, false);

                    self.hits.push(HitItem {
                        rect: row_rect,
                        clip,
                        action: HitAction::SettingSlider {
                            mod_idx,
                            set_idx,
                            track_x,
                            track_w,
                        },
                        tooltip: Some(setting.description),
                    });
                }
                SettingKind::Mode { .. } => {
                    draw.text(setting.name, [inner_x, sty], theme.text_muted, false);
                    let val_str = format!("< {} >", setting.display_value());
                    let vw = draw.text_width(&val_str);
                    draw.text(
                        &val_str,
                        [x + w - vw - 5.0, sty],
                        theme.accent,
                        false,
                    );
                    self.hits.push(HitItem {
                        rect: row_rect,
                        clip,
                        action: HitAction::SettingMode { mod_idx, set_idx },
                        tooltip: Some(setting.description),
                    });
                }
                SettingKind::Keybind { .. } => {
                    let listening_set =
                        self.listening == Some(KeybindTarget::Setting { mod_idx, set_idx });
                    draw.text(setting.name, [inner_x, sty], theme.text_muted, false);
                    let val_str = if listening_set {
                        "Press key...".to_owned()
                    } else {
                        setting.display_value()
                    };
                    let vw = draw.text_width(&val_str);
                    draw.text(
                        &val_str,
                        [x + w - vw - 5.0, sty],
                        if listening_set {
                            theme.accent
                        } else {
                            theme.text
                        },
                        false,
                    );
                    self.hits.push(HitItem {
                        rect: row_rect,
                        clip,
                        action: HitAction::SettingKeybind { mod_idx, set_idx },
                        tooltip: Some(setting.description),
                    });
                }
                SettingKind::Color {
                    value, expanded, ..
                } => {
                    draw.text(setting.name, [inner_x, sty], theme.text_muted, false);
                    let swatch_rect = [x + w - 20.0, y + 2.5, 14.0, set_h - 5.0];
                    draw_bordered_rect(draw, swatch_rect, *value, theme.border);

                    self.hits.push(HitItem {
                        rect: row_rect,
                        clip,
                        action: HitAction::SettingColorHeader { mod_idx, set_idx },
                        tooltip: Some(setting.description),
                    });

                    if *expanded {
                        y += set_h;
                        let drawer_rect = [x, y, w, COLOR_DRAWER_H];
                        draw.fill(drawer_rect, with_alpha(theme.setting_bg, 0xF5));
                        draw.fill([x, y, 2.0, COLOR_DRAWER_H], theme.accent);

                        let (a, r, g, b) = argb_parts(*value);
                        let channels: [(&str, u8, u32); 4] = [
                            ("R", r, 0xFFEF_5350),
                            ("G", g, 0xFF4C_AF50),
                            ("B", b, 0xFF42_A5F5),
                            ("A", a, 0xFFB0_BEC5),
                        ];
                        for (ci, (cname, cval, bar_col)) in channels.iter().enumerate() {
                            let cy = y + 2.0 + ci as f32 * 12.0;
                            let track_x = inner_x + 12.0;
                            let track_w = (inner_w - 14.0).max(24.0);
                            let ch_rect = [inner_x, cy, inner_w, 11.0];
                            draw.text(cname, [inner_x, cy + 1.5], theme.text_muted, false);
                            draw.fill([track_x, cy + 1.0, track_w, 9.0], theme.slider_track);
                            let frac = *cval as f32 / 255.0;
                            draw.fill([track_x, cy + 1.0, track_w * frac, 9.0], *bar_col);
                            let num_str = format!("{cval}");
                            let nw = draw.text_width(&num_str);
                            draw.text(
                                &num_str,
                                [track_x + track_w - nw - 2.0, cy + 1.5],
                                0xFFFF_FFFF,
                                false,
                            );
                            self.hits.push(HitItem {
                                rect: ch_rect,
                                clip,
                                action: HitAction::SettingColorChannel {
                                    mod_idx,
                                    set_idx,
                                    channel: ci as u8,
                                    track_x,
                                    track_w,
                                },
                                tooltip: Some("Drag to adjust color channel."),
                            });
                        }
                        y += COLOR_DRAWER_H - set_h;
                    }
                }
            }

            y += set_h;
        }

        // Small bottom padding strip for the open settings drawer
        draw.fill([x, y, w, 3.0], theme.setting_bg);
        draw.fill([x, y + 2.0, w, 1.0], with_alpha(theme.border, 0x80));
        let _ = drawer_start;
        y + 3.0
    }

    fn hovered_tooltip(&self, mouse: [f32; 2]) -> Option<&'static str> {
        for item in self.hits.iter().rev() {
            if item.contains(mouse)
                && let Some(tip) = item.tooltip
            {
                return Some(tip);
            }
        }
        None
    }

    pub fn mouse_pressed(
        &mut self,
        mouse: [f32; 2],
        button: i32,
        manager: &mut ModuleManager,
    ) -> bool {
        // Clicking anywhere while listening cancels keybind capture cleanly.
        if self.listening.is_some() {
            self.listening = None;
            return true;
        }

        let Some(action) = self
            .hits
            .iter()
            .rev()
            .find(|h| h.contains(mouse))
            .map(|h| h.action.clone())
        else {
            self.search_focused = false;
            return false;
        };

        self.search_focused = matches!(action, HitAction::SearchBox);

        match action {
            HitAction::SearchBox => {}
            HitAction::ClearSearch => {
                self.search.clear();
            }
            HitAction::ResetLayout => {
                if button == 0 {
                    manager.reset_panel_layout(self.last_screen_size[0]);
                }
            }
            HitAction::PanelHeader(cat) => {
                self.bring_to_front(cat);
                if button == 0 {
                    let p = manager.panels[cat.index()];
                    self.drag = Some(ActiveDrag::Panel {
                        cat,
                        offset_x: mouse[0] - p.x,
                        offset_y: mouse[1] - p.y,
                    });
                } else if button == 1 {
                    let p = &mut manager.panels[cat.index()];
                    p.collapsed = !p.collapsed;
                    manager.dirty = true;
                }
            }
            HitAction::PanelBody(cat) => {
                self.bring_to_front(cat);
            }
            HitAction::ModuleRow(mod_idx) => {
                let cat = manager.modules[mod_idx].category;
                self.bring_to_front(cat);
                let is_config_only = manager.modules[mod_idx].id == ModuleId::ClickGui;
                match button {
                    0 => {
                        if is_config_only {
                            manager.modules[mod_idx].expanded = !manager.modules[mod_idx].expanded;
                        } else {
                            manager.modules[mod_idx].enabled = !manager.modules[mod_idx].enabled;
                            manager.dirty = true;
                        }
                    }
                    1 => {
                        manager.modules[mod_idx].expanded = !manager.modules[mod_idx].expanded;
                    }
                    2 if !is_config_only => {
                        self.listening = Some(KeybindTarget::Module(mod_idx));
                    }
                    _ => {}
                }
            }
            HitAction::SettingBool { mod_idx, set_idx } => {
                let setting = &mut manager.modules[mod_idx].settings[set_idx];
                if button == 0 {
                    if let SettingKind::Bool { value, .. } = &mut setting.kind {
                        *value = !*value;
                        manager.dirty = true;
                    }
                } else if button == 1 && setting.reset() {
                    manager.dirty = true;
                }
            }
            HitAction::SettingSlider {
                mod_idx,
                set_idx,
                track_x,
                track_w,
            } => {
                let setting = &mut manager.modules[mod_idx].settings[set_idx];
                if button == 0 {
                    let frac = ((mouse[0] - track_x) / track_w.max(1.0)).clamp(0.0, 1.0);
                    if setting.set_slider_fraction(frac) {
                        manager.dirty = true;
                    }
                    self.drag = Some(ActiveDrag::Slider {
                        mod_idx,
                        set_idx,
                        track_x,
                        track_w,
                    });
                } else if button == 1 && setting.reset() {
                    manager.dirty = true;
                }
            }
            HitAction::SettingMode { mod_idx, set_idx } => {
                let setting = &mut manager.modules[mod_idx].settings[set_idx];
                if let SettingKind::Mode { index, options, .. } = &mut setting.kind
                    && !options.is_empty()
                {
                    if button == 0 {
                        *index = (*index + 1) % options.len();
                        manager.dirty = true;
                    } else if button == 1 {
                        *index = if *index == 0 {
                            options.len() - 1
                        } else {
                            *index - 1
                        };
                        manager.dirty = true;
                    }
                }
            }
            HitAction::SettingKeybind { mod_idx, set_idx } => {
                if button == 0 {
                    self.listening = Some(KeybindTarget::Setting { mod_idx, set_idx });
                } else if button == 1
                    && manager.modules[mod_idx].settings[set_idx].reset()
                {
                    manager.dirty = true;
                }
            }
            HitAction::SettingColorHeader { mod_idx, set_idx } => {
                let setting = &mut manager.modules[mod_idx].settings[set_idx];
                if button == 0 {
                    if let SettingKind::Color { expanded, .. } = &mut setting.kind {
                        *expanded = !*expanded;
                    }
                } else if button == 1 && setting.reset() {
                    manager.dirty = true;
                }
            }
            HitAction::SettingColorChannel {
                mod_idx,
                set_idx,
                channel,
                track_x,
                track_w,
            } => {
                if button == 0 {
                    update_color_channel(
                        manager, mod_idx, set_idx, channel, mouse[0], track_x, track_w,
                    );
                    self.drag = Some(ActiveDrag::ColorChannel {
                        mod_idx,
                        set_idx,
                        channel,
                        track_x,
                        track_w,
                    });
                } else if button == 1
                    && manager.modules[mod_idx].settings[set_idx].reset()
                {
                    manager.dirty = true;
                }
            }
        }

        true
    }

    pub fn mouse_dragged(&mut self, mouse: [f32; 2], manager: &mut ModuleManager) -> bool {
        let Some(drag) = self.drag else {
            return false;
        };
        match drag {
            ActiveDrag::Panel {
                cat,
                offset_x,
                offset_y,
            } => {
                let panel = &mut manager.panels[cat.index()];
                panel.x = (mouse[0] - offset_x).max(2.0);
                panel.y = (mouse[1] - offset_y).max(TOP_BAR_H + 4.0);
                manager.dirty = true;
            }
            ActiveDrag::Slider {
                mod_idx,
                set_idx,
                track_x,
                track_w,
            } => {
                let frac = ((mouse[0] - track_x) / track_w.max(1.0)).clamp(0.0, 1.0);
                if manager.modules[mod_idx].settings[set_idx].set_slider_fraction(frac) {
                    manager.dirty = true;
                }
            }
            ActiveDrag::ColorChannel {
                mod_idx,
                set_idx,
                channel,
                track_x,
                track_w,
            } => {
                update_color_channel(
                    manager, mod_idx, set_idx, channel, mouse[0], track_x, track_w,
                );
            }
        }
        true
    }

    pub fn mouse_released(&mut self) -> bool {
        self.drag.take().is_some()
    }

    pub fn mouse_scrolled(
        &mut self,
        mouse: [f32; 2],
        scroll_y: f64,
        modifiers: u32,
        manager: &mut ModuleManager,
    ) -> bool {
        if scroll_y.abs() < 1e-4 {
            return false;
        }
        let dir = if scroll_y > 0.0 { 1 } else { -1 };

        // First check if hovering directly over a slider to fine-adjust it.
        for item in self.hits.iter().rev() {
            if !item.contains(mouse) {
                continue;
            }
            match item.action {
                HitAction::SettingSlider {
                    mod_idx, set_idx, ..
                } => {
                    let mult = if modifiers & 1 != 0 { 5 } else { 1 };
                    if manager.modules[mod_idx].settings[set_idx].nudge(dir * mult) {
                        manager.dirty = true;
                    }
                    return true;
                }
                HitAction::PanelBody(cat) => {
                    let p = &mut manager.panels[cat.index()];
                    p.scroll = (p.scroll - (scroll_y as f32) * 18.0).max(0.0);
                    return true;
                }
                _ => {}
            }
        }
        false
    }

    /// Handles a key press while the ClickGUI screen is open.
    /// Returns `true` if the press was consumed (preventing Escape-close).
    pub fn key_pressed(&mut self, key: &str, manager: &mut ModuleManager) -> bool {
        if let Some(target) = self.listening.take() {
            let clear = matches!(
                key,
                "key.keyboard.escape" | "key.keyboard.backspace" | "key.keyboard.delete"
            );
            let new_key = if clear { None } else { Some(key.to_owned()) };
            match target {
                KeybindTarget::Module(mod_idx) => {
                    manager.modules[mod_idx].keybind = new_key;
                    manager.dirty = true;
                }
                KeybindTarget::Setting { mod_idx, set_idx } => {
                    if let SettingKind::Keybind { key: slot, .. } =
                        &mut manager.modules[mod_idx].settings[set_idx].kind
                    {
                        *slot = new_key;
                        manager.dirty = true;
                    }
                }
            }
            return true;
        }

        if key == "key.keyboard.backspace" && !self.search.is_empty() {
            self.search.pop();
            return true;
        }

        if key == "key.keyboard.escape" && !self.search.is_empty() {
            self.search.clear();
            self.search_focused = false;
            return true;
        }

        // Let Escape close the mod screen cleanly.
        false
    }

    pub fn char_typed(&mut self, c: char) -> bool {
        if self.listening.is_some() {
            return true;
        }
        if !c.is_control() && self.search.len() < 40 {
            self.search.push(c);
            self.search_focused = true;
            return true;
        }
        false
    }
}

fn update_color_channel(
    manager: &mut ModuleManager,
    mod_idx: usize,
    set_idx: usize,
    channel: u8,
    mouse_x: f32,
    track_x: f32,
    track_w: f32,
) {
    let frac = ((mouse_x - track_x) / track_w.max(1.0)).clamp(0.0, 1.0);
    let val = (frac * 255.0).round().clamp(0.0, 255.0) as u8;
    if let SettingKind::Color { value, .. } = &mut manager.modules[mod_idx].settings[set_idx].kind {
        let (mut a, mut r, mut g, mut b) = argb_parts(*value);
        match channel {
            0 => r = val,
            1 => g = val,
            2 => b = val,
            _ => a = val,
        }
        let next = pack_argb(a, r, g, b);
        if next != *value {
            *value = next;
            manager.dirty = true;
        }
    }
    // Automatically switch theme preset to Custom when editing a ClickGUI custom color.
    if manager.modules[mod_idx].id == ModuleId::ClickGui
        && let Some(preset) = manager.modules[mod_idx].setting_mut("preset")
        && let SettingKind::Mode { index, options, .. } = &mut preset.kind
    {
        *index = options.len().saturating_sub(1);
    }
}

fn draw_tooltip(
    draw: &mut dyn DrawContext,
    size: [f32; 2],
    mouse: [f32; 2],
    text: &str,
    theme: &Theme,
) {
    let tw = draw.text_width(text);
    let pad = 5.0;
    let w = tw + pad * 2.0;
    let h = 15.0;
    let mut x = mouse[0] + 10.0;
    let mut y = mouse[1] + 12.0;
    if x + w > size[0] - 4.0 {
        x = (size[0] - w - 4.0).max(4.0);
    }
    if y + h > size[1] - 18.0 {
        y = (mouse[1] - h - 6.0).max(TOP_BAR_H + 4.0);
    }
    let rect = [x, y, w, h];
    draw_bordered_rect(draw, rect, with_alpha(theme.header_bg, 0xF5), theme.accent);
    draw.text(text, [x + pad, y + 3.5], theme.text, false);
}

fn draw_bordered_rect(draw: &mut dyn DrawContext, rect: [f32; 4], bg: u32, border: u32) {
    draw.fill(rect, bg);
    draw_outline(draw, rect, border);
}

fn draw_outline(draw: &mut dyn DrawContext, [x, y, w, h]: [f32; 4], color: u32) {
    draw.fill([x, y, w, 1.0], color);
    draw.fill([x, y + h - 1.0, w, 1.0], color);
    draw.fill([x, y, 1.0, h], color);
    draw.fill([x + w - 1.0, y, 1.0, h], color);
}

fn rect_contains([x, y, w, h]: [f32; 4], [px, py]: [f32; 2]) -> bool {
    px >= x && px <= x + w && py >= y && py <= y + h
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_and_keybind_listening_work() {
        let mut mgr = ModuleManager::new();
        let mut gui = ClickGuiState::default();

        assert!(gui.char_typed('a'));
        assert!(gui.char_typed('i'));
        assert!(gui.char_typed('m'));
        assert_eq!(gui.search, "aim");
        assert_eq!(mgr.indices_in_category(Category::Combat, &gui.search).len(), 1);
        assert_eq!(mgr.indices_in_category(Category::Player, &gui.search).len(), 0);

        // Escape clears non-empty search without closing screen.
        assert!(gui.key_pressed("key.keyboard.escape", &mut mgr));
        assert!(gui.search.is_empty());
        // Second Escape lets the game close the screen.
        assert!(!gui.key_pressed("key.keyboard.escape", &mut mgr));

        // Simulate middle-click keybind capture on Aim Assist (index 0).
        gui.listening = Some(KeybindTarget::Module(0));
        assert!(gui.key_pressed("key.keyboard.r", &mut mgr));
        assert_eq!(mgr.get(ModuleId::AimAssist).keybind.as_deref(), Some("key.keyboard.r"));
        assert!(!gui.is_listening());

        // Triggering the hotkey toggles the module.
        assert!(!mgr.is_enabled(ModuleId::AimAssist));
        assert_eq!(mgr.trigger_keybind("key.keyboard.r"), 1);
        assert!(mgr.is_enabled(ModuleId::AimAssist));
    }
}
