//! Linux process monitor: top processes by CPU, with RAM, via `sysinfo`.
//!
//! Per-process GPU/VRAM is not wired up yet (Linux exposes it through
//! `/sys/class/drm/*/clients` and vendor fdinfo, which is a separate task). The
//! shape mirrors the Windows `procmon` so the UI can be shared.

use std::process::Command;

use sysinfo::{ProcessesToUpdate, System};

pub struct Proc {
    pub name: String,
    pub pid: u32,
    /// Percent of total CPU (0..100).
    pub cpu: f32,
    /// Resident memory in bytes.
    pub mem: u64,
}

pub struct ProcMonitor {
    sys: System,
}

impl Default for ProcMonitor {
    fn default() -> Self {
        Self::new()
    }
}

impl ProcMonitor {
    pub fn new() -> Self {
        let mut sys = System::new();
        sys.refresh_processes(ProcessesToUpdate::All, true);
        ProcMonitor { sys }
    }

    /// Refresh and return the top `limit` processes sorted by CPU.
    pub fn snapshot(&mut self, limit: usize) -> Vec<Proc> {
        self.sys.refresh_processes(ProcessesToUpdate::All, true);
        let ncpu = self.sys.cpus().len().max(1) as f32;
        let mut procs: Vec<Proc> = self
            .sys
            .processes()
            .values()
            .map(|p| Proc {
                name: p.name().to_string_lossy().to_string(),
                pid: p.pid().as_u32(),
                cpu: p.cpu_usage() / ncpu,
                mem: p.memory(),
            })
            .collect();
        procs.sort_by(|a, b| {
            b.cpu
                .partial_cmp(&a.cpu)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        procs.truncate(limit);
        procs
    }
}

/// Send SIGTERM to a process. Returns false if the signal could not be sent
/// (e.g. no permission, or the pid is gone). Uses `kill(1)` to avoid a `libc`
/// dependency; the UI escalates to `pkexec kill` when this is denied.
pub fn terminate(pid: u32) -> bool {
    Command::new("kill")
        .arg(pid.to_string())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// A `pkexec`-ready force-kill for processes the user can't signal directly.
pub fn force_kill_argv(pid: u32) -> Vec<String> {
    vec![
        "pkexec".into(),
        "kill".into(),
        "-9".into(),
        pid.to_string(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_returns_processes() {
        let mut m = ProcMonitor::new();
        let v = m.snapshot(20);
        assert!(!v.is_empty());
        assert!(v.len() <= 20);
    }

    #[test]
    fn force_kill_is_pkexec_wrapped() {
        assert_eq!(force_kill_argv(1234), vec!["pkexec", "kill", "-9", "1234"]);
    }
}
