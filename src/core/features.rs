//! Windows optional features, DISM enable/disable. Unlike tweaks these aren't
//! registry-reversible, so enable/disable shell out to DISM under elevation.
//! Disabling a feature is the natural inverse of enabling it.
//!
//! Enabling/disabling needs admin, and so does an authoritative state query
//! (`Get-WindowsOptionalFeature` / `DISM /Get-Features`). To avoid a UAC prompt
//! just to *look*, [`detect_state`] reports a best-effort current state from
//! cheap, unelevated file / registry probes; features whose payload also ships
//! when they're off report `Unknown`.

use crate::core::action::{Hive, RegValue};
use crate::core::registry;

pub struct Feature {
    pub id: &'static str,
    pub name: &'static str,
    pub desc: &'static str,
    /// DISM `FeatureName`. Multiple (comma-joined) for umbrella features.
    pub dism: &'static str,
}

/// Live state of an optional feature, as far as we can tell without elevation.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum State {
    Enabled,
    Disabled,
    /// Can't tell without elevation (DISM/Get-WindowsOptionalFeature need admin).
    Unknown,
}

impl State {
    /// UI code: 0 = unknown, 1 = enabled, 2 = disabled.
    pub fn code(self) -> i32 {
        match self {
            State::Unknown => 0,
            State::Enabled => 1,
            State::Disabled => 2,
        }
    }
}

/// Best-effort, UNELEVATED detection of whether a feature is enabled, by probing
/// for the files / registry it installs. `Get-WindowsOptionalFeature -Online` and
/// `DISM /Get-Features` both require elevation, so we avoid a UAC-just-to-look by
/// checking cheap, readable signals instead. Features whose payload also ships
/// when the feature is off (wsl.exe, VM Platform on Windows 11) return `Unknown`.
pub fn detect_state(id: &str) -> State {
    let sys = std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".into()) + "\\System32\\";
    let has = |rel: &str| std::path::Path::new(&(sys.clone() + rel)).exists();
    let yes = |b: bool| {
        if b {
            State::Enabled
        } else {
            State::Disabled
        }
    };
    match id {
        "netfx3" => yes(registry::read(
            Hive::Hklm,
            "SOFTWARE\\Microsoft\\NET Framework Setup\\NDP\\v3.5",
            "Install",
        )
        .unwrap_or(None)
            == Some(RegValue::Dword(1))),
        "hyperv" => yes(has("vmms.exe")),
        "sandbox" => yes(has("WindowsSandbox.exe")),
        "iis" => yes(has("inetsrv\\w3wp.exe")),
        "telnet" => yes(has("telnet.exe")),
        "tftp" => yes(has("tftp.exe")),
        "directplay" => yes(has("dplayx.dll")),
        "smb1" => yes(has("drivers\\mrxsmb10.sys")),
        "nfs" => yes(has("mount.exe")),
        // wsl.exe and the VM Platform payload ship on Windows 11 even with the
        // optional feature disabled, so file presence would false-positive.
        _ => State::Unknown,
    }
}

pub fn catalog() -> &'static [Feature] {
    &[
        Feature {
            id: "netfx3",
            name: ".NET Framework 3.5",
            desc: "Legacy .NET runtime (3.0/2.0) for older apps and games.",
            dism: "NetFx3",
        },
        Feature {
            id: "hyperv",
            name: "Hyper-V",
            desc: "Microsoft's type-1 hypervisor and the Hyper-V Manager.",
            dism: "Microsoft-Hyper-V-All",
        },
        Feature {
            id: "sandbox",
            name: "Windows Sandbox",
            desc: "Disposable, isolated desktop for running untrusted software.",
            dism: "Containers-DisposableClientVM",
        },
        Feature {
            id: "wsl",
            name: "Windows Subsystem for Linux",
            desc: "Run Linux distributions natively. Pairs with VM Platform.",
            dism: "Microsoft-Windows-Subsystem-Linux,VirtualMachinePlatform",
        },
        Feature {
            id: "vmplatform",
            name: "Virtual Machine Platform",
            desc: "Virtualization layer required by WSL 2 and Android subsystem.",
            dism: "VirtualMachinePlatform",
        },
        Feature {
            id: "iis",
            name: "Internet Information Services",
            desc: "Microsoft's web server (IIS) with the management console.",
            dism: "IIS-WebServerRole,IIS-WebServer,IIS-ManagementConsole",
        },
        Feature {
            id: "telnet",
            name: "Telnet Client",
            desc: "Command-line Telnet client for testing TCP services.",
            dism: "TelnetClient",
        },
        Feature {
            id: "tftp",
            name: "TFTP Client",
            desc: "Trivial FTP client, handy for network-booting devices.",
            dism: "TFTP",
        },
        Feature {
            id: "directplay",
            name: "Legacy Media (DirectPlay)",
            desc: "DirectPlay compatibility shim some old games still need.",
            dism: "DirectPlay",
        },
        Feature {
            id: "smb1",
            name: "SMB 1.0 / CIFS",
            desc: "Obsolete file-sharing protocol. Insecure, enable only if forced.",
            dism: "SMB1Protocol",
        },
        Feature {
            id: "nfs",
            name: "NFS Client",
            desc: "Mount Unix/Linux/NAS network shares over NFS.",
            dism: "ServicesForNFS-ClientOnly,ClientForNFS-Infrastructure,NFS-Administration",
        },
    ]
}

/// PowerShell that enables (or disables) every DISM component of a feature.
pub fn dism_script(f: &Feature, enable: bool) -> String {
    let verb = if enable {
        "Enable-Feature"
    } else {
        "Disable-Feature"
    };
    let all = if enable { " /All" } else { "" };
    f.dism
        .split(',')
        .map(|name| format!("DISM /Online /{verb} /FeatureName:{name}{all} /NoRestart"))
        .collect::<Vec<_>>()
        .join("; ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_is_sane() {
        assert!(catalog().len() >= 8);
        for f in catalog() {
            assert!(!f.id.is_empty() && !f.name.is_empty() && !f.dism.is_empty());
        }
    }

    #[test]
    fn detect_state_never_panics_and_wsl_is_unknown() {
        for f in catalog() {
            let s = detect_state(f.id);
            if f.id == "wsl" || f.id == "vmplatform" {
                assert!(s == State::Unknown, "{} should be Unknown", f.id);
            }
        }
        // An unknown id is Unknown, not a panic.
        assert!(detect_state("does-not-exist") == State::Unknown);
    }

    #[test]
    fn enable_script_covers_all_components() {
        let wsl = catalog().iter().find(|f| f.id == "wsl").unwrap();
        let s = dism_script(wsl, true);
        assert!(s.contains("Microsoft-Windows-Subsystem-Linux"));
        assert!(s.contains("VirtualMachinePlatform"));
        assert!(s.contains("/Enable-Feature"));
        assert!(s.contains("/All"));
        let off = dism_script(wsl, false);
        assert!(off.contains("/Disable-Feature"));
        assert!(!off.contains("/All"));
    }
}
