//! Declarative config: capture the applied tweaks + active mode to a portable
//! TOML file, and replay it on a fresh install.
//!
//! Export is a pure snapshot of current state. Import replays it through the
//! same reversible engine, so everything it does is journaled and undoable.
//! Elevated (HKLM) tweaks are skipped on import here — they need the broker —
//! and app installs are captured for reference but not auto-run.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::core::journal::Journal;
use crate::core::{engine, modes, tweaks};

#[derive(Serialize, Deserialize, Default, Debug, Clone, PartialEq)]
pub struct Config {
    /// Ids of tweaks that are currently applied.
    #[serde(default)]
    pub tweaks: Vec<String>,
    /// Active mode id, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
    /// winget ids to install (captured for reference; not auto-run on import).
    #[serde(default)]
    pub apps: Vec<String>,
}

impl Config {
    pub fn to_toml(&self) -> Result<String, toml::ser::Error> {
        toml::to_string_pretty(self)
    }
    pub fn from_toml(s: &str) -> Result<Config, toml::de::Error> {
        toml::from_str(s)
    }
}

/// Default export location: `%USERPROFILE%\neonprime-config.toml`.
pub fn default_path() -> PathBuf {
    let mut p = std::env::var_os("USERPROFILE")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    p.push("neonprime-config.toml");
    p
}

/// Snapshot the current applied tweaks and active mode.
pub fn capture() -> Config {
    let tweaks = tweaks::catalog()
        .iter()
        .filter(|t| t.is_applied())
        .map(|t| t.id.to_string())
        .collect();
    Config {
        tweaks,
        mode: modes::active(),
        apps: Vec::new(),
    }
}

/// Replay a config: apply each (unelevated) tweak's `on` actions and activate
/// the mode. Returns how many tweak actions were applied.
pub fn apply(cfg: &Config, jrnl: &mut Journal, journal_path: &Path) -> usize {
    let tweak_catalog = tweaks::catalog();
    let mut applied = 0;

    for id in &cfg.tweaks {
        let Some(t) = tweak_catalog.iter().find(|t| t.id == *id) else {
            continue;
        };
        if t.needs_elevation() {
            continue; // requires the broker; out of scope for declarative replay
        }
        for a in &t.on {
            if let Ok(reversal) = engine::apply(a) {
                jrnl.record(format!("import {}", t.name), a.clone(), reversal);
                applied += 1;
            }
        }
    }

    if let Some(mode_id) = &cfg.mode {
        if let Some(m) = modes::catalog().iter().find(|m| m.id == *mode_id) {
            for a in &m.actions {
                if let Ok(reversal) = engine::apply(a) {
                    jrnl.record(format!("import mode {}", m.name), a.clone(), reversal);
                }
            }
        }
    }

    let _ = jrnl.save(journal_path);
    applied
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toml_roundtrip_preserves_fields() {
        let cfg = Config {
            tweaks: vec!["show-file-extensions".into(), "dark-mode".into()],
            mode: Some("game".into()),
            apps: vec!["Git.Git".into()],
        };
        let s = cfg.to_toml().unwrap();
        let back = Config::from_toml(&s).unwrap();
        assert_eq!(cfg, back);
    }

    #[test]
    fn empty_config_roundtrips() {
        let cfg = Config::default();
        let s = cfg.to_toml().unwrap();
        let back = Config::from_toml(&s).unwrap();
        assert_eq!(cfg, back);
        assert!(back.mode.is_none());
    }

    #[test]
    fn capture_returns_known_tweak_ids() {
        // Every captured id must exist in the catalog.
        let ids: Vec<_> = tweaks::catalog().iter().map(|t| t.id.to_string()).collect();
        for id in capture().tweaks {
            assert!(ids.contains(&id), "captured unknown id {id}");
        }
    }
}
