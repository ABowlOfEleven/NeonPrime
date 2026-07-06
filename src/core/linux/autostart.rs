//! Autostart manager, the Linux analog of the Windows Startup panel. Lists XDG
//! autostart entries (user `~/.config/autostart` merged over system
//! `/etc/xdg/autostart`) and toggles them via the freedesktop `Hidden` key.
//! No elevation: disabling a system entry writes a user-level override.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

pub struct Entry {
    pub name: String,
    /// Absolute path of the effective .desktop file.
    pub file: String,
    pub enabled: bool,
    /// True when this comes only from the system dir (no user copy yet).
    pub system: bool,
}

fn user_dir() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::var_os("HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("/root"))
                .join(".config")
        })
        .join("autostart")
}

/// Parse a .desktop file's display name and whether it is enabled.
/// Enabled means neither `Hidden=true` nor `X-GNOME-Autostart-enabled=false`.
fn parse(path: &Path) -> (String, bool) {
    let text = fs::read_to_string(path).unwrap_or_default();
    let mut in_section = false;
    let mut name = String::new();
    let mut hidden = false;
    let mut gnome_disabled = false;
    for line in text.lines() {
        let l = line.trim();
        if l.starts_with('[') {
            in_section = l.eq_ignore_ascii_case("[desktop entry]");
            continue;
        }
        if !in_section {
            continue;
        }
        if let Some(v) = l.strip_prefix("Name=") {
            if name.is_empty() {
                name = v.trim().to_string();
            }
        } else if let Some(v) = l.strip_prefix("Hidden=") {
            hidden = v.trim().eq_ignore_ascii_case("true");
        } else if let Some(v) = l.strip_prefix("X-GNOME-Autostart-enabled=") {
            gnome_disabled = v.trim().eq_ignore_ascii_case("false");
        }
    }
    if name.is_empty() {
        name = path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
    }
    (name, !hidden && !gnome_disabled)
}

/// All autostart entries, user overriding system, sorted by name.
pub fn entries() -> Vec<Entry> {
    let mut map: BTreeMap<String, Entry> = BTreeMap::new();
    let dirs = [
        (PathBuf::from("/etc/xdg/autostart"), true),
        (user_dir(), false),
    ];
    for (dir, system) in dirs {
        let Ok(rd) = fs::read_dir(&dir) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            if p.extension().and_then(|s| s.to_str()) != Some("desktop") {
                continue;
            }
            let base = p.file_name().map(|s| s.to_string_lossy().to_string());
            let Some(base) = base else { continue };
            let (name, enabled) = parse(&p);
            map.insert(
                base,
                Entry {
                    name,
                    file: p.to_string_lossy().to_string(),
                    enabled,
                    system,
                },
            );
        }
    }
    let mut v: Vec<Entry> = map.into_values().collect();
    v.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    v
}

/// Enable or disable an entry by writing a user-level .desktop with the right
/// `Hidden` flag. A system entry is copied into the user dir first.
pub fn set_enabled(file: &str, enabled: bool) -> std::io::Result<()> {
    let src = Path::new(file);
    let base = src
        .file_name()
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "bad path"))?;
    let dir = user_dir();
    fs::create_dir_all(&dir)?;
    let dst = dir.join(base);

    // Start from the user file if present, else the (system) source.
    let content = fs::read_to_string(&dst)
        .or_else(|_| fs::read_to_string(src))
        .unwrap_or_default();

    if content.is_empty() {
        let hidden = if enabled { "" } else { "Hidden=true\n" };
        return fs::write(&dst, format!("[Desktop Entry]\nType=Application\n{hidden}"));
    }

    // Drop existing Hidden lines; add Hidden=true after the header when disabling.
    let mut out = String::new();
    let mut handled = false;
    for line in content.lines() {
        if line.trim_start().starts_with("Hidden=") {
            continue;
        }
        out.push_str(line);
        out.push('\n');
        if !handled && line.trim().eq_ignore_ascii_case("[desktop entry]") {
            if !enabled {
                out.push_str("Hidden=true\n");
            }
            handled = true;
        }
    }
    fs::write(&dst, out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_reads_name_and_hidden() {
        let mut p = std::env::temp_dir();
        p.push("neonprime-autostart-test.desktop");
        fs::write(
            &p,
            "[Desktop Entry]\nType=Application\nName=Test App\nHidden=true\n",
        )
        .unwrap();
        let (name, enabled) = parse(&p);
        assert_eq!(name, "Test App");
        assert!(!enabled);
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn parse_defaults_to_enabled() {
        let mut p = std::env::temp_dir();
        p.push("neonprime-autostart-test2.desktop");
        fs::write(&p, "[Desktop Entry]\nName=Plain\n").unwrap();
        let (_, enabled) = parse(&p);
        assert!(enabled);
        let _ = fs::remove_file(&p);
    }
}
