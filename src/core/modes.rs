//! System modes — named bundles of [`Action`]s applied as a set.
//!
//! A mode flips the machine's whole personality with one click. Each mode's
//! `actions` are applied through the same reversible engine, and the active mode
//! is recorded in a registry marker so it survives restarts.
//!
//! For now the auto-applied actions are deliberately benign (just the marker),
//! so switching modes is safe and observable. Real power-plan, service, GPU, and
//! network actions plug into `actions` as new [`Action`] variants are added —
//! the apply/revert plumbing is already mode-agnostic.

use crate::core::action::{Action, Hive, RegValue};
use crate::core::registry;

pub struct Mode {
    /// Stable id stored in the marker: "ai" | "game" | "work".
    pub id: &'static str,
    pub name: &'static str,
    pub tagline: &'static str,
    pub desc: &'static str,
    pub actions: Vec<Action>,
}

const STATE_PATH: &str = "Software\\NeonPrime\\State";
const MARKER: &str = "ActiveMode";

fn marker(id: &str) -> Action {
    Action::SetReg {
        hive: Hive::Hkcu,
        path: STATE_PATH.into(),
        name: MARKER.into(),
        value: RegValue::Sz(id.into()),
    }
}

pub fn catalog() -> Vec<Mode> {
    vec![
        Mode {
            id: "ai",
            name: "AI / Inference",
            tagline: "GPU unleashed",
            desc: "Free the GPU for models, clear VRAM pressure, suspend background bloat.",
            actions: vec![marker("ai")],
        },
        Mode {
            id: "game",
            name: "Game",
            tagline: "Frames first",
            desc: "Game-process priority, latency-tuned networking, inference paused.",
            actions: vec![marker("game")],
        },
        Mode {
            id: "work",
            name: "Work",
            tagline: "Calm & balanced",
            desc: "Balanced power, notifications tamed — the quiet profile.",
            actions: vec![marker("work")],
        },
    ]
}

/// The currently-active mode id, read from the marker.
pub fn active() -> Option<String> {
    match registry::read(Hive::Hkcu, STATE_PATH, MARKER) {
        Ok(Some(RegValue::Sz(s))) => Some(s),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_has_three_modes_with_unique_ids() {
        let c = catalog();
        assert_eq!(c.len(), 3);
        let ids: Vec<_> = c.iter().map(|m| m.id).collect();
        assert_eq!(ids, ["ai", "game", "work"]);
    }

    #[test]
    fn activating_sets_marker() {
        use crate::core::engine;
        let c = catalog();
        for a in &c[1].actions {
            engine::apply(a).unwrap();
        }
        assert_eq!(active().as_deref(), Some("game"));
        for a in &c[0].actions {
            engine::apply(a).unwrap();
        }
        assert_eq!(active().as_deref(), Some("ai"));
        // cleanup
        let _ = registry::delete(Hive::Hkcu, STATE_PATH, MARKER);
    }
}
