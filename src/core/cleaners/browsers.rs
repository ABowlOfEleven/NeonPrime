//! Browser profile cleaners: Chromium-family (Chrome, Edge, Brave, Vivaldi) and
//! Firefox. Detected at runtime, so only installed browsers appear, and every
//! profile a browser has is covered by one cleaner.
//!
//! Options split into a safe default (Cache, on) and destructive ones (Cookies,
//! History, Form data, Sessions) that are off, carry a warning, and are guarded:
//! the caller refuses to run them while the browser is open, since deleting a
//! live profile's cookie or history database corrupts it. This is file-deletion
//! granularity only (whole DB files), not row-level pruning.

use super::{path, CleanAction, Cleaner, CleanerOption, Group, Source};
use sysinfo::{ProcessesToUpdate, System};

/// Every detected browser as a cleaner, Chromium family first then Firefox.
pub fn browser_cleaners() -> Vec<Cleaner> {
    let mut v = Vec::new();
    for s in CHROMIUM {
        if let Some(c) = chromium_cleaner(s.id, s.name, s.root, s.proc) {
            v.push(c);
        }
    }
    if let Some(c) = firefox_cleaner() {
        v.push(c);
    }
    v
}

/// True if any process whose base name matches one of `names` (case-insensitive,
/// e.g. "chrome.exe") is running. Used to guard destructive browser options.
pub fn any_running(names: &[String]) -> bool {
    if names.is_empty() {
        return false;
    }
    let wanted: Vec<String> = names.iter().map(|n| n.to_ascii_lowercase()).collect();
    let mut sys = System::new();
    sys.refresh_processes(ProcessesToUpdate::All, true);
    sys.processes().values().any(|p| {
        let n = p.name().to_string_lossy().to_ascii_lowercase();
        wanted.contains(&n)
    })
}

struct ChromiumSpec {
    id: &'static str,
    name: &'static str,
    /// Raw `%VAR%` path of the "User Data" directory.
    root: &'static str,
    proc: &'static str,
}

const CHROMIUM: &[ChromiumSpec] = &[
    ChromiumSpec {
        id: "chrome",
        name: "Google Chrome",
        root: "%LOCALAPPDATA%\\Google\\Chrome\\User Data",
        proc: "chrome.exe",
    },
    ChromiumSpec {
        id: "edge",
        name: "Microsoft Edge",
        root: "%LOCALAPPDATA%\\Microsoft\\Edge\\User Data",
        proc: "msedge.exe",
    },
    ChromiumSpec {
        id: "brave",
        name: "Brave",
        root: "%LOCALAPPDATA%\\BraveSoftware\\Brave-Browser\\User Data",
        proc: "brave.exe",
    },
    ChromiumSpec {
        id: "vivaldi",
        name: "Vivaldi",
        root: "%LOCALAPPDATA%\\Vivaldi\\User Data",
        proc: "vivaldi.exe",
    },
];

/// Build one option. `warning`/`guard` mark destructive options.
fn opt(
    id: &str,
    label: &str,
    desc: &str,
    default_on: bool,
    warning: Option<&str>,
    guard: bool,
    actions: Vec<CleanAction>,
) -> CleanerOption {
    CleanerOption {
        id: id.into(),
        label: label.into(),
        desc: desc.into(),
        default_on,
        warning: warning.map(|s| s.to_string()),
        elevated: false,
        guard_running: guard,
        actions,
    }
}

/// A Chromium cleaner for `root` if any profile is present, covering all
/// profiles. Named separately from the const spec so it is testable with an
/// arbitrary root.
fn chromium_cleaner(id: &str, name: &str, root: &str, procname: &str) -> Option<Cleaner> {
    let profiles = chromium_profiles(root);
    if profiles.is_empty() {
        return None;
    }
    // EmptyDir on each named subdir of every profile.
    let dirs = |subs: &[&str]| -> Vec<CleanAction> {
        let mut a = Vec::new();
        for p in &profiles {
            for s in subs {
                a.push(CleanAction::EmptyDir {
                    root: format!("{root}\\{p}\\{s}"),
                });
            }
        }
        a
    };
    // Delete named files inside `subdir` (empty = the profile root) of every
    // profile.
    let files = |names: &[&str], subdir: &str| -> Vec<CleanAction> {
        let mut a = Vec::new();
        for p in &profiles {
            let base = if subdir.is_empty() {
                format!("{root}\\{p}")
            } else {
                format!("{root}\\{p}\\{subdir}")
            };
            for n in names {
                a.push(CleanAction::Files {
                    root: base.clone(),
                    mask: (*n).into(),
                    recurse: false,
                    remove_self: false,
                });
            }
        }
        a
    };

    let mut sessions = files(
        &[
            "Current Session",
            "Current Tabs",
            "Last Session",
            "Last Tabs",
        ],
        "",
    );
    sessions.extend(dirs(&["Sessions"]));

    let mut cookies = files(&["Cookies"], "Network");
    cookies.extend(files(&["Cookies"], ""));

    let options = vec![
        opt(
            "cache",
            "Cache",
            "Cached images, scripts, and service-worker data.",
            true,
            None,
            false,
            dirs(&[
                "Cache",
                "Code Cache",
                "GPUCache",
                "Service Worker\\CacheStorage",
            ]),
        ),
        opt(
            "cookies",
            "Cookies",
            "Login cookies for every site.",
            false,
            Some("Signs you out of websites."),
            true,
            cookies,
        ),
        opt(
            "history",
            "History",
            "Browsing and download history.",
            false,
            Some("Erases your browsing history."),
            true,
            files(&["History", "Visited Links"], ""),
        ),
        opt(
            "formdata",
            "Form data",
            "Saved form and autofill entries.",
            false,
            Some("Clears saved form and autofill entries."),
            true,
            files(&["Web Data"], ""),
        ),
        opt(
            "sessions",
            "Sessions",
            "Open tabs and the restore session.",
            false,
            Some("Closes tabs you could otherwise restore."),
            true,
            sessions,
        ),
    ];

    Some(Cleaner {
        id: id.into(),
        name: name.into(),
        group: Group::Browsers,
        source: Source::Builtin,
        running_procs: vec![procname.into()],
        options,
    })
}

/// Profile directory names under a Chromium "User Data" root: "Default" and
/// "Profile N" folders that actually contain a Preferences file.
fn chromium_profiles(raw_root: &str) -> Vec<String> {
    let Ok(root) = path::expand_and_validate(raw_root) else {
        return Vec::new();
    };
    let mut v = Vec::new();
    if let Ok(rd) = std::fs::read_dir(&root) {
        for e in rd.flatten() {
            if !e.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                continue;
            }
            let name = e.file_name().to_string_lossy().to_string();
            if (name == "Default" || name.starts_with("Profile "))
                && e.path().join("Preferences").is_file()
            {
                v.push(name);
            }
        }
    }
    v.sort();
    v
}

/// A Firefox cleaner covering every profile, if any exist. Firefox splits its
/// data: profile databases live under %APPDATA%, caches under %LOCALAPPDATA%,
/// with matching profile folder names in both.
fn firefox_cleaner() -> Option<Cleaner> {
    const ROAM: &str = "%APPDATA%\\Mozilla\\Firefox\\Profiles";
    const LOCAL: &str = "%LOCALAPPDATA%\\Mozilla\\Firefox\\Profiles";
    let profiles = firefox_profiles(ROAM);
    if profiles.is_empty() {
        return None;
    }

    let mut cache = Vec::new();
    for p in &profiles {
        cache.push(CleanAction::EmptyDir {
            root: format!("{LOCAL}\\{p}\\cache2"),
        });
        cache.push(CleanAction::EmptyDir {
            root: format!("{LOCAL}\\{p}\\startupCache"),
        });
    }

    // Delete a named database file in each profile (roaming).
    let db = |name: &str| -> Vec<CleanAction> {
        profiles
            .iter()
            .map(|p| CleanAction::Files {
                root: format!("{ROAM}\\{p}"),
                mask: name.into(),
                recurse: false,
                remove_self: false,
            })
            .collect()
    };

    let mut sessions = db("sessionstore.jsonlz4");
    for p in &profiles {
        sessions.push(CleanAction::EmptyDir {
            root: format!("{ROAM}\\{p}\\sessionstore-backups"),
        });
    }

    let options = vec![
        opt(
            "cache",
            "Cache",
            "Disk and startup caches.",
            true,
            None,
            false,
            cache,
        ),
        opt(
            "cookies",
            "Cookies",
            "Login cookies for every site.",
            false,
            Some("Signs you out of websites."),
            true,
            db("cookies.sqlite"),
        ),
        opt(
            "history",
            "History",
            "Browsing and download history.",
            false,
            Some("Erases your browsing history."),
            true,
            db("places.sqlite"),
        ),
        opt(
            "formdata",
            "Form data",
            "Saved form and search history.",
            false,
            Some("Clears saved form entries."),
            true,
            db("formhistory.sqlite"),
        ),
        opt(
            "sessions",
            "Sessions",
            "Open tabs and the restore session.",
            false,
            Some("Closes tabs you could otherwise restore."),
            true,
            sessions,
        ),
    ];

    Some(Cleaner {
        id: "firefox".into(),
        name: "Mozilla Firefox".into(),
        group: Group::Browsers,
        source: Source::Builtin,
        running_procs: vec!["firefox.exe".into()],
        options,
    })
}

/// Firefox profile folder names under a Profiles root.
fn firefox_profiles(raw_root: &str) -> Vec<String> {
    let Ok(root) = path::expand_and_validate(raw_root) else {
        return Vec::new();
    };
    let mut v = Vec::new();
    if let Ok(rd) = std::fs::read_dir(&root) {
        for e in rd.flatten() {
            if e.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                v.push(e.file_name().to_string_lossy().to_string());
            }
        }
    }
    v.sort();
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn any_running_is_false_for_empty() {
        assert!(!any_running(&[]));
    }

    #[test]
    fn detected_browsers_have_safe_cache_and_guarded_destructive_options() {
        // Whatever the CI runner has installed, every produced cleaner must obey
        // the safety contract.
        for c in browser_cleaners() {
            assert!(
                !c.running_procs.is_empty(),
                "browser cleaner names a process"
            );
            let cache = c
                .options
                .iter()
                .find(|o| o.id == "cache")
                .expect("has a cache option");
            assert!(cache.default_on && !cache.guard_running && cache.warning.is_none());
            for o in c.options.iter().filter(|o| o.id != "cache") {
                assert!(!o.default_on, "{} default off", o.id);
                assert!(o.guard_running, "{} guarded", o.id);
                assert!(o.warning.is_some(), "{} warned", o.id);
            }
        }
    }

    #[test]
    fn chromium_cleaner_covers_all_profiles() {
        // Build a fake Chromium tree inside %TEMP% (inside the sandbox), with two
        // profiles, and point the builder at it.
        let base = std::env::temp_dir().join(format!("np_chromium_{}", std::process::id()));
        let user_data = base.join("User Data");
        for p in ["Default", "Profile 1", "NotAProfile"] {
            fs::create_dir_all(user_data.join(p)).unwrap();
        }
        // Only Default and Profile 1 get a Preferences file (the marker).
        fs::write(user_data.join("Default").join("Preferences"), "{}").unwrap();
        fs::write(user_data.join("Profile 1").join("Preferences"), "{}").unwrap();

        let root = user_data.to_string_lossy().to_string();
        let c = chromium_cleaner("t", "Test", &root, "x.exe").expect("detected");
        assert_eq!(c.options.len(), 5);
        assert_eq!(c.running_procs, vec!["x.exe"]);

        // Cache targets 4 subdirs across 2 valid profiles = 8 actions; the
        // Preferences-less "NotAProfile" is ignored.
        let cache = c.options.iter().find(|o| o.id == "cache").unwrap();
        assert_eq!(cache.actions.len(), 8);

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn chromium_cleaner_absent_when_no_profiles() {
        let empty = std::env::temp_dir().join(format!("np_chromium_empty_{}", std::process::id()));
        fs::create_dir_all(&empty).unwrap();
        let root = empty.to_string_lossy().to_string();
        assert!(chromium_cleaner("t", "Test", &root, "x.exe").is_none());
        let _ = fs::remove_dir_all(&empty);
    }
}
