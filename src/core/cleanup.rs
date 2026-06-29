//! Disk cleanup + analyzer. Sizes are computed by walking directories (or the
//! shell Recycle Bin API); reclaim deletes contents. User-scoped targets clean
//! unelevated; system caches need the elevated shell.

use std::path::PathBuf;

pub struct Target {
    pub id: &'static str,
    pub name: &'static str,
    pub desc: &'static str,
    /// Cleaning requires admin (cleared via an elevated shell).
    pub elevated: bool,
}

pub fn catalog() -> &'static [Target] {
    &[
        Target { id: "temp", name: "Temporary files", desc: "Your user %TEMP% folder.", elevated: false },
        Target { id: "recycle", name: "Recycle Bin", desc: "Deleted files across all drives.", elevated: false },
        Target { id: "thumbs", name: "Thumbnail cache", desc: "Explorer thumbnail & icon caches.", elevated: false },
        Target { id: "syscache", name: "System & update cache", desc: "C:\\Windows\\Temp and the Windows Update download cache.", elevated: true },
    ]
}

/// Directories backing a target id (empty for the Recycle Bin special-case).
fn paths(id: &str) -> Vec<PathBuf> {
    let win = std::env::var_os("SystemRoot").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("C:\\Windows"));
    match id {
        "temp" => std::env::var_os("TEMP").map(PathBuf::from).into_iter().collect(),
        "thumbs" => std::env::var_os("LOCALAPPDATA")
            .map(|p| PathBuf::from(p).join("Microsoft\\Windows\\Explorer"))
            .into_iter()
            .collect(),
        "syscache" => vec![win.join("Temp"), win.join("SoftwareDistribution\\Download")],
        _ => Vec::new(),
    }
}

/// Recursively sum file sizes under `dir`, ignoring entries we can't read.
fn dir_size(dir: &std::path::Path) -> u64 {
    let mut total = 0u64;
    let Ok(rd) = std::fs::read_dir(dir) else { return 0 };
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

/// Reclaimable bytes for a target.
pub fn size_of(id: &str) -> u64 {
    if id == "recycle" {
        return recycle_bin_size();
    }
    paths(id).iter().map(|p| dir_size(p)).sum()
}

/// Clean an unelevated target. Best-effort: locked files are skipped.
pub fn clean(id: &str) -> std::io::Result<()> {
    if id == "recycle" {
        empty_recycle_bin();
        return Ok(());
    }
    for dir in paths(id) {
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
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

/// Elevated PowerShell to clear an admin target's directories.
pub fn clean_script(id: &str) -> Option<String> {
    let dirs = paths(id);
    if dirs.is_empty() {
        return None;
    }
    let mut parts: Vec<String> = dirs
        .iter()
        .map(|d| format!("Remove-Item \"{}\\*\" -Recurse -Force -ErrorAction SilentlyContinue", d.display()))
        .collect();
    parts.push("Write-Host 'System caches cleared.'".into());
    Some(parts.join("; "))
}

/// Human-readable size (e.g. "1.4 GB").
pub fn human(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    let b = bytes as f64;
    if b < KB {
        format!("{bytes} B")
    } else if b < KB * KB {
        format!("{:.0} KB", b / KB)
    } else if b < KB * KB * KB {
        format!("{:.1} MB", b / (KB * KB))
    } else {
        format!("{:.2} GB", b / (KB * KB * KB))
    }
}

fn recycle_bin_size() -> u64 {
    use windows::Win32::UI::Shell::{SHQueryRecycleBinW, SHQUERYRBINFO};
    let mut info = SHQUERYRBINFO { cbSize: std::mem::size_of::<SHQUERYRBINFO>() as u32, i64Size: 0, i64NumItems: 0 };
    let ok = unsafe { SHQueryRecycleBinW(windows::core::PCWSTR::null(), &mut info) };
    if ok.is_ok() && info.i64Size > 0 {
        info.i64Size as u64
    } else {
        0
    }
}

fn empty_recycle_bin() {
    use windows::Win32::UI::Shell::{SHEmptyRecycleBinW, SHERB_NOCONFIRMATION, SHERB_NOPROGRESSUI, SHERB_NOSOUND};
    unsafe {
        let _ = SHEmptyRecycleBinW(
            None,
            windows::core::PCWSTR::null(),
            SHERB_NOCONFIRMATION | SHERB_NOPROGRESSUI | SHERB_NOSOUND,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn human_scales() {
        assert_eq!(human(512), "512 B");
        assert!(human(5 * 1024 * 1024).ends_with("MB"));
        assert!(human(3 * 1024 * 1024 * 1024).ends_with("GB"));
    }

    #[test]
    fn catalog_has_targets_and_one_elevated() {
        assert!(catalog().len() >= 3);
        assert!(catalog().iter().any(|t| t.elevated));
        assert!(clean_script("syscache").is_some());
        assert!(clean_script("recycle").is_none());
    }

    #[test]
    fn size_of_temp_does_not_panic() {
        let _ = size_of("temp");
    }
}
