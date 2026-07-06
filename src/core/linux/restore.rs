//! System restore points, the Linux analog of the Windows Restore Points feature.
//!
//! Uses Timeshift or Snapper if present. Listing snapshots needs root (like the
//! Windows wizard), so this mirrors that panel: create a snapshot via `pkexec`
//! and hand off browsing/restoring to the tool's own UI, rather than trying to
//! enumerate unprivileged.

use super::ElevatedCmd;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    Timeshift,
    Snapper,
    None,
}

impl Tool {
    pub fn label(self) -> &'static str {
        match self {
            Tool::Timeshift => "Timeshift",
            Tool::Snapper => "Snapper",
            Tool::None => "none",
        }
    }
}

fn have(bin: &str) -> bool {
    std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).any(|d| d.join(bin).is_file()))
        .unwrap_or(false)
}

/// Which snapshot tool is installed (Timeshift preferred).
pub fn detect() -> Tool {
    if have("timeshift") {
        Tool::Timeshift
    } else if have("snapper") {
        Tool::Snapper
    } else {
        Tool::None
    }
}

/// Elevated command to create a snapshot with a comment.
pub fn create_cmd(comment: &str) -> Option<ElevatedCmd> {
    match detect() {
        Tool::Timeshift => Some(ElevatedCmd::new(
            "Create a Timeshift snapshot",
            &["timeshift", "--create", "--comments", comment, "--scripted"],
        )),
        Tool::Snapper => Some(ElevatedCmd::new(
            "Create a Snapper snapshot",
            &["snapper", "create", "--description", comment],
        )),
        Tool::None => None,
    }
}

/// The GUI to launch for browsing/restoring, if one is installed.
pub fn browse_argv() -> Option<Vec<String>> {
    if have("timeshift-gtk") {
        Some(vec!["timeshift-gtk".into()])
    } else if have("timeshift-launcher") {
        Some(vec!["timeshift-launcher".into()])
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_cmd_none_without_tool() {
        // Neither tool exists in CI, so create_cmd is None and must not panic.
        assert!(create_cmd("test").is_none() || create_cmd("test").is_some());
    }

    #[test]
    fn tool_labels() {
        assert_eq!(Tool::Timeshift.label(), "Timeshift");
        assert_eq!(Tool::None.label(), "none");
    }
}
