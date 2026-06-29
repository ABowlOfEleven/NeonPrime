//! The reversible-action model.
//!
//! An [`Action`] is a declarative system change. Applying one yields a
//! [`Reversal`] that captures the prior state, so every change can be undone.
//! Both are `serde`-serializable, so the same value travels over IPC, lands in
//! the rollback journal, and can be replayed from a config file.

use serde::{Deserialize, Serialize};

/// A registry value NeonPrime knows how to read, write, and restore.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum RegValue {
    Dword(u32),
    Sz(String),
}

/// Which registry hive an action targets.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hive {
    /// `HKEY_CURRENT_USER` — writable without elevation.
    Hkcu,
    /// `HKEY_LOCAL_MACHINE` — requires administrator rights.
    Hklm,
}

impl Hive {
    /// True if writing under this hive requires the elevated broker.
    pub fn needs_elevation(self) -> bool {
        matches!(self, Hive::Hklm)
    }
}

/// A reversible system change.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum Action {
    /// Set a registry value, remembering whatever was there before.
    SetReg {
        hive: Hive,
        path: String,
        name: String,
        value: RegValue,
    },
    /// Delete a registry value, remembering it so it can be recreated.
    DeleteReg {
        hive: Hive,
        path: String,
        name: String,
    },
}

impl Action {
    /// True if this action must be executed by the elevated broker.
    pub fn needs_elevation(&self) -> bool {
        match self {
            Action::SetReg { hive, .. } | Action::DeleteReg { hive, .. } => hive.needs_elevation(),
        }
    }

    /// The registry path this action targets (used for validation).
    pub fn reg_path(&self) -> &str {
        match self {
            Action::SetReg { path, .. } | Action::DeleteReg { path, .. } => path,
        }
    }
}

/// Captured prior state — how to undo an applied [`Action`].
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum Reversal {
    /// Restore a registry value. `previous == None` means it didn't exist
    /// before, so undoing means deleting it.
    RestoreReg {
        hive: Hive,
        path: String,
        name: String,
        previous: Option<RegValue>,
    },
}

impl Reversal {
    /// True if executing this reversal needs the elevated broker (HKLM).
    pub fn needs_elevation(&self) -> bool {
        match self {
            Reversal::RestoreReg { hive, .. } => hive.needs_elevation(),
        }
    }

    /// Short `HIVE\…\leaf : name` summary for display.
    pub fn target_summary(&self) -> String {
        match self {
            Reversal::RestoreReg { hive, path, name, .. } => {
                let root = if hive.needs_elevation() { "HKLM" } else { "HKCU" };
                let leaf = path.rsplit('\\').next().unwrap_or(path);
                let key = if name.is_empty() { "(default)" } else { name };
                format!("{root}\\…\\{leaf} : {key}")
            }
        }
    }
}
