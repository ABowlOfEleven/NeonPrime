//! Lightweight persisted UI settings (`%APPDATA%\NeonPrime\settings.json`).

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Default, Debug, Clone)]
pub struct Settings {
    /// Selected theme: 0 Holo, 1 HEV, 2 Mann Co. (TF2), 3 Aperture (Portal),
    /// 4 SteamOS (Steam Deck Gaming Mode).
    #[serde(default)]
    pub theme: i32,
    /// Legacy pre-multi-theme flag. Read once to migrate an old config, then
    /// dropped (never written back).
    #[serde(default, skip_serializing)]
    pub theme_hev: bool,
}

fn path() -> PathBuf {
    let mut p = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    p.push("NeonPrime");
    p.push("settings.json");
    p
}

impl Settings {
    pub fn load() -> Self {
        let mut s: Settings = std::fs::read_to_string(path())
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        // Migrate the old boolean HEV flag to the theme index.
        if s.theme == 0 && s.theme_hev {
            s.theme = 1;
        }
        s.theme_hev = false;
        s
    }

    pub fn save(&self) {
        let p = path();
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(s) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(&p, s);
        }
    }
}
