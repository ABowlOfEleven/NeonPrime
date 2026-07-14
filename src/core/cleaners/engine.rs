//! Runs cleaner actions: [`preview`] measures what would be freed without
//! deleting, [`execute`] deletes in-process, and [`elevated_script`] emits a
//! PowerShell script for options that need admin (the app itself runs
//! unelevated). Every path is expanded and sandboxed via [`super::path`] first,
//! so an out-of-sandbox root simply contributes nothing rather than escaping.

use std::path::Path;

use super::{path, recycle, CleanAction, Cleaner};

#[derive(Default, Clone, Copy)]
pub struct Preview {
    pub bytes: u64,
    pub files: u64,
}

#[derive(Default, Clone, Copy)]
pub struct CleanResult {
    pub freed: u64,
    pub files: u64,
    /// Entries the delete could not remove (locked, in use).
    pub errors: u64,
}

/// True when option `i` is selected. Missing indices default to unselected so a
/// short slice is safe.
fn is_selected(selected: &[bool], i: usize) -> bool {
    selected.get(i).copied().unwrap_or(false)
}

/// Sum the size of everything the selected options would remove. Read-only.
pub fn preview(cleaner: &Cleaner, selected: &[bool]) -> Preview {
    let mut pv = Preview::default();
    for (i, opt) in cleaner.options.iter().enumerate() {
        if !is_selected(selected, i) {
            continue;
        }
        for act in &opt.actions {
            match act {
                CleanAction::RecycleBin => {
                    pv.bytes = pv.bytes.saturating_add(recycle::size());
                }
                CleanAction::EmptyDir { root } => measure(root, "*", true, &mut pv),
                CleanAction::Files {
                    root,
                    mask,
                    recurse,
                    ..
                } => measure(root, mask, *recurse, &mut pv),
            }
        }
    }
    pv
}

/// Delete what the selected, unelevated options name. Elevated options are
/// skipped here (see [`elevated_script`]).
pub fn execute(cleaner: &Cleaner, selected: &[bool], secure: bool) -> CleanResult {
    // `secure` (overwrite-then-delete) is reserved for the secure-deletion
    // feature; wired through now so the signature is stable.
    let _ = secure;
    let mut r = CleanResult::default();
    for (i, opt) in cleaner.options.iter().enumerate() {
        if !is_selected(selected, i) || opt.elevated {
            continue;
        }
        for act in &opt.actions {
            match act {
                CleanAction::RecycleBin => {
                    let before = recycle::size();
                    recycle::empty();
                    r.freed = r.freed.saturating_add(before);
                }
                CleanAction::EmptyDir { root } => empty_dir(root, &mut r),
                CleanAction::Files {
                    root,
                    mask,
                    recurse,
                    remove_self,
                } => delete_files(root, mask, *recurse, *remove_self, &mut r),
            }
        }
    }
    r
}

/// PowerShell for the selected elevated options, or None if none are selected.
/// Run by the caller through the elevated shell.
pub fn elevated_script(cleaner: &Cleaner, selected: &[bool]) -> Option<String> {
    let mut parts: Vec<String> = Vec::new();
    for (i, opt) in cleaner.options.iter().enumerate() {
        if !is_selected(selected, i) || !opt.elevated {
            continue;
        }
        for act in &opt.actions {
            match act {
                CleanAction::EmptyDir { root } => {
                    if let Ok(d) = path::expand_and_validate(root) {
                        parts.push(format!(
                            "Remove-Item \"{}\\*\" -Recurse -Force -ErrorAction SilentlyContinue",
                            d.display()
                        ));
                    }
                }
                CleanAction::Files {
                    root,
                    mask,
                    recurse,
                    ..
                } => {
                    if let Ok(d) = path::expand_and_validate(root) {
                        let rec = if *recurse { " -Recurse" } else { "" };
                        parts.push(format!(
                            "Remove-Item \"{}\\{}\"{} -Force -ErrorAction SilentlyContinue",
                            d.display(),
                            mask,
                            rec
                        ));
                    }
                }
                // The Recycle Bin is never an elevated action.
                CleanAction::RecycleBin => {}
            }
        }
    }
    if parts.is_empty() {
        return None;
    }
    parts.push("Write-Host 'System caches cleared.'".into());
    Some(parts.join("; "))
}

/// Measure files under a sandboxed root matching `mask`.
fn measure(root_raw: &str, mask: &str, recurse: bool, pv: &mut Preview) {
    let Ok(dir) = path::expand_and_validate(root_raw) else {
        return;
    };
    walk(&dir, mask, recurse, &mut |_p, len| {
        pv.bytes = pv.bytes.saturating_add(len);
        pv.files += 1;
    });
}

/// Delete every entry inside a sandboxed root (files and subdirectories),
/// keeping the directory itself.
fn empty_dir(root_raw: &str, r: &mut CleanResult) {
    let Ok(dir) = path::expand_and_validate(root_raw) else {
        r.errors += 1;
        return;
    };
    let Ok(rd) = std::fs::read_dir(&dir) else {
        return;
    };
    for entry in rd.flatten() {
        let p = entry.path();
        let Ok(ft) = entry.file_type() else { continue };
        if ft.is_symlink() {
            continue;
        }
        let sz = if ft.is_dir() {
            dir_size(&p)
        } else {
            entry.metadata().map(|m| m.len()).unwrap_or(0)
        };
        let removed = if ft.is_dir() {
            std::fs::remove_dir_all(&p)
        } else {
            std::fs::remove_file(&p)
        };
        match removed {
            Ok(()) => {
                r.freed = r.freed.saturating_add(sz);
                r.files += 1;
            }
            Err(_) => r.errors += 1,
        }
    }
}

/// Delete files under a sandboxed root matching `mask`; optionally remove the
/// root afterwards.
fn delete_files(root_raw: &str, mask: &str, recurse: bool, remove_self: bool, r: &mut CleanResult) {
    let Ok(dir) = path::expand_and_validate(root_raw) else {
        r.errors += 1;
        return;
    };
    let mut victims: Vec<(std::path::PathBuf, u64)> = Vec::new();
    walk(&dir, mask, recurse, &mut |p, len| {
        victims.push((p.to_path_buf(), len))
    });
    for (p, len) in victims {
        match std::fs::remove_file(&p) {
            Ok(()) => {
                r.freed = r.freed.saturating_add(len);
                r.files += 1;
            }
            Err(_) => r.errors += 1,
        }
    }
    if remove_self {
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// Visit every non-symlink file under `dir` whose name matches `mask`.
fn walk(dir: &Path, mask: &str, recurse: bool, f: &mut dyn FnMut(&Path, u64)) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in rd.flatten() {
        let Ok(ft) = entry.file_type() else { continue };
        if ft.is_symlink() {
            continue;
        }
        let p = entry.path();
        if ft.is_dir() {
            if recurse {
                walk(&p, mask, recurse, f);
            }
        } else if matches_mask(&entry.file_name().to_string_lossy(), mask) {
            let len = entry.metadata().map(|m| m.len()).unwrap_or(0);
            f(&p, len);
        }
    }
}

/// Recursively sum file sizes under `dir` (used to report freed bytes for a
/// removed subdirectory).
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

/// Case-insensitive filename match for the small glob subset cleaner masks use:
/// `*` or `*.*` (all), `*.ext` (suffix), `prefix*` (prefix), or a literal name.
fn matches_mask(name: &str, mask: &str) -> bool {
    if mask == "*" || mask == "*.*" || mask.is_empty() {
        return true;
    }
    let name = name.to_ascii_lowercase();
    let mask = mask.to_ascii_lowercase();
    match (mask.starts_with('*'), mask.ends_with('*')) {
        (true, true) => {
            let mid = &mask[1..mask.len() - 1];
            mid.is_empty() || name.contains(mid)
        }
        (true, false) => name.ends_with(&mask[1..]),
        (false, true) => name.starts_with(&mask[..mask.len() - 1]),
        (false, false) => name == mask,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::cleaners::{CleanAction, Cleaner, CleanerOption, Group, Source};
    use std::fs;

    #[test]
    fn mask_matching() {
        assert!(matches_mask("anything.tmp", "*"));
        assert!(matches_mask("app.log", "*.log"));
        assert!(!matches_mask("app.txt", "*.log"));
        assert!(matches_mask("cache123", "cache*"));
        assert!(matches_mask("thumbcache_idx.db", "*cache*"));
        assert!(matches_mask("exact.dat", "exact.dat"));
        assert!(!matches_mask("other.dat", "exact.dat"));
    }

    fn temp_cleaner(root: &str) -> Cleaner {
        Cleaner {
            id: "t".into(),
            name: "t".into(),
            group: Group::System,
            source: Source::Builtin,
            running_procs: Vec::new(),
            options: vec![CleanerOption {
                id: "t".into(),
                label: "t".into(),
                desc: String::new(),
                default_on: true,
                warning: None,
                elevated: false,
                guard_running: false,
                actions: vec![CleanAction::EmptyDir { root: root.into() }],
            }],
        }
    }

    #[test]
    fn preview_then_execute_frees_a_temp_subtree() {
        // Work inside %TEMP%, which is inside the sandbox.
        let base = std::env::temp_dir().join(format!("np_cleaner_test_{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(base.join("sub")).unwrap();
        fs::write(base.join("a.bin"), vec![0u8; 2048]).unwrap();
        fs::write(base.join("sub/b.bin"), vec![0u8; 1024]).unwrap();

        let root = base.to_string_lossy().to_string();
        let cleaner = temp_cleaner(&root);

        let pv = preview(&cleaner, &[true]);
        assert_eq!(pv.bytes, 3072, "preview should sum both files");
        assert_eq!(pv.files, 2);

        let r = execute(&cleaner, &[true], false);
        assert_eq!(r.freed, 3072);
        assert!(base.read_dir().unwrap().next().is_none(), "root emptied");

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn out_of_sandbox_root_is_a_noop() {
        let cleaner = temp_cleaner("%SystemRoot%\\System32");
        // Nothing measured, nothing deleted, one error recorded on execute.
        assert_eq!(preview(&cleaner, &[true]).bytes, 0);
        assert_eq!(execute(&cleaner, &[true], false).errors, 1);
    }

    #[test]
    fn elevated_option_is_scripted_not_executed() {
        let mut c = temp_cleaner("%SystemRoot%\\Temp");
        c.options[0].elevated = true;
        // execute skips elevated options entirely.
        let r = execute(&c, &[true], false);
        assert_eq!(r.files, 0);
        // and a script is emitted instead.
        let s = elevated_script(&c, &[true]).expect("script");
        assert!(s.contains("Remove-Item"));
        assert!(s.contains("Temp"));
    }
}
