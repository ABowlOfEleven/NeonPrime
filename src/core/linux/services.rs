//! systemd service manager (read + privileged actions as commands).
//!
//! Listing shells out to `systemctl` (unprivileged). Start/stop/enable/disable
//! are returned as [`ElevatedCmd`]s the UI runs via `pkexec`, mirroring the
//! Windows services panel that routes changes through the elevated broker.

use std::process::Command;

use super::ElevatedCmd;

#[derive(Debug, Clone)]
pub struct Svc {
    /// Unit name, e.g. "sshd.service".
    pub name: String,
    pub description: String,
    /// The unit is currently active (running).
    pub running: bool,
    /// Enabled to start at boot (vendor preset ignored).
    pub enabled: bool,
}

fn run(args: &[&str]) -> Option<String> {
    let out = Command::new("systemctl").args(args).output().ok()?;
    if out.status.success() {
        Some(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        None
    }
}

/// Enumerate service units with their active + enabled state.
pub fn services() -> Vec<Svc> {
    // Boot-enabled state, keyed by unit name.
    let enabled: std::collections::HashMap<String, bool> = run(&[
        "list-unit-files",
        "--type=service",
        "--no-legend",
        "--no-pager",
        "--plain",
    ])
    .unwrap_or_default()
    .lines()
    .filter_map(|l| {
        let mut f = l.split_whitespace();
        let name = f.next()?.to_string();
        let state = f.next().unwrap_or("");
        Some((name, state == "enabled"))
    })
    .collect();

    run(&[
        "list-units",
        "--type=service",
        "--all",
        "--no-legend",
        "--no-pager",
        "--plain",
    ])
    .unwrap_or_default()
    .lines()
    .filter_map(|line| {
        // UNIT LOAD ACTIVE SUB DESCRIPTION...
        let mut f = line.split_whitespace();
        let name = f.next()?.to_string();
        let _load = f.next()?;
        let active = f.next()?;
        let sub = f.next()?;
        let description = f.collect::<Vec<_>>().join(" ");
        if name.is_empty() {
            return None;
        }
        Some(Svc {
            enabled: enabled.get(&name).copied().unwrap_or(false),
            running: active == "active" && sub == "running",
            description,
            name,
        })
    })
    .collect()
}

pub fn start(name: &str) -> ElevatedCmd {
    ElevatedCmd::new(format!("Start {name}"), &["systemctl", "start", name])
}
pub fn stop(name: &str) -> ElevatedCmd {
    ElevatedCmd::new(format!("Stop {name}"), &["systemctl", "stop", name])
}
pub fn enable(name: &str) -> ElevatedCmd {
    ElevatedCmd::new(
        format!("Enable {name} at boot"),
        &["systemctl", "enable", name],
    )
}
pub fn disable(name: &str) -> ElevatedCmd {
    ElevatedCmd::new(
        format!("Disable {name} at boot"),
        &["systemctl", "disable", name],
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn action_commands_are_wrapped() {
        assert_eq!(
            stop("sshd.service").pkexec(),
            vec!["pkexec", "systemctl", "stop", "sshd.service"]
        );
        assert!(enable("cups").summary.contains("boot"));
    }

    #[test]
    fn listing_does_not_panic() {
        // Returns [] where systemd is absent (CI containers); must not panic.
        let _ = services();
    }
}
