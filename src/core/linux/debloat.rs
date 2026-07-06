//! Debloat, the Linux analog of the Windows Debloat panel: remove commonly
//! unwanted preinstalled packages (and the snap system). Package names differ by
//! distro, so each item carries per-manager names; removal goes through the
//! primary manager via `pkexec`, and installed state is probed live.

use std::process::Command;

use super::{pkg, ElevatedCmd};

pub struct Bloat {
    pub id: &'static str,
    pub name: &'static str,
    pub desc: &'static str,
    pub apt: &'static str,
    pub dnf: &'static str,
    pub pacman: &'static str,
}

pub fn catalog() -> &'static [Bloat] {
    &[
        Bloat {
            id: "snapd",
            name: "Snap (snapd)",
            desc: "The snap package system and daemon.",
            apt: "snapd",
            dnf: "snapd",
            pacman: "snapd",
        },
        Bloat {
            id: "thunderbird",
            name: "Thunderbird",
            desc: "Mozilla email client.",
            apt: "thunderbird",
            dnf: "thunderbird",
            pacman: "thunderbird",
        },
        Bloat {
            id: "transmission",
            name: "Transmission",
            desc: "BitTorrent client.",
            apt: "transmission-gtk",
            dnf: "transmission-gtk",
            pacman: "transmission-gtk",
        },
        Bloat {
            id: "rhythmbox",
            name: "Rhythmbox",
            desc: "GNOME music player.",
            apt: "rhythmbox",
            dnf: "rhythmbox",
            pacman: "rhythmbox",
        },
        Bloat {
            id: "cheese",
            name: "Cheese",
            desc: "GNOME webcam app.",
            apt: "cheese",
            dnf: "cheese",
            pacman: "cheese",
        },
        Bloat {
            id: "aisleriot",
            name: "Solitaire (Aisleriot)",
            desc: "GNOME solitaire card games.",
            apt: "aisleriot",
            dnf: "aisleriot",
            pacman: "aisleriot",
        },
    ]
}

/// The package name for the current distro's manager, or None if unmapped.
pub fn pkg_name(b: &Bloat) -> Option<&'static str> {
    let name = match pkg::primary()? {
        pkg::Manager::Apt => b.apt,
        pkg::Manager::Dnf => b.dnf,
        pkg::Manager::Pacman => b.pacman,
        _ => return None,
    };
    (!name.is_empty()).then_some(name)
}

/// Is `name` an installed package? Tries dpkg, then rpm, then pacman.
pub fn is_installed(name: &str) -> bool {
    let probe = |cmd: &str, args: &[&str]| {
        Command::new(cmd)
            .args(args)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    };
    probe("dpkg", &["-s", name]) || probe("rpm", &["-q", name]) || probe("pacman", &["-Q", name])
}

/// Elevated removal command for a bloat item, or None if unmapped here.
pub fn remove_cmd(b: &Bloat) -> Option<ElevatedCmd> {
    let name = pkg_name(b)?;
    Some(pkg::remove(pkg::primary()?, name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_includes_snapd() {
        assert!(catalog().iter().any(|b| b.id == "snapd"));
    }

    #[test]
    fn is_installed_of_nonsense_is_false() {
        assert!(!is_installed("neonprime-definitely-not-a-package-xyz"));
    }
}
