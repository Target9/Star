//! Custom font & typography helpers built on Horizon's `DrawContext::gui_paint`.
//!
//! Defaults to Minecraft's smooth thin `minecraft:uniform` font at `0.90x` scale
//! so panels and HUD overlays stay crisp and uncluttered.

use mod_api::client::hud_layout::PoseOp;
use mod_api::text::TextStyle;
use mod_api::{DrawContext, GuiDraw, Text};

use crate::module::{ModuleId, ModuleManager};

pub const FONT_PRESETS: &[&str] = &["Smooth", "Minecraft", "Illager", "Galactic"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FontFace {
    Smooth,
    Minecraft,
    Illager,
    Galactic,
}

impl FontFace {
    pub fn from_index(idx: usize) -> Self {
        match idx {
            0 => Self::Smooth,
            1 => Self::Minecraft,
            2 => Self::Illager,
            3 => Self::Galactic,
            _ => Self::Smooth,
        }
    }

    pub fn font_id(self) -> Option<&'static str> {
        match self {
            Self::Smooth => Some("minecraft:uniform"),
            Self::Minecraft => None,
            Self::Illager => Some("minecraft:illageralt"),
            Self::Galactic => Some("minecraft:alt"),
        }
    }

    /// Approximate advance ratio relative to `minecraft:default` (`DrawContext::text_width`).
    pub fn width_ratio(self) -> f32 {
        match self {
            Self::Smooth => 0.84,
            Self::Minecraft => 1.0,
            Self::Illager => 1.0,
            Self::Galactic => 1.0,
        }
    }
}

/// Active typography configuration resolved from the Client Settings (`ClickGUI`) module.
#[derive(Clone, Copy, Debug)]
pub struct FontConfig {
    pub face: FontFace,
    pub scale: f32,
    pub shadow: bool,
    pub bold_headers: bool,
    pub italic_values: bool,
}

impl Default for FontConfig {
    fn default() -> Self {
        Self {
            face: FontFace::Smooth,
            scale: 0.90,
            shadow: false,
            bold_headers: true,
            italic_values: false,
        }
    }
}

impl FontConfig {
    pub fn from_manager(manager: &ModuleManager) -> Self {
        let gui = manager.get(ModuleId::ClickGui);
        Self {
            face: FontFace::from_index(gui.mode("font_face", 0)),
            scale: (gui.float("font_scale", 0.90) as f32).clamp(0.65, 1.35),
            shadow: gui.bool("font_shadow", false),
            bold_headers: gui.bool("bold_headers", true),
            italic_values: gui.bool("italic_values", false),
        }
    }

    /// Returns a copy scaled by `factor` (e.g. for HUD badges or floating nametags).
    pub fn scaled(self, factor: f32) -> Self {
        Self {
            scale: (self.scale * factor).clamp(0.50, 2.0),
            ..self
        }
    }

    /// Rendered glyph line height in GUI pixels.
    pub fn line_height(self) -> f32 {
        8.5 * self.scale
    }

    /// Vertically centered Y coordinate for a single line inside a row `[row_y, row_y + row_h]`.
    pub fn center_y(self, row_y: f32, row_h: f32) -> f32 {
        row_y + (row_h - self.line_height()) * 0.5 + 0.5
    }

    /// Estimated rendered width of `text` in GUI pixels.
    pub fn width(self, draw: &dyn DrawContext, text: &str, bold: bool) -> f32 {
        if text.is_empty() {
            return 0.0;
        }
        let base = draw.text_width(text);
        let bold_extra = if bold {
            text.chars().count() as f32 * 0.65
        } else {
            0.0
        };
        (base * self.face.width_ratio() + bold_extra) * self.scale
    }

    /// Draws styled text using Horizon's `GuiDraw::Text` pose stack.
    pub fn draw(
        self,
        draw: &mut dyn DrawContext,
        text: &str,
        pos: [f32; 2],
        argb: u32,
        bold: bool,
        italic: bool,
    ) {
        if text.is_empty() || (argb >> 24) == 0 {
            return;
        }

        if self.face == FontFace::Minecraft
            && (self.scale - 1.0).abs() < 1e-3
            && !bold
            && !italic
        {
            draw.text(text, pos, argb, self.shadow);
            return;
        }

        let mut component = Text::literal(text);
        let mut style = TextStyle::default();
        if let Some(font_id) = self.face.font_id() {
            style.font = Some(font_id.to_owned());
        }
        if bold {
            style.bold = Some(true);
        }
        if italic {
            style.italic = Some(true);
        }
        component.style = style;

        let ops = vec![
            PoseOp::Translate(pos[0], pos[1]),
            PoseOp::Scale(self.scale, self.scale),
        ];
        let cmds = [
            GuiDraw::Push(ops),
            GuiDraw::Text {
                text: component,
                x: 0,
                y: 0,
                color: argb,
                shadow: self.shadow,
            },
            GuiDraw::Pop,
        ];
        if draw.gui_paint(&cmds).is_err() {
            draw.text(text, pos, argb, self.shadow);
        }
    }

    pub fn draw_header(self, draw: &mut dyn DrawContext, text: &str, pos: [f32; 2], argb: u32) {
        self.draw(draw, text, pos, argb, self.bold_headers, false);
    }

    pub fn draw_body(self, draw: &mut dyn DrawContext, text: &str, pos: [f32; 2], argb: u32) {
        self.draw(draw, text, pos, argb, false, false);
    }

    pub fn draw_value(self, draw: &mut dyn DrawContext, text: &str, pos: [f32; 2], argb: u32) {
        self.draw(draw, text, pos, argb, false, self.italic_values);
    }

    /// Truncates `text` with `".."` when it would exceed `max_width` GUI pixels.
    pub fn fit(self, draw: &dyn DrawContext, text: &str, max_width: f32, bold: bool) -> String {
        if max_width <= 4.0 {
            return String::new();
        }
        if self.width(draw, text, bold) <= max_width {
            return text.to_owned();
        }
        let ellipsis = "..";
        let ell_w = self.width(draw, ellipsis, bold);
        let budget = (max_width - ell_w).max(0.0);
        let mut out = String::new();
        for ch in text.chars() {
            out.push(ch);
            if self.width(draw, &out, bold) > budget {
                out.pop();
                break;
            }
        }
        out.push_str(ellipsis);
        out
    }

    /// Draws `text` clipped to `max_width` so labels never collide with right-aligned values.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_fitted(
        self,
        draw: &mut dyn DrawContext,
        text: &str,
        pos: [f32; 2],
        max_width: f32,
        argb: u32,
        bold: bool,
        italic: bool,
    ) {
        let fitted = self.fit(draw, text, max_width, bold);
        self.draw(draw, &fitted, pos, argb, bold, italic);
    }
}
