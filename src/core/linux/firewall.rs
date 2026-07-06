//! Firewall control via ufw (Uncomplicated Firewall), the closest honest analog
//! of the Windows firewall panel. Per-application outbound blocking (what the
//! Windows side does) has no clean ufw equivalent, so this exposes the pieces
//! that map: enabled state plus enable / disable / reset.
//!
//! `ufw status` needs root, so the enabled flag is read from the world-readable
//! `/etc/ufw/ufw.conf`; mutations go through `pkexec`.

use super::ElevatedCmd;

/// Is ufw installed?
pub fn available() -> bool {
    let Some(path) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&path).any(|d| d.join("ufw").is_file())
}

/// Whether ufw is enabled, from `/etc/ufw/ufw.conf` (`ENABLED=yes|no`).
/// None if the file is unreadable or the key is absent.
pub fn enabled() -> Option<bool> {
    let text = std::fs::read_to_string("/etc/ufw/ufw.conf").ok()?;
    for line in text.lines() {
        let line = line.trim();
        if let Some(val) = line.strip_prefix("ENABLED=") {
            return Some(val.trim().eq_ignore_ascii_case("yes"));
        }
    }
    None
}

pub fn enable() -> ElevatedCmd {
    ElevatedCmd::new("Enable the firewall (ufw)", &["ufw", "--force", "enable"])
}

pub fn disable() -> ElevatedCmd {
    ElevatedCmd::new("Disable the firewall (ufw)", &["ufw", "disable"])
}

pub fn reset() -> ElevatedCmd {
    ElevatedCmd::new(
        "Reset all firewall rules (ufw)",
        &["ufw", "--force", "reset"],
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn action_commands_are_wrapped() {
        assert_eq!(
            enable().pkexec(),
            vec!["pkexec", "ufw", "--force", "enable"]
        );
        assert_eq!(disable().argv, vec!["ufw", "disable"]);
    }

    #[test]
    fn enabled_reads_without_panic() {
        // On a box without ufw this is just None; must not panic.
        let _ = enabled();
        let _ = available();
    }
}
