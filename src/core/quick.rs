//! Quick Actions, one-click system maintenance. Each action is a process
//! invocation (optionally elevated via UAC). Unlike tweaks these are not part of
//! the reversible model, they're fire-and-forget maintenance.

pub struct QuickAction {
    pub id: &'static str,
    pub name: &'static str,
    pub desc: &'static str,
    /// Destructive-ish (deletes data), surfaced with a warning accent.
    pub danger: bool,
    /// Needs administrator rights (launched via UAC).
    pub elevated: bool,
}

/// How to run an action: program + args, and whether to elevate.
pub struct Invocation {
    pub program: String,
    pub args: Vec<String>,
    pub elevated: bool,
    /// Keep a visible console open (long repairs like SFC/DISM the user watches).
    /// Ignored for non-elevated actions.
    pub visible: bool,
}

pub fn catalog() -> Vec<QuickAction> {
    vec![
        QuickAction {
            id: "restart-explorer",
            name: "Restart Explorer",
            desc: "Restart the Windows shell, applies taskbar / context-menu tweaks.",
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
        QuickAction {
            id: "remove-ps-profile",
            name: "Remove PowerShell profile",
            desc: "Undo the NeonPrime profile: restore your previous $PROFILE from the backup, or reset it to the default.",
            danger: true,
            elevated: false,
        },
        // ── System configuration (WinUtil feature-tab parity) ────────
        QuickAction {
            id: "enable-ssh-server",
            name: "Enable OpenSSH server",
            desc: "Install and start the built-in OpenSSH server, set it to auto-start, and open TCP 22 in the firewall.",
            danger: false,
            elevated: true,
        },
        QuickAction {
            id: "ntp-fix",
            name: "Fix system clock (NTP resync)",
            desc: "Point the time service at pool.ntp.org and force a resync, fixes a wrong or drifting clock.",
            danger: false,
            elevated: true,
        },
        QuickAction {
            id: "registry-backup",
            name: "Back up the registry",
            desc: "Re-enable Windows' periodic registry backup and run it now (writes to RegBack).",
            danger: false,
            elevated: true,
        },
        QuickAction {
            id: "legacy-recovery-enable",
            name: "Enable legacy F8 boot menu",
            desc: "Restore the old F8 'Advanced Boot Options' menu at startup.",
            danger: false,
            elevated: true,
        },
        QuickAction {
            id: "legacy-recovery-disable",
            name: "Disable legacy F8 boot menu",
            desc: "Return to the modern (fast) boot behavior without the F8 menu.",
            danger: false,
            elevated: true,
        },
        QuickAction {
            id: "autologin",
            name: "Set up auto sign-in",
            desc: "Open the Windows user-accounts dialog (netplwiz) to configure passwordless sign-in yourself.",
            danger: false,
            elevated: false,
        },
        // ── Open Windows settings (control-panel applets) ────────────
        QuickAction {
            id: "open-control-panel",
            name: "Open Control Panel",
            desc: "Launch the classic Control Panel.",
            danger: false,
            elevated: false,
        },
        QuickAction {
            id: "open-computer-mgmt",
            name: "Open Computer Management",
            desc: "Disk Management, Event Viewer, local users, and services in one console.",
            danger: false,
            elevated: false,
        },
        QuickAction {
            id: "open-network",
            name: "Open Network Connections",
            desc: "The classic network adapters panel (ncpa.cpl).",
            danger: false,
            elevated: false,
        },
        QuickAction {
            id: "open-power",
            name: "Open Power Options",
            desc: "The classic power-plan control panel.",
            danger: false,
            elevated: false,
        },
        QuickAction {
            id: "open-sound",
            name: "Open Sound settings",
            desc: "The classic playback/recording devices panel.",
            danger: false,
            elevated: false,
        },
        QuickAction {
            id: "open-system",
            name: "Open System Properties",
            desc: "The classic System properties (sysdm.cpl).",
            danger: false,
            elevated: false,
        },
        QuickAction {
            id: "open-region",
            name: "Open Region settings",
            desc: "The classic region and formats panel.",
            danger: false,
            elevated: false,
        },
        QuickAction {
            id: "open-datetime",
            name: "Open Date & Time",
            desc: "The classic date, time, and time-zone panel.",
            danger: false,
            elevated: false,
        },
        QuickAction {
            id: "open-printers",
            name: "Open Printers",
            desc: "Devices and Printers.",
            danger: false,
            elevated: false,
        },
        QuickAction {
            id: "open-restore",
            name: "Open System Restore",
            desc: "Launch the System Restore wizard (rstrui).",
            danger: false,
            elevated: false,
        },
        // ── Repair & diagnostics (elevated, visible console) ─────────
        QuickAction {
            id: "sfc-scan",
            name: "Scan system files (SFC)",
            desc: "Run sfc /scannow to scan and repair protected system files, in an elevated console you can watch.",
            danger: false,
            elevated: true,
        },
        QuickAction {
            id: "dism-health",
            name: "Repair component store (DISM)",
            desc: "Run DISM /Online /Cleanup-Image /RestoreHealth to repair the Windows image, in an elevated console.",
            danger: false,
            elevated: true,
        },
        QuickAction {
            id: "chkdsk-scan",
            name: "Check disk (read-only)",
            desc: "Run a read-only chkdsk on the system drive. Use /f from an admin prompt to fix (needs a reboot).",
            danger: false,
            elevated: true,
        },
        QuickAction {
            id: "gpupdate",
            name: "Refresh Group Policy",
            desc: "Run gpupdate /force to reapply computer and user policy.",
            danger: false,
            elevated: true,
        },
        QuickAction {
            id: "enable-rdp",
            name: "Enable Remote Desktop",
            desc: "Allow inbound RDP connections and open the Remote Desktop firewall group.",
            danger: false,
            elevated: true,
        },
        QuickAction {
            id: "network-reset",
            name: "Reset network stack",
            desc: "Reset Winsock and TCP/IP, then release, renew, and flush DNS. Finish with a reboot.",
            danger: false,
            elevated: true,
        },
        QuickAction {
            id: "battery-report",
            name: "Battery health report",
            desc: "Generate a powercfg battery report to your user folder and open it.",
            danger: false,
            elevated: false,
        },
        QuickAction {
            id: "system-report",
            name: "System information report",
            desc: "Write full systeminfo output to your user folder and open it.",
            danger: false,
            elevated: false,
        },
        // ── Management consoles (MMC snap-ins) ───────────────────────
        QuickAction {
            id: "open-eventvwr",
            name: "Open Event Viewer",
            desc: "The full Windows event log console (eventvwr.msc).",
            danger: false,
            elevated: false,
        },
        QuickAction {
            id: "open-services",
            name: "Open Services console",
            desc: "The services.msc management console.",
            danger: false,
            elevated: false,
        },
        QuickAction {
            id: "open-taskschd",
            name: "Open Task Scheduler",
            desc: "The taskschd.msc console for scheduled tasks.",
            danger: false,
            elevated: false,
        },
        QuickAction {
            id: "open-diskmgmt",
            name: "Open Disk Management",
            desc: "The diskmgmt.msc console for partitions and volumes.",
            danger: false,
            elevated: false,
        },
        QuickAction {
            id: "open-devmgmt",
            name: "Open Device Manager",
            desc: "The devmgmt.msc console for hardware and drivers.",
            danger: false,
            elevated: false,
        },
        QuickAction {
            id: "open-lusrmgr",
            name: "Open Local Users & Groups",
            desc: "The lusrmgr.msc console for local accounts and groups.",
            danger: false,
            elevated: false,
        },
        QuickAction {
            id: "open-gpedit",
            name: "Open Group Policy Editor",
            desc: "The gpedit.msc console (not present on Windows Home).",
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
        visible: false,
    };
    // Elevated action that keeps a visible console open (long-running repairs).
    let invc = |program: &str, args: &[&str]| Invocation {
        program: program.into(),
        args: args.iter().map(|s| s.to_string()).collect(),
        elevated: true,
        visible: true,
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

        // ── System configuration (elevated one-shots) ───────────────
        // These run via `powershell -Command`; the elevated launcher joins the
        // args back together, so scripts here use only double quotes (never
        // single quotes) to avoid the -ArgumentList re-quoting pitfall.
        "enable-ssh-server" => inv(
            "powershell",
            &[
                "-NoProfile",
                "-Command",
                "Add-WindowsCapability -Online -Name OpenSSH.Server~~~~0.0.1.0; \
                 Set-Service -Name sshd -StartupType Automatic; Start-Service sshd; \
                 New-NetFirewallRule -Name sshd-neonprime -DisplayName OpenSSH-Server \
                 -Enabled True -Direction Inbound -Protocol TCP -Action Allow -LocalPort 22 \
                 -ErrorAction SilentlyContinue",
            ],
            true,
        ),
        "ntp-fix" => inv(
            "powershell",
            &[
                "-NoProfile",
                "-Command",
                "w32tm /config /manualpeerlist:pool.ntp.org /syncfromflags:manual /update; \
                 Stop-Service w32time -ErrorAction SilentlyContinue; Start-Service w32time; \
                 w32tm /resync",
            ],
            true,
        ),
        "registry-backup" => inv(
            "powershell",
            &[
                "-NoProfile",
                "-Command",
                "reg add \"HKLM\\SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion\\Configuration Manager\" \
                 /v EnablePeriodicBackup /t REG_DWORD /d 1 /f; \
                 schtasks /run /tn \"\\Microsoft\\Windows\\Registry\\RegIdleBackup\"",
            ],
            true,
        ),
        "legacy-recovery-enable" => {
            inv("bcdedit", &["/set", "{default}", "bootmenupolicy", "legacy"], true)
        }
        "legacy-recovery-disable" => inv(
            "bcdedit",
            &["/set", "{default}", "bootmenupolicy", "standard"],
            true,
        ),
        "autologin" => inv("netplwiz", &[], false),

        // ── Open Windows settings (non-elevated launches) ────────────
        "open-control-panel" => inv("control", &[], false),
        "open-computer-mgmt" => inv("mmc", &["compmgmt.msc"], false),
        "open-network" => inv("control", &["ncpa.cpl"], false),
        "open-power" => inv("control", &["powercfg.cpl"], false),
        "open-sound" => inv("control", &["mmsys.cpl"], false),
        "open-system" => inv("control", &["sysdm.cpl"], false),
        "open-region" => inv("control", &["intl.cpl"], false),
        "open-datetime" => inv("control", &["timedate.cpl"], false),
        "open-printers" => inv("control", &["printers"], false),
        "open-restore" => inv("rstrui", &[], false),

        // ── Repair & diagnostics (elevated, visible console) ─────────
        // `invc` keeps the elevated console open; cmd /k holds it after the tool
        // exits so the user can read the result.
        "sfc-scan" => invc("cmd", &["/k", "sfc /scannow"]),
        "dism-health" => invc("cmd", &["/k", "DISM /Online /Cleanup-Image /RestoreHealth"]),
        "chkdsk-scan" => invc("cmd", &["/k", "chkdsk"]),
        "gpupdate" => invc("cmd", &["/k", "gpupdate /force"]),
        "enable-rdp" => invc(
            "powershell",
            &[
                "-NoProfile",
                "-Command",
                "Set-ItemProperty -Path \"HKLM:\\System\\CurrentControlSet\\Control\\Terminal Server\" \
                 -Name fDenyTSConnections -Value 0; \
                 Enable-NetFirewallRule -DisplayGroup \"Remote Desktop\"; \
                 Write-Host \"Remote Desktop enabled.\"",
            ],
        ),
        "network-reset" => invc(
            "cmd",
            &[
                "/k",
                "netsh winsock reset & netsh int ip reset & ipconfig /release & \
                 ipconfig /renew & ipconfig /flushdns & echo. & echo Reboot to finish.",
            ],
        ),
        "battery-report" => inv(
            "cmd",
            &[
                "/c",
                "powercfg /batteryreport /output \"%USERPROFILE%\\battery-report.html\" & \
                 start \"\" \"%USERPROFILE%\\battery-report.html\"",
            ],
            false,
        ),
        "system-report" => inv(
            "cmd",
            &[
                "/c",
                "systeminfo > \"%USERPROFILE%\\systeminfo.txt\" & \
                 notepad \"%USERPROFILE%\\systeminfo.txt\"",
            ],
            false,
        ),

        // ── Management consoles (MMC snap-ins) ───────────────────────
        "open-eventvwr" => inv("eventvwr", &[], false),
        "open-services" => inv("mmc", &["services.msc"], false),
        "open-taskschd" => inv("mmc", &["taskschd.msc"], false),
        "open-diskmgmt" => inv("mmc", &["diskmgmt.msc"], false),
        "open-devmgmt" => inv("mmc", &["devmgmt.msc"], false),
        "open-lusrmgr" => inv("mmc", &["lusrmgr.msc"], false),
        "open-gpedit" => inv("mmc", &["gpedit.msc"], false),

        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_runnable_action_has_an_invocation() {
        for a in catalog() {
            // The profile install/remove are launched specially by the UI.
            if a.id == "install-ps-profile" || a.id == "remove-ps-profile" {
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
