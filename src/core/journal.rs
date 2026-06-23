//! The rollback journal: a persisted log of applied actions, each carrying the
//! reversal needed to undo it. Lives in `%APPDATA%\NeonPrime\journal.json`.

use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::core::action::{Action, Reversal};

/// One applied change.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Entry {
    pub id: u64,
    /// Unix seconds when applied.
    pub ts: u64,
    pub label: String,
    pub action: Action,
    pub reversal: Reversal,
    /// False once the entry has been reverted.
    pub active: bool,
}

#[derive(Serialize, Deserialize, Default)]
pub struct Journal {
    pub entries: Vec<Entry>,
    #[serde(skip)]
    next_id: u64,
}

/// Default on-disk location: `%APPDATA%\NeonPrime\journal.json`.
pub fn default_path() -> PathBuf {
    let mut p = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    p.push("NeonPrime");
    p.push("journal.json");
    p
}

impl Journal {
    /// Load from disk, or start empty if absent/corrupt.
    pub fn load(path: &Path) -> Self {
        let mut j: Journal = std::fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        j.next_id = j.entries.iter().map(|e| e.id).max().unwrap_or(0) + 1;
        j
    }

    pub fn save(&self, path: &Path) -> io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let s = serde_json::to_string_pretty(self).map_err(io::Error::other)?;
        std::fs::write(path, s)
    }

    /// Record a newly-applied action, returning its entry id.
    pub fn record(&mut self, label: impl Into<String>, action: Action, reversal: Reversal) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        self.entries.push(Entry {
            id,
            ts: now(),
            label: label.into(),
            action,
            reversal,
            active: true,
        });
        id
    }

    /// Find an active entry by id.
    pub fn get(&self, id: u64) -> Option<&Entry> {
        self.entries.iter().find(|e| e.id == id)
    }

    /// Mark an entry reverted (call after its reversal has been executed).
    pub fn mark_reverted(&mut self, id: u64) {
        if let Some(e) = self.entries.iter_mut().find(|e| e.id == id) {
            e.active = false;
        }
    }

    /// Entries still in effect, most-recent first.
    pub fn active(&self) -> impl Iterator<Item = &Entry> {
        self.entries.iter().rev().filter(|e| e.active)
    }
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::action::{Hive, RegValue};

    fn sample() -> (Action, Reversal) {
        (
            Action::SetReg {
                hive: Hive::Hkcu,
                path: "Software\\NeonPrime\\Test".into(),
                name: "X".into(),
                value: RegValue::Dword(1),
            },
            Reversal::RestoreReg {
                hive: Hive::Hkcu,
                path: "Software\\NeonPrime\\Test".into(),
                name: "X".into(),
                previous: None,
            },
        )
    }

    #[test]
    fn record_roundtrips_through_disk() {
        let dir = std::env::temp_dir().join(format!("np-journal-{}", std::process::id()));
        let path = dir.join("journal.json");
        let _ = std::fs::remove_dir_all(&dir);

        let mut j = Journal::load(&path);
        let (a, r) = sample();
        let id = j.record("set X", a, r);
        j.save(&path).unwrap();

        let reloaded = Journal::load(&path);
        assert_eq!(reloaded.entries.len(), 1);
        assert_eq!(reloaded.get(id).unwrap().label, "set X");
        // next_id must continue past the loaded max.
        let mut reloaded = reloaded;
        let (a, r) = sample();
        let id2 = reloaded.record("set X again", a, r);
        assert_eq!(id2, id + 1);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn mark_reverted_excludes_from_active() {
        let mut j = Journal::default();
        j.next_id = 1;
        let (a, r) = sample();
        let id = j.record("x", a, r);
        assert_eq!(j.active().count(), 1);
        j.mark_reverted(id);
        assert_eq!(j.active().count(), 0);
    }
}
