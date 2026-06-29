//! Quick Actions — one-click system maintenance. Each action is a process
//! invocation (optionally elevated via UAC). Unlike tweaks these are not part of
//! the reversible model — they're fire-and-forget maintenance.

pub struct QuickAction {
    pub id: &'static str,
    pub name: &'static str,
    pub desc: &'static str,
    /// Destructive-ish (deletes data) — surfaced with a warning accent.
    pub danger: bool,
    /// Needs administrator rights (launched via UAC).
    pub elevated: bool,
}

/// How to run an action: program + args, and whether to elevate.
pub struct Invocation {
    pub program: String,
    pub args: Vec<String>,
    pub elevated: bool,
}

pub fn catalog() -> Vec<QuickAction> {
    vec![
        QuickAction {
            id: "restart-explorer",
            name: "Restart Explorer",
            desc: "Restart the Windows shell — applies taskbar / context-menu tweaks.",
            danger: false,
            elevated: false,
        },
        QuickAction {
            id: "flush-dns",
            name: "Flush DNS cache",
            desc: "Clear the resolver cache to fix stale or broken name lookups.",
            danger: false,
            elevated: false,
        },
        QuickAction {
            id: "clear-temp",
            name: "Clear temp files",
            desc: "Delete the contents of your %TEMP% folder.",
            danger: true,
            elevated: false,
        },
        QuickAction {
            id: "empty-recycle-bin",
            name: "Empty Recycle Bin",
            desc: "Permanently remove everything in the Recycle Bin.",
            danger: true,
            elevated: false,
        },
        QuickAction {
            id: "create-restore-point",
            name: "Create restore point",
            desc: "Snapshot system state so you can roll back later (needs admin).",
            danger: false,
            elevated: true,
        },
        QuickAction {
            id: "install-ps-profile",
            name: "Install PowerShell profile",
            desc: "Set up the NeonPrime shell: Oh My Posh prompt, smart cd, icons, and handy functions (incl. live `temps`).",
            danger: false,
            elevated: false,
        },
    ]
}

pub fn invocation(id: &str) -> Option<Invocation> {
    let inv = |program: &str, args: &[&str], elevated: bool| Invocation {
        program: program.into(),
        args: args.iter().map(|s| s.to_string()).collect(),
        elevated,
    };
    Some(match id {
        "restart-explorer" => inv(
            "cmd",
            &["/c", "taskkill /f /im explorer.exe & start explorer.exe"],
            false,
        ),
        "flush-dns" => inv("ipconfig", &["/flushdns"], false),
        "clear-temp" => inv("cmd", &["/c", "del /q /f /s \"%TEMP%\\*\""], false),
        "empty-recycle-bin" => inv(
            "powershell",
            &[
                "-NoProfile",
                "-Command",
                "Clear-RecycleBin -Force -ErrorAction SilentlyContinue",
            ],
            false,
        ),
        "create-restore-point" => inv(
            "powershell",
            &[
                "-NoProfile",
                "-Command",
                "Checkpoint-Computer -Description 'NeonPrime' -RestorePointType 'MODIFY_SETTINGS'",
            ],
            true,
        ),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_runnable_action_has_an_invocation() {
        for a in catalog() {
            // The profile installer is launched specially by the UI (visible console).
            if a.id == "install-ps-profile" {
                continue;
            }
            let inv = invocation(a.id).expect("invocation");
            assert!(!inv.program.is_empty());
            assert_eq!(inv.elevated, a.elevated);
        }
    }

    #[test]
    fn unknown_id_is_none() {
        assert!(invocation("nope").is_none());
    }
}
