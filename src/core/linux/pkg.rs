//! Package-manager abstraction: the Linux analog of the winget-backed installer.
//!
//! Detects which managers are present (apt, dnf, pacman, zypper, flatpak) and
//! produces install/remove/update command lines. System managers are wrapped in
//! `pkexec`; Flatpak installs per-user and is run directly.

use std::path::Path;

use super::ElevatedCmd;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Manager {
    Apt,
    Dnf,
    Pacman,
    Zypper,
    Flatpak,
}

impl Manager {
    pub fn bin(self) -> &'static str {
        match self {
            Manager::Apt => "apt-get",
            Manager::Dnf => "dnf",
            Manager::Pacman => "pacman",
            Manager::Zypper => "zypper",
            Manager::Flatpak => "flatpak",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Manager::Apt => "APT",
            Manager::Dnf => "DNF",
            Manager::Pacman => "Pacman",
            Manager::Zypper => "Zypper",
            Manager::Flatpak => "Flatpak",
        }
    }
}

/// Is `bin` an executable somewhere on `$PATH`?
fn have(bin: &str) -> bool {
    let Some(path) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&path).any(|dir| dir.join(bin).is_file())
}

/// Managers available on this system, most "native" first.
pub fn detected() -> Vec<Manager> {
    let all = [
        Manager::Apt,
        Manager::Dnf,
        Manager::Pacman,
        Manager::Zypper,
        Manager::Flatpak,
    ];
    all.into_iter().filter(|m| have(m.bin())).collect()
}

/// The most likely system (non-Flatpak) manager for this distro, if any.
pub fn primary() -> Option<Manager> {
    detected().into_iter().find(|m| *m != Manager::Flatpak)
}

pub fn install(m: Manager, pkg: &str) -> ElevatedCmd {
    let s = format!("Install {pkg} via {}", m.label());
    match m {
        Manager::Apt => ElevatedCmd::new(s, &["apt-get", "install", "-y", pkg]),
        Manager::Dnf => ElevatedCmd::new(s, &["dnf", "install", "-y", pkg]),
        Manager::Pacman => ElevatedCmd::new(s, &["pacman", "-S", "--noconfirm", pkg]),
        Manager::Zypper => ElevatedCmd::new(s, &["zypper", "--non-interactive", "install", pkg]),
        Manager::Flatpak => ElevatedCmd::new(s, &["flatpak", "install", "-y", "flathub", pkg]),
    }
}

pub fn remove(m: Manager, pkg: &str) -> ElevatedCmd {
    let s = format!("Remove {pkg} via {}", m.label());
    match m {
        Manager::Apt => ElevatedCmd::new(s, &["apt-get", "remove", "-y", pkg]),
        Manager::Dnf => ElevatedCmd::new(s, &["dnf", "remove", "-y", pkg]),
        Manager::Pacman => ElevatedCmd::new(s, &["pacman", "-Rs", "--noconfirm", pkg]),
        Manager::Zypper => ElevatedCmd::new(s, &["zypper", "--non-interactive", "remove", pkg]),
        Manager::Flatpak => ElevatedCmd::new(s, &["flatpak", "uninstall", "-y", pkg]),
    }
}

/// Upgrade everything (the "Update All" button).
pub fn update_all(m: Manager) -> ElevatedCmd {
    let s = format!("Update all packages via {}", m.label());
    match m {
        Manager::Apt => ElevatedCmd::new(s, &["sh", "-c", "apt-get update && apt-get upgrade -y"]),
        Manager::Dnf => ElevatedCmd::new(s, &["dnf", "upgrade", "-y"]),
        Manager::Pacman => ElevatedCmd::new(s, &["pacman", "-Syu", "--noconfirm"]),
        Manager::Zypper => ElevatedCmd::new(s, &["zypper", "--non-interactive", "update"]),
        Manager::Flatpak => ElevatedCmd::new(s, &["flatpak", "update", "-y"]),
    }
}

/// Whether an install/remove for this manager needs a privilege prompt.
pub fn needs_privilege(m: Manager) -> bool {
    m != Manager::Flatpak
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn install_lines_are_correct() {
        assert_eq!(
            install(Manager::Apt, "htop").argv,
            vec!["apt-get", "install", "-y", "htop"]
        );
        assert_eq!(
            install(Manager::Flatpak, "org.gimp.GIMP").argv,
            vec!["flatpak", "install", "-y", "flathub", "org.gimp.GIMP"]
        );
    }

    #[test]
    fn flatpak_needs_no_privilege() {
        assert!(!needs_privilege(Manager::Flatpak));
        assert!(needs_privilege(Manager::Dnf));
    }
}
