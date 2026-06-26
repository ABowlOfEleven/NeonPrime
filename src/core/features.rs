//! Windows optional features — DISM enable/disable. Unlike tweaks these aren't
//! registry-reversible, so enable/disable shell out to DISM under elevation.
//! Disabling a feature is the natural inverse of enabling it.
//!
//! We don't probe live state: `Get-WindowsOptionalFeature` / `DISM /Get-Features`
//! both require elevation, so reading state would mean a UAC prompt just to open
//! the panel. Instead — like WinUtil — each feature offers explicit
//! Enable/Disable actions.

pub struct Feature {
    pub id: &'static str,
    pub name: &'static str,
    pub desc: &'static str,
    /// DISM `FeatureName`. Multiple (comma-joined) for umbrella features.
    pub dism: &'static str,
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
            desc: "Obsolete file-sharing protocol. Insecure — enable only if forced.",
            dism: "SMB1Protocol",
        },
    ]
}

/// PowerShell that enables (or disables) every DISM component of a feature.
pub fn dism_script(f: &Feature, enable: bool) -> String {
    let verb = if enable { "Enable-Feature" } else { "Disable-Feature" };
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
