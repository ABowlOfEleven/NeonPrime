//! Gaming setup, with multi-GPU (hybrid) handling.
//!
//! On laptops/desktops with an iGPU + dGPU, games should run on the discrete
//! GPU. Following the CachyOS/Arch guidance (which recommends PRIME render
//! offload and advises against optimus-manager/Bumblebee), this offers:
//!
//! - the correct launch-option prefix for the dGPU (NVIDIA PRIME env vars, or
//!   `DRI_PRIME=1` for Mesa AMD/Intel),
//! - `switcheroo-control`, the no-fuss per-app "Run using dedicated graphics
//!   card" that KDE/GNOME/Cinnamon expose once its service is enabled,
//! - gaming tools (GameMode, MangoHud, Gamescope), or the CachyOS meta packages.

use std::process::Command;

use super::{gpudriver, pkg, ElevatedCmd};
use gpudriver::Vendor;

/// A hybrid system has two or more GPUs.
pub fn is_hybrid() -> bool {
    gpudriver::detect().len() >= 2
}

pub fn has_nvidia() -> bool {
    gpudriver::detect()
        .iter()
        .any(|g| g.vendor == Vendor::Nvidia)
}

/// Steam / Lutris launch-option prefix to run a game on the discrete GPU.
/// NVIDIA needs the PRIME render-offload env vars (these work everywhere, and are
/// what the `prime-run` shorthand sets); Mesa AMD/Intel dGPUs use `DRI_PRIME=1`.
pub fn launch_options() -> String {
    if has_nvidia() {
        "__NV_PRIME_RENDER_OFFLOAD=1 __VK_LAYER_NV_optimus=NVIDIA_only __GLX_VENDOR_LIBRARY_NAME=nvidia %command%".into()
    } else if is_hybrid() {
        "DRI_PRIME=1 %command%".into()
    } else {
        "%command%".into()
    }
}

fn is_cachyos() -> bool {
    std::fs::read_to_string("/etc/os-release")
        .map(|s| s.lines().any(|l| l.trim() == "ID=cachyos"))
        .unwrap_or(false)
}

/// Install gaming tools: the CachyOS meta packages, or GameMode/MangoHud/
/// Gamescope via the primary manager.
pub fn install_tools_cmd() -> Option<ElevatedCmd> {
    if is_cachyos() {
        return Some(ElevatedCmd::new(
            "Install CachyOS gaming meta packages",
            &[
                "pacman",
                "-S",
                "--noconfirm",
                "cachyos-gaming-meta",
                "cachyos-gaming-applications",
            ],
        ));
    }
    let m = pkg::primary()?;
    use pkg::Manager::*;
    let argv: Vec<&str> = match m {
        Apt => vec!["apt-get", "install", "-y", "gamemode", "mangohud"],
        Dnf => vec!["dnf", "install", "-y", "gamemode", "mangohud", "gamescope"],
        Pacman => vec![
            "pacman",
            "-S",
            "--noconfirm",
            "gamemode",
            "mangohud",
            "gamescope",
        ],
        Zypper => vec![
            "zypper",
            "--non-interactive",
            "install",
            "gamemode",
            "mangohud",
        ],
        Flatpak => return None,
    };
    Some(ElevatedCmd::new(
        "Install gaming tools (GameMode, MangoHud, Gamescope)",
        &argv,
    ))
}

/// Install + enable `switcheroo-control`: the desktop's per-app "Run using
/// dedicated graphics card" for every application (KDE/GNOME/Cinnamon). This is
/// the no-fuss way to make apps launch on the dGPU.
pub fn switcheroo_cmd() -> Option<ElevatedCmd> {
    let m = pkg::primary()?;
    use pkg::Manager::*;
    let install = match m {
        Apt => "apt-get install -y switcheroo-control",
        Dnf => "dnf install -y switcheroo-control",
        Pacman => "pacman -S --noconfirm switcheroo-control",
        Zypper => "zypper --non-interactive install switcheroo-control",
        Flatpak => return None,
    };
    let script = format!("{install} && systemctl enable --now switcheroo-control");
    Some(ElevatedCmd::new(
        "Enable per-app GPU switching (switcheroo-control)",
        &["sh", "-c", &script],
    ))
}

/// Is switcheroo-control's service already active?
pub fn switcheroo_active() -> bool {
    Command::new("systemctl")
        .args(["is-active", "switcheroo-control"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "active")
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn launch_options_always_has_command_placeholder() {
        assert!(launch_options().contains("%command%"));
    }

    #[test]
    fn commands_do_not_panic() {
        let _ = install_tools_cmd();
        let _ = switcheroo_cmd();
        let _ = switcheroo_active();
        let _ = is_hybrid();
    }
}
