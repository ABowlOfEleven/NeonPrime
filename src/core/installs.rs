//! The app install catalog, imported from WinUtil's curated application list
//! (MIT-licensed). Installing shells out to `winget install --id <id> -e` and
//! removing to `winget uninstall --id <id> -e`, both in a visible elevated
//! console so winget can show progress and elevate for machine-scope packages.

use std::collections::{BTreeMap, HashSet};

use serde::Deserialize;

use crate::core::action::{Hive, RegValue};
use crate::core::registry;

/// One app, ready for the UI.
pub struct App {
    pub name: String,
    pub desc: String,
    /// winget package id (`--id`, exact match). Empty for GitHub-release apps.
    pub id: String,
    pub category: String,
    /// `owner/name` for apps installed from a GitHub release MSI instead of
    /// winget. Empty for normal winget apps.
    pub repo: String,
}

// Shape of `winget export` output, we only want the package identifiers.
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
/// off the UI thread. Returns an empty set if winget is missing or errors, the
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

/// Add/Remove-Programs display names (lowercased) from the registry uninstall
/// keys. This catches apps installed outside winget (e.g. a GitHub-release MSI),
/// which `winget export` omits. Fast (registry only), safe to call off-thread.
pub fn installed_arp_names() -> HashSet<String> {
    const UNINSTALL: &str = "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall";
    const WOW: &str = "SOFTWARE\\WOW6432Node\\Microsoft\\Windows\\CurrentVersion\\Uninstall";
    let mut set = HashSet::new();
    for (hive, base) in [
        (Hive::Hklm, UNINSTALL),
        (Hive::Hklm, WOW),
        (Hive::Hkcu, UNINSTALL),
    ] {
        for sub in registry::list_subkeys(hive, base) {
            let path = format!("{base}\\{sub}");
            if let Ok(Some(RegValue::Sz(name))) = registry::read(hive, &path, "DisplayName") {
                if !name.is_empty() {
                    set.insert(name.to_lowercase());
                }
            }
        }
    }
    set
}

/// A snapshot of what's installed, from both winget and Add/Remove Programs.
pub struct Installed {
    /// winget package identifiers (lowercased).
    pub ids: HashSet<String>,
    /// Add/Remove-Programs display names (lowercased).
    pub arp: HashSet<String>,
}

/// Scan installed state from winget + the registry. Slow (winget); off-thread.
pub fn scan_installed() -> Installed {
    Installed {
        ids: installed_ids(),
        arp: installed_arp_names(),
    }
}

/// Whether `app` is installed. winget apps match by exact package id; author
/// (GitHub-release) apps match their name against Add/Remove Programs.
pub fn is_installed(app: &App, scan: &Installed) -> bool {
    if !app.id.is_empty() {
        scan.ids.contains(&app.id.to_lowercase())
    } else {
        let n = app.name.to_lowercase();
        scan.arp.iter().any(|d| d.contains(&n))
    }
}

/// A PowerShell script that downloads the latest GitHub-release MSI for `repo`
/// (`owner/name`) and installs it. Runs in a visible elevated console.
pub fn github_install_script(repo: &str) -> String {
    format!(
        "$ErrorActionPreference = 'Stop'\n\
         Write-Host 'Fetching the latest release of {repo}...' -ForegroundColor Cyan\n\
         $rel = Invoke-RestMethod -Uri 'https://api.github.com/repos/{repo}/releases/latest' -Headers @{{ 'User-Agent' = 'NeonPrime' }}\n\
         $asset = $rel.assets | Where-Object {{ $_.name -like '*x64.msi' -or $_.name -like '*Setup.msi' -or $_.name -like '*.msi' }} | Select-Object -First 1\n\
         if (-not $asset) {{ Write-Host 'No .msi asset in the latest release.' -ForegroundColor Red; Read-Host 'Press Enter to close'; exit 1 }}\n\
         $out = Join-Path $env:TEMP $asset.name\n\
         Write-Host \"Downloading $($asset.name)...\" -ForegroundColor Yellow\n\
         Invoke-WebRequest -Uri $asset.browser_download_url -OutFile $out\n\
         Write-Host 'Installing...' -ForegroundColor Yellow\n\
         $p = Start-Process msiexec.exe -ArgumentList '/i', ('\"' + $out + '\"') -Wait -PassThru\n\
         Write-Host \"Done (exit $($p.ExitCode)).\" -ForegroundColor Green\n"
    )
}

/// A `winget` command line to uninstall an Add/Remove-Programs app by name.
pub fn uninstall_named_cmd(name: &str) -> String {
    format!("winget uninstall --name \"{name}\"")
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

/// The full app catalog, parsed from the bundled WinUtil data plus the NeonPrime
/// author's own public projects, sorted by name.
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
            repo: String::new(),
        })
        .collect();
    apps.extend(author_apps());
    apps.sort_by_key(|a| a.name.to_lowercase());
    apps
}

/// The NeonPrime author's public Rust projects (search "ABowlOfEleven" to find
/// them all). hopscout ships on winget; the others install from their GitHub
/// release MSI. NeonPrime itself is intentionally omitted, you're running it.
fn author_apps() -> Vec<App> {
    let author = |name: &str, desc: &str, id: &str, repo: &str| App {
        name: name.into(),
        desc: desc.into(),
        id: id.into(),
        category: "ABowlOfEleven".into(),
        repo: repo.into(),
    };
    vec![
        author(
            "hopscout",
            "A modern MTR-style traceroute and network monitor (Rust): CLI + GUI.",
            "ABowlOfEleven.hopscout",
            "",
        ),
        author(
            "GenomeForge",
            "A local, native genome browser and variant explorer, your DNA stays on your machine.",
            "",
            "ABowlOfEleven/genomeforge",
        ),
        author(
            "Formant",
            "A creator-grade, CPU-only Rust vocal processor. A lightweight Voicemod alternative.",
            "",
            "ABowlOfEleven/Formant",
        ),
    ]
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

/// True for a syntactically valid winget id (Publisher.Package tokens only).
/// Guards the elevated install script against injection from an imported profile.
pub fn is_safe_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 100
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '+' | '_'))
}

/// One elevated console script that installs every id in turn (for applying a
/// provisioning profile's app set). Unsafe-looking ids are dropped, so this must
/// only ever run on ids that also passed the catalog whitelist at the call site.
/// Empty when there are no installable ids.
pub fn install_many_script(ids: &[String]) -> String {
    ids.iter()
        .filter(|id| is_safe_id(id))
        .map(|id| install_cmd(id))
        .collect::<Vec<_>>()
        .join("; ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_id_rejects_injection() {
        assert!(is_safe_id("Microsoft.PowerShell"));
        assert!(is_safe_id("Git.Git"));
        assert!(!is_safe_id("x; calc.exe"));
        assert!(!is_safe_id("a && b"));
        assert!(!is_safe_id("id`whoami`"));
        assert!(!is_safe_id(""));
    }

    #[test]
    fn install_many_drops_unsafe_ids() {
        let script = install_many_script(&["Git.Git".into(), "evil; calc".into()]);
        assert!(script.contains("Git.Git"));
        assert!(!script.contains("calc"));
    }

    #[test]
    fn catalog_parses_many_apps() {
        let c = catalog();
        assert!(
            c.len() > 100,
            "expected the full WinUtil catalog, got {}",
            c.len()
        );
        for a in &c {
            // Every app installs via a winget id OR a GitHub repo.
            assert!(!a.id.is_empty() || !a.repo.is_empty());
            assert!(!a.name.is_empty());
        }
    }

    #[test]
    fn catalog_includes_author_apps() {
        let c = catalog();
        assert!(c
            .iter()
            .any(|a| a.name == "hopscout" && a.id == "ABowlOfEleven.hopscout"));
        assert!(c
            .iter()
            .any(|a| a.name == "GenomeForge" && a.repo == "ABowlOfEleven/genomeforge"));
        assert!(c.iter().any(|a| a.name == "Formant"));
    }

    #[test]
    fn is_installed_matches_winget_id_and_arp_name() {
        let scan = Installed {
            ids: HashSet::from(["mozilla.firefox".to_string()]),
            arp: HashSet::from(["genomeforge 0.1.2".to_string()]),
        };
        let ff = App {
            name: "Firefox".into(),
            desc: String::new(),
            id: "Mozilla.Firefox".into(),
            category: String::new(),
            repo: String::new(),
        };
        assert!(is_installed(&ff, &scan));
        // Author app matched by ARP display name (substring).
        let gf = App {
            name: "GenomeForge".into(),
            desc: String::new(),
            id: String::new(),
            category: String::new(),
            repo: "ABowlOfEleven/genomeforge".into(),
        };
        assert!(is_installed(&gf, &scan));
        let missing = App {
            name: "Nope".into(),
            desc: String::new(),
            id: "No.Thing".into(),
            category: String::new(),
            repo: String::new(),
        };
        assert!(!is_installed(&missing, &scan));
    }

    #[test]
    fn github_script_targets_repo() {
        let s = github_install_script("ABowlOfEleven/genomeforge");
        assert!(s.contains("ABowlOfEleven/genomeforge"));
        assert!(s.contains(".msi"));
        assert!(s.contains("msiexec"));
    }

    #[test]
    fn install_args_shape() {
        let args = install_args("Foo.Bar");
        assert_eq!(args[0], "install");
        assert!(args.contains(&"Foo.Bar".to_string()));
        assert!(args.contains(&"-e".to_string()));
    }
}
