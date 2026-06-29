//! The app install catalog, imported from WinUtil's curated application list
//! (MIT-licensed). Installing shells out to `winget install --id <id> -e`.
//! Uninstall/rollback is out of scope — installs are not reversible actions.

use std::collections::BTreeMap;

use serde::Deserialize;

/// One app, ready for the UI.
pub struct App {
    pub name: String,
    pub desc: String,
    /// winget package id (`--id`, exact match).
    pub id: String,
    pub category: String,
}

/// Shape of each entry in WinUtil's `applications.json` (extra fields ignored).
#[derive(Deserialize)]
struct WinutilApp {
    #[serde(default)]
    category: String,
    #[serde(default)]
    content: String,
    #[serde(default)]
    winget: String,
    #[serde(default)]
    description: String,
}

const APPS_JSON: &str = include_str!("../../assets/winutil-applications.json");

/// The full app catalog, parsed from the bundled WinUtil data and sorted by name.
pub fn catalog() -> Vec<App> {
    let map: BTreeMap<String, WinutilApp> = serde_json::from_str(APPS_JSON).unwrap_or_default();
    let mut apps: Vec<App> = map
        .into_values()
        .filter(|a| !a.winget.is_empty() && a.winget != "na" && !a.content.is_empty())
        .map(|a| App {
            name: a.content,
            desc: a.description,
            id: a.winget,
            category: a.category,
        })
        .collect();
    apps.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    apps
}

/// The full `winget` argument vector for installing an app id.
pub fn install_args(id: &str) -> Vec<String> {
    vec![
        "install".into(),
        "--id".into(),
        id.into(),
        "-e".into(),
        "--accept-source-agreements".into(),
        "--accept-package-agreements".into(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_parses_many_apps() {
        let c = catalog();
        assert!(
            c.len() > 100,
            "expected the full WinUtil catalog, got {}",
            c.len()
        );
        for a in &c {
            assert!(!a.id.is_empty());
            assert!(!a.name.is_empty());
        }
    }

    #[test]
    fn install_args_shape() {
        let args = install_args("Foo.Bar");
        assert_eq!(args[0], "install");
        assert!(args.contains(&"Foo.Bar".to_string()));
        assert!(args.contains(&"-e".to_string()));
    }
}
