//! The app install catalog, imported from WinUtil's curated application list
//! (MIT-licensed). Installing shells out to `winget install --id <id> -e` and
//! removing to `winget uninstall --id <id> -e`, both in a visible elevated
//! console so winget can show progress and elevate for machine-scope packages.

use std::collections::{BTreeMap, HashSet};

use serde::Deserialize;

/// One app, ready for the UI.
pub struct App {
    pub name: String,
    pub desc: String,
    /// winget package id (`--id`, exact match).
    pub id: String,
    pub category: String,
}

// Shape of `winget export` output — we only want the package identifiers.
#[derive(Deserialize)]
struct WingetExport {
    #[serde(rename = "Sources", default)]
    sources: Vec<WingetSource>,
}
#[derive(Deserialize)]
struct WingetSource {
    #[serde(rename = "Packages", default)]
    packages: Vec<WingetPkg>,
}
#[derive(Deserialize)]
struct WingetPkg {
    #[serde(rename = "PackageIdentifier", default)]
    identifier: String,
}

/// The set of currently-installed winget package ids (lowercased for
/// case-insensitive matching against the catalog), via `winget export`.
///
/// This queries winget's sources and takes a few seconds, so callers must run it
/// off the UI thread. Returns an empty set if winget is missing or errors — the
/// UI treats "empty scan result" as "state unknown", never as "nothing installed".
pub fn installed_ids() -> HashSet<String> {
    #[cfg(windows)]
    use std::os::windows::process::CommandExt;

    let out = std::env::temp_dir().join("neonprime-winget-export.json");
    let mut cmd = std::process::Command::new("winget");
    cmd.args([
        "export",
        "-o",
        &out.to_string_lossy(),
        "--accept-source-agreements",
        "--disable-interactivity",
    ]);
    #[cfg(windows)]
    cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW

    let mut set = HashSet::new();
    if cmd.output().is_ok() {
        if let Ok(text) = std::fs::read_to_string(&out) {
            if let Ok(export) = serde_json::from_str::<WingetExport>(&text) {
                for src in export.sources {
                    for pkg in src.packages {
                        if !pkg.identifier.is_empty() {
                            set.insert(pkg.identifier.to_lowercase());
                        }
                    }
                }
            }
        }
    }
    let _ = std::fs::remove_file(&out);
    set
}

/// True if `id` is present in a set from [`installed_ids`] (case-insensitive).
pub fn is_installed(id: &str, installed: &HashSet<String>) -> bool {
    installed.contains(&id.to_lowercase())
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
    apps.sort_by_key(|a| a.name.to_lowercase());
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

/// A `winget` command line (as a single string) to install `id`, for running in
/// a console. Winget ids are safe tokens (letters, digits, dots, hyphens).
pub fn install_cmd(id: &str) -> String {
    format!("winget install --id {id} -e --accept-source-agreements --accept-package-agreements")
}

/// A `winget` command line to uninstall `id`.
pub fn uninstall_cmd(id: &str) -> String {
    format!("winget uninstall --id {id} -e")
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
    fn is_installed_is_case_insensitive() {
        let mut set = HashSet::new();
        set.insert("mozilla.firefox".to_string());
        assert!(is_installed("Mozilla.Firefox", &set));
        assert!(is_installed("mozilla.firefox", &set));
        assert!(!is_installed("Foo.Bar", &set));
    }

    #[test]
    fn install_args_shape() {
        let args = install_args("Foo.Bar");
        assert_eq!(args[0], "install");
        assert!(args.contains(&"Foo.Bar".to_string()));
        assert!(args.contains(&"-e".to_string()));
    }
}
