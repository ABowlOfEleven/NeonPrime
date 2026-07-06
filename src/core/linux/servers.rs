//! Enable common servers: OpenSSH (remote shell) and Samba (SMB file sharing).
//!
//! Installs the package and enables + starts the systemd unit in one `pkexec`
//! step. Unit and package names differ across distros, so each server carries
//! both. Samba shares still need `/etc/samba/smb.conf` + a samba user; this just
//! gets the service running.

use std::process::Command;

use super::{pkg, ElevatedCmd};

pub struct Server {
    pub id: &'static str,
    pub name: &'static str,
    pub desc: &'static str,
    pub apt_pkg: &'static str,
    pub dnf_pkg: &'static str,
    pub pac_pkg: &'static str,
    /// systemd unit on Debian/Ubuntu vs RPM/Arch (they differ for ssh + samba).
    pub deb_unit: &'static str,
    pub rpm_unit: &'static str,
}

pub fn catalog() -> &'static [Server] {
    &[
        Server {
            id: "ssh",
            name: "OpenSSH server",
            desc: "Accept incoming SSH connections (remote shell / scp / sftp).",
            apt_pkg: "openssh-server",
            dnf_pkg: "openssh-server",
            pac_pkg: "openssh",
            deb_unit: "ssh",
            rpm_unit: "sshd",
        },
        Server {
            id: "samba",
            name: "Samba (SMB file sharing)",
            desc: "Windows-compatible file sharing. Shares still need smb.conf + a samba user.",
            apt_pkg: "samba",
            dnf_pkg: "samba",
            pac_pkg: "samba",
            deb_unit: "smbd",
            rpm_unit: "smb",
        },
    ]
}

#[derive(Default, Clone, Copy)]
pub struct Status {
    pub installed: bool,
    pub running: bool,
    pub enabled: bool,
}

fn systemctl(args: &[&str]) -> Option<String> {
    let out = Command::new("systemctl").args(args).output().ok()?;
    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// The systemd unit for this server on the current distro.
pub fn unit(s: &Server) -> &'static str {
    match pkg::primary() {
        Some(pkg::Manager::Apt) => s.deb_unit,
        _ => s.rpm_unit,
    }
}

fn pkg_name(s: &Server) -> Option<&'static str> {
    match pkg::primary()? {
        pkg::Manager::Apt => Some(s.apt_pkg),
        pkg::Manager::Dnf | pkg::Manager::Zypper => Some(s.dnf_pkg),
        pkg::Manager::Pacman => Some(s.pac_pkg),
        pkg::Manager::Flatpak => None,
    }
}

/// Live status: whether the unit exists (installed), is active, and is enabled.
/// Checks the distro unit first, then the other spelling as a fallback.
pub fn status(s: &Server) -> Status {
    for u in [unit(s), s.deb_unit, s.rpm_unit] {
        let enabled_state = systemctl(&["is-enabled", u]);
        let known = matches!(
            enabled_state.as_deref(),
            Some("enabled") | Some("disabled") | Some("static") | Some("masked") | Some("alias")
        );
        if known {
            let running = systemctl(&["is-active", u]).as_deref() == Some("active");
            return Status {
                installed: true,
                running,
                enabled: enabled_state.as_deref() == Some("enabled"),
            };
        }
    }
    Status::default()
}

/// Install the package (if needed) and enable + start the unit, in one step.
pub fn install_enable_cmd(s: &Server) -> Option<ElevatedCmd> {
    let m = pkg::primary()?;
    let name = pkg_name(s)?;
    let u = unit(s);
    let install = match m {
        pkg::Manager::Apt => format!("apt-get install -y {name}"),
        pkg::Manager::Dnf => format!("dnf install -y {name}"),
        pkg::Manager::Pacman => format!("pacman -S --noconfirm {name}"),
        pkg::Manager::Zypper => format!("zypper --non-interactive install {name}"),
        pkg::Manager::Flatpak => return None,
    };
    let script = format!("{install} && systemctl enable --now {u}");
    Some(ElevatedCmd::new(
        format!("Install + enable {}", s.name),
        &["sh", "-c", &script],
    ))
}

/// Stop + disable the unit (leaves the package installed).
pub fn disable_cmd(s: &Server) -> ElevatedCmd {
    ElevatedCmd::new(
        format!("Disable {}", s.name),
        &["systemctl", "disable", "--now", unit(s)],
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_has_ssh_and_samba() {
        assert!(catalog().iter().any(|s| s.id == "ssh"));
        assert!(catalog().iter().any(|s| s.id == "samba"));
    }

    #[test]
    fn disable_targets_a_unit() {
        let ssh = catalog().iter().find(|s| s.id == "ssh").unwrap();
        let c = disable_cmd(ssh);
        assert_eq!(c.argv[0], "systemctl");
        assert!(c.argv.contains(&"disable".to_string()));
    }
}
