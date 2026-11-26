//! Custom TrueType & bitmap font helpers built on Horizon's `DrawContext::gui_paint`.
//!
//! Ships crisp 2x-oversampled TrueType fonts under `assets/star/font/` (matching
//! Horizon's `FilterMode::Nearest` HUD atlas without nearest-neighbor decimation):
//! `JetBrains Mono`, `Comfortaa`, `Tw Cen MT`, `Pixelation`, `Geist`, `Geist Bold`,
//! `Geist Mono`, `Bricolage`, `Pixelify`, plus vanilla `Minecraft`.

use mod_api::client::hud_layout::PoseOp;
use mod_api::text::TextStyle;
use mod_api::{DrawContext, GuiDraw, Text};

use crate::font_metrics;
use crate::module::{ModuleId, ModuleManager};

pub const FONT_PRESETS: &[&str] = &[
    "JetBrains Mono",
    "Comfortaa",
    "Tw Cen MT",
    "Pixelation",
    "Geist",
    "Geist Bold",
    "Geist Mono",
    "Bricolage",
    "Pixelify",
    "Minecraft",
];

/// Index into [`FONT_PRESETS`] of the font used until the player picks another (Minecraft).
pub const DEFAULT_FONT_INDEX: usize = 9;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FontFace {
    JetBrainsMono,
    Comfortaa,
    TwCenMt,
    Pixelation,
    Geist,
    GeistBold,
    GeistMono,
    Bricolage,
    Pixelify,
    Minecraft,
}

impl FontFace {
    pub fn from_index(idx: usize) -> Self {
        match idx {
            0 => Self::JetBrainsMono,
            1 => Self::Comfortaa,
            2 => Self::TwCenMt,
            3 => Self::Pixelation,
            4 => Self::Geist,
            5 => Self::GeistBold,
            6 => Self::GeistMono,
            7 => Self::Bricolage,
            8 => Self::Pixelify,
            9 => Self::Minecraft,
            _ => Self::Minecraft,
        }
    }

    /// Resource ID of the font provider JSON under `assets/star/font/`.
    /// Uses dedicated real TrueType Bold/SemiBold faces where bundled so headers
    /// stay razor-sharp instead of using Minecraft's 1px double-draw synthetic bold.
    pub fn font_id(self, bold: bool) -> Option<&'static str> {
        match self {
            Self::JetBrainsMono if bold => Some("star:jetbrains_mono_bold"),
            Self::JetBrainsMono => Some("star:jetbrains_mono"),
            Self::Comfortaa if bold => Some("star:comfortaa_bold"),
            Self::Comfortaa => Some("star:comfortaa"),
            Self::TwCenMt => Some("star:tw_cen_mt"),
            Self::Pixelation => Some("star:pixelation"),
            Self::Geist if bold => Some("star:geist_semibold"),
            Self::Geist => Some("star:geist"),
            Self::GeistBold => Some("star:geist_semibold"),
            Self::GeistMono if bold => Some("star:geist_mono_bold"),
            Self::GeistMono => Some("star:geist_mono"),
            Self::Bricolage if bold => Some("star:bricolage_bold"),
            Self::Bricolage => Some("star:bricolage"),
            Self::Pixelify if bold => Some("star:pixelify_bold"),
            Self::Pixelify => Some("star:pixelify"),
            Self::Minecraft => None,
        }
    }

    /// Exact ASCII (32..=126) half-pixel advance table matching Horizon's `TrueType::advance`.
    fn ascii_advances_half_px(self, bold: bool) -> Option<&'static [u8; 95]> {
        match self {
            Self::JetBrainsMono if bold => Some(&font_metrics::JETBRAINS_MONO_BOLD),
            Self::JetBrainsMono => Some(&font_metrics::JETBRAINS_MONO),
            Self::Comfortaa if bold => Some(&font_metrics::COMFORTAA_BOLD),
            Self::Comfortaa => Some(&font_metrics::COMFORTAA),
            Self::TwCenMt => Some(&font_metrics::TW_CEN_MT),
            Self::Pixelation => Some(&font_metrics::PIXELATION),
            Self::Geist if bold => Some(&font_metrics::GEIST_SEMIBOLD),
            Self::Geist => Some(&font_metrics::GEIST),
            Self::GeistBold => Some(&font_metrics::GEIST_SEMIBOLD),
            Self::GeistMono if bold => Some(&font_metrics::GEIST_MONO_BOLD),
            Self::GeistMono => Some(&font_metrics::GEIST_MONO),
            Self::Bricolage if bold => Some(&font_metrics::BRICOLAGE_BOLD),
            Self::Bricolage => Some(&font_metrics::BRICOLAGE),
            Self::Pixelify if bold => Some(&font_metrics::PIXELIFY_BOLD),
            Self::Pixelify => Some(&font_metrics::PIXELIFY),
            Self::Minecraft => None,
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
            face: FontFace::Minecraft,
            scale: 1.00,
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
            face: FontFace::from_index(gui.mode("font_face", DEFAULT_FONT_INDEX)),
            scale: (gui.float("font_scale", 1.00) as f32).clamp(0.70, 1.35),
            shadow: gui.bool("font_shadow", false),
            bold_headers: gui.bool("bold_headers", true),
            italic_values: gui.bool("italic_values", false),
        }
    }

    /// Returns a copy with a different font face (used for live previews inside the Font dropdown).
    pub fn with_face(self, face: FontFace) -> Self {
        Self { face, ..self }
    }

    /// Returns a copy scaled by `factor` (e.g. for floating nametags).
    pub fn scaled(self, factor: f32) -> Self {
        Self {
            scale: (self.scale * factor).clamp(0.50, 2.0),
            ..self
        }
    }

    /// Visual glyph cap height in GUI pixels (baseline sits at `7.0 * scale` for both
    /// vanilla bitmap glyphs and our calibrated TrueType faces with `shift: [0, 0]`).
    pub fn line_height(self) -> f32 {
        7.0 * self.scale
    }

    /// Vertically centered Y coordinate for a single line inside `[row_y, row_y + row_h]`,
    /// snapped to whole GUI pixels so Horizon's nearest-sampled atlas stays crisp.
    pub fn center_y(self, row_y: f32, row_h: f32) -> f32 {
        (row_y + (row_h - self.line_height()) * 0.5).round()
    }

    /// Exact rendered width of `text` in GUI pixels.
    ///
    /// Uses pre-extracted Horizon `TrueType::advance` tables for every bundled TrueType
    /// face so layout, right-aligned badges, and top-bar buttons never clip or overlap.
    pub fn width(self, draw: &dyn DrawContext, text: &str, bold: bool) -> f32 {
        if text.is_empty() {
            return 0.0;
        }
        if let Some(table) = self.face.ascii_advances_half_px(bold) {
            let mut total = 0.0f32;
            let mut non_ascii = String::new();
            for ch in text.chars() {
                let code = ch as u32;
                if (32..=126).contains(&code) {
                    total += table[(code - 32) as usize] as f32 * 0.5;
                } else {
                    non_ascii.push(ch);
                }
            }
            if !non_ascii.is_empty() {
                total += draw.text_width(&non_ascii);
            }
            return total * self.scale;
        }

        // Vanilla Minecraft bitmap font (1px synthetic bold offset per glyph).
        let base = draw.text_width(text);
        let bold_extra = if bold {
            text.chars().count() as f32
        } else {
            0.0
        };
        (base + bold_extra) * self.scale
    }

    /// Draws styled text using Horizon's `GuiDraw::Text`.
    ///
    /// At native `1.00x` scale, emits `GuiDraw::Text` directly at integer GUI coordinates
    /// with no matrix transform so every glyph maps cleanly onto Horizon's HUD atlas.
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

        let px = pos[0].round();
        let py = pos[1].round();
        let native_scale = (self.scale - 1.0).abs() < 1e-3;

        if self.face == FontFace::Minecraft && native_scale && !bold && !italic {
            draw.text(text, [px, py], argb, self.shadow);
            return;
        }

        let mut component = Text::literal(text);
        let mut style = TextStyle::default();
        if let Some(font_id) = self.face.font_id(bold) {
            style.font = Some(font_id.to_owned());
        } else if bold {
            // Only use Minecraft's synthetic 1px double-draw bold for the vanilla bitmap font;
            // TrueType faces either use a dedicated Bold/SemiBold face or render single-pass.
            style.bold = Some(true);
        }
        if italic {
            style.italic = Some(true);
        }
        component.style = style;

        if native_scale {
            let cmds = [GuiDraw::Text {
                text: component,
                x: px as i32,
                y: py as i32,
                color: argb,
                shadow: self.shadow,
            }];
            if draw.gui_paint(&cmds).is_err() {
                draw.text(text, [px, py], argb, self.shadow);
            }
            return;
        }

        let ops = vec![
            PoseOp::Translate(px, py),
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
            draw.text(text, [px, py], argb, self.shadow);
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

#[cfg(test)]
mod tests {
    use super::*;

    struct DummyDraw;

    impl DrawContext for DummyDraw {
        fn text_width(&self, text: &str) -> f32 {
            text.chars().count() as f32 * 6.0
        }
    }

    #[test]
    fn monospace_width_is_exact_and_uniform() {
        let draw = DummyDraw;
        assert_eq!(FontConfig::default().face, FontFace::Minecraft);
        let font = FontConfig::default().with_face(FontFace::JetBrainsMono);
        assert!((font.scale - 1.0).abs() < 1e-6);
        assert_eq!(font.width(&draw, "M", false), font.width(&draw, "i", false));
        assert_eq!(font.width(&draw, "Reset Layout", false), 66.0);
        assert_eq!(font.width(&draw, "[C]", false), 16.5);
    }

    #[test]
    fn bold_true_type_faces_map_without_synthetic_bold() {
        assert_eq!(
            FontFace::JetBrainsMono.font_id(true),
            Some("star:jetbrains_mono_bold")
        );
        assert_eq!(
            FontFace::Comfortaa.font_id(true),
            Some("star:comfortaa_bold")
        );
        assert_eq!(FontFace::Geist.font_id(true), Some("star:geist_semibold"));
        assert_eq!(
            FontFace::GeistMono.font_id(true),
            Some("star:geist_mono_bold")
        );
        assert_eq!(
            FontFace::Bricolage.font_id(true),
            Some("star:bricolage_bold")
        );
        assert_eq!(FontFace::Pixelify.font_id(true), Some("star:pixelify_bold"));
    }

    #[test]
    fn center_y_snaps_to_integer_pixels() {
        let font = FontConfig::default();
        let y = font.center_y(0.0, 18.0);
        assert_eq!(y.fract(), 0.0);
        assert_eq!(y, 6.0);
    }
}
