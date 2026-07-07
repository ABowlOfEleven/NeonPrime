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
    Security,
}

impl Category {
    pub fn label(self) -> &'static str {
        match self {
            Category::Sandbox => "SANDBOX",
            Category::Interface => "INTERFACE",
            Category::Privacy => "PRIVACY",
            Category::Performance => "PERFORMANCE",
            Category::Security => "SECURITY",
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
    /// A caveat shown prominently for this toggle (empty = none). Used on the
    /// security hardening tweaks to explain what could break.
    pub warn: &'static str,
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

    /// Attach a warning/caveat (builder style).
    pub fn warned(mut self, warn: &'static str) -> Self {
        self.warn = warn;
        self
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
        warn: "",
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
// One positional arg per registry field keeps the catalog entries terse and
// tabular; a params struct would bloat every call site for no real gain.
#[allow(clippy::too_many_arguments)]
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
        warn: "",
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

/// A single-string tweak whose `off` restores an explicit default string.
#[allow(clippy::too_many_arguments)]
fn sz(
    id: &'static str,
    name: &'static str,
    desc: &'static str,
    category: Category,
    hive: Hive,
    path: &'static str,
    key: &'static str,
    on_val: &'static str,
    off_val: &'static str,
) -> Tweak {
    Tweak {
        id,
        name,
        desc,
        category,
        warn: "",
        on: vec![set(hive, path, key, RegValue::Sz(on_val.into()))],
        off: vec![set(hive, path, key, RegValue::Sz(off_val.into()))],
        probe: Probe {
            hive,
            path: path.into(),
            name: key.into(),
            applied: Some(RegValue::Sz(on_val.into())),
        },
    }
}

/// Terse `RegValue` constructors for the multi-action tweaks below.
fn dv(v: u32) -> RegValue {
    RegValue::Dword(v)
}
fn sv(v: &str) -> RegValue {
    RegValue::Sz(v.into())
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
            warn: "",
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
            warn: "",
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

        // ── Security hardening (HKLM, elevated). Real hardening, not just
        //    telemetry. Each carries a warning about what it does / could break. ─
        dw("harden-smb1", "Disable SMBv1 (legacy file sharing)",
            "Turns off the obsolete SMBv1 protocol on the file server.",
            Security, Hklm, "SYSTEM\\CurrentControlSet\\Services\\LanmanServer\\Parameters", "SMB1", 0, 1)
            .warned("SMBv1 is how WannaCry/NotPetya spread. Safe to disable unless you connect to very old NAS boxes or Windows XP shares."),
        dw_del("harden-autorun", "Block AutoRun / AutoPlay",
            "Stops programs auto-executing from USB drives, CDs, and network shares.",
            Security, Hklm, "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Policies\\Explorer", "NoDriveTypeAutoRun", 255)
            .warned("Closes a classic USB-malware path. You still open removable drives manually; nothing important breaks."),
        dw_del("harden-smartscreen", "Require SmartScreen for apps",
            "Warns before running unrecognized downloaded programs.",
            Security, Hklm, "SOFTWARE\\Policies\\Microsoft\\Windows\\System", "EnableSmartScreen", 1)
            .warned("Adds a prompt for unknown executables. Slightly more friction installing niche or unsigned apps."),
        dw_del("harden-llmnr", "Disable LLMNR name resolution",
            "Turns off Link-Local Multicast Name Resolution.",
            Security, Hklm, "SOFTWARE\\Policies\\Microsoft\\Windows NT\\DNSClient", "EnableMulticast", 0)
            .warned("LLMNR is routinely abused to steal credentials on untrusted LANs (Responder attacks). Safe at home; may slightly slow local hostname lookups on networks without proper DNS."),
        dw_del("harden-wdigest", "Stop caching passwords in memory",
            "Disables WDigest clear-text credential caching.",
            Security, Hklm, "SYSTEM\\CurrentControlSet\\Control\\SecurityProviders\\WDigest", "UseLogonCredential", 0)
            .warned("Blocks tools like mimikatz from scraping your plain-text password out of RAM. No downside on modern Windows."),
        dw_del("harden-pua", "Block potentially unwanted apps",
            "Enables Microsoft Defender PUA protection (adware / bundleware).",
            Security, Hklm, "SOFTWARE\\Policies\\Microsoft\\Windows Defender", "PUAProtection", 1)
            .warned("Defender blocks bundled adware and 'optional offers'. Rarely, an aggressive freeware installer gets flagged."),
        dw_del("harden-wsh", "Disable Windows Script Host",
            "Blocks .vbs / .js scripts from running via wscript / cscript.",
            Security, Hklm, "SOFTWARE\\Microsoft\\Windows Script Host\\Settings", "Enabled", 0)
            .warned("Shuts a common script-malware path. Some legacy app installers and admin scripts rely on WSH and will stop working, leave this OFF if you use .vbs/.js scripts."),
        dw("harden-nolmhash", "Don't store weak LM password hashes",
            "Stops Windows storing the legacy, easily-cracked LM hash of your password.",
            Security, Hklm, "SYSTEM\\CurrentControlSet\\Control\\Lsa", "NoLMHash", 1, 0)
            .warned("No downside unless you authenticate to pre-Windows-2000 systems (essentially never)."),
        dw("harden-no-rdp", "Block incoming Remote Desktop",
            "Denies inbound RDP connections to this PC.",
            Security, Hklm, "SYSTEM\\CurrentControlSet\\Control\\Terminal Server", "fDenyTSConnections", 1, 0)
            .warned("If you connect to this machine via Remote Desktop, this locks that out. Leave OFF if you use RDP to reach this PC."),
        dw("harden-no-remote-assist", "Disable Remote Assistance",
            "Turns off the Windows Remote Assistance invitation feature.",
            Security, Hklm, "SYSTEM\\CurrentControlSet\\Control\\Remote Assistance", "fAllowToGetHelp", 0, 1)
            .warned("Removes a rarely-used remote-help channel. Only matters if you actually use Windows Remote Assistance."),

        // ══ WinUtil-parity tweaks (reversible registry only) ═════════════
        // ── Interface ────────────────────────────────────────────────
        dw_del("show-battery-percentage", "Show battery percentage",
            "Show the exact battery percentage in the system tray.",
            Interface, Hkcu, EXPLORER_ADV, "IsBatteryPercentageEnabled", 1),
        dw_del("end-task-taskbar", "Add 'End Task' to taskbar right-click",
            "Enable the developer option to kill an app straight from its taskbar button.",
            Interface, Hkcu, "Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\Advanced\\TaskbarDeveloperSettings", "TaskbarEndTask", 1),
        dw("always-show-scrollbars", "Always show scrollbars",
            "Stop scrollbars from auto-hiding in modern apps.",
            Interface, Hkcu, "Control Panel\\Accessibility", "DynamicScrollbars", 0, 1),
        sz("numlock-on-startup", "Num Lock on at startup",
            "Turn Num Lock on automatically when you sign in.",
            Interface, Hkcu, "Control Panel\\Keyboard", "InitialKeyboardIndicators", "2", "0"),
        dw("verbose-logon", "Verbose sign-in messages",
            "Show detailed status messages during sign-in and shutdown (useful for diagnosing hangs).",
            Interface, Hklm, "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Policies\\System", "VerboseStatus", 1, 0),
        Tweak {
            id: "detailed-bsod",
            name: "Detailed blue-screen (BSoD)",
            desc: "Show the technical stop-code parameters (and drop the sad-face) on a blue screen.",
            category: Interface,
            warn: "",
            on: vec![
                set(Hklm, CRASH, "DisplayParameters", dv(1)),
                set(Hklm, CRASH, "DisableEmoticon", dv(1)),
            ],
            off: vec![
                set(Hklm, CRASH, "DisplayParameters", dv(0)),
                set(Hklm, CRASH, "DisableEmoticon", dv(0)),
            ],
            probe: Probe { hive: Hklm, path: CRASH.into(), name: "DisplayParameters".into(), applied: Some(dv(1)) },
        },
        Tweak {
            id: "hide-home-gallery",
            name: "Hide Home & Gallery in Explorer",
            desc: "Remove the Home and Gallery entries from the File Explorer navigation pane.",
            category: Interface,
            warn: "",
            on: vec![
                set(Hkcu, "Software\\Classes\\CLSID\\{f874310e-b6b7-47dc-bc84-b9e6b38f5903}", "System.IsPinnedToNameSpaceTree", dv(0)),
                set(Hkcu, "Software\\Classes\\CLSID\\{e88865ea-0e1c-4e20-9aa6-edcd0212c87c}", "System.IsPinnedToNameSpaceTree", dv(0)),
            ],
            off: vec![
                del(Hkcu, "Software\\Classes\\CLSID\\{f874310e-b6b7-47dc-bc84-b9e6b38f5903}", "System.IsPinnedToNameSpaceTree"),
                del(Hkcu, "Software\\Classes\\CLSID\\{e88865ea-0e1c-4e20-9aa6-edcd0212c87c}", "System.IsPinnedToNameSpaceTree"),
            ],
            probe: Probe { hive: Hkcu, path: "Software\\Classes\\CLSID\\{f874310e-b6b7-47dc-bc84-b9e6b38f5903}".into(), name: "System.IsPinnedToNameSpaceTree".into(), applied: Some(dv(0)) },
        },
        Tweak {
            id: "hide-start-recommendations",
            name: "Hide Start menu recommendations",
            desc: "Remove the 'Recommended' section (recent files / suggested apps) from the Start menu.",
            category: Interface,
            warn: "",
            on: vec![
                set(Hklm, "SOFTWARE\\Microsoft\\PolicyManager\\current\\device\\Start", "HideRecommendedSection", dv(1)),
                set(Hklm, "SOFTWARE\\Policies\\Microsoft\\Windows\\Explorer", "HideRecommendedSection", dv(1)),
            ],
            off: vec![
                set(Hklm, "SOFTWARE\\Microsoft\\PolicyManager\\current\\device\\Start", "HideRecommendedSection", dv(0)),
                set(Hklm, "SOFTWARE\\Policies\\Microsoft\\Windows\\Explorer", "HideRecommendedSection", dv(0)),
            ],
            probe: Probe { hive: Hklm, path: "SOFTWARE\\Policies\\Microsoft\\Windows\\Explorer".into(), name: "HideRecommendedSection".into(), applied: Some(dv(1)) },
        },
        dw("disable-login-blur", "Disable sign-in screen blur",
            "Turn off the acrylic blur behind the sign-in screen for a sharper wallpaper.",
            Interface, Hklm, "SOFTWARE\\Policies\\Microsoft\\Windows\\System", "DisableAcrylicBackgroundOnLogon", 1, 0),
        dw_del("disable-lockscreen", "Disable the lock screen",
            "Go straight to the sign-in prompt instead of the lock-screen wallpaper.",
            Interface, Hklm, "SOFTWARE\\Policies\\Microsoft\\Windows\\Personalization", "NoLockScreen", 1),

        // ── Privacy ──────────────────────────────────────────────────
        Tweak {
            id: "disable-activity-history",
            name: "Disable Activity History / Timeline",
            desc: "Stop Windows recording and uploading your activity timeline.",
            category: Privacy,
            warn: "",
            on: vec![
                set(Hklm, POLICY_SYSTEM, "EnableActivityFeed", dv(0)),
                set(Hklm, POLICY_SYSTEM, "PublishUserActivities", dv(0)),
                set(Hklm, POLICY_SYSTEM, "UploadUserActivities", dv(0)),
            ],
            off: vec![
                del(Hklm, POLICY_SYSTEM, "EnableActivityFeed"),
                del(Hklm, POLICY_SYSTEM, "PublishUserActivities"),
                del(Hklm, POLICY_SYSTEM, "UploadUserActivities"),
            ],
            probe: Probe { hive: Hklm, path: POLICY_SYSTEM.into(), name: "EnableActivityFeed".into(), applied: Some(dv(0)) },
        },
        Tweak {
            id: "disable-location",
            name: "Disable location tracking",
            desc: "Deny the system-wide location sensor and stop Maps auto-updates.",
            category: Privacy,
            warn: "",
            on: vec![
                set(Hklm, "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\CapabilityAccessManager\\ConsentStore\\location", "Value", sv("Deny")),
                set(Hklm, "SYSTEM\\Maps", "AutoUpdateEnabled", dv(0)),
            ],
            off: vec![
                set(Hklm, "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\CapabilityAccessManager\\ConsentStore\\location", "Value", sv("Allow")),
                set(Hklm, "SYSTEM\\Maps", "AutoUpdateEnabled", dv(1)),
            ],
            probe: Probe { hive: Hklm, path: "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\CapabilityAccessManager\\ConsentStore\\location".into(), name: "Value".into(), applied: Some(sv("Deny")) },
        },
        dw_del("disable-delivery-optimization", "Disable Delivery Optimization",
            "Stop Windows sharing update files peer-to-peer with other PCs.",
            Privacy, Hklm, "SOFTWARE\\Policies\\Microsoft\\Windows\\DeliveryOptimization", "DODownloadMode", 0),
        dw("disable-background-apps", "Disable background apps",
            "Stop UWP/Store apps from running in the background.",
            Privacy, Hkcu, "Software\\Microsoft\\Windows\\CurrentVersion\\BackgroundAccessApplications", "GlobalUserDisabled", 1, 0),
        Tweak {
            id: "disable-notifications",
            name: "Disable notifications & action center",
            desc: "Turn off toast notifications and the notification center.",
            category: Privacy,
            warn: "",
            on: vec![
                set(Hkcu, EXPLORER_POLICY_CU, "DisableNotificationCenter", dv(1)),
                set(Hkcu, "Software\\Microsoft\\Windows\\CurrentVersion\\PushNotifications", "ToastEnabled", dv(0)),
            ],
            off: vec![
                del(Hkcu, EXPLORER_POLICY_CU, "DisableNotificationCenter"),
                set(Hkcu, "Software\\Microsoft\\Windows\\CurrentVersion\\PushNotifications", "ToastEnabled", dv(1)),
            ],
            probe: Probe { hive: Hkcu, path: "Software\\Microsoft\\Windows\\CurrentVersion\\PushNotifications".into(), name: "ToastEnabled".into(), applied: Some(dv(0)) },
        },
        dw_del("disable-notepad-ai", "Disable Notepad AI features",
            "Turn off the Copilot/Rewrite AI features baked into Notepad.",
            Privacy, Hklm, "SOFTWARE\\Policies\\WindowsNotepad", "DisableAIFeatures", 1),
        Tweak {
            id: "edge-debloat",
            name: "Debloat Microsoft Edge",
            desc: "Apply Edge enterprise policies that strip telemetry, shopping, rewards, feedback, and first-run nags. Does not remove Edge.",
            category: Privacy,
            warn: "",
            on: EDGE_KEYS.iter().map(|(k, v)| set(Hklm, EDGE, k, dv(*v))).collect(),
            off: EDGE_KEYS.iter().map(|(k, _)| del(Hklm, EDGE, k)).collect(),
            probe: Probe { hive: Hklm, path: EDGE.into(), name: "PersonalizationReportingEnabled".into(), applied: Some(dv(0)) },
        },
        Tweak {
            id: "brave-debloat",
            name: "Debloat Brave browser",
            desc: "Apply Brave enterprise policies that disable Rewards, Wallet, VPN, AI chat, news, and telemetry pings. Harmless if Brave is not installed.",
            category: Privacy,
            warn: "",
            on: BRAVE_KEYS.iter().map(|(k, v)| set(Hklm, BRAVE, k, dv(*v))).collect(),
            off: BRAVE_KEYS.iter().map(|(k, _)| del(Hklm, BRAVE, k)).collect(),
            probe: Probe { hive: Hklm, path: BRAVE.into(), name: "BraveRewardsDisabled".into(), applied: Some(dv(1)) },
        },

        // ── Performance ──────────────────────────────────────────────
        Tweak {
            id: "visual-fx-performance",
            name: "Adjust visuals for best performance",
            desc: "Turn off animations, shadows, and window drag effects for a snappier desktop.",
            category: Performance,
            warn: "",
            on: vec![
                set(Hkcu, DESKTOP, "DragFullWindows", sv("0")),
                set(Hkcu, "Control Panel\\Desktop\\WindowMetrics", "MinAnimate", sv("0")),
                set(Hkcu, EXPLORER_ADV, "ListviewAlphaSelect", dv(0)),
                set(Hkcu, EXPLORER_ADV, "ListviewShadow", dv(0)),
                set(Hkcu, EXPLORER_ADV, "TaskbarAnimations", dv(0)),
                set(Hkcu, "Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\VisualEffects", "VisualFXSetting", dv(3)),
                set(Hkcu, "Software\\Microsoft\\Windows\\DWM", "EnableAeroPeek", dv(0)),
            ],
            off: vec![
                set(Hkcu, DESKTOP, "DragFullWindows", sv("1")),
                set(Hkcu, "Control Panel\\Desktop\\WindowMetrics", "MinAnimate", sv("1")),
                set(Hkcu, EXPLORER_ADV, "ListviewAlphaSelect", dv(1)),
                set(Hkcu, EXPLORER_ADV, "ListviewShadow", dv(1)),
                set(Hkcu, EXPLORER_ADV, "TaskbarAnimations", dv(1)),
                set(Hkcu, "Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\VisualEffects", "VisualFXSetting", dv(1)),
                set(Hkcu, "Software\\Microsoft\\Windows\\DWM", "EnableAeroPeek", dv(1)),
            ],
            probe: Probe { hive: Hkcu, path: "Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\VisualEffects".into(), name: "VisualFXSetting".into(), applied: Some(dv(3)) },
        },
        Tweak {
            id: "game-mode",
            name: "Enable Game Mode",
            desc: "Let Windows prioritize the foreground game (auto Game Mode).",
            category: Performance,
            warn: "",
            on: vec![
                set(Hkcu, "Software\\Microsoft\\GameBar", "AllowAutoGameMode", dv(1)),
                set(Hkcu, "Software\\Microsoft\\GameBar", "AutoGameModeEnabled", dv(1)),
            ],
            off: vec![
                set(Hkcu, "Software\\Microsoft\\GameBar", "AllowAutoGameMode", dv(0)),
                set(Hkcu, "Software\\Microsoft\\GameBar", "AutoGameModeEnabled", dv(0)),
            ],
            probe: Probe { hive: Hkcu, path: "Software\\Microsoft\\GameBar".into(), name: "AutoGameModeEnabled".into(), applied: Some(dv(1)) },
        },
        dw("disable-fullscreen-optimizations", "Disable fullscreen optimizations",
            "Force exclusive fullscreen for games, which can lower input latency.",
            Performance, Hkcu, "System\\GameConfigStore", "GameDVR_DXGIHonorFSEWindowsCompatible", 1, 0),
        dw("disable-storage-sense", "Disable Storage Sense",
            "Stop Windows from automatically deleting temp files and old downloads.",
            Performance, Hkcu, "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\StorageSense\\Parameters\\StoragePolicy", "01", 0, 1),
        Tweak {
            id: "disable-hibernation",
            name: "Disable hibernation",
            desc: "Turn off hibernation and reclaim the hiberfil.sys disk space.",
            category: Performance,
            warn: "For the full effect (deleting hiberfil.sys) also run `powercfg /hibernate off` once. Leave this OFF if you use Fast Startup or hibernate.",
            on: vec![
                set(Hklm, "System\\CurrentControlSet\\Control\\Session Manager\\Power", "HibernateEnabled", dv(0)),
                set(Hklm, "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Explorer\\FlyoutMenuSettings", "ShowHibernateOption", dv(0)),
            ],
            off: vec![
                set(Hklm, "System\\CurrentControlSet\\Control\\Session Manager\\Power", "HibernateEnabled", dv(1)),
                set(Hklm, "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Explorer\\FlyoutMenuSettings", "ShowHibernateOption", dv(1)),
            ],
            probe: Probe { hive: Hklm, path: "System\\CurrentControlSet\\Control\\Session Manager\\Power".into(), name: "HibernateEnabled".into(), applied: Some(dv(0)) },
        },
        dw("utc-clock", "Set hardware clock to UTC",
            "Store the system clock in UTC so it matches Linux on a dual-boot machine.",
            Performance, Hklm, "SYSTEM\\CurrentControlSet\\Control\\TimeZoneInformation", "RealTimeIsUniversal", 1, 0)
            .warned("Only useful for Linux dual-boot. On a Windows-only PC leave this OFF or the clock can drift by your timezone offset."),
        dw("prefer-ipv4", "Prefer IPv4 over IPv6",
            "Make Windows try IPv4 first, which can fix slow DNS on some networks.",
            Performance, Hklm, "SYSTEM\\CurrentControlSet\\Services\\Tcpip6\\Parameters", "DisabledComponents", 32, 0)
            .warned("Leaves IPv6 enabled but deprioritized. Safe on most home networks; do not use if your ISP or VPN is IPv6-only."),

        // ── Security ─────────────────────────────────────────────────
        dw_del("disable-wpbt", "Block firmware-injected software (WPBT)",
            "Stop the Windows Platform Binary Table from letting your motherboard auto-run vendor software.",
            Security, Hklm, "SYSTEM\\CurrentControlSet\\Control\\Session Manager", "DisableWpbtExecution", 1)
            .warned("Closes a firmware-level auto-run channel abused by some OEMs (e.g. the Lenovo/Superfish class). No downside for normal use."),
        Tweak {
            id: "block-razer-autoinstall",
            name: "Block automatic driver-software installs",
            desc: "Stop Windows Update from auto-installing vendor bloat (like Razer Synapse) alongside device drivers.",
            category: Security,
            warn: "Windows will still install the core device driver; only the bundled vendor app is blocked. You can install that app manually if you want it.",
            on: vec![
                set(Hklm, "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\DriverSearching", "SearchOrderConfig", dv(0)),
                set(Hklm, "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Device Installer", "DisableCoInstallers", dv(1)),
            ],
            off: vec![
                set(Hklm, "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\DriverSearching", "SearchOrderConfig", dv(1)),
                set(Hklm, "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Device Installer", "DisableCoInstallers", dv(0)),
            ],
            probe: Probe { hive: Hklm, path: "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Device Installer".into(), name: "DisableCoInstallers".into(), applied: Some(dv(1)) },
        },
        Tweak {
            id: "rdp-unsigned-warning",
            name: "Suppress unsigned-RDP-file warning",
            desc: "Stop the 'publisher can't be identified' prompt when opening your own .rdp files.",
            // Interface, not Security: this suppresses a safety prompt, so it does
            // not belong in the hardening score. It shows in the Tweaks panel.
            category: Interface,
            warn: "Only suppress this if you exclusively open .rdp files you created yourself; the warning is a real check on untrusted RDP files.",
            on: vec![
                set(Hklm, "SOFTWARE\\Policies\\Microsoft\\Windows NT\\Terminal Services\\Client", "RedirectionWarningDialogVersion", dv(1)),
                set(Hkcu, "SOFTWARE\\Microsoft\\Terminal Server Client", "RdpLaunchConsentAccepted", dv(1)),
            ],
            off: vec![
                del(Hklm, "SOFTWARE\\Policies\\Microsoft\\Windows NT\\Terminal Services\\Client", "RedirectionWarningDialogVersion"),
                del(Hkcu, "SOFTWARE\\Microsoft\\Terminal Server Client", "RdpLaunchConsentAccepted"),
            ],
            probe: Probe { hive: Hkcu, path: "SOFTWARE\\Microsoft\\Terminal Server Client".into(), name: "RdpLaunchConsentAccepted".into(), applied: Some(dv(1)) },
        },
    ]
}

// Registry roots and key tables used by the multi-action tweaks above.
const CRASH: &str = "SYSTEM\\CurrentControlSet\\Control\\CrashControl";
const POLICY_SYSTEM: &str = "SOFTWARE\\Policies\\Microsoft\\Windows\\System";
const EXPLORER_POLICY_CU: &str = "Software\\Policies\\Microsoft\\Windows\\Explorer";
const EDGE: &str = "SOFTWARE\\Policies\\Microsoft\\Edge";
const BRAVE: &str = "SOFTWARE\\Policies\\BraveSoftware\\Brave";

/// Edge debloat policy values (all DWORDs; `off` deletes them).
const EDGE_KEYS: &[(&str, u32)] = &[
    ("PersonalizationReportingEnabled", 0),
    ("ShowRecommendationsEnabled", 0),
    ("HideFirstRunExperience", 1),
    ("UserFeedbackAllowed", 0),
    ("ConfigureDoNotTrack", 1),
    ("AlternateErrorPagesEnabled", 0),
    ("EdgeCollectionsEnabled", 0),
    ("EdgeShoppingAssistantEnabled", 0),
    ("MicrosoftEdgeInsiderPromotionEnabled", 0),
    ("ShowMicrosoftRewards", 0),
    ("WebWidgetAllowed", 0),
    ("DiagnosticData", 0),
    ("EdgeAssetDeliveryServiceEnabled", 0),
    ("WalletDonationEnabled", 0),
    ("DefaultBrowserSettingsCampaignEnabled", 0),
];

/// Brave debloat policy values (all DWORDs; `off` deletes them).
const BRAVE_KEYS: &[(&str, u32)] = &[
    ("BraveRewardsDisabled", 1),
    ("BraveWalletDisabled", 1),
    ("BraveVPNDisabled", 1),
    ("BraveAIChatEnabled", 0),
    ("BraveStatsPingEnabled", 0),
    ("BraveNewsDisabled", 1),
    ("BraveTalkDisabled", 1),
    ("TorDisabled", 1),
    ("BraveP3AEnabled", 0),
    ("UrlKeyedAnonymizedDataCollectionEnabled", 0),
    ("SafeBrowsingExtendedReportingEnabled", 0),
    ("MetricsReportingEnabled", 0),
];

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
