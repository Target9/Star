//! Short on-screen notifications (module toggled, profile loaded, error recovered).
//!
//! Any code can call [`notify`] without holding the client state lock; the HUD drains the
//! queue each frame, stamps each toast with the frame time, fades it in and out, and stacks
//! the visible ones under the top edge of the screen.

use std::collections::VecDeque;
use std::sync::Mutex;

use mod_api::DrawContext;

use crate::font::FontConfig;
use crate::theme::{Theme, with_alpha};

const LIFETIME_MS: u64 = 3200;
const FADE_IN_MS: u64 = 180;
const FADE_OUT_MS: u64 = 420;
const MAX_VISIBLE: usize = 5;
const MAX_QUEUED: usize = 24;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToastKind {
    Info,
    Success,
    Warning,
    Error,
}

static PENDING: Mutex<Vec<(ToastKind, String)>> = Mutex::new(Vec::new());

/// Queues a toast. Safe to call from anywhere, including while handling a panic.
pub fn notify(kind: ToastKind, text: impl Into<String>) {
    let mut queue = PENDING.lock().unwrap_or_else(|p| p.into_inner());
    if queue.len() < MAX_QUEUED {
        queue.push((kind, text.into()));
    }
}

struct Toast {
    kind: ToastKind,
    text: String,
    born_ms: u64,
}

/// Visible toasts, owned by the client state.
#[derive(Default)]
pub struct Toasts {
    items: VecDeque<Toast>,
}

/// Opacity in `0.0..=1.0` of a toast that is `age_ms` old.
pub fn toast_alpha(age_ms: u64) -> f32 {
    if age_ms >= LIFETIME_MS {
        return 0.0;
    }
    let fade_in = (age_ms as f32 / FADE_IN_MS as f32).min(1.0);
    let remaining = LIFETIME_MS - age_ms;
    let fade_out = (remaining as f32 / FADE_OUT_MS as f32).min(1.0);
    fade_in.min(fade_out).clamp(0.0, 1.0)
}

impl Toasts {
    /// Moves queued toasts in, stamps them with `now_ms`, and drops expired ones.
    pub fn update(&mut self, now_ms: u64) {
        let drained: Vec<_> = {
            let mut queue = PENDING.lock().unwrap_or_else(|p| p.into_inner());
            std::mem::take(&mut *queue)
        };
        for (kind, text) in drained {
            // Collapse an identical toast that is already on screen instead of stacking it.
            if let Some(existing) = self
                .items
                .iter_mut()
                .find(|t| t.kind == kind && t.text == text)
            {
                existing.born_ms = now_ms;
                continue;
            }
            self.items.push_back(Toast {
                kind,
                text,
                born_ms: now_ms,
            });
        }
        self.items
            .retain(|t| now_ms.saturating_sub(t.born_ms) < LIFETIME_MS);
        while self.items.len() > MAX_VISIBLE {
            self.items.pop_front();
        }
    }

    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn clear(&mut self) {
        self.items.clear();
    }

    pub fn render(
        &self,
        draw: &mut dyn DrawContext,
        theme: &Theme,
        font: FontConfig,
        now_ms: u64,
    ) {
        if self.items.is_empty() {
            return;
        }
        let size = draw.size();
        let row_h = (font.line_height() + 8.0).max(16.0);
        let gap = 3.0;
        let mut y = 8.0;
        for toast in self.items.iter().rev() {
            let age = now_ms.saturating_sub(toast.born_ms);
            let alpha = toast_alpha(age);
            if alpha <= 0.01 {
                continue;
            }
            let a8 = (alpha * 255.0).round() as u8;
            let text_w = font.width(draw, &toast.text, false);
            let w = text_w + 18.0;
            let x = (size[0] - w) * 0.5;
            // Slide down slightly while fading in.
            let slide = (1.0 - (age as f32 / FADE_IN_MS as f32).min(1.0)) * -6.0;
            let ry = y + slide;
            let accent = match toast.kind {
                ToastKind::Info => theme.accent,
                ToastKind::Success => 0xFF3C_D070,
                ToastKind::Warning => 0xFFF5_A623,
                ToastKind::Error => 0xFFE5_3950,
            };
            let bg_alpha = ((theme.panel_bg >> 24) as f32 * alpha).round() as u8;
            draw.fill([x, ry, w, row_h], with_alpha(theme.panel_bg, bg_alpha.max(0x30)));
            draw.fill([x, ry, 2.0, row_h], with_alpha(accent, a8));
            font.draw_body(
                draw,
                &toast.text,
                [x + 9.0, font.center_y(ry, row_h)],
                with_alpha(theme.text, a8),
            );
            y += row_h + gap;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // The pending queue is process-global, so tests that use it must not interleave.
    static LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn alpha_fades_in_holds_and_fades_out() {
        assert!(toast_alpha(0) < 0.01);
        assert!((toast_alpha(FADE_IN_MS) - 1.0).abs() < 1e-3);
        assert!((toast_alpha(1500) - 1.0).abs() < 1e-3);
        let near_end = toast_alpha(LIFETIME_MS - FADE_OUT_MS / 2);
        assert!(near_end > 0.4 && near_end < 0.6, "got {near_end}");
        assert_eq!(toast_alpha(LIFETIME_MS), 0.0);
        assert_eq!(toast_alpha(LIFETIME_MS + 5000), 0.0);
    }

    #[test]
    fn update_stamps_dedupes_and_expires() {
        let _guard = LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let mut toasts = Toasts::default();
        // The queue is global; drain anything other tests left behind first.
        toasts.update(0);
        toasts.clear();

        notify(ToastKind::Info, "hello");
        notify(ToastKind::Info, "hello");
        toasts.update(1_000);
        assert_eq!(toasts.items.len(), 1);

        toasts.update(1_000 + LIFETIME_MS + 1);
        assert!(toasts.is_empty());
    }

    #[test]
    fn visible_toasts_are_capped() {
        let _guard = LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let mut toasts = Toasts::default();
        toasts.update(0);
        toasts.clear();
        for i in 0..(MAX_VISIBLE + 4) {
            notify(ToastKind::Warning, format!("toast-cap-{i}"));
        }
        toasts.update(10);
        assert!(toasts.items.len() <= MAX_VISIBLE);
    }
}
