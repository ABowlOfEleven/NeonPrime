//! Power profiles, the Linux analog of the Windows power-plan switcher.
//!
//! Uses power-profiles-daemon via `powerprofilesctl`, which most modern desktops
//! ship (GNOME/KDE power panels drive the same daemon). Switching is authorized
//! by polkit for the active session, so it runs directly, no pkexec needed.

use std::process::Command;

pub struct Profile {
    pub id: &'static str,
    pub name: &'static str,
}

pub fn profiles() -> &'static [Profile] {
    &[
        Profile {
            id: "power-saver",
            name: "POWER SAVER",
        },
        Profile {
            id: "balanced",
            name: "BALANCED",
        },
        Profile {
            id: "performance",
            name: "PERFORMANCE",
        },
    ]
}

/// Is power-profiles-daemon available (the CLI present)?
pub fn available() -> bool {
    Command::new("powerprofilesctl")
        .arg("version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Currently active profile id, e.g. "balanced".
pub fn active() -> Option<String> {
    let out = Command::new("powerprofilesctl").arg("get").output().ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

/// Command line to switch to profile `id` (index into [`profiles`]).
pub fn set_argv(id: &str) -> Vec<String> {
    vec!["powerprofilesctl".into(), "set".into(), id.into()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn three_profiles_low_to_high() {
        let p = profiles();
        assert_eq!(p.len(), 3);
        assert_eq!(p[0].id, "power-saver");
        assert_eq!(p[2].id, "performance");
    }

    #[test]
    fn set_builds_ppctl_command() {
        assert_eq!(
            set_argv("performance"),
            vec!["powerprofilesctl", "set", "performance"]
        );
    }
}
