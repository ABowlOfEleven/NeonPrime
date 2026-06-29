//! Startup apps manager — enable/disable per-user (HKCU) startup entries.
//!
//! Disabling moves the entry out of the `Run` key into a NeonPrime backup key
//! and deletes it from `Run`; enabling restores it. This is fully reversible and
//! keeps disabled entries visible so they can be turned back on.
//!
//! Scope is HKCU only for now (where most apps register themselves); the
//! machine-wide HKLM `Run` key would need the elevated broker.

use std::io;

use crate::core::action::{Hive, RegValue};
use crate::core::registry;

const RUN: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
const BACKUP: &str = "Software\\NeonPrime\\StartupDisabled";

pub struct StartupEntry {
    pub name: String,
    pub command: String,
    pub enabled: bool,
}

/// All startup entries: enabled ones from `Run`, disabled ones from our backup.
pub fn list() -> Vec<StartupEntry> {
    let mut out: Vec<StartupEntry> = Vec::new();
    for (name, command) in registry::list_string_values(Hive::Hkcu, RUN) {
        out.push(StartupEntry {
            name,
            command,
            enabled: true,
        });
    }
    for (name, command) in registry::list_string_values(Hive::Hkcu, BACKUP) {
        out.push(StartupEntry {
            name,
            command,
            enabled: false,
        });
    }
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    out
}

/// Move an entry out of `Run` into the backup key.
pub fn disable(name: &str, command: &str) -> io::Result<()> {
    registry::write(Hive::Hkcu, BACKUP, name, &RegValue::Sz(command.to_string()))?;
    registry::delete(Hive::Hkcu, RUN, name)
}

/// Restore an entry from the backup key into `Run`.
pub fn enable(name: &str, command: &str) -> io::Result<()> {
    registry::write(Hive::Hkcu, RUN, name, &RegValue::Sz(command.to_string()))?;
    registry::delete(Hive::Hkcu, BACKUP, name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disable_then_enable_roundtrips() {
        let name = "NeonPrimeStartupTest";
        let cmd = "\"C:\\fake\\app.exe\" --autostart";
        // start clean
        let _ = registry::delete(Hive::Hkcu, RUN, name);
        let _ = registry::delete(Hive::Hkcu, BACKUP, name);

        // seed an enabled entry
        registry::write(Hive::Hkcu, RUN, name, &RegValue::Sz(cmd.into())).unwrap();
        assert!(list().iter().any(|e| e.name == name && e.enabled));

        disable(name, cmd).unwrap();
        assert!(list().iter().any(|e| e.name == name && !e.enabled));
        assert!(registry::read(Hive::Hkcu, RUN, name).unwrap().is_none());

        enable(name, cmd).unwrap();
        assert!(list().iter().any(|e| e.name == name && e.enabled));
        assert!(registry::read(Hive::Hkcu, BACKUP, name).unwrap().is_none());

        // cleanup
        let _ = registry::delete(Hive::Hkcu, RUN, name);
    }
}
