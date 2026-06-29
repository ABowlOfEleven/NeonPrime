//! UWP/Appx debloat + telemetry scheduled-task disabling.
//!
//! Per-user Appx removal (`Remove-AppxPackage` without `-AllUsers`) needs no
//! elevation, and `Get-AppxPackage` lists the current user's packages unelevated
//! — so unlike DISM features we *can* honestly show installed/removed state.
//! Removal is one-directional here; every app listed is reinstallable from the
//! Microsoft Store. Telemetry tasks are disabled via `schtasks` (elevated).

use std::collections::HashSet;
use std::process::Command;

pub struct Bloat {
    pub id: &'static str,
    pub name: &'static str,
    pub desc: &'static str,
    /// Substring matched against each installed package `Name`.
    pub pkg: &'static str,
}

pub fn catalog() -> &'static [Bloat] {
    &[
        Bloat { id: "copilot", name: "Copilot", desc: "The Windows Copilot app.", pkg: "Microsoft.Copilot" },
        Bloat { id: "teams", name: "Teams (consumer)", desc: "The personal Microsoft Teams / Chat app.", pkg: "MicrosoftTeams" },
        Bloat { id: "xboxbar", name: "Xbox Game Bar", desc: "The Game Bar overlay (Win+G).", pkg: "Microsoft.XboxGamingOverlay" },
        Bloat { id: "bingnews", name: "Bing News", desc: "The Microsoft News app.", pkg: "Microsoft.BingNews" },
        Bloat { id: "bingweather", name: "Weather", desc: "The MSN Weather app.", pkg: "Microsoft.BingWeather" },
        Bloat { id: "solitaire", name: "Solitaire Collection", desc: "Microsoft Solitaire (ad-supported).", pkg: "Microsoft.MicrosoftSolitaireCollection" },
        Bloat { id: "gethelp", name: "Get Help", desc: "The Windows support / Get Help app.", pkg: "Microsoft.GetHelp" },
        Bloat { id: "tips", name: "Tips", desc: "The Windows Tips / Get Started app.", pkg: "Microsoft.Getstarted" },
        Bloat { id: "maps", name: "Maps", desc: "The Windows Maps app.", pkg: "Microsoft.WindowsMaps" },
        Bloat { id: "feedback", name: "Feedback Hub", desc: "The Windows Feedback Hub.", pkg: "Microsoft.WindowsFeedbackHub" },
        Bloat { id: "mrportal", name: "Mixed Reality Portal", desc: "The Windows Mixed Reality portal.", pkg: "Microsoft.MixedReality.Portal" },
        Bloat { id: "clipchamp", name: "Clipchamp", desc: "The Clipchamp video editor.", pkg: "Clipchamp.Clipchamp" },
        Bloat { id: "quickassist", name: "Quick Assist", desc: "Remote-assistance tool (scam-abuse vector).", pkg: "MicrosoftCorporationII.QuickAssist" },
        Bloat { id: "people", name: "People", desc: "The People contacts app.", pkg: "Microsoft.People" },
    ]
}

/// `Name`s of every Appx package installed for the current user. Unelevated;
/// slow (~1-2s), so call off-thread.
pub fn installed_names() -> HashSet<String> {
    let out = Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "Get-AppxPackage | Select-Object -ExpandProperty Name",
        ])
        .output();
    match out {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout)
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect(),
        _ => HashSet::new(),
    }
}

pub fn is_present(b: &Bloat, installed: &HashSet<String>) -> bool {
    installed.iter().any(|n| n.contains(b.pkg))
}

/// Remove a bloat package for the current user (no elevation).
pub fn remove(b: &Bloat) -> std::io::Result<bool> {
    let cmd = format!("Get-AppxPackage *{}* | Remove-AppxPackage", b.pkg);
    let status = Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &cmd])
        .status()?;
    Ok(status.success())
}

/// Scheduled tasks that phone telemetry home. Disabling needs elevation.
pub fn telemetry_tasks() -> &'static [&'static str] {
    &[
        "\\Microsoft\\Windows\\Application Experience\\Microsoft Compatibility Appraiser",
        "\\Microsoft\\Windows\\Application Experience\\ProgramDataUpdater",
        "\\Microsoft\\Windows\\Autochk\\Proxy",
        "\\Microsoft\\Windows\\Customer Experience Improvement Program\\Consolidator",
        "\\Microsoft\\Windows\\Customer Experience Improvement Program\\UsbCeip",
        "\\Microsoft\\Windows\\DiskDiagnostic\\Microsoft-Windows-DiskDiagnosticDataCollector",
    ]
}

/// Elevated PowerShell that disables every telemetry scheduled task.
pub fn disable_tasks_script() -> String {
    let mut parts: Vec<String> = telemetry_tasks()
        .iter()
        .map(|t| format!("schtasks /Change /TN \"{t}\" /Disable"))
        .collect();
    parts.push("Write-Host 'Telemetry tasks disabled.'".into());
    parts.join("; ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_and_tasks_nonempty() {
        assert!(catalog().len() >= 10);
        assert!(telemetry_tasks().len() >= 4);
    }

    #[test]
    fn present_matches_on_substring() {
        let b = &catalog()[0]; // copilot
        let mut set = HashSet::new();
        assert!(!is_present(b, &set));
        set.insert("Microsoft.Copilot_1.0_x64__8wekyb3d8bbwe".to_string());
        assert!(is_present(b, &set));
    }

    #[test]
    fn disable_script_covers_every_task() {
        let s = disable_tasks_script();
        for t in telemetry_tasks() {
            assert!(s.contains(t));
        }
        assert!(s.contains("/Disable"));
    }
}
