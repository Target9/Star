//! Movement modules: Sprint, Toggle Sneak, and Auto Walk.
//!
//! All three work through Horizon's `movement_input` hook, which runs after the
//! local player's input is read and before it is applied. They only change the
//! player's own client-side input (the same keys you could hold yourself).
//!
//! Registering the hook has a side effect: while any listener exists, Horizon
//! holds sprint off whenever the final `sprint` flag is false and disables the
//! vanilla double-tap latch. [`MoveInputState`] therefore re-implements the
//! vanilla latch (sprint key or double-tap forward) so that installing Star does
//! not change how sprinting feels when none of these modules is enabled.

use mod_api::MovementInputEvent;

use crate::module::{Category, Module, ModuleId};
use crate::settings::Setting;

/// Window in which a second forward press counts as a double tap.
const DOUBLE_TAP_MS: u64 = 300;

pub fn build_sprint_module() -> Module {
    Module {
        id: ModuleId::Sprint,
        key: "sprint",
        name: "Sprint",
        description: "Keeps you sprinting while you move, without holding the sprint key.",
        category: Category::Movement,
        enabled: false,
        keybind: None,
        expanded: false,
        show_in_arraylist: true,
        settings: vec![
            Setting::mode(
                "mode",
                "Mode",
                "Forward sprints only when walking forward; Omni sprints in any direction.",
                &["Forward", "Omni"],
                0,
            )
            .group("Sprint"),
        ],
    }
}

pub fn build_toggle_sneak_module() -> Module {
    Module {
        id: ModuleId::ToggleSneak,
        key: "toggle_sneak",
        name: "Toggle Sneak",
        description: "Holds sneak for you while enabled. Bind a key to switch it on and off.",
        category: Category::Movement,
        enabled: false,
        keybind: None,
        expanded: false,
        show_in_arraylist: true,
        settings: Vec::new(),
    }
}

pub fn build_auto_walk_module() -> Module {
    Module {
        id: ModuleId::AutoWalk,
        key: "auto_walk",
        name: "Auto Walk",
        description: "Walks forward without holding W. Hold S to pause it.",
        category: Category::Movement,
        enabled: false,
        keybind: None,
        expanded: false,
        show_in_arraylist: true,
        settings: Vec::new(),
    }
}

/// How the Sprint module chooses when to request sprint.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SprintMode {
    Forward,
    Omni,
}

/// A snapshot of the movement modules, taken once per input event.
#[derive(Clone, Copy, Debug, Default)]
pub struct MoveConfig {
    pub sprint: Option<SprintMode>,
    pub toggle_sneak: bool,
    pub auto_walk: bool,
}

impl MoveConfig {
    pub fn from_modules(sprint: &Module, sneak: &Module, walk: &Module) -> Self {
        Self {
            sprint: sprint.enabled.then(|| {
                if sprint.mode("mode", 0) == 1 {
                    SprintMode::Omni
                } else {
                    SprintMode::Forward
                }
            }),
            toggle_sneak: sneak.enabled,
            auto_walk: walk.enabled,
        }
    }
}

/// Per-session input memory used to emulate vanilla's sprint latch.
#[derive(Debug, Default)]
pub struct MoveInputState {
    latch: bool,
    prev_forward: bool,
    last_forward_press_ms: Option<u64>,
}

impl MoveInputState {
    /// Rewrites one movement-input event according to `cfg`.
    pub fn apply(&mut self, cfg: MoveConfig, event: &mut MovementInputEvent, now_ms: u64) {
        let raw_forward = event.forward_key;

        // Vanilla sprint latch: the sprint key or a double tap on forward starts a sprint
        // that lasts until you stop pressing forward or start sneaking.
        if raw_forward && !self.prev_forward {
            if self
                .last_forward_press_ms
                .is_some_and(|t| now_ms.saturating_sub(t) <= DOUBLE_TAP_MS)
            {
                self.latch = true;
            }
            self.last_forward_press_ms = Some(now_ms);
        }
        if event.sprint {
            self.latch = true;
        }
        if !raw_forward || event.sneak {
            self.latch = false;
        }
        self.prev_forward = raw_forward;

        if cfg.toggle_sneak {
            event.sneak = true;
        }

        if cfg.auto_walk && !event.backward_key {
            event.forward_key = true;
            event.forward = 1.0;
        }

        let moving_forward = event.forward_key;
        let moving_any =
            moving_forward || event.backward_key || event.left_key || event.right_key;

        let want_sprint = match cfg.sprint {
            Some(SprintMode::Forward) => moving_forward,
            Some(SprintMode::Omni) => moving_any,
            None => false,
        } && !event.sneak;

        event.sprint = want_sprint || (self.latch && moving_forward && !event.sneak);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(forward: bool) -> MovementInputEvent {
        MovementInputEvent {
            forward: if forward { 1.0 } else { 0.0 },
            strafe: 0.0,
            jump: false,
            sneak: false,
            forward_key: forward,
            backward_key: false,
            left_key: false,
            right_key: false,
            sprint: false,
        }
    }

    #[test]
    fn passive_state_does_not_sprint_without_input() {
        let mut state = MoveInputState::default();
        let mut ev = event(true);
        state.apply(MoveConfig::default(), &mut ev, 0);
        assert!(!ev.sprint);
        assert!(ev.forward_key);
    }

    #[test]
    fn sprint_key_latches_until_forward_released() {
        let mut state = MoveInputState::default();
        let mut ev = event(true);
        ev.sprint = true;
        state.apply(MoveConfig::default(), &mut ev, 0);
        assert!(ev.sprint);

        let mut ev = event(true);
        state.apply(MoveConfig::default(), &mut ev, 50);
        assert!(ev.sprint, "latch survives after the sprint key is released");

        let mut ev = event(false);
        state.apply(MoveConfig::default(), &mut ev, 100);
        assert!(!ev.sprint);
    }

    #[test]
    fn double_tap_forward_starts_sprint() {
        let mut state = MoveInputState::default();
        state.apply(MoveConfig::default(), &mut event(true), 0);
        state.apply(MoveConfig::default(), &mut event(false), 100);
        let mut ev = event(true);
        state.apply(MoveConfig::default(), &mut ev, 200);
        assert!(ev.sprint);
    }

    #[test]
    fn slow_second_tap_does_not_sprint() {
        let mut state = MoveInputState::default();
        state.apply(MoveConfig::default(), &mut event(true), 0);
        state.apply(MoveConfig::default(), &mut event(false), 100);
        let mut ev = event(true);
        state.apply(MoveConfig::default(), &mut ev, 900);
        assert!(!ev.sprint);
    }

    #[test]
    fn sprint_module_sprints_forward_but_not_while_sneaking() {
        let cfg = MoveConfig {
            sprint: Some(SprintMode::Forward),
            ..MoveConfig::default()
        };
        let mut state = MoveInputState::default();
        let mut ev = event(true);
        state.apply(cfg, &mut ev, 0);
        assert!(ev.sprint);

        let mut ev = event(true);
        ev.sneak = true;
        state.apply(cfg, &mut ev, 20);
        assert!(!ev.sprint);
    }

    #[test]
    fn omni_sprints_sideways_and_forward_mode_does_not() {
        let mut ev = event(false);
        ev.left_key = true;
        let mut omni = MoveInputState::default();
        omni.apply(
            MoveConfig {
                sprint: Some(SprintMode::Omni),
                ..MoveConfig::default()
            },
            &mut ev,
            0,
        );
        assert!(ev.sprint);

        let mut ev = event(false);
        ev.left_key = true;
        let mut fwd = MoveInputState::default();
        fwd.apply(
            MoveConfig {
                sprint: Some(SprintMode::Forward),
                ..MoveConfig::default()
            },
            &mut ev,
            0,
        );
        assert!(!ev.sprint);
    }

    #[test]
    fn auto_walk_holds_forward_until_backward_pressed() {
        let cfg = MoveConfig {
            auto_walk: true,
            ..MoveConfig::default()
        };
        let mut state = MoveInputState::default();
        let mut ev = event(false);
        state.apply(cfg, &mut ev, 0);
        assert!(ev.forward_key);
        assert!((ev.forward - 1.0).abs() < 1e-6);

        let mut ev = event(false);
        ev.backward_key = true;
        state.apply(cfg, &mut ev, 20);
        assert!(!ev.forward_key);
    }

    #[test]
    fn toggle_sneak_forces_sneak() {
        let cfg = MoveConfig {
            toggle_sneak: true,
            ..MoveConfig::default()
        };
        let mut ev = event(false);
        MoveInputState::default().apply(cfg, &mut ev, 0);
        assert!(ev.sneak);
    }
}
