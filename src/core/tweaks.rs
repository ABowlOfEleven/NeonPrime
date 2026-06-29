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
        let current =
            registry::read(self.probe.hive, &self.probe.path, &self.probe.name).unwrap_or(None);
        current == self.probe.applied
    }
}

// Common registry roots, kept as constants for readability.
const EXPLORER_ADV: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\Advanced";
const PERSONALIZE: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize";
const SEARCH: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Search";
const ADV_INFO: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\AdvertisingInfo";
const PRIVACY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Privacy";
const CDM: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\ContentDeliveryManager";
const DESKTOP: &str = "Control Panel\\Desktop";
const EXPLORER_POLICY: &str = "Software\\Policies\\Microsoft\\Windows\\Explorer";
const CLSID_CTX: &str =
    "Software\\Classes\\CLSID\\{86ca1aa0-34aa-4e8b-a509-50c905bae2a2}\\InprocServer32";

fn set(hive: Hive, path: &str, name: &str, v: RegValue) -> Action {
    Action::SetReg {
        hive,
        path: path.into(),
        name: name.into(),
        value: v,
    }
}
fn del(hive: Hive, path: &str, name: &str) -> Action {
    Action::DeleteReg {
        hive,
        path: path.into(),
        name: name.into(),
    }
}

/// A DWORD tweak whose `off` restores an explicit default value.
#[allow(clippy::too_many_arguments)]
fn dw(
    id: &'static str,
    name: &'static str,
    desc: &'static str,
    category: Category,
    hive: Hive,
    path: &'static str,
    key: &'static str,
    on_val: u32,
    off_val: u32,
) -> Tweak {
    Tweak {
        id,
        name,
        desc,
        category,
        on: vec![set(hive, path, key, RegValue::Dword(on_val))],
        off: vec![set(hive, path, key, RegValue::Dword(off_val))],
        probe: Probe {
            hive,
            path: path.into(),
            name: key.into(),
            applied: Some(RegValue::Dword(on_val)),
        },
    }
}

/// A DWORD tweak whose default is "value absent", so `off` deletes it.
fn dw_del(
    id: &'static str,
    name: &'static str,
    desc: &'static str,
    category: Category,
    hive: Hive,
    path: &'static str,
    key: &'static str,
    on_val: u32,
) -> Tweak {
    Tweak {
        id,
        name,
        desc,
        category,
        on: vec![set(hive, path, key, RegValue::Dword(on_val))],
        off: vec![del(hive, path, key)],
        probe: Probe {
            hive,
            path: path.into(),
            name: key.into(),
            applied: Some(RegValue::Dword(on_val)),
        },
    }
}

/// The full catalog. Index in this vec doubles as the UI row id.
pub fn catalog() -> Vec<Tweak> {
    use Category::*;
    use Hive::*;

    vec![
        // ── Sandbox (safe to toggle in automated tests) ──────────────
        dw_del("sandbox-demo", "Demo toggle (safe sandbox)",
            "Writes only to HKCU\\Software\\NeonPrime\\Test — proves the apply/undo pipeline, changes nothing real.",
            Sandbox, Hkcu, "Software\\NeonPrime\\Test", "DemoTweak", 1),

        // ── Interface (HKCU, no elevation) ───────────────────────────
        dw("show-file-extensions", "Show file extensions",
            "Reveal extensions for known file types in Explorer.",
            Interface, Hkcu, EXPLORER_ADV, "HideFileExt", 0, 1),
        dw("show-hidden-files", "Show hidden files",
            "Display hidden files and folders in Explorer.",
            Interface, Hkcu, EXPLORER_ADV, "Hidden", 1, 2),
        dw("dark-mode", "Dark mode (apps)",
            "Use the dark theme for apps that follow the system setting.",
            Interface, Hkcu, PERSONALIZE, "AppsUseLightTheme", 0, 1),
        dw("dark-mode-system", "Dark mode (system / taskbar)",
            "Use the dark theme for the taskbar, Start, and system surfaces.",
            Interface, Hkcu, PERSONALIZE, "SystemUsesLightTheme", 0, 1),
        dw("disable-transparency", "Disable transparency effects",
            "Turn off acrylic/transparency for a flatter, snappier shell.",
            Interface, Hkcu, PERSONALIZE, "EnableTransparency", 0, 1),
        dw("taskbar-align-left", "Left-align the taskbar",
            "Move taskbar icons to the left edge (Windows 11).",
            Interface, Hkcu, EXPLORER_ADV, "TaskbarAl", 0, 1),
        dw("hide-task-view", "Hide Task View button",
            "Remove the Task View button from the taskbar.",
            Interface, Hkcu, EXPLORER_ADV, "ShowTaskViewButton", 0, 1),
        dw("hide-widgets", "Hide Widgets button",
            "Remove the Widgets button from the taskbar (Windows 11).",
            Interface, Hkcu, EXPLORER_ADV, "TaskbarDa", 0, 1),
        dw("hide-taskbar-search", "Hide taskbar search box",
            "Collapse the taskbar search field to reclaim space.",
            Interface, Hkcu, SEARCH, "SearchboxTaskbarMode", 0, 1),
        dw("show-seconds-clock", "Show seconds in the clock",
            "Display seconds on the taskbar clock.",
            Interface, Hkcu, EXPLORER_ADV, "ShowSecondsInSystemClock", 1, 0),
        dw("explorer-to-thispc", "Open Explorer to This PC",
            "Start File Explorer at This PC instead of Home / Quick access.",
            Interface, Hkcu, EXPLORER_ADV, "LaunchTo", 1, 2),
        Tweak {
            id: "classic-context-menu",
            name: "Classic right-click menu",
            desc: "Restore the full Windows 10 context menu (Windows 11). Needs an Explorer restart.",
            category: Interface,
            on: vec![set(Hkcu, CLSID_CTX, "", RegValue::Sz(String::new()))],
            off: vec![del(Hkcu, CLSID_CTX, "")],
            probe: Probe { hive: Hkcu, path: CLSID_CTX.into(), name: String::new(), applied: Some(RegValue::Sz(String::new())) },
        },

        // ── Privacy (HKCU, no elevation) ─────────────────────────────
        dw_del("disable-start-web-search", "Disable Start menu web search",
            "Stop the Start menu from sending searches to Bing.",
            Privacy, Hkcu, EXPLORER_POLICY, "DisableSearchBoxSuggestions", 1),
        dw("disable-advertising-id", "Disable advertising ID",
            "Stop apps from using your advertising ID to profile you.",
            Privacy, Hkcu, ADV_INFO, "Enabled", 0, 1),
        dw("disable-tailored-experiences", "Disable tailored experiences",
            "Stop Windows tailoring tips and ads from your diagnostic data.",
            Privacy, Hkcu, PRIVACY, "TailoredExperiencesWithDiagnosticDataEnabled", 0, 1),
        dw("disable-start-tracking", "Disable recently-opened tracking",
            "Stop tracking recently opened files in Start and Jump Lists.",
            Privacy, Hkcu, EXPLORER_ADV, "Start_TrackDocs", 0, 1),
        dw("disable-suggestions", "Disable Settings suggestions",
            "Turn off 'suggested content' ads in the Settings app.",
            Privacy, Hkcu, CDM, "SystemPaneSuggestionsEnabled", 0, 1),

        // ── Performance (HKCU) ───────────────────────────────────────
        Tweak {
            id: "fast-menu-delay",
            name: "Faster menu animations",
            desc: "Drop the menu show delay from 400ms to 0 for a snappier shell.",
            category: Performance,
            on: vec![set(Hkcu, DESKTOP, "MenuShowDelay", RegValue::Sz("0".into()))],
            off: vec![set(Hkcu, DESKTOP, "MenuShowDelay", RegValue::Sz("400".into()))],
            probe: Probe { hive: Hkcu, path: DESKTOP.into(), name: "MenuShowDelay".into(), applied: Some(RegValue::Sz("0".into())) },
        },

        // ── Privacy / Performance (HKLM, needs elevated broker) ──────
        dw_del("disable-telemetry", "Disable Windows telemetry",
            "Set the diagnostic data collection policy to the minimum.",
            Privacy, Hklm, "SOFTWARE\\Policies\\Microsoft\\Windows\\DataCollection", "AllowTelemetry", 0),
        dw_del("disable-copilot", "Disable Windows Copilot",
            "Turn off the Copilot integration system-wide.",
            Privacy, Hklm, "SOFTWARE\\Policies\\Microsoft\\Windows\\WindowsCopilot", "TurnOffWindowsCopilot", 1),
        dw_del("disable-consumer-features", "Disable consumer app pushes",
            "Stop Windows auto-installing promoted and sponsored apps.",
            Privacy, Hklm, "SOFTWARE\\Policies\\Microsoft\\Windows\\CloudContent", "DisableWindowsConsumerFeatures", 1),
        dw_del("disable-cortana", "Disable Cortana",
            "Turn off Cortana via policy.",
            Privacy, Hklm, "SOFTWARE\\Policies\\Microsoft\\Windows\\Windows Search", "AllowCortana", 0),
        dw("long-paths", "Enable long file paths",
            "Allow paths longer than 260 characters (dev-friendly).",
            Performance, Hklm, "SYSTEM\\CurrentControlSet\\Control\\FileSystem", "LongPathsEnabled", 1, 0),

        // ── Services (HKLM Start value: 2=auto, 3=manual, 4=disabled) ─
        dw("svc-diagtrack", "Disable telemetry service",
            "Stop the Connected User Experiences and Telemetry service (DiagTrack).",
            Privacy, Hklm, "SYSTEM\\CurrentControlSet\\Services\\DiagTrack", "Start", 4, 2),
        dw("svc-dmwappush", "Disable WAP push service",
            "Stop dmwappushservice (device-management WAP push message routing).",
            Privacy, Hklm, "SYSTEM\\CurrentControlSet\\Services\\dmwappushservice", "Start", 4, 3),
        dw("svc-sysmain", "Set SysMain (Superfetch) to manual",
            "Cut background prefetch disk activity — helps on SSDs and low-RAM systems.",
            Performance, Hklm, "SYSTEM\\CurrentControlSet\\Services\\SysMain", "Start", 3, 2),
        dw("svc-wmpnetwork", "Disable WMP network sharing",
            "Stop the Windows Media Player network sharing service.",
            Performance, Hklm, "SYSTEM\\CurrentControlSet\\Services\\WMPNetworkSvc", "Start", 4, 3),
        dw("svc-fax", "Disable Fax service",
            "Stop the Fax service (rarely needed).",
            Performance, Hklm, "SYSTEM\\CurrentControlSet\\Services\\Fax", "Start", 4, 3),
    ]
}

/// Curated "Essential Tweaks" — a safe, no-elevation recommended set applied by
/// the one-click button (mirrors WinUtil's flagship preset, HKCU-only).
pub fn essential_ids() -> &'static [&'static str] {
    &[
        "show-file-extensions",
        "disable-advertising-id",
        "disable-tailored-experiences",
        "disable-start-web-search",
        "disable-suggestions",
        "disable-start-tracking",
        "fast-menu-delay",
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
