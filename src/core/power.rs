//! Windows power-plan switching via `powercfg`. Setting the active scheme is
//! done elevated; reading the active scheme is unelevated. "Ultimate
//! Performance" is hidden by default, so we duplicate it into existence first.

use std::process::Command;

pub struct Plan {
    pub id: &'static str,
    pub name: &'static str,
    pub guid: &'static str,
}

pub fn plans() -> &'static [Plan] {
    &[
        Plan { id: "balanced", name: "BALANCED", guid: "381b4222-f694-41f0-9685-ff5bb260df2e" },
        Plan { id: "high", name: "HIGH PERF", guid: "8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c" },
        Plan { id: "ultimate", name: "ULTIMATE", guid: "e9a42b02-d5df-448d-aa00-03f14749eb61" },
    ]
}

/// GUID of the currently-active power scheme (unelevated), lowercased.
pub fn active_guid() -> Option<String> {
    let out = Command::new("powercfg").arg("/getactivescheme").output().ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout);
    s.split_whitespace()
        .find(|t| t.len() == 36 && t.matches('-').count() == 4)
        .map(|t| t.to_lowercase())
}

/// Index into [`plans`] of the active scheme, or -1 if unknown/custom.
pub fn active_index() -> i32 {
    match active_guid() {
        Some(g) => plans()
            .iter()
            .position(|p| p.guid.eq_ignore_ascii_case(&g))
            .map(|i| i as i32)
            .unwrap_or(-1),
        None => -1,
    }
}

/// Activate an existing power scheme by GUID (unelevated, best-effort). Used by
/// System Modes — standard schemes switch without a UAC prompt.
pub fn set_active(guid: &str) -> bool {
    Command::new("powercfg")
        .args(["/setactive", guid])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Elevated PowerShell that activates plan `idx` (creating Ultimate if needed).
pub fn set_script(idx: usize) -> Option<String> {
    let p = plans().get(idx)?;
    Some(if p.id == "ultimate" {
        // -duplicatescheme is a no-op (non-zero exit) if it already exists.
        format!("powercfg -duplicatescheme {0} 2>$null; powercfg /setactive {0}", p.guid)
    } else {
        format!("powercfg /setactive {}", p.guid)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn three_plans_with_distinct_guids() {
        let p = plans();
        assert_eq!(p.len(), 3);
        assert_ne!(p[0].guid, p[1].guid);
        for pl in p {
            assert_eq!(pl.guid.len(), 36);
        }
    }

    #[test]
    fn set_script_handles_ultimate_specially() {
        assert!(set_script(0).unwrap().contains("/setactive"));
        assert!(set_script(2).unwrap().contains("-duplicatescheme"));
        assert!(set_script(9).is_none());
    }
}
