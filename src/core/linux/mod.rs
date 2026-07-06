//! Linux backend (scaffold).
//!
//! A read-mostly system surface for the Linux port, mirroring the Windows
//! feature set where it maps cleanly:
//!
//! | Windows                     | Linux                                  |
//! |-----------------------------|----------------------------------------|
//! | DXGI/PDH/NVML + sysinfo     | sysinfo + `/proc` + `/sys` hwmon       |
//! | `GetExtendedTcpTable`       | `/proc/net/tcp` + `/proc/net/tcp6`     |
//! | Services (SCM via `Get-Service`) | systemd (`systemctl`)             |
//! | winget                      | apt / dnf / flatpak                     |
//! | DNS via netsh               | `resolvectl` / NetworkManager           |
//!
//! Following the Windows model, privileged operations are not performed inline.
//! Each returns a ready-to-run [`ElevatedCmd`] (a `pkexec`-friendly command line)
//! that the UI runs after an explicit confirmation, the Linux analog of handing a
//! script to the elevated broker.

pub mod apps;
pub mod autostart;
pub mod cleanup;
pub mod debloat;
pub mod dns;
pub mod firewall;
pub mod gaming;
pub mod gpudriver;
pub mod netmon;
pub mod pkg;
pub mod power;
pub mod procmon;
pub mod quick;
pub mod restore;
pub mod servers;
pub mod services;
pub mod telemetry;
pub mod tweaks;

/// A privileged command the UI should run (typically via `pkexec`), plus a
/// human-readable summary for the confirmation prompt. Nothing here executes on
/// its own; the UI owns the decision to run it, exactly like the Windows broker
/// scripts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ElevatedCmd {
    /// Argv (program first), run without a shell to avoid quoting pitfalls.
    pub argv: Vec<String>,
    /// One-line description shown to the user before running.
    pub summary: String,
}

impl ElevatedCmd {
    pub fn new(summary: impl Into<String>, argv: &[&str]) -> Self {
        ElevatedCmd {
            argv: argv.iter().map(|s| s.to_string()).collect(),
            summary: summary.into(),
        }
    }

    /// Wrap this command in `pkexec` for a graphical privilege prompt.
    pub fn pkexec(&self) -> Vec<String> {
        let mut v = vec!["pkexec".to_string()];
        v.extend(self.argv.iter().cloned());
        v
    }
}

/// Human-readable size (e.g. "1.4 GiB"), shared by the Linux views.
pub fn human(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    let b = bytes as f64;
    if b < KB {
        format!("{bytes} B")
    } else if b < KB * KB {
        format!("{:.0} KiB", b / KB)
    } else if b < KB * KB * KB {
        format!("{:.1} MiB", b / (KB * KB))
    } else {
        format!("{:.2} GiB", b / (KB * KB * KB))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn elevated_cmd_wraps_pkexec() {
        let c = ElevatedCmd::new("Restart sshd", &["systemctl", "restart", "sshd"]);
        assert_eq!(c.pkexec(), vec!["pkexec", "systemctl", "restart", "sshd"]);
        assert_eq!(c.summary, "Restart sshd");
    }

    #[test]
    fn human_scales() {
        assert_eq!(human(512), "512 B");
        assert!(human(5 * 1024 * 1024).ends_with("MiB"));
        assert!(human(3 * 1024 * 1024 * 1024).ends_with("GiB"));
    }
}
