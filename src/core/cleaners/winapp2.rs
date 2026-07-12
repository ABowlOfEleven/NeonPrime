//! winapp2.ini importer.
//!
//! Parses the community CCleaner-style cleaner definitions into the shared
//! [`Cleaner`] model. This is untrusted input, so it is deliberately narrow:
//!
//! * **File cleaning only.** `RegKey` directives are recognized but never turned
//!   into actions, and an entry that has *only* registry keys is dropped. This is
//!   the "reg deletes disabled" decision.
//! * **Sandbox still applies.** Every `FileKey` path is a raw string that the
//!   engine expands and confines via [`super::path`] at execution time, exactly
//!   like a built-in cleaner. A definition cannot point the deleter outside the
//!   cache/temp sandbox no matter what it says.
//! * **Detection filters the list.** `DetectFile` / `Detect` / `SpecialDetect`
//!   decide whether the software is installed, so the panel only shows relevant
//!   cleaners. Detection paths are expanded but not confined (they may legitimately
//!   check Program Files); they are never deleted.
//!
//! The parsed result is cached and keyed on the file's modification time, so
//! [`imported_cleaners`] is cheap to call repeatedly (including from the panel's
//! rebuild) and only re-parses when the file actually changes.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::SystemTime;

use super::{path, CleanAction, Cleaner, CleanerOption, Group, Source};
use crate::core::action::Hive;
use crate::core::registry;

/// Where NeonPrime looks for the winapp2.ini import: alongside the config file
/// in the app's data directory. Drop a winapp2.ini there and rescan.
pub fn winapp2_path() -> PathBuf {
    let mut p = crate::core::config::default_path();
    p.set_file_name("winapp2.ini");
    p
}

struct Cache {
    mtime: Option<SystemTime>,
    cleaners: Vec<Cleaner>,
}

static CACHE: Mutex<Option<Cache>> = Mutex::new(None);

/// Imported cleaners for currently-installed software, from the winapp2.ini at
/// [`winapp2_path`]. Empty when the file is absent. Re-parses only when the
/// file's mtime changes.
pub fn imported_cleaners() -> Vec<Cleaner> {
    let path = winapp2_path();
    let mtime = std::fs::metadata(&path)
        .ok()
        .and_then(|m| m.modified().ok());
    let mut guard = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(c) = guard.as_ref() {
        if c.mtime == mtime && mtime.is_some() {
            return c.cleaners.clone();
        }
    }
    let cleaners = if mtime.is_some() {
        parse_file(&path)
    } else {
        Vec::new()
    };
    *guard = Some(Cache {
        mtime,
        cleaners: cleaners.clone(),
    });
    cleaners
}

/// Drop the cache so the next [`imported_cleaners`] re-parses even if the mtime
/// is unchanged (used by an explicit re-import).
pub fn invalidate_import() {
    *CACHE.lock().unwrap_or_else(|e| e.into_inner()) = None;
}

/// Parse a winapp2.ini file. Empty on a missing/unreadable file.
pub fn parse_file(path: &Path) -> Vec<Cleaner> {
    match std::fs::read_to_string(path) {
        Ok(text) => parse_str(&text),
        Err(_) => Vec::new(),
    }
}

/// Parse winapp2.ini text into cleaners for installed software.
pub fn parse_str(text: &str) -> Vec<Cleaner> {
    split_sections(text)
        .into_iter()
        .filter_map(|s| build_cleaner(&s))
        .collect()
}

/// A raw `[Name]` section: its title and ordered key/value lines.
struct Section {
    name: String,
    entries: Vec<(String, String)>,
}

fn split_sections(text: &str) -> Vec<Section> {
    let mut out: Vec<Section> = Vec::new();
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with(';') {
            continue;
        }
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            out.push(Section {
                name: name.trim().to_string(),
                entries: Vec::new(),
            });
        } else if let Some((k, v)) = line.split_once('=') {
            if let Some(sec) = out.last_mut() {
                sec.entries
                    .push((k.trim().to_string(), v.trim().to_string()));
            }
        }
    }
    out
}

/// Case-insensitive lookup of the first value for `key`.
fn get<'a>(entries: &'a [(String, String)], key: &str) -> Option<&'a str> {
    entries
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(key))
        .map(|(_, v)| v.as_str())
}

/// All values whose key matches `prefix` followed by digits (e.g. FileKey1,
/// FileKey2), returned in numeric order.
fn numbered(entries: &[(String, String)], prefix: &str) -> Vec<String> {
    let mut hits: Vec<(u32, String)> = entries
        .iter()
        .filter_map(|(k, v)| {
            let rest = k.strip_prefix_ci(prefix)?;
            let n: u32 = rest.parse().ok()?;
            Some((n, v.clone()))
        })
        .collect();
    hits.sort_by_key(|(n, _)| *n);
    hits.into_iter().map(|(_, v)| v).collect()
}

trait StripPrefixCi {
    fn strip_prefix_ci(&self, prefix: &str) -> Option<&str>;
}
impl StripPrefixCi for String {
    fn strip_prefix_ci(&self, prefix: &str) -> Option<&str> {
        // Compare on bytes and take the tail via get(): an adversarial key whose
        // multibyte char straddles prefix.len() must yield None, not panic on a
        // non-char-boundary str slice (winapp2.ini is untrusted input).
        let bytes = self.as_bytes();
        if bytes.len() >= prefix.len()
            && bytes[..prefix.len()].eq_ignore_ascii_case(prefix.as_bytes())
        {
            self.get(prefix.len()..)
        } else {
            None
        }
    }
}

fn build_cleaner(sec: &Section) -> Option<Cleaner> {
    if !detects(&sec.entries) {
        return None;
    }
    // File cleaning only: RegKey directives are intentionally ignored.
    let mut actions: Vec<CleanAction> = Vec::new();
    for fk in numbered(&sec.entries, "FileKey") {
        actions.extend(parse_filekey(&fk));
    }
    if actions.is_empty() {
        return None;
    }

    let default_on = get(&sec.entries, "Default")
        .map(|v| v.eq_ignore_ascii_case("true"))
        .unwrap_or(true);
    let warning = get(&sec.entries, "Warning")
        .filter(|w| !w.is_empty())
        .map(|w| w.to_string());

    let option = CleanerOption {
        id: "winapp2".into(),
        label: sec.name.clone(),
        desc: describe(sec),
        default_on,
        warning,
        elevated: false,
        guard_running: false,
        actions,
    };

    Some(Cleaner {
        id: format!("winapp2:{}", sec.name),
        name: sec.name.clone(),
        group: Group::Imported,
        source: Source::Winapp2,
        running_procs: Vec::new(),
        options: vec![option],
    })
}

/// A short description line for the panel: the winapp2 Section/category if given.
fn describe(sec: &Section) -> String {
    if let Some(s) = get(&sec.entries, "Section") {
        format!("Imported cleaner ({s}).")
    } else {
        "Imported cleaner (winapp2).".to_string()
    }
}

/// Parse one `FileKey` value: `path|mask1;mask2|FLAGS`. Missing masks mean all
/// files; RECURSE / REMOVESELF flags are honored.
fn parse_filekey(value: &str) -> Vec<CleanAction> {
    let mut parts = value.split('|');
    let Some(root) = parts.next().map(|s| s.trim().to_string()) else {
        return Vec::new();
    };
    if root.is_empty() {
        return Vec::new();
    }
    let masks_raw = parts.next().unwrap_or("").trim();
    let flags = parts.next().unwrap_or("").to_ascii_uppercase();
    let recurse = flags.contains("RECURSE") || flags.contains("REMOVESELF");
    let remove_self = flags.contains("REMOVESELF");

    let masks: Vec<String> = if masks_raw.is_empty() || masks_raw == "*.*" || masks_raw == "*" {
        vec!["*".to_string()]
    } else {
        masks_raw
            .split(';')
            .map(|m| m.trim())
            .filter(|m| !m.is_empty())
            .map(|m| m.to_string())
            .collect()
    };

    masks
        .into_iter()
        .map(|mask| CleanAction::Files {
            root: root.clone(),
            mask,
            recurse,
            remove_self,
        })
        .collect()
}

/// Evaluate a section's detection directives. No detects means it always applies;
/// otherwise any single DetectFile / Detect / SpecialDetect passing is enough.
fn detects(entries: &[(String, String)]) -> bool {
    let files = numbered_with_bare(entries, "DetectFile");
    let regs = numbered_with_bare(entries, "Detect");
    let special: Vec<&str> = entries
        .iter()
        .filter(|(k, _)| k.eq_ignore_ascii_case("SpecialDetect"))
        .map(|(_, v)| v.as_str())
        .collect();

    if files.is_empty() && regs.is_empty() && special.is_empty() {
        return true;
    }
    files.iter().any(|f| detect_file(f))
        || regs.iter().any(|r| detect_reg(r))
        || special.iter().any(|s| special_detect(s))
}

/// Values for `key` and `key1`, `key2`, ... together (winapp2 allows both the
/// bare directive and numbered variants).
fn numbered_with_bare(entries: &[(String, String)], key: &str) -> Vec<String> {
    let mut v = Vec::new();
    if let Some(bare) = get(entries, key) {
        v.push(bare.to_string());
    }
    v.extend(numbered(entries, key));
    v
}

/// A DetectFile passes when the (expanded) path exists. A trailing wildcard is
/// treated as "the directory exists".
fn detect_file(raw: &str) -> bool {
    let cleaned = raw.trim_end_matches(['*', '\\']);
    match path::expand_only(cleaned) {
        Some(p) => p.exists(),
        None => false,
    }
}

/// A Detect passes when the registry key exists. Only HKCU / HKLM are supported.
fn detect_reg(raw: &str) -> bool {
    let (hive, sub) = split_hive(raw);
    match hive {
        Some(h) => registry::key_exists(h, sub),
        None => false,
    }
}

fn split_hive(raw: &str) -> (Option<Hive>, &str) {
    let raw = raw.trim();
    for (prefix, hive) in [
        ("HKCU\\", Hive::Hkcu),
        ("HKEY_CURRENT_USER\\", Hive::Hkcu),
        ("HKLM\\", Hive::Hklm),
        ("HKEY_LOCAL_MACHINE\\", Hive::Hklm),
    ] {
        if raw.len() >= prefix.len() && raw[..prefix.len()].eq_ignore_ascii_case(prefix) {
            return (Some(hive), &raw[prefix.len()..]);
        }
    }
    (None, raw)
}

/// The common winapp2 SpecialDetect constants, mapped to a known-path check.
/// Unrecognized constants are treated as present (do not over-filter).
fn special_detect(name: &str) -> bool {
    let exists = |p: &str| path::expand_only(p).map(|x| x.exists()).unwrap_or(false);
    match name.to_ascii_uppercase().as_str() {
        "DET_CHROME" => exists("%LOCALAPPDATA%\\Google\\Chrome\\User Data"),
        "DET_FIREFOX" | "DET_MOZILLA" => exists("%APPDATA%\\Mozilla\\Firefox"),
        "DET_OPERA" => exists("%APPDATA%\\Opera Software"),
        "DET_THUNDERBIRD" => exists("%APPDATA%\\Thunderbird"),
        "DET_IE" => true, // Internet Explorer components ship with Windows.
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn actions_of(c: &Cleaner) -> &[CleanAction] {
        &c.options[0].actions
    }

    #[test]
    fn parses_filekeys_masks_and_flags() {
        let ini = "\
[Sample App *]
DetectFile=%WinDir%
Default=True
FileKey1=%LocalAppData%\\Sample\\Cache|*.*|RECURSE
FileKey2=%AppData%\\Sample|*.log;*.tmp
";
        let cs = parse_str(ini);
        assert_eq!(cs.len(), 1);
        let c = &cs[0];
        assert_eq!(c.name, "Sample App *");
        assert_eq!(c.source, Source::Winapp2);
        // FileKey1 -> 1 action (recurse); FileKey2 -> 2 mask actions.
        let a = actions_of(c);
        assert_eq!(a.len(), 3);
        match &a[0] {
            CleanAction::Files {
                root,
                mask,
                recurse,
                ..
            } => {
                assert!(root.ends_with("Sample\\Cache"));
                assert_eq!(mask, "*");
                assert!(recurse);
            }
            _ => panic!("expected Files"),
        }
    }

    #[test]
    fn default_false_and_warning_are_carried() {
        let ini = "\
[Risky]
Default=False
Warning=This removes saved data.
FileKey1=%AppData%\\Risky|*.*
";
        let c = &parse_str(ini)[0];
        assert!(!c.options[0].default_on);
        assert_eq!(
            c.options[0].warning.as_deref(),
            Some("This removes saved data.")
        );
    }

    #[test]
    fn regkey_only_entries_are_dropped() {
        // Reg deletes disabled: an entry with no FileKey produces nothing.
        let ini = "\
[Reg Only]
RegKey1=HKCU\\Software\\Foo
";
        assert!(parse_str(ini).is_empty());
    }

    #[test]
    fn regkeys_never_become_actions() {
        let ini = "\
[Mixed]
FileKey1=%Temp%\\Mixed|*.*
RegKey1=HKCU\\Software\\Foo\\Bar
";
        let c = &parse_str(ini)[0];
        // Only the FileKey survived; the RegKey contributed no action.
        assert_eq!(actions_of(c).len(), 1);
        assert!(matches!(actions_of(c)[0], CleanAction::Files { .. }));
    }

    #[test]
    fn detection_drops_absent_software() {
        // A DetectFile that cannot exist filters the entry out entirely.
        let ini = "\
[Ghost]
DetectFile=%LocalAppData%\\__definitely_not_installed_neonprime__
FileKey1=%LocalAppData%\\Ghost|*.*
";
        assert!(parse_str(ini).is_empty());
    }

    #[test]
    fn no_detect_directive_always_applies() {
        let ini = "\
[Generic]
FileKey1=%Temp%\\Generic|*.*
";
        assert_eq!(parse_str(ini).len(), 1);
    }

    #[test]
    fn detectfile_present_includes_entry() {
        // %WinDir% always exists, so this entry is kept.
        let ini = "\
[Present]
DetectFile=%WinDir%
FileKey1=%Temp%\\Present|*.*
";
        assert_eq!(parse_str(ini).len(), 1);
    }

    #[test]
    fn hive_split_recognizes_prefixes() {
        assert!(matches!(
            split_hive("HKCU\\Software\\X"),
            (Some(Hive::Hkcu), "Software\\X")
        ));
        assert!(matches!(
            split_hive("HKEY_LOCAL_MACHINE\\SOFTWARE\\Y"),
            (Some(Hive::Hklm), "SOFTWARE\\Y")
        ));
        assert!(matches!(split_hive("HKCR\\Whatever"), (None, _)));
    }

    #[test]
    fn adversarial_utf8_key_does_not_panic() {
        // "FileKe" (6 bytes) + U+1F600 (4 bytes): byte index 7 = len("FileKey")
        // lands inside the emoji, which panicked the old str-slice matcher.
        let ini = "[Evil]\nFileKe\u{1F600}=x\nFileKey1=%TEMP%\\a|*|RECURSE\n";
        let cleaners = parse_str(ini); // must not panic
        // The valid FileKey1 still parses; the malformed key is ignored.
        assert_eq!(cleaners.len(), 1);
    }
}
