//! Meteor-inspired ClickGUI rendered on Horizon's mod screen (`host.mod_screen_open("star")`).
//!
//! Features:
//! - Top navigation tabs: `Modules` | `Settings` | `HUD`
//! - Draggable, collapsible, scrollable category panels on the `Modules` tab
//! - Dedicated centered settings cards for Client Settings (Theme, Font, Layout, Colors) & HUD
//! - Reusable floating Entity Selector modal window for exact entity-type filtering
//! - Live search bar (clears placeholder text immediately while focused/typing)
//! - Inline setting drawers, RGBA color pickers, keybind capture, and hover tooltips

use mod_api::{ClientHost, DrawContext};

use crate::entity_filter::{self, EntityGroup};
use crate::font::{FontConfig, FontFace};
use crate::module::{Category, ModuleId, ModuleManager};
use crate::settings::{SettingKind, format_key_name};
use crate::theme::{Theme, with_alpha};

const TOP_BAR_H: f32 = 24.0;
const HEADER_H: f32 = 19.0;
const FOOTER_H: f32 = 16.0;
const SETTING_H: f32 = 16.0;
const GROUP_H: f32 = 14.0;
const COLOR_CHANNEL_H: f32 = 13.0;
const COLOR_DRAWER_H: f32 = COLOR_CHANNEL_H * 4.0 + 4.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GuiTab {
    Modules,
    Settings,
    Hud,
}

impl GuiTab {
    const ALL: [GuiTab; 3] = [GuiTab::Modules, GuiTab::Settings, GuiTab::Hud];

    fn label(self) -> &'static str {
        match self {
            Self::Modules => "Modules",
            Self::Settings => "Settings",
            Self::Hud => "HUD",
        }
    }
}

/// Interactive state for the floating Entity Selector modal window.
#[derive(Clone, Debug)]
struct EntityPickerState {
    mod_idx: usize,
    set_idx: usize,
    search: String,
    search_focused: bool,
    scroll: f32,
    /// `None` = All, `Some(None)` = Selected only, `Some(Some(g))` = Group filter
    filter: PickerFilter,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PickerFilter {
    All,
    Selected,
    Group(EntityGroup),
}

/// Interactive state for an open floating dropdown popover (Font, Theme, Mode settings).
#[derive(Clone, Copy, Debug)]
struct DropdownState {
    mod_idx: usize,
    set_idx: usize,
    anchor: [f32; 4],
    scroll: f32,
}

/// Persistent interactive state for the ClickGUI while the mod screen is open.
#[derive(Debug)]
pub struct ClickGuiState {
    pub active_tab: GuiTab,
    pub search: String,
    pub search_focused: bool,
    layout_initialized: bool,
    screen_size: [f32; 2],
    panel_order: Vec<Category>,
    dragging: Option<DragTarget>,
    listening_bind: Option<BindTarget>,
    entity_picker: Option<EntityPickerState>,
    open_dropdown: Option<DropdownState>,
    drawer_anim: Vec<f32>,
    last_frame_ms: Option<u64>,
    settings_scroll: f32,
    hud_scroll: f32,
    hits: Vec<HitRegion>,
    panel_rects: Vec<(Category, [f32; 4], f32)>,
    card_scroll_rect: Option<([f32; 4], f32)>,
    picker_list_rect: Option<([f32; 4], f32)>,
    picker_window_rect: Option<[f32; 4]>,
    dropdown_list_rect: Option<([f32; 4], f32)>,
    hovered_tooltip: Option<String>,
}

impl Default for ClickGuiState {
    fn default() -> Self {
        Self {
            active_tab: GuiTab::Modules,
            search: String::new(),
            search_focused: false,
            layout_initialized: false,
            screen_size: [640.0, 360.0],
            panel_order: Category::MODULE_PANELS.to_vec(),
            dragging: None,
            listening_bind: None,
            entity_picker: None,
            open_dropdown: None,
            drawer_anim: Vec::new(),
            last_frame_ms: None,
            settings_scroll: 0.0,
            hud_scroll: 0.0,
            hits: Vec::new(),
            panel_rects: Vec::new(),
            card_scroll_rect: None,
            picker_list_rect: None,
            picker_window_rect: None,
            dropdown_list_rect: None,
            hovered_tooltip: None,
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum DragTarget {
    Panel {
        category: Category,
        offset_x: f32,
        offset_y: f32,
    },
    Slider {
        mod_idx: usize,
        set_idx: usize,
        x: f32,
        w: f32,
    },
    ColorChannel {
        mod_idx: usize,
        set_idx: usize,
        channel: u8,
        x: f32,
        w: f32,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BindTarget {
    Module(usize),
    Setting { mod_idx: usize, set_idx: usize },
}

#[derive(Clone, Debug)]
struct HitRegion {
    rect: [f32; 4],
    kind: HitKind,
    tooltip: Option<String>,
}

#[derive(Clone, Debug)]
enum HitKind {
    Tab(GuiTab),
    SearchBar,
    SearchClear,
    ResetLayout,
    PanelHeader(Category),
    PanelCollapse(Category),
    ModuleRow(usize),
    ModuleBind(usize),
    ToggleModuleEnabled(usize),
    SettingBool { mod_idx: usize, set_idx: usize },
    SettingSlider { mod_idx: usize, set_idx: usize },
    SettingMode { mod_idx: usize, set_idx: usize, anchor: [f32; 4] },
    DropdownOption { mod_idx: usize, set_idx: usize, option_idx: usize },
    SettingColorHeader { mod_idx: usize, set_idx: usize },
    SettingColorChannel { mod_idx: usize, set_idx: usize, channel: u8 },
    SettingKeybind { mod_idx: usize, set_idx: usize },
    SettingEntityFilter { mod_idx: usize, set_idx: usize },
    // Entity picker modal hits
    PickerClose,
    PickerSearchBar,
    PickerSearchClear,
    PickerSelectAll,
    PickerSelectNone,
    PickerToggleGroup(EntityGroup),
    PickerFilterTab(PickerFilter),
    PickerEntityToggle(String),
}

impl ClickGuiState {
    pub fn is_listening(&self) -> bool {
        self.listening_bind.is_some()
    }

    pub fn on_close(&mut self) {
        self.dragging = None;
        self.listening_bind = None;
        self.search_focused = false;
        self.entity_picker = None;
        self.open_dropdown = None;
        self.last_frame_ms = None;
    }

    fn step_animations(&mut self, now_ms: u64, manager: &ModuleManager) {
        if self.drawer_anim.len() != manager.modules.len() {
            self.drawer_anim = manager
                .modules
                .iter()
                .map(|m| if m.expanded { 1.0 } else { 0.0 })
                .collect();
        }

        let dt = self
            .last_frame_ms
            .map_or(0.0, |prev| (now_ms.saturating_sub(prev) as f32 / 1000.0).clamp(0.0, 0.05));
        self.last_frame_ms = Some(now_ms);

        // ~140ms snappy open/close duration
        let step = dt / 0.14;
        if step <= 0.0 {
            return;
        }
        for (anim, m) in self.drawer_anim.iter_mut().zip(&manager.modules) {
            if m.expanded {
                *anim = (*anim + step).min(1.0);
            } else {
                *anim = (*anim - step).max(0.0);
            }
        }
    }

    fn drawer_ease(&self, idx: usize) -> f32 {
        let t = self.drawer_anim.get(idx).copied().unwrap_or(0.0).clamp(0.0, 1.0);
        let inv = 1.0 - t;
        1.0 - inv * inv * inv
    }

    /// Renders the entire ClickGUI and rebuilds interactive hit regions for the frame.
    pub fn render(
        &mut self,
        host: &dyn ClientHost,
        draw: &mut dyn DrawContext,
        mouse: [f32; 2],
        manager: &mut ModuleManager,
    ) {
        let size = draw.size();
        self.screen_size = size;

        if !self.layout_initialized {
            let needs_default = Category::MODULE_PANELS
                .iter()
                .all(|c| (manager.panels[c.index()].y - 38.0).abs() < 0.5);
            if needs_default {
                manager.reset_panel_layout(size[0]);
                manager.dirty = false;
            }
            self.layout_initialized = true;
        }

        let now_ms = host.millis();
        self.step_animations(now_ms, manager);

        let theme = Theme::from_manager(manager, now_ms);
        let font = FontConfig::from_manager(manager);
        let gui_mod = manager.get(ModuleId::ClickGui);
        let use_blur = gui_mod.bool("blur", true);
        let backdrop_dim = gui_mod.float("backdrop_dim", 0.45) as f32;
        let show_tooltips = gui_mod.bool("descriptions", true);

        if use_blur {
            let _ = host.apply_screen_blur();
        }
        if backdrop_dim > 0.005 {
            let alpha = (backdrop_dim.clamp(0.0, 0.92) * 255.0).round() as u32;
            draw.fill([0.0, 0.0, size[0], size[1]], alpha << 24);
        }

        self.hits.clear();
        self.panel_rects.clear();
        self.card_scroll_rect = None;
        self.picker_list_rect = None;
        self.picker_window_rect = None;
        self.dropdown_list_rect = None;
        self.hovered_tooltip = None;

        // When a modal or dropdown popover is open, suppress hover on background elements.
        let bg_mouse = if self.entity_picker.is_some() || self.open_dropdown.is_some() {
            [-9999.0, -9999.0]
        } else {
            mouse
        };

        self.render_top_bar(draw, size, bg_mouse, &theme, font);

        match self.active_tab {
            GuiTab::Modules => {
                let order = self.panel_order.clone();
                for category in order {
                    self.render_panel(draw, category, size, bg_mouse, manager, &theme, font);
                }
            }
            GuiTab::Settings => {
                let idx = manager.index_of(ModuleId::ClickGui);
                self.render_settings_card(
                    draw,
                    idx,
                    "Client Settings",
                    "Customize Star's font, theme, colors, layout, and controls.",
                    false,
                    size,
                    bg_mouse,
                    manager,
                    &theme,
                    font,
                );
            }
            GuiTab::Hud => {
                let idx = manager.index_of(ModuleId::Hud);
                self.render_settings_card(
                    draw,
                    idx,
                    "HUD Overlay",
                    "Configure watermark, coordinates, active module list, and opacity.",
                    true,
                    size,
                    bg_mouse,
                    manager,
                    &theme,
                    font,
                );
            }
        }

        self.render_footer(draw, size, &theme, font);

        if self.entity_picker.is_some() {
            // Clear background hits so modal captures all interactions cleanly.
            self.hits.clear();
            self.render_entity_picker(draw, size, mouse, manager, &theme, font);
        } else if self.open_dropdown.is_some() {
            self.render_dropdown(draw, size, mouse, manager, &theme, font);
        } else if show_tooltips {
            let tip = self.hovered_tooltip.clone().or_else(|| {
                self.hits
                    .iter()
                    .rev()
                    .find(|h| rect_contains(h.rect, mouse))
                    .and_then(|h| h.tooltip.clone())
            });
            if let Some(tip) = tip {
                self.render_tooltip(draw, size, mouse, &tip, &theme, font);
            }
        }
    }

    fn render_top_bar(
        &mut self,
        draw: &mut dyn DrawContext,
        size: [f32; 2],
        mouse: [f32; 2],
        theme: &Theme,
        font: FontConfig,
    ) {
        let w = size[0];
        draw.fill_gradient(
            [0.0, 0.0, w, TOP_BAR_H],
            theme.header_bg,
            with_alpha(theme.panel_bg, 0xEC),
        );
        draw.fill([0.0, TOP_BAR_H - 1.2, w, 1.2], with_alpha(theme.accent, 0xCC));

        // Brand title on left
        let title = "STAR";
        let ver = "v0.2.6";
        let ty = font.center_y(0.0, TOP_BAR_H - 1.0);
        font.draw_header(draw, title, [10.0, ty], theme.accent);
        let title_w = font.width(draw, title, font.bold_headers);
        font.draw_body(draw, ver, [15.0 + title_w, ty], theme.text_muted);

        // Top navigation tabs directly in the top bar: Modules | Settings | HUD
        let mut tab_x = 15.0 + title_w + font.width(draw, ver, false) + 14.0;
        let tab_h = TOP_BAR_H - 5.0;
        let tab_y = 2.5;
        for tab in GuiTab::ALL {
            let label = tab.label();
            let active = self.active_tab == tab;
            let lw = font.width(draw, label, active && font.bold_headers);
            let tab_w = (lw + 14.0).max(46.0);
            let rect = [tab_x, tab_y, tab_w, tab_h];
            let hover = rect_contains(rect, mouse);

            if active {
                draw.fill(rect, with_alpha(theme.accent, 0x36));
                draw.fill([tab_x, TOP_BAR_H - 2.0, tab_w, 2.0], theme.accent);
            } else if hover {
                draw.fill(rect, with_alpha(theme.module_hover, 0xCC));
            }

            let text_x = tab_x + (tab_w - lw) * 0.5;
            let text_y = font.center_y(tab_y, tab_h);
            if active {
                font.draw_header(draw, label, [text_x, text_y], theme.text);
            } else {
                font.draw_body(
                    draw,
                    label,
                    [text_x, text_y],
                    if hover { theme.text } else { theme.text_muted },
                );
            }

            self.hits.push(HitRegion {
                rect,
                kind: HitKind::Tab(tab),
                tooltip: None,
            });
            tab_x += tab_w + 4.0;
        }

        // Modules tab controls on right: Search box + Reset Layout
        if self.active_tab == GuiTab::Modules {
            let reset_label = "Reset Layout";
            let reset_text_w = font.width(draw, reset_label, false);
            let reset_w = reset_text_w + 14.0;
            let reset_h = 15.0;
            let reset_x = w - reset_w - 8.0;
            let reset_y = (TOP_BAR_H - reset_h) * 0.5 - 0.5;
            let reset_rect = [reset_x, reset_y, reset_w, reset_h];
            let reset_hover = rect_contains(reset_rect, mouse);
            draw.fill(
                reset_rect,
                if reset_hover {
                    theme.module_hover
                } else {
                    theme.setting_bg
                },
            );
            draw_outline(draw, reset_rect, theme.border);
            font.draw_body(
                draw,
                reset_label,
                [
                    reset_x + (reset_w - reset_text_w) * 0.5,
                    font.center_y(reset_y, reset_h),
                ],
                if reset_hover {
                    theme.text
                } else {
                    theme.text_muted
                },
            );
            self.hits.push(HitRegion {
                rect: reset_rect,
                kind: HitKind::ResetLayout,
                tooltip: Some("Reset all category panels to their default positions.".into()),
            });

            // Search box to the left of Reset Layout (clamped so it never overlaps tabs)
            let avail_right = reset_x - 8.0;
            let avail_w = (avail_right - (tab_x + 8.0)).max(0.0);
            if avail_w >= 72.0 {
                let search_w = avail_w.min(185.0);
                let search_h = 15.0;
                let search_x = avail_right - search_w;
                let search_y = (TOP_BAR_H - search_h) * 0.5 - 0.5;
                let search_rect = [search_x, search_y, search_w, search_h];
                let search_hover = rect_contains(search_rect, mouse);

                let box_bg = if self.search_focused || search_hover {
                    theme.module_hover
                } else {
                    theme.setting_bg
                };
                draw.fill(search_rect, box_bg);
                draw_outline(
                    draw,
                    search_rect,
                    if self.search_focused {
                        theme.accent
                    } else {
                        theme.border
                    },
                );

                // Clear button when search has text
                let clear_space = if !self.search.is_empty() { 14.0 } else { 0.0 };
                let text_max_w = search_w - 10.0 - clear_space;
                let text_y = font.center_y(search_y, search_h);

                // Only show placeholder when unfocused AND empty!
                if !self.search_focused && self.search.is_empty() {
                    font.draw_fitted(
                        draw,
                        "Search modules...",
                        [search_x + 5.0, text_y],
                        text_max_w,
                        theme.text_muted,
                        false,
                        false,
                    );
                } else {
                    let display = if self.search_focused {
                        format!("{}|", self.search)
                    } else {
                        self.search.clone()
                    };
                    font.draw_fitted(
                        draw,
                        &display,
                        [search_x + 5.0, text_y],
                        text_max_w,
                        theme.text,
                        false,
                        false,
                    );
                }

                self.hits.push(HitRegion {
                    rect: search_rect,
                    kind: HitKind::SearchBar,
                    tooltip: None,
                });

                if !self.search.is_empty() {
                    let clr_rect = [search_x + search_w - 14.0, search_y, 14.0, search_h];
                    let clr_hover = rect_contains(clr_rect, mouse);
                    font.draw_body(
                        draw,
                        "x",
                        [clr_rect[0] + 4.5, text_y],
                        if clr_hover {
                            theme.accent
                        } else {
                            theme.text_muted
                        },
                    );
                    self.hits.push(HitRegion {
                        rect: clr_rect,
                        kind: HitKind::SearchClear,
                        tooltip: Some("Clear search".into()),
                    });
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn render_panel(
        &mut self,
        draw: &mut dyn DrawContext,
        category: Category,
        size: [f32; 2],
        mouse: [f32; 2],
        manager: &mut ModuleManager,
        theme: &Theme,
        font: FontConfig,
    ) {
        let gui_mod = manager.get(ModuleId::ClickGui);
        let panel_w = gui_mod.int("panel_width", 154) as f32;
        let row_h = gui_mod.int("row_height", 17) as f32;
        let show_binds = gui_mod.bool("show_keybinds", true);

        let panel = &mut manager.panels[category.index()];
        panel.x = panel.x.clamp(4.0, (size[0] - panel_w - 4.0).max(4.0));
        panel.y = panel
            .y
            .clamp(TOP_BAR_H + 4.0, (size[1] - HEADER_H - FOOTER_H - 4.0).max(TOP_BAR_H + 4.0));
        let px = panel.x;
        let py = panel.y;
        let collapsed = panel.collapsed;

        let mod_indices = manager.indices_in_category(category, &self.search);
        let content_h = if mod_indices.is_empty() {
            row_h + 6.0
        } else {
            let mut h = 0.0;
            for &idx in &mod_indices {
                h += row_h;
                let ease = self.drawer_ease(idx);
                if ease > 0.005 {
                    let m = &manager.modules[idx];
                    h += module_drawer_height(m, true) * ease;
                }
            }
            h + 2.0
        };

        let max_body_h = (size[1] - py - HEADER_H - FOOTER_H - 8.0).max(row_h);
        let body_h = if collapsed {
            0.0
        } else {
            content_h.min(max_body_h)
        };
        let max_scroll = (content_h - body_h).max(0.0);
        let scroll = {
            let p = &mut manager.panels[category.index()];
            p.scroll = p.scroll.clamp(0.0, max_scroll);
            p.scroll
        };

        let total_h = HEADER_H + body_h;
        let panel_rect = [px, py, panel_w, total_h];

        // Drop shadow
        draw.fill([px + 2.0, py + 2.0, panel_w, total_h], 0x4800_0000);

        // Header bar
        let header_rect = [px, py, panel_w, HEADER_H];
        draw.fill(header_rect, theme.header_bg);
        draw.fill([px, py, panel_w, 1.5], theme.accent);

        // Category icon pill
        let badge_rect = [px + 5.0, py + 4.0, 11.0, 11.0];
        draw.fill(badge_rect, with_alpha(theme.accent, 0x35));
        let badge = category.badge();
        let bw = font.width(draw, badge, font.bold_headers);
        font.draw_header(
            draw,
            badge,
            [
                badge_rect[0] + (badge_rect[2] - bw) * 0.5,
                font.center_y(badge_rect[1], badge_rect[3]),
            ],
            theme.accent,
        );

        let header_ty = font.center_y(py, HEADER_H);
        font.draw_header(draw, category.name(), [px + 20.0, header_ty], theme.text);

        // Module count & collapse glyph
        let count_str = format!("{}", mod_indices.len());
        let cw = font.width(draw, &count_str, false);
        font.draw_body(
            draw,
            &count_str,
            [px + panel_w - 20.0 - cw, header_ty],
            theme.text_muted,
        );

        let collapse_rect = [px + panel_w - 16.0, py, 16.0, HEADER_H];
        let collapse_hover = rect_contains(collapse_rect, mouse);
        let glyph = if collapsed { "+" } else { "-" };
        font.draw_header(
            draw,
            glyph,
            [px + panel_w - 11.0, header_ty],
            if collapse_hover {
                theme.accent
            } else {
                theme.text_muted
            },
        );

        self.hits.push(HitRegion {
            rect: [px, py, panel_w - 16.0, HEADER_H],
            kind: HitKind::PanelHeader(category),
            tooltip: None,
        });
        self.hits.push(HitRegion {
            rect: collapse_rect,
            kind: HitKind::PanelCollapse(category),
            tooltip: None,
        });

        if collapsed {
            draw_outline(draw, header_rect, theme.border);
            return;
        }

        // Panel body
        let body_rect = [px, py + HEADER_H, panel_w, body_h];
        draw.fill(body_rect, theme.panel_bg);
        self.panel_rects.push((category, body_rect, max_scroll));

        let clipped = draw.push_clip(body_rect).is_ok();
        let mut cursor_y = py + HEADER_H - scroll;

        if mod_indices.is_empty() {
            let empty_msg = if self.search.is_empty() {
                "No modules yet"
            } else {
                "No matches"
            };
            let ew = font.width(draw, empty_msg, false);
            font.draw_body(
                draw,
                empty_msg,
                [
                    px + (panel_w - ew) * 0.5,
                    font.center_y(cursor_y, row_h + 4.0),
                ],
                theme.text_muted,
            );
        } else {
            for idx in mod_indices {
                let row_rect = [px, cursor_y, panel_w, row_h];
                let row_visible = cursor_y + row_h > body_rect[1] && cursor_y < body_rect[1] + body_rect[3];
                let row_hover = row_visible && rect_contains(body_rect, mouse) && rect_contains(row_rect, mouse);

                let m = &manager.modules[idx];
                let enabled = m.enabled;
                let expanded = m.expanded;
                let mod_name = m.name;
                let mod_desc = m.description;
                let mod_bind = m.keybind.clone();

                if row_visible {
                    let row_bg = if enabled {
                        if row_hover {
                            with_alpha(theme.accent, 0x44)
                        } else {
                            theme.module_enabled_bg
                        }
                    } else if row_hover {
                        theme.module_hover
                    } else {
                        0x0000_0000
                    };
                    if (row_bg >> 24) > 0 {
                        draw.fill(row_rect, row_bg);
                    }
                    if enabled {
                        draw.fill([px, cursor_y, 2.0, row_h], theme.accent);
                    }

                    let listening_this = self.listening_bind == Some(BindTarget::Module(idx));
                    let bind_label = if listening_this {
                        Some("[...]".to_owned())
                    } else if show_binds {
                        mod_bind.as_deref().map(|k| format!("[{}]", format_key_name(Some(k))))
                    } else {
                        None
                    };

                    let chevron = if expanded { "v" } else { ">" };
                    let chev_w = font.width(draw, chevron, false);
                    let mut right_edge = px + panel_w - 6.0 - chev_w;
                    let text_y = font.center_y(cursor_y, row_h);

                    font.draw_body(
                        draw,
                        chevron,
                        [right_edge, text_y],
                        if expanded { theme.accent } else { theme.text_muted },
                    );

                    if let Some(bl) = bind_label {
                        let bw = font.width(draw, &bl, false);
                        right_edge -= bw + 5.0;
                        font.draw_body(
                            draw,
                            &bl,
                            [right_edge, text_y],
                            if listening_this {
                                theme.accent
                            } else {
                                theme.text_muted
                            },
                        );
                    }

                    let name_max_w = (right_edge - (px + 10.0)).max(20.0);
                    font.draw_fitted(
                        draw,
                        mod_name,
                        [px + 7.0, text_y],
                        name_max_w,
                        if enabled { theme.text } else { theme.text_muted },
                        false,
                        false,
                    );

                    if row_hover {
                        self.hovered_tooltip = Some(mod_desc.to_owned());
                    }

                    if let Some(hit_rect) = intersect_rects(row_rect, body_rect) {
                        self.hits.push(HitRegion {
                            rect: hit_rect,
                            kind: HitKind::ModuleRow(idx),
                            tooltip: Some(mod_desc.to_owned()),
                        });
                    }
                }

                cursor_y += row_h;

                let ease = self.drawer_ease(idx);
                if ease > 0.005 {
                    let full_h = module_drawer_height(&manager.modules[idx], true);
                    let anim_h = full_h * ease;
                    let anim_rect = [px, cursor_y, panel_w, anim_h];
                    if let Some(drawer_clip) = intersect_rects(anim_rect, body_rect) {
                        let sub_clip = draw.push_clip(drawer_clip).is_ok();
                        let hit_mark = self.hits.len();
                        let drawer_mouse = if expanded && ease > 0.80 {
                            mouse
                        } else {
                            [-9999.0, -9999.0]
                        };
                        let _ = self.render_settings_list(
                            draw,
                            idx,
                            px,
                            cursor_y,
                            panel_w,
                            drawer_clip,
                            drawer_mouse,
                            manager,
                            theme,
                            font,
                            true,
                        );
                        if !expanded || ease < 0.80 {
                            self.hits.truncate(hit_mark);
                        }
                        if sub_clip {
                            let _ = draw.pop_clip();
                        }
                    }
                    cursor_y += anim_h;
                }

                // Subtle separator
                if cursor_y > body_rect[1] && cursor_y < body_rect[1] + body_rect[3] {
                    draw.fill(
                        [px + 4.0, cursor_y - 0.5, panel_w - 8.0, 0.5],
                        with_alpha(theme.border, 0x60),
                    );
                }
            }
        }

        if clipped {
            let _ = draw.pop_clip();
        }

        // Scrollbar
        if max_scroll > 0.5 {
            let track_x = px + panel_w - 3.0;
            let thumb_h = ((body_h / content_h) * body_h).clamp(14.0, body_h);
            let thumb_y = py + HEADER_H + (scroll / max_scroll) * (body_h - thumb_h);
            draw.fill([track_x, thumb_y, 2.0, thumb_h], with_alpha(theme.accent, 0xB8));
        }

        draw_outline(draw, panel_rect, theme.border);
    }

    /// Renders the centered Settings or HUD page from the top navigation tabs.
    #[allow(clippy::too_many_arguments)]
    fn render_settings_card(
        &mut self,
        draw: &mut dyn DrawContext,
        mod_idx: usize,
        title: &str,
        subtitle: &str,
        show_enable_toggle: bool,
        size: [f32; 2],
        mouse: [f32; 2],
        manager: &mut ModuleManager,
        theme: &Theme,
        font: FontConfig,
    ) {
        let card_w = (size[0] - 48.0).clamp(240.0, 340.0);
        let card_x = (size[0] - card_w) * 0.5;
        let card_y = TOP_BAR_H + 12.0;
        let max_card_h = (size[1] - card_y - FOOTER_H - 12.0).max(80.0);
        let card_header_h = 30.0;

        let content_h = module_drawer_height(&manager.modules[mod_idx], false) + 8.0;
        let body_h = content_h.min(max_card_h - card_header_h).max(40.0);
        let total_h = card_header_h + body_h;
        let max_scroll = (content_h - body_h).max(0.0);

        let scroll_ref = if self.active_tab == GuiTab::Settings {
            &mut self.settings_scroll
        } else {
            &mut self.hud_scroll
        };
        *scroll_ref = scroll_ref.clamp(0.0, max_scroll);
        let scroll = *scroll_ref;

        let card_rect = [card_x, card_y, card_w, total_h];
        draw.fill([card_x + 3.0, card_y + 3.0, card_w, total_h], 0x5500_0000);
        draw.fill([card_x, card_y, card_w, card_header_h], theme.header_bg);
        draw.fill([card_x, card_y, card_w, 2.0], theme.accent);

        font.draw_header(draw, title, [card_x + 10.0, card_y + 6.0], theme.text);
        font.draw_fitted(
            draw,
            subtitle,
            [card_x + 10.0, card_y + 17.5],
            card_w - 60.0,
            theme.text_muted,
            false,
            false,
        );

        if show_enable_toggle {
            let enabled = manager.modules[mod_idx].enabled;
            let sw_w = 22.0;
            let sw_h = 11.0;
            let sw_x = card_x + card_w - sw_w - 10.0;
            let sw_y = card_y + (card_header_h - sw_h) * 0.5;
            let sw_rect = [sw_x, sw_y, sw_w, sw_h];
            draw.fill(
                sw_rect,
                if enabled {
                    theme.accent
                } else {
                    theme.slider_track
                },
            );
            let knob_x = if enabled { sw_x + sw_w - 9.0 } else { sw_x + 1.5 };
            draw.fill([knob_x, sw_y + 1.5, 7.5, sw_h - 3.0], 0xFFFF_FFFF);
            self.hits.push(HitRegion {
                rect: sw_rect,
                kind: HitKind::ToggleModuleEnabled(mod_idx),
                tooltip: Some("Enable or disable the HUD overlay.".into()),
            });
        }

        let body_rect = [card_x, card_y + card_header_h, card_w, body_h];
        draw.fill(body_rect, theme.panel_bg);
        self.card_scroll_rect = Some((body_rect, max_scroll));

        let clipped = draw.push_clip(body_rect).is_ok();
        let _ = self.render_settings_list(
            draw,
            mod_idx,
            card_x,
            card_y + card_header_h + 2.0 - scroll,
            card_w,
            body_rect,
            mouse,
            manager,
            theme,
            font,
            false,
        );
        if clipped {
            let _ = draw.pop_clip();
        }

        if max_scroll > 0.5 {
            let track_x = card_x + card_w - 3.5;
            let thumb_h = ((body_h / content_h) * body_h).clamp(16.0, body_h);
            let thumb_y = body_rect[1] + (scroll / max_scroll) * (body_h - thumb_h);
            draw.fill([track_x, thumb_y, 2.5, thumb_h], with_alpha(theme.accent, 0xC4));
        }

        draw_outline(draw, card_rect, theme.border);
    }

    #[allow(clippy::too_many_arguments)]
    fn render_settings_list(
        &mut self,
        draw: &mut dyn DrawContext,
        mod_idx: usize,
        px: f32,
        mut y: f32,
        w: f32,
        clip: [f32; 4],
        mouse: [f32; 2],
        manager: &ModuleManager,
        theme: &Theme,
        font: FontConfig,
        show_module_bind: bool,
    ) -> f32 {
        let m = &manager.modules[mod_idx];
        let drawer_h = module_drawer_height(m, show_module_bind);
        draw.fill([px, y, w, drawer_h], theme.setting_bg);
        draw.fill([px, y, 1.5, drawer_h], with_alpha(theme.accent, 0x70));

        // Optional per-module keybind row (on Modules tab)
        if show_module_bind && m.id != ModuleId::ClickGui {
            let bind_rect = [px, y, w, SETTING_H];
            let vis = y + SETTING_H > clip[1] && y < clip[1] + clip[3];
            let hover = vis && rect_contains(clip, mouse) && rect_contains(bind_rect, mouse);
            if vis {
                if hover {
                    draw.fill(bind_rect, theme.module_hover);
                    self.hovered_tooltip =
                        Some(
                            "Click to assign a toggle keybind. Press a key or mouse button; ESC/Backspace/RMB clears."
                                .into(),
                        );
                }
                let listening = self.listening_bind == Some(BindTarget::Module(mod_idx));
                let val_str = if listening {
                    "Press key...".to_owned()
                } else {
                    format_key_name(m.keybind.as_deref())
                };
                let vw = font.width(draw, &val_str, false);
                let ty = font.center_y(y, SETTING_H);
                font.draw_body(draw, "Bind", [px + 7.0, ty], theme.text_muted);
                font.draw_value(
                    draw,
                    &val_str,
                    [px + w - 7.0 - vw, ty],
                    if listening { theme.accent } else { theme.text },
                );
                if let Some(hit) = intersect_rects(bind_rect, clip) {
                    self.hits.push(HitRegion {
                        rect: hit,
                        kind: HitKind::ModuleBind(mod_idx),
                        tooltip: None,
                    });
                }
            }
            y += SETTING_H;
        }

        let mut last_group: Option<&str> = None;
        for (set_idx, setting) in m.settings.iter().enumerate() {
            if let Some(grp) = setting.group
                && last_group != Some(grp)
            {
                last_group = Some(grp);
                if y + GROUP_H > clip[1] && y < clip[1] + clip[3] {
                    let gy = font.center_y(y, GROUP_H);
                    let gw = font.width(draw, grp, font.bold_headers);
                    font.draw_header(draw, grp, [px + 7.0, gy], with_alpha(theme.accent, 0xDF));
                    let line_x = px + 11.0 + gw;
                    if line_x < px + w - 8.0 {
                        draw.fill(
                            [line_x, y + GROUP_H * 0.52, px + w - 7.0 - line_x, 0.7],
                            with_alpha(theme.accent, 0x45),
                        );
                    }
                }
                y += GROUP_H;
            }

            let row_rect = [px, y, w, SETTING_H];
            let vis = y + SETTING_H > clip[1] && y < clip[1] + clip[3];
            let hover = vis && rect_contains(clip, mouse) && rect_contains(row_rect, mouse);
            let ty = font.center_y(y, SETTING_H);

            if hover {
                self.hovered_tooltip = Some(setting.description.to_owned());
            }

            match &setting.kind {
                SettingKind::Bool { value, .. } => {
                    if vis {
                        if hover {
                            draw.fill(row_rect, theme.module_hover);
                        }
                        let sw_w = 18.0;
                        let sw_h = 9.0;
                        let sw_x = px + w - sw_w - 7.0;
                        let sw_y = y + (SETTING_H - sw_h) * 0.5;
                        let label_max = (sw_x - (px + 11.0)).max(20.0);
                        font.draw_fitted(
                            draw,
                            setting.name,
                            [px + 7.0, ty],
                            label_max,
                            if *value { theme.text } else { theme.text_muted },
                            false,
                            false,
                        );
                        draw.fill(
                            [sw_x, sw_y, sw_w, sw_h],
                            if *value {
                                theme.accent
                            } else {
                                theme.slider_track
                            },
                        );
                        let knob_x = if *value { sw_x + sw_w - 8.0 } else { sw_x + 1.0 };
                        draw.fill([knob_x, sw_y + 1.0, 7.0, sw_h - 2.0], 0xFFFF_FFFF);
                        if let Some(hit) = intersect_rects(row_rect, clip) {
                            self.hits.push(HitRegion {
                                rect: hit,
                                kind: HitKind::SettingBool { mod_idx, set_idx },
                                tooltip: Some(setting.description.to_owned()),
                            });
                        }
                    }
                    y += SETTING_H;
                }

                SettingKind::Float { .. } | SettingKind::Int { .. } => {
                    if vis {
                        let frac = setting.slider_fraction().unwrap_or(0.0);
                        let bar_rect = [px + 5.0, y + 2.0, w - 10.0, SETTING_H - 4.0];
                        draw.fill(bar_rect, theme.slider_track);
                        let fill_w = bar_rect[2] * frac;
                        if fill_w > 0.5 {
                            draw.fill(
                                [bar_rect[0], bar_rect[1], fill_w, bar_rect[3]],
                                with_alpha(theme.accent, if hover { 0xC8 } else { 0x98 }),
                            );
                        }
                        let val_str = setting.display_value();
                        let vw = font.width(draw, &val_str, false);
                        let label_max = (bar_rect[2] - vw - 10.0).max(20.0);
                        font.draw_fitted(
                            draw,
                            setting.name,
                            [bar_rect[0] + 4.0, ty],
                            label_max,
                            theme.text,
                            false,
                            false,
                        );
                        font.draw_value(
                            draw,
                            &val_str,
                            [bar_rect[0] + bar_rect[2] - vw - 4.0, ty],
                            theme.text,
                        );
                        if let Some(hit) = intersect_rects(row_rect, clip) {
                            self.hits.push(HitRegion {
                                rect: hit,
                                kind: HitKind::SettingSlider { mod_idx, set_idx },
                                tooltip: Some(setting.description.to_owned()),
                            });
                        }
                    }
                    y += SETTING_H;
                }

                SettingKind::Mode { .. } => {
                    if vis {
                        if hover {
                            draw.fill(row_rect, theme.module_hover);
                        }
                        let is_open = self
                            .open_dropdown
                            .is_some_and(|d| d.mod_idx == mod_idx && d.set_idx == set_idx);
                        let val = setting.display_value();
                        let chevron = if is_open { "^" } else { "v" };
                        let vw = font.width(draw, &val, false);
                        let cw = font.width(draw, chevron, false);
                        let pill_w = (vw + cw + 14.0).clamp(52.0, (w * 0.58).max(52.0));
                        let pill_h = SETTING_H - 3.5;
                        let pill_x = px + w - pill_w - 6.0;
                        let pill_y = y + 1.75;
                        let pill_rect = [pill_x, pill_y, pill_w, pill_h];

                        if is_open
                            && let Some(dd) = &mut self.open_dropdown
                        {
                            dd.anchor = pill_rect;
                        }

                        draw.fill(
                            pill_rect,
                            if is_open {
                                with_alpha(theme.accent, 0x38)
                            } else if hover {
                                theme.module_hover
                            } else {
                                theme.slider_track
                            },
                        );
                        draw_outline(
                            draw,
                            pill_rect,
                            if is_open { theme.accent } else { theme.border },
                        );

                        let label_max = (pill_x - (px + 10.0)).max(20.0);
                        font.draw_fitted(
                            draw,
                            setting.name,
                            [px + 7.0, ty],
                            label_max,
                            theme.text_muted,
                            false,
                            false,
                        );
                        let pty = font.center_y(pill_y, pill_h);
                        let val_max = (pill_w - cw - 11.0).max(16.0);
                        font.draw_fitted(
                            draw,
                            &val,
                            [pill_x + 4.5, pty],
                            val_max,
                            if is_open { theme.text } else { theme.accent },
                            false,
                            font.italic_values,
                        );
                        font.draw_body(
                            draw,
                            chevron,
                            [pill_x + pill_w - cw - 4.0, pty],
                            if is_open {
                                theme.accent
                            } else {
                                theme.text_muted
                            },
                        );
                        if let Some(hit) = intersect_rects(row_rect, clip) {
                            self.hits.push(HitRegion {
                                rect: hit,
                                kind: HitKind::SettingMode {
                                    mod_idx,
                                    set_idx,
                                    anchor: pill_rect,
                                },
                                tooltip: Some(setting.description.to_owned()),
                            });
                        }
                    }
                    y += SETTING_H;
                }

                SettingKind::EntityFilter { value, .. } => {
                    if vis {
                        if hover {
                            draw.fill(row_rect, theme.module_hover);
                        }
                        let badge = format!("{}  >", value.summary());
                        let bw = font.width(draw, &badge, false);
                        let pill_w = (bw + 10.0).min(w * 0.56);
                        let pill_h = SETTING_H - 4.0;
                        let pill_x = px + w - pill_w - 6.0;
                        let pill_y = y + 2.0;
                        let pill_rect = [pill_x, pill_y, pill_w, pill_h];

                        draw.fill(
                            pill_rect,
                            if hover {
                                with_alpha(theme.accent, 0x45)
                            } else {
                                with_alpha(theme.accent, 0x28)
                            },
                        );
                        draw_outline(draw, pill_rect, with_alpha(theme.accent, 0x90));

                        let label_max = (pill_x - (px + 11.0)).max(20.0);
                        font.draw_fitted(
                            draw,
                            setting.name,
                            [px + 7.0, ty],
                            label_max,
                            theme.text,
                            false,
                            false,
                        );
                        font.draw_fitted(
                            draw,
                            &badge,
                            [pill_x + 5.0, font.center_y(pill_y, pill_h)],
                            pill_w - 8.0,
                            theme.text,
                            false,
                            false,
                        );

                        if let Some(hit) = intersect_rects(row_rect, clip) {
                            self.hits.push(HitRegion {
                                rect: hit,
                                kind: HitKind::SettingEntityFilter { mod_idx, set_idx },
                                tooltip: Some(setting.description.to_owned()),
                            });
                        }
                    }
                    y += SETTING_H;
                }

                SettingKind::Color {
                    value, expanded, ..
                } => {
                    if vis {
                        if hover {
                            draw.fill(row_rect, theme.module_hover);
                        }
                        let sw = [px + w - 18.0, y + 3.0, 11.0, SETTING_H - 6.0];
                        let label_max = (sw[0] - (px + 11.0)).max(20.0);
                        font.draw_fitted(
                            draw,
                            setting.name,
                            [px + 7.0, ty],
                            label_max,
                            theme.text_muted,
                            false,
                            false,
                        );
                        draw.fill(sw, *value);
                        draw_outline(draw, sw, theme.border);
                        if let Some(hit) = intersect_rects(row_rect, clip) {
                            self.hits.push(HitRegion {
                                rect: hit,
                                kind: HitKind::SettingColorHeader { mod_idx, set_idx },
                                tooltip: Some(setting.description.to_owned()),
                            });
                        }
                    }
                    y += SETTING_H;

                    if *expanded {
                        let channels = [
                            ("R", ((*value >> 16) & 0xFF) as u8, 0xFFD9_4A4A_u32),
                            ("G", ((*value >> 8) & 0xFF) as u8, 0xFF45_C96A_u32),
                            ("B", (*value & 0xFF) as u8, 0xFF4D_8CF5_u32),
                            ("A", ((*value >> 24) & 0xFF) as u8, 0xFFB8_B8C8_u32),
                        ];
                        for (ci, (label, val, col)) in channels.into_iter().enumerate() {
                            let c_rect = [px + 8.0, y + 1.0, w - 16.0, COLOR_CHANNEL_H - 2.0];
                            if y + COLOR_CHANNEL_H > clip[1] && y < clip[1] + clip[3] {
                                draw.fill(c_rect, theme.slider_track);
                                let frac = val as f32 / 255.0;
                                draw.fill(
                                    [c_rect[0], c_rect[1], c_rect[2] * frac, c_rect[3]],
                                    with_alpha(col, 0xB8),
                                );
                                let cty = font.center_y(c_rect[1], c_rect[3]);
                                font.draw_body(draw, label, [c_rect[0] + 3.0, cty], theme.text);
                                let vstr = format!("{val}");
                                let vw = font.width(draw, &vstr, false);
                                font.draw_value(
                                    draw,
                                    &vstr,
                                    [c_rect[0] + c_rect[2] - vw - 3.0, cty],
                                    theme.text,
                                );
                                if let Some(hit) = intersect_rects(c_rect, clip) {
                                    self.hits.push(HitRegion {
                                        rect: hit,
                                        kind: HitKind::SettingColorChannel {
                                            mod_idx,
                                            set_idx,
                                            channel: ci as u8,
                                        },
                                        tooltip: None,
                                    });
                                }
                            }
                            y += COLOR_CHANNEL_H;
                        }
                        y += 4.0;
                    }
                }

                SettingKind::Keybind { .. } => {
                    if vis {
                        if hover {
                            draw.fill(row_rect, theme.module_hover);
                        }
                        let listening =
                            self.listening_bind == Some(BindTarget::Setting { mod_idx, set_idx });
                        let val_str = if listening {
                            "Press key...".to_owned()
                        } else {
                            setting.display_value()
                        };
                        let vw = font.width(draw, &val_str, false);
                        let label_max = (w - vw - 18.0).max(20.0);
                        font.draw_fitted(
                            draw,
                            setting.name,
                            [px + 7.0, ty],
                            label_max,
                            theme.text_muted,
                            false,
                            false,
                        );
                        font.draw_value(
                            draw,
                            &val_str,
                            [px + w - 7.0 - vw, ty],
                            if listening { theme.accent } else { theme.text },
                        );
                        if let Some(hit) = intersect_rects(row_rect, clip) {
                            self.hits.push(HitRegion {
                                rect: hit,
                                kind: HitKind::SettingKeybind { mod_idx, set_idx },
                                tooltip: Some(setting.description.to_owned()),
                            });
                        }
                    }
                    y += SETTING_H;
                }
            }
        }

        y
    }

    /// Renders the floating dropdown menu for any open `SettingKind::Mode` control.
    /// For `font_face`, each option previews itself in its own TrueType font face!
    fn render_dropdown(
        &mut self,
        draw: &mut dyn DrawContext,
        size: [f32; 2],
        mouse: [f32; 2],
        manager: &ModuleManager,
        theme: &Theme,
        font: FontConfig,
    ) {
        let Some(dd) = self.open_dropdown else {
            return;
        };
        let Some(module) = manager.modules.get(dd.mod_idx) else {
            self.open_dropdown = None;
            return;
        };
        let Some(setting) = module.settings.get(dd.set_idx) else {
            self.open_dropdown = None;
            return;
        };
        let SettingKind::Mode {
            index: selected,
            options,
            ..
        } = &setting.kind
        else {
            self.open_dropdown = None;
            return;
        };
        if options.is_empty() {
            self.open_dropdown = None;
            return;
        }

        let is_font_dropdown = setting.id == "font_face";
        let item_h = 15.5;
        let max_opt_w = options
            .iter()
            .enumerate()
            .map(|(i, opt)| {
                let preview_font = if is_font_dropdown {
                    font.with_face(FontFace::from_index(i))
                } else {
                    font
                };
                preview_font.width(draw, opt, false)
            })
            .fold(0.0_f32, f32::max);

        let drop_w = (max_opt_w + 22.0).max(dd.anchor[2]).min(size[0] - 16.0);
        let content_h = options.len() as f32 * item_h + 4.0;
        let space_below = (size[1] - FOOTER_H - 8.0 - (dd.anchor[1] + dd.anchor[3])).max(0.0);
        let space_above = (dd.anchor[1] - (TOP_BAR_H + 8.0)).max(0.0);
        let open_above = space_below < content_h.min(130.0) && space_above > space_below;
        let max_h = if open_above {
            space_above.min(180.0)
        } else {
            space_below.min(180.0)
        }
        .max(item_h + 4.0);
        let drop_h = content_h.min(max_h);
        let max_scroll = (content_h - drop_h).max(0.0);
        let scroll = dd.scroll.clamp(0.0, max_scroll);
        if let Some(st) = &mut self.open_dropdown {
            st.scroll = scroll;
        }

        let drop_x = (dd.anchor[0] + dd.anchor[2] - drop_w).clamp(6.0, (size[0] - drop_w - 6.0).max(6.0));
        let drop_y = if open_above {
            (dd.anchor[1] - drop_h - 1.0).max(TOP_BAR_H + 4.0)
        } else {
            dd.anchor[1] + dd.anchor[3] + 1.0
        };
        let drop_rect = [drop_x, drop_y, drop_w, drop_h];
        self.dropdown_list_rect = Some((drop_rect, max_scroll));

        // Shadow & container
        draw.fill([drop_x + 2.0, drop_y + 2.0, drop_w, drop_h], 0x6800_0000);
        draw.fill(drop_rect, with_alpha(theme.header_bg, 0xFC));

        let clipped = draw.push_clip(drop_rect).is_ok();
        for (opt_idx, opt) in options.iter().enumerate() {
            let iy = drop_y + 2.0 + opt_idx as f32 * item_h - scroll;
            if iy + item_h < drop_y || iy > drop_y + drop_h {
                continue;
            }
            let row = [drop_x + 1.0, iy, drop_w - 2.0, item_h];
            let hover = rect_contains(drop_rect, mouse) && rect_contains(row, mouse);
            let is_selected = opt_idx == *selected;

            if is_selected {
                draw.fill(row, with_alpha(theme.accent, if hover { 0x4C } else { 0x34 }));
                draw.fill([drop_x + 1.0, iy, 2.0, item_h], theme.accent);
            } else if hover {
                draw.fill(row, theme.module_hover);
            }

            let preview_font = if is_font_dropdown {
                font.with_face(FontFace::from_index(opt_idx))
            } else {
                font
            };
            let ty = preview_font.center_y(iy, item_h);
            preview_font.draw_fitted(
                draw,
                opt,
                [drop_x + 7.0, ty],
                drop_w - 14.0,
                if is_selected || hover {
                    theme.text
                } else {
                    theme.text_muted
                },
                false,
                false,
            );

            if let Some(hit) = intersect_rects(row, drop_rect) {
                self.hits.push(HitRegion {
                    rect: hit,
                    kind: HitKind::DropdownOption {
                        mod_idx: dd.mod_idx,
                        set_idx: dd.set_idx,
                        option_idx: opt_idx,
                    },
                    tooltip: None,
                });
            }
        }
        if clipped {
            let _ = draw.pop_clip();
        }

        if max_scroll > 0.5 {
            let track_x = drop_x + drop_w - 3.0;
            let thumb_h = ((drop_h / content_h) * drop_h).clamp(12.0, drop_h);
            let thumb_y = drop_y + (scroll / max_scroll) * (drop_h - thumb_h);
            draw.fill([track_x, thumb_y, 2.0, thumb_h], with_alpha(theme.accent, 0xCC));
        }

        draw_outline(draw, drop_rect, theme.accent);
    }

    /// Renders the floating Entity Selector modal window for picking exact entity types.
    fn render_entity_picker(
        &mut self,
        draw: &mut dyn DrawContext,
        size: [f32; 2],
        mouse: [f32; 2],
        manager: &ModuleManager,
        theme: &Theme,
        font: FontConfig,
    ) {
        let Some(picker) = self.entity_picker.clone() else {
            return;
        };
        let Some(module) = manager.modules.get(picker.mod_idx) else {
            self.entity_picker = None;
            return;
        };
        let Some(setting) = module.settings.get(picker.set_idx) else {
            self.entity_picker = None;
            return;
        };
        let Some(filter) = setting.as_entity_filter() else {
            self.entity_picker = None;
            return;
        };

        // Dim background behind modal
        draw.fill([0.0, 0.0, size[0], size[1]], 0x7A00_0000);

        let mw = (size[0] - 40.0).clamp(260.0, 340.0);
        let mh = (size[1] - 44.0).clamp(180.0, 290.0);
        let mx = (size[0] - mw) * 0.5;
        let my = (size[1] - mh) * 0.5;
        let modal_rect = [mx, my, mw, mh];
        self.picker_window_rect = Some(modal_rect);

        draw.fill([mx + 3.0, my + 3.0, mw, mh], 0x6000_0000);
        draw.fill(modal_rect, with_alpha(theme.panel_bg, 0xF8));
        draw.fill([mx, my, mw, 22.0], theme.header_bg);
        draw.fill([mx, my, mw, 2.0], theme.accent);

        // Header title & close X
        let title = format!("{}  -  {}", module.name, setting.name);
        font.draw_header(
            draw,
            &title,
            [mx + 9.0, font.center_y(my, 22.0)],
            theme.text,
        );
        let close_rect = [mx + mw - 20.0, my + 2.0, 18.0, 18.0];
        let close_hover = rect_contains(close_rect, mouse);
        if close_hover {
            draw.fill(close_rect, with_alpha(theme.accent, 0x40));
        }
        font.draw_header(
            draw,
            "x",
            [close_rect[0] + 6.5, font.center_y(close_rect[1], close_rect[3])],
            if close_hover { theme.accent } else { theme.text_muted },
        );
        self.hits.push(HitRegion {
            rect: close_rect,
            kind: HitKind::PickerClose,
            tooltip: None,
        });

        // Search input row
        let mut cy = my + 27.0;
        let search_h = 16.0;
        let search_rect = [mx + 8.0, cy, mw - 16.0, search_h];
        let search_hover = rect_contains(search_rect, mouse);
        draw.fill(
            search_rect,
            if picker.search_focused || search_hover {
                theme.module_hover
            } else {
                theme.setting_bg
            },
        );
        draw_outline(
            draw,
            search_rect,
            if picker.search_focused {
                theme.accent
            } else {
                theme.border
            },
        );
        let sty = font.center_y(cy, search_h);
        let clear_pad = if !picker.search.is_empty() { 15.0 } else { 0.0 };
        if !picker.search_focused && picker.search.is_empty() {
            font.draw_fitted(
                draw,
                "Search entities...",
                [search_rect[0] + 5.0, sty],
                search_rect[2] - 10.0,
                theme.text_muted,
                false,
                false,
            );
        } else {
            let disp = if picker.search_focused {
                format!("{}|", picker.search)
            } else {
                picker.search.clone()
            };
            font.draw_fitted(
                draw,
                &disp,
                [search_rect[0] + 5.0, sty],
                search_rect[2] - 10.0 - clear_pad,
                theme.text,
                false,
                false,
            );
        }
        self.hits.push(HitRegion {
            rect: search_rect,
            kind: HitKind::PickerSearchBar,
            tooltip: None,
        });
        if !picker.search.is_empty() {
            let clr = [search_rect[0] + search_rect[2] - 14.0, cy, 14.0, search_h];
            font.draw_body(draw, "x", [clr[0] + 4.5, sty], theme.text_muted);
            self.hits.push(HitRegion {
                rect: clr,
                kind: HitKind::PickerSearchClear,
                tooltip: None,
            });
        }
        cy += search_h + 5.0;

        // Quick-select preset chips: [All] [None] | [Players] [Hostile] [Neutral] [Passive] [Misc]
        let chip_h = 13.5;
        let mut cx = mx + 8.0;
        let quick_actions: [(&str, HitKind, bool, u32); 7] = [
            ("All", HitKind::PickerSelectAll, false, theme.accent),
            ("None", HitKind::PickerSelectNone, false, theme.accent),
            (
                "Players",
                HitKind::PickerToggleGroup(EntityGroup::Players),
                filter.group_fully_selected(EntityGroup::Players),
                EntityGroup::Players.color(theme.accent),
            ),
            (
                "Hostile",
                HitKind::PickerToggleGroup(EntityGroup::Hostile),
                filter.group_fully_selected(EntityGroup::Hostile),
                EntityGroup::Hostile.color(theme.accent),
            ),
            (
                "Neutral",
                HitKind::PickerToggleGroup(EntityGroup::Neutral),
                filter.group_fully_selected(EntityGroup::Neutral),
                EntityGroup::Neutral.color(theme.accent),
            ),
            (
                "Passive",
                HitKind::PickerToggleGroup(EntityGroup::Passive),
                filter.group_fully_selected(EntityGroup::Passive),
                EntityGroup::Passive.color(theme.accent),
            ),
            (
                "Misc",
                HitKind::PickerToggleGroup(EntityGroup::Misc),
                filter.group_fully_selected(EntityGroup::Misc),
                EntityGroup::Misc.color(theme.accent),
            ),
        ];

        for (label, kind, active, col) in quick_actions {
            let cw = font.width(draw, label, false) + 9.0;
            if cx + cw > mx + mw - 8.0 {
                cx = mx + 8.0;
                cy += chip_h + 3.0;
            }
            let rect = [cx, cy, cw, chip_h];
            let hover = rect_contains(rect, mouse);
            draw.fill(
                rect,
                if active {
                    with_alpha(col, 0x45)
                } else if hover {
                    theme.module_hover
                } else {
                    theme.setting_bg
                },
            );
            draw_outline(
                draw,
                rect,
                if active { col } else { theme.border },
            );
            font.draw_body(
                draw,
                label,
                [cx + 4.5, font.center_y(cy, chip_h)],
                if active || hover { theme.text } else { theme.text_muted },
            );
            self.hits.push(HitRegion {
                rect,
                kind,
                tooltip: None,
            });
            cx += cw + 3.0;
        }
        cy += chip_h + 5.0;

        // Filter view tabs: All | Selected (N) | Groups
        let tab_h = 13.0;
        let mut tx = mx + 8.0;
        let selected_label = format!("Selected ({})", filter.len());
        let filter_tabs: Vec<(String, PickerFilter)> = vec![
            ("Show All".into(), PickerFilter::All),
            (selected_label, PickerFilter::Selected),
            ("Players".into(), PickerFilter::Group(EntityGroup::Players)),
            ("Hostile".into(), PickerFilter::Group(EntityGroup::Hostile)),
            ("Neutral".into(), PickerFilter::Group(EntityGroup::Neutral)),
            ("Passive".into(), PickerFilter::Group(EntityGroup::Passive)),
            ("Misc".into(), PickerFilter::Group(EntityGroup::Misc)),
        ];
        for (label, flt) in filter_tabs {
            let tw = font.width(draw, &label, false) + 7.0;
            if tx + tw > mx + mw - 8.0 {
                break;
            }
            let rect = [tx, cy, tw, tab_h];
            let active = picker.filter == flt;
            let hover = rect_contains(rect, mouse);
            if active {
                draw.fill([tx, cy + tab_h - 1.5, tw, 1.5], theme.accent);
            }
            font.draw_body(
                draw,
                &label,
                [tx + 3.5, font.center_y(cy, tab_h)],
                if active {
                    theme.accent
                } else if hover {
                    theme.text
                } else {
                    theme.text_muted
                },
            );
            self.hits.push(HitRegion {
                rect,
                kind: HitKind::PickerFilterTab(flt),
                tooltip: None,
            });
            tx += tw + 2.0;
        }
        cy += tab_h + 4.0;

        // Scrollable 2-column entity checklist
        let footer_bar_h = 22.0;
        let list_rect = [mx + 8.0, cy, mw - 16.0, (my + mh - footer_bar_h - cy - 4.0).max(40.0)];
        draw.fill(list_rect, theme.setting_bg);
        draw_outline(draw, list_rect, theme.border);

        let q = picker.search.trim().to_ascii_lowercase();
        let entries: Vec<&entity_filter::EntityEntry> = entity_filter::catalog()
            .iter()
            .filter(|e| {
                let pass_tab = match picker.filter {
                    PickerFilter::All => true,
                    PickerFilter::Selected => filter.contains(&e.id),
                    PickerFilter::Group(g) => e.group == g,
                };
                if !pass_tab {
                    return false;
                }
                q.is_empty()
                    || e.display.to_ascii_lowercase().contains(&q)
                    || e.id.to_ascii_lowercase().contains(&q)
            })
            .collect();

        let cols = 2usize;
        let item_h = 15.5;
        let rows = entries.len().div_ceil(cols);
        let content_h = (rows as f32 * item_h + 6.0).max(item_h);
        let max_scroll = (content_h - list_rect[3]).max(0.0);
        let scroll = picker.scroll.clamp(0.0, max_scroll);
        if let Some(st) = &mut self.entity_picker {
            st.scroll = scroll;
        }
        self.picker_list_rect = Some((list_rect, max_scroll));

        let clipped = draw.push_clip(list_rect).is_ok();
        if entries.is_empty() {
            let msg = "No matching entities";
            let ew = font.width(draw, msg, false);
            font.draw_body(
                draw,
                msg,
                [
                    list_rect[0] + (list_rect[2] - ew) * 0.5,
                    list_rect[1] + 14.0,
                ],
                theme.text_muted,
            );
        } else {
            let col_w = (list_rect[2] - 8.0) / cols as f32;
            for (i, entry) in entries.iter().enumerate() {
                let row = i / cols;
                let col = i % cols;
                let ix = list_rect[0] + 3.0 + col as f32 * col_w;
                let iy = list_rect[1] + 3.0 + row as f32 * item_h - scroll;
                if iy + item_h < list_rect[1] || iy > list_rect[1] + list_rect[3] {
                    continue;
                }
                let cell = [ix, iy, col_w - 3.0, item_h - 1.5];
                let hover = rect_contains(list_rect, mouse) && rect_contains(cell, mouse);
                let checked = filter.contains(&entry.id);
                let gcol = entry.group.color(theme.accent);

                if checked {
                    draw.fill(cell, with_alpha(gcol, if hover { 0x3C } else { 0x24 }));
                } else if hover {
                    draw.fill(cell, theme.module_hover);
                }

                // Checkbox square
                let cb = [ix + 3.0, iy + (cell[3] - 8.0) * 0.5, 8.0, 8.0];
                draw.fill(cb, if checked { gcol } else { theme.slider_track });
                if checked {
                    draw.fill([cb[0] + 2.0, cb[1] + 2.0, 4.0, 4.0], 0xFFFF_FFFF);
                } else {
                    draw_outline(draw, cb, theme.border);
                }

                let text_x = ix + 14.0;
                let text_max = (cell[2] - 17.0).max(16.0);
                font.draw_fitted(
                    draw,
                    &entry.display,
                    [text_x, font.center_y(iy, cell[3])],
                    text_max,
                    if checked { theme.text } else { theme.text_muted },
                    false,
                    false,
                );

                if let Some(hit) = intersect_rects(cell, list_rect) {
                    self.hits.push(HitRegion {
                        rect: hit,
                        kind: HitKind::PickerEntityToggle(entry.id.clone()),
                        tooltip: None,
                    });
                }
            }
        }
        if clipped {
            let _ = draw.pop_clip();
        }

        if max_scroll > 0.5 {
            let track_x = list_rect[0] + list_rect[2] - 3.5;
            let thumb_h = ((list_rect[3] / content_h) * list_rect[3]).clamp(14.0, list_rect[3]);
            let thumb_y = list_rect[1] + (scroll / max_scroll) * (list_rect[3] - thumb_h);
            draw.fill([track_x, thumb_y, 2.5, thumb_h], with_alpha(theme.accent, 0xC8));
        }

        // Modal footer: count + Done button
        let fy = my + mh - footer_bar_h;
        let status = format!("{} selected", filter.len());
        font.draw_body(
            draw,
            &status,
            [mx + 10.0, font.center_y(fy, footer_bar_h)],
            theme.text_muted,
        );

        let done_w = 48.0;
        let done_h = 15.0;
        let done_rect = [mx + mw - done_w - 8.0, fy + (footer_bar_h - done_h) * 0.5, done_w, done_h];
        let done_hover = rect_contains(done_rect, mouse);
        draw.fill(
            done_rect,
            if done_hover {
                theme.accent
            } else {
                with_alpha(theme.accent, 0xC8)
            },
        );
        let dw = font.width(draw, "Done", font.bold_headers);
        font.draw_header(
            draw,
            "Done",
            [
                done_rect[0] + (done_w - dw) * 0.5,
                font.center_y(done_rect[1], done_rect[3]),
            ],
            0xFFFF_FFFF,
        );
        self.hits.push(HitRegion {
            rect: done_rect,
            kind: HitKind::PickerClose,
            tooltip: None,
        });

        draw_outline(draw, modal_rect, theme.accent);
    }

    fn render_footer(
        &self,
        draw: &mut dyn DrawContext,
        size: [f32; 2],
        theme: &Theme,
        font: FontConfig,
    ) {
        let hint = if self.entity_picker.is_some() {
            "Click entities to toggle  |  Scroll to browse  |  ESC Close Picker"
        } else if self.listening_bind.is_some() {
            "Press any key or mouse button to bind  |  ESC / Backspace / RMB clear"
        } else if self.active_tab == GuiTab::Modules {
            "LMB Toggle  |  RMB Settings / Reset  |  MMB Bind  |  Scroll Adjust  |  ESC Close"
        } else {
            "LMB Toggle / Drag Slider  |  RMB Reset Setting  |  Scroll to browse  |  ESC Close"
        };
        let hw = font.width(draw, hint, false);
        let bar_w = hw + 18.0;
        let x = (size[0] - bar_w) * 0.5;
        let y = size[1] - FOOTER_H - 3.0;
        draw.fill([x, y, bar_w, FOOTER_H], with_alpha(theme.panel_bg, 0xD2));
        draw.fill([x, y, bar_w, 1.0], with_alpha(theme.accent, 0x70));
        font.draw_body(
            draw,
            hint,
            [x + 9.0, font.center_y(y, FOOTER_H)],
            theme.text_muted,
        );
    }

    fn render_tooltip(
        &self,
        draw: &mut dyn DrawContext,
        size: [f32; 2],
        mouse: [f32; 2],
        text: &str,
        theme: &Theme,
        font: FontConfig,
    ) {
        let tw = font.width(draw, text, false);
        let w = tw + 12.0;
        let h = (font.line_height() + 6.0).max(14.0);
        let x = (mouse[0] + 10.0).min((size[0] - w - 4.0).max(4.0));
        let y = (mouse[1] + 12.0).min((size[1] - h - FOOTER_H - 6.0).max(TOP_BAR_H + 4.0));
        draw.fill([x, y, w, h], with_alpha(theme.header_bg, 0xF4));
        draw.fill([x, y, 2.0, h], theme.accent);
        draw_outline(draw, [x, y, w, h], theme.border);
        font.draw_body(draw, text, [x + 6.0, font.center_y(y, h)], theme.text);
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Input handlers
    // ─────────────────────────────────────────────────────────────────────────

    pub fn mouse_pressed(
        &mut self,
        mouse: [f32; 2],
        button: i32,
        manager: &mut ModuleManager,
    ) -> bool {
        self.search_focused = false;
        if let Some(p) = &mut self.entity_picker {
            p.search_focused = false;
        }

        // While capturing a keybind, mouse buttons assign (or clear with RMB).
        if self.listening_bind.is_some() {
            if button == 1 {
                return self.key_pressed("key.keyboard.escape", manager);
            }
            if let Some(key) = crate::settings::mouse_button_key(button) {
                return self.key_pressed(key, manager);
            }
            return true;
        }

        // Hit-test topmost element first
        let hit = self
            .hits
            .iter()
            .rev()
            .find(|h| rect_contains(h.rect, mouse))
            .cloned();

        let Some(hit) = hit else {
            self.listening_bind = None;
            if self.open_dropdown.take().is_some() {
                return true;
            }
            // Clicking outside the Entity Picker modal closes it cleanly.
            if let Some(win) = self.picker_window_rect
                && !rect_contains(win, mouse)
            {
                self.entity_picker = None;
                return true;
            }
            return false;
        };

        // If a dropdown is open and the user clicks something other than its own options or anchor, close it.
        if let Some(dd) = self.open_dropdown {
            let is_own = matches!(
                hit.kind,
                HitKind::DropdownOption { mod_idx, set_idx, .. }
                | HitKind::SettingMode { mod_idx, set_idx, .. }
                    if mod_idx == dd.mod_idx && set_idx == dd.set_idx
            );
            if !is_own {
                self.open_dropdown = None;
                return true;
            }
        }

        match hit.kind {
            HitKind::Tab(tab) => {
                if button == 0 {
                    self.active_tab = tab;
                    self.listening_bind = None;
                    self.dragging = None;
                    self.open_dropdown = None;
                }
                true
            }
            HitKind::SearchBar => {
                if button == 0 {
                    self.search_focused = true;
                } else if button == 1 {
                    self.search.clear();
                }
                true
            }
            HitKind::SearchClear => {
                self.search.clear();
                self.search_focused = true;
                true
            }
            HitKind::ResetLayout => {
                if button == 0 {
                    manager.reset_panel_layout(self.screen_size[0]);
                }
                true
            }
            HitKind::PanelHeader(category) => {
                self.bring_to_front(category);
                if button == 0 {
                    let p = manager.panels[category.index()];
                    self.dragging = Some(DragTarget::Panel {
                        category,
                        offset_x: mouse[0] - p.x,
                        offset_y: mouse[1] - p.y,
                    });
                } else if button == 1 {
                    let p = &mut manager.panels[category.index()];
                    p.collapsed = !p.collapsed;
                    manager.dirty = true;
                }
                true
            }
            HitKind::PanelCollapse(category) => {
                self.bring_to_front(category);
                let p = &mut manager.panels[category.index()];
                p.collapsed = !p.collapsed;
                manager.dirty = true;
                true
            }
            HitKind::ToggleModuleEnabled(idx) => {
                if button == 0 {
                    let m = &mut manager.modules[idx];
                    m.enabled = !m.enabled;
                    manager.dirty = true;
                }
                true
            }
            HitKind::ModuleRow(idx) => {
                let category = manager.modules[idx].category;
                self.bring_to_front(category);
                match button {
                    0 => {
                        let m = &mut manager.modules[idx];
                        if m.id == ModuleId::ClickGui {
                            m.expanded = !m.expanded;
                        } else {
                            m.enabled = !m.enabled;
                            manager.dirty = true;
                        }
                    }
                    1 => {
                        let opening = !manager.modules[idx].expanded;
                        if opening {
                            for (i, m) in manager.modules.iter_mut().enumerate() {
                                if i != idx && m.category != Category::Client {
                                    m.expanded = false;
                                }
                            }
                        }
                        manager.modules[idx].expanded = opening;
                        self.open_dropdown = None;
                    }
                    2 if manager.modules[idx].id != ModuleId::ClickGui => {
                        self.listening_bind = Some(BindTarget::Module(idx));
                    }
                    _ => {}
                }
                true
            }
            HitKind::ModuleBind(idx) => {
                if button == 0 {
                    self.listening_bind = Some(BindTarget::Module(idx));
                } else if button == 1 {
                    manager.modules[idx].keybind = None;
                    manager.dirty = true;
                }
                true
            }
            HitKind::SettingBool { mod_idx, set_idx } => {
                let setting = &mut manager.modules[mod_idx].settings[set_idx];
                if button == 0 {
                    if let SettingKind::Bool { value, .. } = &mut setting.kind {
                        *value = !*value;
                        manager.dirty = true;
                    }
                } else if button == 1 && setting.reset() {
                    manager.dirty = true;
                }
                true
            }
            HitKind::SettingSlider { mod_idx, set_idx } => {
                if button == 0 {
                    let bar_x = hit.rect[0] + 5.0;
                    let bar_w = (hit.rect[2] - 10.0).max(1.0);
                    let frac = ((mouse[0] - bar_x) / bar_w).clamp(0.0, 1.0);
                    if manager.modules[mod_idx].settings[set_idx].set_slider_fraction(frac) {
                        manager.dirty = true;
                    }
                    self.dragging = Some(DragTarget::Slider {
                        mod_idx,
                        set_idx,
                        x: bar_x,
                        w: bar_w,
                    });
                } else if button == 1 && manager.modules[mod_idx].settings[set_idx].reset() {
                    manager.dirty = true;
                }
                true
            }
            HitKind::SettingMode {
                mod_idx,
                set_idx,
                anchor,
            } => {
                if button == 0 {
                    let already_open = self
                        .open_dropdown
                        .is_some_and(|d| d.mod_idx == mod_idx && d.set_idx == set_idx);
                    if already_open {
                        self.open_dropdown = None;
                    } else {
                        self.open_dropdown = Some(DropdownState {
                            mod_idx,
                            set_idx,
                            anchor,
                            scroll: 0.0,
                        });
                    }
                } else if button == 1 {
                    self.open_dropdown = None;
                    if manager.modules[mod_idx].settings[set_idx].reset() {
                        manager.dirty = true;
                    }
                }
                true
            }
            HitKind::DropdownOption {
                mod_idx,
                set_idx,
                option_idx,
            } => {
                if button == 0 {
                    let setting = &mut manager.modules[mod_idx].settings[set_idx];
                    if let SettingKind::Mode { index, options, .. } = &mut setting.kind
                        && option_idx < options.len()
                    {
                        *index = option_idx;
                        manager.dirty = true;
                    }
                    self.open_dropdown = None;
                }
                true
            }
            HitKind::SettingEntityFilter { mod_idx, set_idx } => {
                if button == 0 {
                    self.entity_picker = Some(EntityPickerState {
                        mod_idx,
                        set_idx,
                        search: String::new(),
                        search_focused: false,
                        scroll: 0.0,
                        filter: PickerFilter::All,
                    });
                } else if button == 1 && manager.modules[mod_idx].settings[set_idx].reset() {
                    manager.dirty = true;
                }
                true
            }
            HitKind::SettingColorHeader { mod_idx, set_idx } => {
                let setting = &mut manager.modules[mod_idx].settings[set_idx];
                if button == 0 {
                    if let SettingKind::Color { expanded, .. } = &mut setting.kind {
                        *expanded = !*expanded;
                    }
                } else if button == 1 && setting.reset() {
                    manager.dirty = true;
                }
                true
            }
            HitKind::SettingColorChannel {
                mod_idx,
                set_idx,
                channel,
            } => {
                if button == 0 {
                    let frac = ((mouse[0] - hit.rect[0]) / hit.rect[2].max(1.0)).clamp(0.0, 1.0);
                    set_color_channel(
                        &mut manager.modules[mod_idx].settings[set_idx],
                        channel,
                        frac,
                    );
                    manager.dirty = true;
                    self.dragging = Some(DragTarget::ColorChannel {
                        mod_idx,
                        set_idx,
                        channel,
                        x: hit.rect[0],
                        w: hit.rect[2].max(1.0),
                    });
                }
                true
            }
            HitKind::SettingKeybind { mod_idx, set_idx } => {
                if button == 0 {
                    self.listening_bind = Some(BindTarget::Setting { mod_idx, set_idx });
                } else if button == 1 && manager.modules[mod_idx].settings[set_idx].reset() {
                    manager.dirty = true;
                }
                true
            }

            // Entity Picker modal actions
            HitKind::PickerClose => {
                self.entity_picker = None;
                true
            }
            HitKind::PickerSearchBar => {
                if let Some(p) = &mut self.entity_picker {
                    if button == 0 {
                        p.search_focused = true;
                    } else if button == 1 {
                        p.search.clear();
                        p.scroll = 0.0;
                    }
                }
                true
            }
            HitKind::PickerSearchClear => {
                if let Some(p) = &mut self.entity_picker {
                    p.search.clear();
                    p.search_focused = true;
                    p.scroll = 0.0;
                }
                true
            }
            HitKind::PickerSelectAll => {
                if let Some(p) = &self.entity_picker
                    && let Some(ef) = manager.modules[p.mod_idx].settings[p.set_idx]
                        .as_entity_filter_mut()
                {
                    ef.select_all();
                    manager.dirty = true;
                }
                true
            }
            HitKind::PickerSelectNone => {
                if let Some(p) = &self.entity_picker
                    && let Some(ef) = manager.modules[p.mod_idx].settings[p.set_idx]
                        .as_entity_filter_mut()
                {
                    ef.clear();
                    manager.dirty = true;
                }
                true
            }
            HitKind::PickerToggleGroup(group) => {
                if let Some(p) = &self.entity_picker
                    && let Some(ef) = manager.modules[p.mod_idx].settings[p.set_idx]
                        .as_entity_filter_mut()
                {
                    ef.toggle_group(group);
                    manager.dirty = true;
                }
                true
            }
            HitKind::PickerFilterTab(flt) => {
                if let Some(p) = &mut self.entity_picker {
                    p.filter = flt;
                    p.scroll = 0.0;
                }
                true
            }
            HitKind::PickerEntityToggle(entity_id) => {
                if let Some(p) = &self.entity_picker
                    && let Some(ef) = manager.modules[p.mod_idx].settings[p.set_idx]
                        .as_entity_filter_mut()
                {
                    ef.toggle(&entity_id);
                    manager.dirty = true;
                }
                true
            }
        }
    }

    pub fn mouse_dragged(&mut self, mouse: [f32; 2], manager: &mut ModuleManager) -> bool {
        let Some(drag) = self.dragging else {
            return false;
        };
        match drag {
            DragTarget::Panel {
                category,
                offset_x,
                offset_y,
            } => {
                let panel_w = manager.get(ModuleId::ClickGui).int("panel_width", 154) as f32;
                let p = &mut manager.panels[category.index()];
                p.x = (mouse[0] - offset_x).clamp(4.0, (self.screen_size[0] - panel_w - 4.0).max(4.0));
                p.y = (mouse[1] - offset_y).clamp(
                    TOP_BAR_H + 4.0,
                    (self.screen_size[1] - HEADER_H - FOOTER_H - 4.0).max(TOP_BAR_H + 4.0),
                );
                manager.dirty = true;
                true
            }
            DragTarget::Slider {
                mod_idx,
                set_idx,
                x,
                w,
            } => {
                let frac = ((mouse[0] - x) / w.max(1.0)).clamp(0.0, 1.0);
                if manager.modules[mod_idx].settings[set_idx].set_slider_fraction(frac) {
                    manager.dirty = true;
                }
                true
            }
            DragTarget::ColorChannel {
                mod_idx,
                set_idx,
                channel,
                x,
                w,
            } => {
                let frac = ((mouse[0] - x) / w.max(1.0)).clamp(0.0, 1.0);
                set_color_channel(
                    &mut manager.modules[mod_idx].settings[set_idx],
                    channel,
                    frac,
                );
                manager.dirty = true;
                true
            }
        }
    }

    pub fn mouse_released(&mut self) -> bool {
        self.dragging.take().is_some()
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

        // Scroll inside open Dropdown popover first
        if let Some((rect, max_scroll)) = self.dropdown_list_rect
            && let Some(dd) = &mut self.open_dropdown
        {
            if rect_contains(rect, mouse) && max_scroll > 0.5 {
                dd.scroll = (dd.scroll - scroll_y as f32 * 18.0).clamp(0.0, max_scroll);
            }
            return true;
        }

        // Scroll inside Entity Picker list when open
        if let Some((rect, max_scroll)) = self.picker_list_rect
            && let Some(p) = &mut self.entity_picker
        {
            if rect_contains(rect, mouse) && max_scroll > 0.5 {
                p.scroll = (p.scroll - scroll_y as f32 * 18.0).clamp(0.0, max_scroll);
            }
            return true;
        }

        // Fine-adjust slider when hovering directly over one
        if let Some(hit) = self
            .hits
            .iter()
            .rev()
            .find(|h| rect_contains(h.rect, mouse))
            && let HitKind::SettingSlider { mod_idx, set_idx } = hit.kind
        {
            let mult = if (modifiers & 1) != 0 { 5 } else { 1 };
            let dir = if scroll_y > 0.0 { 1 } else { -1 };
            if manager.modules[mod_idx].settings[set_idx].nudge(dir * mult) {
                manager.dirty = true;
                return true;
            }
        }

        // Scroll Settings / HUD card
        if let Some((rect, max_scroll)) = self.card_scroll_rect
            && rect_contains(rect, mouse)
            && max_scroll > 0.5
        {
            let s = if self.active_tab == GuiTab::Settings {
                &mut self.settings_scroll
            } else {
                &mut self.hud_scroll
            };
            *s = (*s - scroll_y as f32 * 18.0).clamp(0.0, max_scroll);
            return true;
        }

        // Scroll whichever category panel is under the cursor
        for &(category, rect, max_scroll) in self.panel_rects.iter().rev() {
            if rect_contains(rect, mouse) && max_scroll > 0.5 {
                let p = &mut manager.panels[category.index()];
                p.scroll = (p.scroll - scroll_y as f32 * 18.0).clamp(0.0, max_scroll);
                return true;
            }
        }
        false
    }

    pub fn key_pressed(&mut self, key: &str, manager: &mut ModuleManager) -> bool {
        // Keybind capture has highest priority
        if let Some(target) = self.listening_bind.take() {
            let cleared = matches!(
                key,
                "key.keyboard.escape" | "key.keyboard.backspace" | "key.keyboard.delete"
            );
            let value = if cleared { None } else { Some(key.to_owned()) };
            match target {
                BindTarget::Module(idx) => {
                    manager.modules[idx].keybind = value;
                    manager.dirty = true;
                }
                BindTarget::Setting { mod_idx, set_idx } => {
                    if let SettingKind::Keybind { key: slot, .. } =
                        &mut manager.modules[mod_idx].settings[set_idx].kind
                    {
                        *slot = value;
                        manager.dirty = true;
                    }
                }
            }
            return true;
        }

        // Close open Dropdown on ESC
        if self.open_dropdown.is_some() && key == "key.keyboard.escape" {
            self.open_dropdown = None;
            return true;
        }

        // Entity Picker modal keyboard handling
        if let Some(p) = &mut self.entity_picker {
            if key == "key.keyboard.escape" {
                self.entity_picker = None;
                return true;
            }
            if p.search_focused {
                if key == "key.keyboard.backspace" {
                    p.search.pop();
                    p.scroll = 0.0;
                    return true;
                }
                if key == "key.keyboard.enter" {
                    p.search_focused = false;
                    return true;
                }
            }
            return true;
        }

        if self.search_focused {
            match key {
                "key.keyboard.escape" | "key.keyboard.enter" => {
                    self.search_focused = false;
                    return true;
                }
                "key.keyboard.backspace" => {
                    self.search.pop();
                    return true;
                }
                _ => {}
            }
        }

        false
    }

    pub fn char_typed(&mut self, c: char) -> bool {
        if self.listening_bind.is_some() {
            return true;
        }
        if let Some(p) = &mut self.entity_picker {
            if !c.is_control() && p.search.len() < 32 {
                p.search_focused = true;
                p.search.push(c);
                p.scroll = 0.0;
                return true;
            }
            return false;
        }
        if self.search_focused && !c.is_control() && self.search.len() < 32 {
            self.search.push(c);
            return true;
        }
        false
    }

    fn bring_to_front(&mut self, category: Category) {
        if let Some(pos) = self.panel_order.iter().position(|&c| c == category) {
            let item = self.panel_order.remove(pos);
            self.panel_order.push(item);
        }
    }
}

fn module_drawer_height(m: &crate::module::Module, include_module_bind: bool) -> f32 {
    let mut h = if include_module_bind && m.id != ModuleId::ClickGui {
        SETTING_H
    } else {
        0.0
    };
    let mut last_group: Option<&str> = None;
    for s in &m.settings {
        if let Some(grp) = s.group
            && last_group != Some(grp)
        {
            last_group = Some(grp);
            h += GROUP_H;
        }
        h += SETTING_H;
        if let SettingKind::Color { expanded: true, .. } = s.kind {
            h += COLOR_DRAWER_H;
        }
    }
    h + 2.0
}

fn set_color_channel(setting: &mut crate::settings::Setting, channel: u8, frac: f32) {
    let byte = (frac.clamp(0.0, 1.0) * 255.0).round() as u32;
    if let SettingKind::Color { value, .. } = &mut setting.kind {
        *value = match channel {
            0 => (*value & 0xFF00_FFFF) | (byte << 16), // R
            1 => (*value & 0xFFFF_00FF) | (byte << 8),  // G
            2 => (*value & 0xFFFF_FF00) | byte,         // B
            _ => (*value & 0x00FF_FFFF) | (byte << 24), // A
        };
    }
}

fn draw_outline(draw: &mut dyn DrawContext, rect: [f32; 4], argb: u32) {
    let [x, y, w, h] = rect;
    let t = 1.0;
    draw.fill([x, y, w, t], argb);
    draw.fill([x, y + h - t, w, t], argb);
    draw.fill([x, y, t, h], argb);
    draw.fill([x + w - t, y, t, h], argb);
}

fn rect_contains(rect: [f32; 4], point: [f32; 2]) -> bool {
    point[0] >= rect[0]
        && point[0] <= rect[0] + rect[2]
        && point[1] >= rect[1]
        && point[1] <= rect[1] + rect[3]
}

fn intersect_rects(a: [f32; 4], b: [f32; 4]) -> Option<[f32; 4]> {
    let x0 = a[0].max(b[0]);
    let y0 = a[1].max(b[1]);
    let x1 = (a[0] + a[2]).min(b[0] + b[2]);
    let y1 = (a[1] + a[3]).min(b[1] + b[3]);
    if x1 > x0 && y1 > y0 {
        Some([x0, y0, x1 - x0, y1 - y0])
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_and_keybind_listening_work() {
        let mut mgr = ModuleManager::new();
        let mut gui = ClickGuiState {
            search_focused: true,
            ..Default::default()
        };
        assert!(gui.char_typed('a'));
        assert!(gui.char_typed('i'));
        assert_eq!(gui.search, "ai");
        assert!(gui.key_pressed("key.keyboard.backspace", &mut mgr));
        assert_eq!(gui.search, "a");

        gui.listening_bind = Some(BindTarget::Module(0));
        assert!(gui.is_listening());
        assert!(gui.key_pressed("key.keyboard.v", &mut mgr));
        assert!(!gui.is_listening());
        assert_eq!(mgr.modules[0].keybind.as_deref(), Some("key.keyboard.v"));
    }

    #[test]
    fn entity_picker_search_and_escape_work() {
        let mut mgr = ModuleManager::new();
        let mut gui = ClickGuiState {
            entity_picker: Some(EntityPickerState {
                mod_idx: 0,
                set_idx: 0,
                search: String::new(),
                search_focused: true,
                scroll: 0.0,
                filter: PickerFilter::All,
            }),
            ..Default::default()
        };
        assert!(gui.char_typed('z'));
        assert!(gui.char_typed('o'));
        assert_eq!(gui.entity_picker.as_ref().unwrap().search, "zo");
        assert!(gui.key_pressed("key.keyboard.escape", &mut mgr));
        assert!(gui.entity_picker.is_none());
    }

    #[test]
    fn single_module_accordion_and_drawer_animation() {
        let mut mgr = ModuleManager::new();
        let mut gui = ClickGuiState::default();
        gui.hits.push(HitRegion {
            rect: [0.0, 0.0, 100.0, 20.0],
            kind: HitKind::ModuleRow(0),
            tooltip: None,
        });
        assert!(gui.mouse_pressed([10.0, 10.0], 1, &mut mgr));
        assert!(mgr.modules[0].expanded);

        gui.hits.clear();
        gui.hits.push(HitRegion {
            rect: [0.0, 0.0, 100.0, 20.0],
            kind: HitKind::ModuleRow(1),
            tooltip: None,
        });
        assert!(gui.mouse_pressed([10.0, 10.0], 1, &mut mgr));
        assert!(!mgr.modules[0].expanded);
        assert!(mgr.modules[1].expanded);

        gui.step_animations(1000, &mgr);
        gui.step_animations(1040, &mgr);
        assert!(gui.drawer_ease(1) > 0.0);
        gui.step_animations(1090, &mgr);
        gui.step_animations(1140, &mgr);
        gui.step_animations(1190, &mgr);
        assert!((gui.drawer_ease(1) - 1.0).abs() < 1e-4);
        assert!(gui.drawer_ease(0) < 1e-4);
    }
}
