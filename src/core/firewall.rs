//! Windows Firewall integration for the Network panel. Reading rules is
//! unelevated; adding/removing block rules needs admin (elevated `netsh`).
//! NeonPrime's rules are name-prefixed so they're easy to find and undo.

use std::process::Command;

const PREFIX: &str = "NeonPrime:";

/// Display names of NeonPrime-created firewall rules (unelevated, best-effort).
pub fn list_names() -> Vec<String> {
    let out = Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "Get-NetFirewallRule -DisplayName 'NeonPrime:*' -ErrorAction SilentlyContinue | \
             Select-Object -ExpandProperty DisplayName",
        ])
        .output();
    match out {
        Ok(o) if o.status.success() => {
            let mut v: Vec<String> = String::from_utf8_lossy(&o.stdout)
                .lines()
                .map(|l| l.trim().to_string())
                .filter(|l| !l.is_empty())
                .collect();
            v.sort();
            v.dedup();
            v
        }
        _ => Vec::new(),
    }
}

fn rule_name(exe: &str) -> String {
    format!("{PREFIX} {exe}")
}

/// Elevated `netsh` to block a program's outbound traffic, or None if no path.
pub fn block_script(exe: &str, path: &str) -> Option<String> {
    if path.is_empty() {
        return None;
    }
    let name = rule_name(exe);
    Some(format!(
        "netsh advfirewall firewall add rule name=\"{name}\" dir=out action=block program=\"{path}\" enable=yes; \
         Write-Host 'Blocked {exe} (outbound).'"
    ))
}

/// Elevated `netsh` to delete a NeonPrime rule by its display name.
pub fn unblock_script(name: &str) -> String {
    format!("netsh advfirewall firewall delete rule name=\"{name}\"; Write-Host 'Rule removed.'")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn block_script_needs_a_path() {
        assert!(block_script("chrome.exe", "").is_none());
        let s = block_script("chrome.exe", "C:\\x\\chrome.exe").unwrap();
        assert!(s.contains("dir=out action=block"));
        assert!(s.contains("NeonPrime: chrome.exe"));
    }

    #[test]
    fn unblock_targets_by_name() {
        assert!(unblock_script("NeonPrime: chrome.exe").contains("delete rule"));
    }

    #[test]
    fn listing_does_not_panic() {
        let _ = list_names();
    }
}
