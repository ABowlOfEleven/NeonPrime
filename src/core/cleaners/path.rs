//! Path sandbox shared by every cleaner source.
//!
//! A cleaner action names its root as a raw string that may contain `%ENV%`
//! variables. Built-in cleaners and imported winapp2 definitions both use this
//! form, and both are validated identically here before the engine touches the
//! filesystem:
//!
//! 1. Reject any `..` traversal up front.
//! 2. Expand `%VARS%` against a fixed allowlist. An unknown variable is a hard
//!    reject, so a definition cannot smuggle in an arbitrary environment value.
//! 3. Confine the result to a small set of safe base directories (the user
//!    profile, Public, ProgramData, and a handful of Windows temp/log dirs).
//!    Anything resolving to a drive root, a system directory, or the bare user
//!    profile is rejected.
//!
//! The net effect: whatever an untrusted definition says, the deleter can only
//! ever act inside caches and temp locations.

use std::path::{Component, Path, PathBuf};

#[derive(Debug, PartialEq, Eq)]
pub enum RejectReason {
    /// A `%VAR%` that is not on the expansion allowlist.
    UnknownVar(String),
    /// The raw path contained `..`.
    Traversal,
    /// The expanded path is not under an allowed base directory.
    OutsideSandbox,
}

/// Expand `%VARS%` (allowlist only) then confine to a safe base directory. On
/// success the returned path is absolute and known to sit inside the sandbox.
pub fn expand_and_validate(raw: &str) -> Result<PathBuf, RejectReason> {
    if raw.contains("..") {
        return Err(RejectReason::Traversal);
    }
    let expanded = expand(raw)?;
    confine(&expanded)?;
    Ok(expanded)
}

/// Expand `%VARS%` (allowlist) WITHOUT confining to the sandbox. Detection-only
/// (e.g. "does %ProgramFiles%\App exist"), never a deletion target. Returns None
/// on an unknown variable.
pub fn expand_only(raw: &str) -> Option<PathBuf> {
    expand(raw).ok()
}

/// Replace every `%NAME%` with its allowlisted value. A `%NAME%` whose name is
/// not recognized is a hard error; a stray unmatched `%` is kept literally.
fn expand(raw: &str) -> Result<PathBuf, RejectReason> {
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw;
    while let Some(start) = rest.find('%') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let Some(end) = after.find('%') else {
            // Unmatched '%': treat literally and stop scanning for variables.
            out.push('%');
            out.push_str(after);
            return Ok(PathBuf::from(out));
        };
        let name = &after[..end];
        let val = lookup(name).ok_or_else(|| RejectReason::UnknownVar(name.to_string()))?;
        out.push_str(&val);
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    Ok(PathBuf::from(out))
}

/// The expansion allowlist. Names are matched case-insensitively. Anything not
/// here cannot be expanded, so definitions are limited to well-known roots.
fn lookup(name: &str) -> Option<String> {
    let env = |k: &str| std::env::var(k).ok();
    let profile = |suffix: &str| env("USERPROFILE").map(|p| format!("{p}{suffix}"));
    match name.to_ascii_uppercase().as_str() {
        "TEMP" | "TMP" => env("TEMP").or_else(|| env("TMP")),
        "LOCALAPPDATA" => env("LOCALAPPDATA"),
        "APPDATA" => env("APPDATA"),
        "LOCALLOWAPPDATA" => profile("\\AppData\\LocalLow"),
        "PROGRAMDATA" | "COMMONAPPDATA" => env("ProgramData"),
        "USERPROFILE" => env("USERPROFILE"),
        "PUBLIC" => env("PUBLIC"),
        "WINDIR" | "SYSTEMROOT" => env("SystemRoot").or_else(|| env("windir")),
        "PROGRAMFILES" => env("ProgramFiles"),
        "PROGRAMFILES(X86)" => env("ProgramFiles(x86)"),
        "COMMONPROGRAMFILES" => env("CommonProgramFiles"),
        "SYSTEMDRIVE" => env("SystemDrive"),
        // Note: personal-folder variables (Documents/Music/Pictures/Videos) are
        // deliberately NOT expandable. They are never a legitimate cache/temp
        // target and expanding them would let an untrusted definition point the
        // deleter at the user's irreplaceable files.
        _ => None,
    }
}

/// Lower-cased path components (drive prefix included, root/`.` dropped) for
/// case-insensitive prefix comparison without touching the filesystem.
fn components_lower(p: &Path) -> Vec<String> {
    p.components()
        .filter_map(|c| match c {
            Component::Prefix(pre) => Some(pre.as_os_str().to_string_lossy().to_ascii_lowercase()),
            Component::Normal(s) => Some(s.to_string_lossy().to_ascii_lowercase()),
            Component::ParentDir => Some("..".to_string()),
            Component::RootDir | Component::CurDir => None,
        })
        .collect()
}

/// True when `path` is at or below `base` (component-wise).
fn under(path: &[String], base: &[String]) -> bool {
    !base.is_empty() && path.len() >= base.len() && path[..base.len()] == *base
}

/// Bases where the directory itself may be targeted (we empty %TEMP%, the
/// Windows temp/update/log dirs, etc.).
fn exact_or_under_bases() -> Vec<PathBuf> {
    let mut v = Vec::new();
    if let Ok(t) = std::env::var("TEMP") {
        v.push(PathBuf::from(t));
    }
    if let Ok(w) = std::env::var("SystemRoot").or_else(|_| std::env::var("windir")) {
        for sub in [
            "Temp",
            "SoftwareDistribution",
            "Logs",
            "Prefetch",
            "Downloaded Program Files",
        ] {
            v.push(PathBuf::from(&w).join(sub));
        }
    }
    v
}

/// Bases where only strict descendants are allowed. Under the user profile only
/// the AppData cache subtrees are eligible, never Documents/Desktop/Pictures/
/// .ssh/etc. `%PUBLIC%` and `%ProgramData%` are machine-wide roots holding apps'
/// persistent (non-cache) data and no shipped cleaner targets them, so they are
/// excluded entirely. This keeps an untrusted cleaner definition confined to
/// caches rather than personal or machine-wide application data.
fn under_only_bases() -> Vec<PathBuf> {
    let mut v = Vec::new();
    if let Ok(up) = std::env::var("USERPROFILE") {
        for sub in ["AppData\\Local", "AppData\\LocalLow", "AppData\\Roaming"] {
            v.push(PathBuf::from(&up).join(sub));
        }
    }
    v
}

/// Confine an expanded path to the sandbox.
fn confine(path: &Path) -> Result<(), RejectReason> {
    let p = components_lower(path);
    if p.is_empty() {
        return Err(RejectReason::OutsideSandbox);
    }
    for base in exact_or_under_bases() {
        let b = components_lower(&base);
        if under(&p, &b) {
            return Ok(());
        }
    }
    for base in under_only_bases() {
        let b = components_lower(&base);
        // Strict descendant only: equal-to-base (e.g. the whole profile) is out.
        if under(&p, &b) && p.len() > b.len() {
            return Ok(());
        }
    }
    Err(RejectReason::OutsideSandbox)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_traversal() {
        assert_eq!(
            expand_and_validate("%TEMP%\\..\\..\\Windows"),
            Err(RejectReason::Traversal)
        );
    }

    #[test]
    fn rejects_unknown_var() {
        match expand_and_validate("%TOTALLY_NOT_A_VAR%\\x") {
            Err(RejectReason::UnknownVar(n)) => assert_eq!(n, "TOTALLY_NOT_A_VAR"),
            other => panic!("expected UnknownVar, got {other:?}"),
        }
    }

    #[test]
    fn allows_temp_and_appdata_caches() {
        assert!(expand_and_validate("%TEMP%").is_ok());
        assert!(
            expand_and_validate("%LOCALAPPDATA%\\Microsoft\\Windows\\Explorer").is_ok(),
            "appdata cache path should be inside the sandbox"
        );
    }

    #[test]
    fn allows_windows_temp_and_update_cache() {
        assert!(expand_and_validate("%SystemRoot%\\Temp").is_ok());
        assert!(expand_and_validate("%SystemRoot%\\SoftwareDistribution\\Download").is_ok());
    }

    #[test]
    fn rejects_system_directories_and_drive_root() {
        assert_eq!(
            expand_and_validate("%SystemRoot%\\System32"),
            Err(RejectReason::OutsideSandbox)
        );
        assert_eq!(
            expand_and_validate("%SystemDrive%\\"),
            Err(RejectReason::OutsideSandbox)
        );
    }

    #[test]
    fn rejects_bare_user_profile() {
        // The whole profile must not be a target, only its descendants.
        assert_eq!(
            expand_and_validate("%USERPROFILE%"),
            Err(RejectReason::OutsideSandbox)
        );
        assert!(expand_and_validate("%USERPROFILE%\\AppData\\Local\\Temp").is_ok());
    }

    #[test]
    fn rejects_personal_folders_as_deletion_targets() {
        // Personal-folder variables are no longer expandable.
        assert_eq!(
            expand_and_validate("%DOCUMENTS%\\quarterly-report.docx"),
            Err(RejectReason::UnknownVar("DOCUMENTS".into()))
        );
        assert!(expand_and_validate("%PICTURES%").is_err());
        assert!(expand_and_validate("%MUSIC%").is_err());
        assert!(expand_and_validate("%VIDEOS%").is_err());
        // A non-cache path under the profile must not be a deletion target.
        assert!(expand_and_validate("%USERPROFILE%\\Documents\\report.docx").is_err());
        assert!(expand_and_validate("%USERPROFILE%\\.ssh\\id_rsa").is_err());
    }

    #[test]
    fn rejects_programdata_and_public_app_data() {
        // Machine-wide app-data roots are no longer deletion targets.
        assert!(expand_and_validate("%ProgramData%\\SomeVendor\\App\\app.db").is_err());
        assert!(expand_and_validate("%PUBLIC%\\SomeVendor\\shared.dat").is_err());
    }

    #[test]
    fn still_allows_real_cache_cleaner_roots() {
        // Every root the shipped built-in cleaners actually use must still pass.
        for p in [
            "%TEMP%",
            "%LOCALAPPDATA%\\Microsoft\\Windows\\Explorer",
            "%LOCALAPPDATA%\\Google\\Chrome\\User Data",
            "%APPDATA%\\Mozilla\\Firefox\\Profiles",
            "%SystemRoot%\\Temp",
            "%SystemRoot%\\SoftwareDistribution\\Download",
        ] {
            assert!(expand_and_validate(p).is_ok(), "cache root wrongly rejected: {p}");
        }
    }
}
