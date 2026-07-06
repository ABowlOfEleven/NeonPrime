//! Disk cleanup + analyzer, the Linux analog of the Windows Cleanup panel.
//!
//! User-scoped targets (cache, trash) are sized by walking directories and
//! cleaned in-process. System targets (package cache, journald logs) are sized
//! best-effort and cleaned via a [`ElevatedCmd`] the UI runs through `pkexec`.

use std::path::{Path, PathBuf};

use super::{pkg, ElevatedCmd};

pub struct Target {
    pub id: &'static str,
    pub name: &'static str,
    pub desc: &'static str,
    /// Cleaning requires root (run through pkexec).
    pub elevated: bool,
}

pub fn catalog() -> &'static [Target] {
    &[
        Target {
            id: "user-cache",
            name: "Application cache",
            desc: "Everything under ~/.cache.",
            elevated: false,
        },
        Target {
            id: "trash",
            name: "Trash",
            desc: "Files in the desktop Trash.",
            elevated: false,
        },
        Target {
            id: "pkg-cache",
            name: "Package cache",
            desc: "Downloaded package archives (apt/dnf/pacman/zypper).",
            elevated: true,
        },
        Target {
            id: "journal",
            name: "System journal",
            desc: "systemd journald logs under /var/log/journal.",
            elevated: true,
        },
    ]
}

fn home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/root"))
}

/// Directories backing a target id.
fn paths(id: &str) -> Vec<PathBuf> {
    match id {
        "user-cache" => vec![home().join(".cache")],
        "trash" => {
            let base = home().join(".local/share/Trash");
            vec![base.join("files"), base.join("info")]
        }
        "pkg-cache" => vec![
            PathBuf::from("/var/cache/apt/archives"),
            PathBuf::from("/var/cache/dnf"),
            PathBuf::from("/var/cache/pacman/pkg"),
            PathBuf::from("/var/cache/zypp/packages"),
        ],
        "journal" => vec![PathBuf::from("/var/log/journal")],
        _ => Vec::new(),
    }
}

/// Recursively sum file sizes under `dir`, ignoring entries we can't read.
fn dir_size(dir: &Path) -> u64 {
    let mut total = 0u64;
    let Ok(rd) = std::fs::read_dir(dir) else {
        return 0;
    };
    for entry in rd.flatten() {
        let Ok(ft) = entry.file_type() else { continue };
        if ft.is_symlink() {
            continue;
        }
        if ft.is_dir() {
            total = total.saturating_add(dir_size(&entry.path()));
        } else if let Ok(md) = entry.metadata() {
            total = total.saturating_add(md.len());
        }
    }
    total
}

/// Reclaimable bytes for a target (best-effort; system paths may be root-only).
pub fn size_of(id: &str) -> u64 {
    paths(id).iter().map(|p| dir_size(p)).sum()
}

/// Clean an unelevated target in-process. Best-effort: locked files are skipped.
pub fn clean(id: &str) -> std::io::Result<()> {
    for dir in paths(id) {
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in rd.flatten() {
            let p = entry.path();
            let _ = if p.is_dir() {
                std::fs::remove_dir_all(&p)
            } else {
                std::fs::remove_file(&p)
            };
        }
    }
    Ok(())
}

/// Elevated command to clear a system target, or None if not applicable.
pub fn clean_cmd(id: &str) -> Option<ElevatedCmd> {
    match id {
        "pkg-cache" => match pkg::primary()? {
            pkg::Manager::Apt => Some(ElevatedCmd::new(
                "Clear apt package cache",
                &["apt-get", "clean"],
            )),
            pkg::Manager::Dnf => Some(ElevatedCmd::new(
                "Clear dnf cache",
                &["dnf", "clean", "all"],
            )),
            pkg::Manager::Pacman => Some(ElevatedCmd::new(
                "Clear pacman cache",
                &["pacman", "-Scc", "--noconfirm"],
            )),
            pkg::Manager::Zypper => {
                Some(ElevatedCmd::new("Clear zypper cache", &["zypper", "clean"]))
            }
            pkg::Manager::Flatpak => None,
        },
        "journal" => Some(ElevatedCmd::new(
            "Vacuum journald logs older than 3 days",
            &["journalctl", "--vacuum-time=3d"],
        )),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_has_user_and_elevated_targets() {
        assert!(catalog().iter().any(|t| !t.elevated));
        assert!(catalog().iter().any(|t| t.elevated));
    }

    #[test]
    fn journal_clean_is_a_vacuum_command() {
        let c = clean_cmd("journal").unwrap();
        assert_eq!(c.argv, vec!["journalctl", "--vacuum-time=3d"]);
    }

    #[test]
    fn user_cache_size_does_not_panic() {
        let _ = size_of("user-cache");
    }
}
