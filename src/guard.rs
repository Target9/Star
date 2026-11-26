//! Panic isolation for Horizon hook handlers.
//!
//! A panic that unwinds out of a hook would take the whole game down, so every hook in
//! `lib.rs` runs through [`run`]. A caught panic is counted against the modules that hook
//! drives; after [`TRIP_AFTER`] panics a module is queued to be switched off (see
//! [`take_trips`], drained on the next client tick, which also shows a toast). The hook
//! itself falls back to its inert default, so the rest of the client keeps working.
//!
//! This relies on the mod being built with `panic = "unwind"` (Cargo's default).

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Mutex;

use crate::module::ModuleId;

/// Panics tolerated per module before it is switched off.
pub const TRIP_AFTER: u32 = 3;

#[derive(Default)]
struct Faults {
    counts: Vec<(ModuleId, u32)>,
    core_panics: u32,
    last_message: Option<String>,
    pending_trips: Vec<ModuleId>,
}

static FAULTS: Mutex<Faults> = Mutex::new(Faults {
    counts: Vec::new(),
    core_panics: 0,
    last_message: None,
    pending_trips: Vec::new(),
});

fn faults() -> std::sync::MutexGuard<'static, Faults> {
    FAULTS.lock().unwrap_or_else(|p| p.into_inner())
}

fn count_of(f: &Faults, id: ModuleId) -> u32 {
    f.counts
        .iter()
        .find(|(m, _)| *m == id)
        .map_or(0, |(_, c)| *c)
}

/// Runs `f`, converting a panic into `fallback`.
///
/// `owners` are the modules the closure drives; they are blamed for a panic. An empty list
/// marks core client code (input, GUI, config). A hook whose every owner has already
/// tripped is skipped entirely so a broken module cannot keep panicking every frame.
pub fn run<R>(owners: &[ModuleId], fallback: R, f: impl FnOnce() -> R) -> R {
    if !owners.is_empty() {
        let f = faults();
        if owners.iter().all(|id| count_of(&f, *id) >= TRIP_AFTER) {
            return fallback;
        }
    }
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(value) => value,
        Err(payload) => {
            record_panic(owners, panic_message(payload.as_ref()));
            fallback
        }
    }
}

fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_owned()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "unknown error".to_owned()
    }
}

fn record_panic(owners: &[ModuleId], message: String) {
    let mut f = faults();
    f.last_message = Some(message);
    if owners.is_empty() {
        f.core_panics = f.core_panics.saturating_add(1);
    }
    for &id in owners {
        if let Some(entry) = f.counts.iter_mut().find(|(m, _)| *m == id) {
            entry.1 = entry.1.saturating_add(1);
        } else {
            f.counts.push((id, 1));
        }
        if count_of(&f, id) == TRIP_AFTER {
            f.pending_trips.push(id);
        }
    }
}

/// Modules that just exceeded the panic budget and must be switched off.
pub fn take_trips() -> Vec<ModuleId> {
    std::mem::take(&mut faults().pending_trips)
}

/// Gives a module a fresh panic budget (called when the player re-enables it).
pub fn clear(id: ModuleId) {
    faults().counts.retain(|(m, _)| *m != id);
}

/// The message of the most recent caught panic, for diagnostics.
#[allow(dead_code)]
pub fn last_message() -> Option<String> {
    faults().last_message.clone()
}

/// Total panics caught in core (ownerless) code.
#[allow(dead_code)]
pub fn core_panics() -> u32 {
    faults().core_panics
}

#[cfg(test)]
mod tests {
    use super::*;

    // Each test uses a module id no other test touches, since the fault table is global.

    #[test]
    fn passes_through_return_value() {
        assert_eq!(run(&[ModuleId::NoFog], 0, || 7), 7);
    }

    #[test]
    fn panic_returns_fallback_and_trips_after_budget() {
        let id = ModuleId::Zoom;
        clear(id);
        take_trips();
        for _ in 0..TRIP_AFTER {
            let out = run(&[id], -1, || -> i32 { panic!("boom") });
            assert_eq!(out, -1);
        }
        assert!(take_trips().contains(&id));
        // Tripped hooks are skipped without running the closure.
        let mut ran = false;
        let out = run(&[id], 5, || {
            ran = true;
            1
        });
        assert_eq!(out, 5);
        assert!(!ran);
        // Re-enabling resets the budget.
        clear(id);
        assert_eq!(run(&[id], 5, || 1), 1);
        assert!(last_message().is_some());
    }

    #[test]
    fn hook_with_a_healthy_owner_still_runs() {
        let bad = ModuleId::Camera;
        let good = ModuleId::Radar;
        clear(bad);
        clear(good);
        for _ in 0..TRIP_AFTER {
            run(&[bad], (), || panic!("x"));
        }
        let mut ran = false;
        run(&[bad, good], (), || ran = true);
        assert!(ran);
        clear(bad);
    }
}
