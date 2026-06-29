//! System modes — named bundles of reversible [`Action`]s plus a power plan.
//!
//! Activating a mode flips the machine's personality in one click: it reverts
//! whatever mode was active, applies its own HKCU registry actions (through the
//! same journaled engine as tweaks, so every change is undoable), and switches
//! the power scheme (best-effort, unelevated). The previous power scheme is
//! saved so switching away restores it. The active mode id lives in a marker so
//! it survives restarts.

use crate::core::action::{Action, Hive, RegValue};
use crate::core::registry;

pub struct Mode {
    /// Stable id stored in the marker: "ai" | "game" | "work".
    pub id: &'static str,
    pub name: &'static str,
    pub tagline: &'static str,
    pub desc: &'static str,
    /// Reversible HKCU registry actions applied while the mode is active.
    pub actions: Vec<Action>,
    /// Power scheme GUID to activate (best-effort, unelevated), or None.
    pub power_guid: Option<&'static str>,
}

const STATE_PATH: &str = "Software\\NeonPrime\\State";
const MARKER: &str = "ActiveMode";
const PREV_POWER: &str = "PrevPowerGuid";

// Standard, always-present power schemes.
pub const BALANCED: &str = "381b4222-f694-41f0-9685-ff5bb260df2e";
pub const HIGH_PERF: &str = "8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c";

// HKCU roots the mode bundles touch.
const GAME_CFG: &str = "System\\GameConfigStore";
const GAME_BAR: &str = "Software\\Microsoft\\GameBar";
const BG_APPS: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\BackgroundAccessApplications";
const PUSH: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\PushNotifications";

fn dw(path: &str, name: &str, v: u32) -> Action {
    Action::SetReg { hive: Hive::Hkcu, path: path.into(), name: name.into(), value: RegValue::Dword(v) }
}

pub fn catalog() -> Vec<Mode> {
    vec![
        Mode {
            id: "ai",
            name: "AI / Inference",
            tagline: "GPU unleashed",
            desc: "High-performance power, GPU freed (Game DVR off), background apps suspended.",
            actions: vec![
                dw(GAME_CFG, "GameDVR_Enabled", 0),
                dw(BG_APPS, "GlobalUserDisabled", 1),
            ],
            power_guid: Some(HIGH_PERF),
        },
        Mode {
            id: "game",
            name: "Game",
            tagline: "Frames first",
            desc: "High-performance power, Game Mode on, Game DVR and background recording off.",
            actions: vec![
                dw(GAME_CFG, "GameDVR_Enabled", 0),
                dw(GAME_BAR, "AutoGameModeEnabled", 1),
                dw(GAME_BAR, "ShowStartupPanel", 0),
            ],
            power_guid: Some(HIGH_PERF),
        },
        Mode {
            id: "work",
            name: "Work",
            tagline: "Calm & balanced",
            desc: "Balanced power and toast notifications silenced — the quiet profile.",
            actions: vec![dw(PUSH, "ToastEnabled", 0)],
            power_guid: Some(BALANCED),
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

/// Record which mode is active (state, not a journaled tweak).
pub fn set_marker(id: &str) {
    let _ = registry::write(Hive::Hkcu, STATE_PATH, MARKER, &RegValue::Sz(id.into()));
}

pub fn clear_marker() {
    let _ = registry::delete(Hive::Hkcu, STATE_PATH, MARKER);
}

/// Remember the pre-mode power scheme so switching away can restore it.
pub fn save_prev_power(guid: &str) {
    let _ = registry::write(Hive::Hkcu, STATE_PATH, PREV_POWER, &RegValue::Sz(guid.into()));
}

/// Take (read + clear) the saved pre-mode power scheme.
pub fn take_prev_power() -> Option<String> {
    let v = match registry::read(Hive::Hkcu, STATE_PATH, PREV_POWER) {
        Ok(Some(RegValue::Sz(s))) => Some(s),
        _ => None,
    };
    let _ = registry::delete(Hive::Hkcu, STATE_PATH, PREV_POWER);
    v
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
    fn every_mode_has_actions_and_a_power_plan() {
        for m in catalog() {
            assert!(!m.actions.is_empty(), "{} has no actions", m.id);
            assert!(m.power_guid.is_some(), "{} has no power plan", m.id);
            // Mode actions are HKCU — no elevation, so activation never prompts UAC.
            assert!(m.actions.iter().all(|a| !a.needs_elevation()));
        }
    }

    #[test]
    fn marker_roundtrips() {
        set_marker("game");
        assert_eq!(active().as_deref(), Some("game"));
        clear_marker();
        assert_eq!(active(), None);
    }
}
