//! The tweak catalog: named, reversible system tweaks built on [`Action`].
//!
//! Each tweak carries an explicit `on` set (apply) and `off` set (restore to the
//! Windows default), plus a `probe` that reports whether it's currently in
//! effect. Using an explicit default for `off` (rather than only the captured
//! prior value) means reverting is deterministic across restarts and even if a
//! tweak was applied outside NeonPrime.
//!
//! The first entry is a **sandbox** tweak that writes only under
//! `HKCU\Software\NeonPrime\Test` — applying it changes nothing the user sees,
//! so it's safe for automated end-to-end testing of the apply/revert pipeline.

use crate::core::action::{Action, Hive, RegValue};
use crate::core::registry;

#[derive(Clone, Copy, Debug)]
pub enum Category {
    Sandbox,
    Interface,
    Privacy,
    Performance,
}

impl Category {
    pub fn label(self) -> &'static str {
        match self {
            Category::Sandbox => "SANDBOX",
            Category::Interface => "INTERFACE",
            Category::Privacy => "PRIVACY",
            Category::Performance => "PERFORMANCE",
        }
    }
}

/// How to tell whether a tweak is currently applied: read `(hive,path,name)`
/// and compare against `applied`. `applied == None` means "applied if absent".
#[derive(Clone)]
pub struct Probe {
    pub hive: Hive,
    pub path: String,
    pub name: String,
    pub applied: Option<RegValue>,
}

#[derive(Clone)]
pub struct Tweak {
    pub id: &'static str,
    pub name: &'static str,
    pub desc: &'static str,
    pub category: Category,
    /// Actions that enable the tweak.
    pub on: Vec<Action>,
    /// Actions that restore the Windows default.
    pub off: Vec<Action>,
    pub probe: Probe,
}

impl Tweak {
    /// True if any action requires the elevated broker (HKLM).
    pub fn needs_elevation(&self) -> bool {
        self.on.iter().chain(&self.off).any(|a| a.needs_elevation())
    }

    /// Read live state and report whether the tweak is currently applied.
    pub fn is_applied(&self) -> bool {
        let current = registry::read(self.probe.hive, &self.probe.path, &self.probe.name)
            .unwrap_or(None);
        current == self.probe.applied
    }
}

// Common registry roots, kept as constants for readability.
const EXPLORER_ADV: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\Advanced";
const PERSONALIZE: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize";
const SEARCH: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Search";

fn set(hive: Hive, path: &str, name: &str, v: RegValue) -> Action {
    Action::SetReg { hive, path: path.into(), name: name.into(), value: v }
}
fn del(hive: Hive, path: &str, name: &str) -> Action {
    Action::DeleteReg { hive, path: path.into(), name: name.into() }
}

/// The full catalog. Index in this vec doubles as the UI row id.
pub fn catalog() -> Vec<Tweak> {
    use Category::*;
    use Hive::*;

    vec![
        // ── Sandbox (safe to toggle in automated tests) ──────────────
        Tweak {
            id: "sandbox-demo",
            name: "Demo toggle (safe sandbox)",
            desc: "Writes only to HKCU\\Software\\NeonPrime\\Test — proves the apply/undo pipeline, changes nothing real.",
            category: Sandbox,
            on: vec![set(Hkcu, "Software\\NeonPrime\\Test", "DemoTweak", RegValue::Dword(1))],
            off: vec![del(Hkcu, "Software\\NeonPrime\\Test", "DemoTweak")],
            probe: Probe { hive: Hkcu, path: "Software\\NeonPrime\\Test".into(), name: "DemoTweak".into(), applied: Some(RegValue::Dword(1)) },
        },
        // ── Interface (HKCU, no elevation) ───────────────────────────
        Tweak {
            id: "show-file-extensions",
            name: "Show file extensions",
            desc: "Reveal extensions for known file types in Explorer.",
            category: Interface,
            on: vec![set(Hkcu, EXPLORER_ADV, "HideFileExt", RegValue::Dword(0))],
            off: vec![set(Hkcu, EXPLORER_ADV, "HideFileExt", RegValue::Dword(1))],
            probe: Probe { hive: Hkcu, path: EXPLORER_ADV.into(), name: "HideFileExt".into(), applied: Some(RegValue::Dword(0)) },
        },
        Tweak {
            id: "show-hidden-files",
            name: "Show hidden files",
            desc: "Display hidden files and folders in Explorer.",
            category: Interface,
            on: vec![set(Hkcu, EXPLORER_ADV, "Hidden", RegValue::Dword(1))],
            off: vec![set(Hkcu, EXPLORER_ADV, "Hidden", RegValue::Dword(2))],
            probe: Probe { hive: Hkcu, path: EXPLORER_ADV.into(), name: "Hidden".into(), applied: Some(RegValue::Dword(1)) },
        },
        Tweak {
            id: "dark-mode",
            name: "Dark mode (apps)",
            desc: "Use the dark theme for apps that follow the system setting.",
            category: Interface,
            on: vec![set(Hkcu, PERSONALIZE, "AppsUseLightTheme", RegValue::Dword(0))],
            off: vec![set(Hkcu, PERSONALIZE, "AppsUseLightTheme", RegValue::Dword(1))],
            probe: Probe { hive: Hkcu, path: PERSONALIZE.into(), name: "AppsUseLightTheme".into(), applied: Some(RegValue::Dword(0)) },
        },
        Tweak {
            id: "hide-taskbar-search",
            name: "Hide taskbar search box",
            desc: "Collapse the taskbar search field to reclaim space.",
            category: Interface,
            on: vec![set(Hkcu, SEARCH, "SearchboxTaskbarMode", RegValue::Dword(0))],
            off: vec![set(Hkcu, SEARCH, "SearchboxTaskbarMode", RegValue::Dword(1))],
            probe: Probe { hive: Hkcu, path: SEARCH.into(), name: "SearchboxTaskbarMode".into(), applied: Some(RegValue::Dword(0)) },
        },
        // ── Privacy (HKCU policy, no elevation) ──────────────────────
        Tweak {
            id: "disable-start-web-search",
            name: "Disable Start menu web search",
            desc: "Stop the Start menu from sending searches to Bing.",
            category: Privacy,
            on: vec![set(Hkcu, "Software\\Policies\\Microsoft\\Windows\\Explorer", "DisableSearchBoxSuggestions", RegValue::Dword(1))],
            off: vec![del(Hkcu, "Software\\Policies\\Microsoft\\Windows\\Explorer", "DisableSearchBoxSuggestions")],
            probe: Probe { hive: Hkcu, path: "Software\\Policies\\Microsoft\\Windows\\Explorer".into(), name: "DisableSearchBoxSuggestions".into(), applied: Some(RegValue::Dword(1)) },
        },
        // ── Privacy / Performance (HKLM, needs elevated broker) ──────
        Tweak {
            id: "disable-telemetry",
            name: "Disable Windows telemetry",
            desc: "Set the diagnostic data collection policy to the minimum.",
            category: Privacy,
            on: vec![set(Hklm, "SOFTWARE\\Policies\\Microsoft\\Windows\\DataCollection", "AllowTelemetry", RegValue::Dword(0))],
            off: vec![del(Hklm, "SOFTWARE\\Policies\\Microsoft\\Windows\\DataCollection", "AllowTelemetry")],
            probe: Probe { hive: Hklm, path: "SOFTWARE\\Policies\\Microsoft\\Windows\\DataCollection".into(), name: "AllowTelemetry".into(), applied: Some(RegValue::Dword(0)) },
        },
        Tweak {
            id: "disable-copilot",
            name: "Disable Windows Copilot",
            desc: "Turn off the Copilot integration system-wide.",
            category: Privacy,
            on: vec![set(Hklm, "SOFTWARE\\Policies\\Microsoft\\Windows\\WindowsCopilot", "TurnOffWindowsCopilot", RegValue::Dword(1))],
            off: vec![del(Hklm, "SOFTWARE\\Policies\\Microsoft\\Windows\\WindowsCopilot", "TurnOffWindowsCopilot")],
            probe: Probe { hive: Hklm, path: "SOFTWARE\\Policies\\Microsoft\\Windows\\WindowsCopilot".into(), name: "TurnOffWindowsCopilot".into(), applied: Some(RegValue::Dword(1)) },
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_is_nonempty_and_first_is_sandbox() {
        let c = catalog();
        assert!(c.len() >= 5);
        assert!(matches!(c[0].category, Category::Sandbox));
        assert!(!c[0].needs_elevation());
    }

    #[test]
    fn hklm_tweaks_flagged_as_elevated() {
        let c = catalog();
        let tele = c.iter().find(|t| t.id == "disable-telemetry").unwrap();
        assert!(tele.needs_elevation());
    }

    #[test]
    fn sandbox_apply_revert_via_engine() {
        use crate::core::engine;
        let c = catalog();
        let t = &c[0];
        // ensure off
        for a in &t.off {
            let _ = engine::apply(a);
        }
        assert!(!t.is_applied());
        for a in &t.on {
            engine::apply(a).unwrap();
        }
        assert!(t.is_applied());
        for a in &t.off {
            engine::apply(a).unwrap();
        }
        assert!(!t.is_applied());
    }
}
