//! Cleaner engine: an owned, source-agnostic model for disk cleaners.
//!
//! A [`Cleaner`] is a named group of [`CleanerOption`]s; each option compiles to
//! a list of [`CleanAction`]s the engine can preview (measure, no delete) or
//! execute (delete). The point of the abstraction is that every source produces
//! the same model: the built-in system cleaners here, the browser cleaners
//! (Phase 2), and imported winapp2 definitions (Phase 3) all run through one
//! sandboxed code path in [`engine`], so a definition can never point the deleter
//! at a path outside the allowlist in [`path`].
//!
//! Phase 1 ships the engine plus the built-in system cleaners only. Each system
//! cleaner has a single option, so the Cleanup panel stays the flat one-row-per
//! -target list it has always been; the hierarchical option UI arrives with the
//! multi-option browser cleaners.

mod builtin;
mod engine;
mod path;
mod recycle;

pub use builtin::system_cleaners;
pub use engine::{elevated_script, execute, preview, CleanResult, Preview};
pub use path::{expand_and_validate, RejectReason};

/// Where a cleaner belongs in the panel's grouping.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Group {
    System,
    Browsers,
    Applications,
    Windows,
    Imported,
}

/// Where a cleaner definition came from. Imported definitions are untrusted and
/// treated more conservatively (file cleaning only, mandatory preview).
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Builtin,
    Winapp2,
}

/// A named cleaner: one or more selectable options that share a heading.
pub struct Cleaner {
    /// Stable slug, e.g. "temp", "firefox", "winapp2:Firefox-Cache".
    pub id: String,
    pub name: String,
    pub group: Group,
    pub source: Source,
    pub options: Vec<CleanerOption>,
}

/// One checkable line under a cleaner (e.g. "Cache", "Cookies").
pub struct CleanerOption {
    pub id: String,
    pub label: String,
    pub desc: String,
    /// Whether this option is ticked by default. Safe caches are on; anything
    /// that destroys user state (cookies, history) is off and carries a warning.
    pub default_on: bool,
    /// Shown in the panel when the option would erase user state.
    pub warning: Option<String>,
    /// Cleaning needs admin; the engine emits a script for the elevated shell
    /// rather than deleting in-process.
    pub elevated: bool,
    pub actions: Vec<CleanAction>,
}

/// A single unit of work. `root` is a raw string that may contain `%ENV%`
/// variables; the engine expands and sandboxes it via [`path`] before any
/// filesystem access, so both built-in and imported roots are validated the same
/// way.
pub enum CleanAction {
    /// Delete files under `root` matching `mask` (`*` = all). `recurse` walks
    /// subdirectories; `remove_self` deletes the (now empty) root afterwards.
    Files {
        root: String,
        mask: String,
        recurse: bool,
        remove_self: bool,
    },
    /// Delete every entry inside `root` (files and subdirectories), keeping the
    /// directory itself.
    EmptyDir { root: String },
    /// Empty the shell Recycle Bin across all drives.
    RecycleBin,
}

/// Human-readable byte size (e.g. "1.4 GB"). Shared byte formatter used by the
/// Cleanup panel and elsewhere.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn human_scales() {
        assert_eq!(human(512), "512 B");
        assert!(human(5 * 1024 * 1024).ends_with("MB"));
        assert!(human(3 * 1024 * 1024 * 1024).ends_with("GB"));
    }
}
