//! GPU detection + driver install helper.
//!
//! Detects the GPU(s) via `lspci` (falling back to `/sys/class/drm`) and offers
//! the right driver-install command per vendor and package manager. Mesa (AMD /
//! Intel) userspace is usually preinstalled; this mainly matters for the
//! proprietary NVIDIA driver and the Vulkan/VA-API userspace bits.

use std::process::Command;

use super::{pkg, ElevatedCmd};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Vendor {
    Nvidia,
    Amd,
    Intel,
    Other,
}

impl Vendor {
    pub fn label(self) -> &'static str {
        match self {
            Vendor::Nvidia => "NVIDIA",
            Vendor::Amd => "AMD",
            Vendor::Intel => "Intel",
            Vendor::Other => "Other",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Gpu {
    pub vendor: Vendor,
    pub name: String,
}

fn vendor_of(text: &str) -> Vendor {
    let t = text.to_lowercase();
    // Order matters and markers must be specific: bare "ati" would match
    // "corpor-ati-on", so use "radeon" / "advanced micro devices" / "ati technologies".
    if t.contains("nvidia") {
        Vendor::Nvidia
    } else if t.contains("amd")
        || t.contains("radeon")
        || t.contains("advanced micro devices")
        || t.contains("ati technologies")
    {
        Vendor::Amd
    } else if t.contains("intel") {
        Vendor::Intel
    } else {
        Vendor::Other
    }
}

/// Detect installed GPUs. Uses `lspci` for names; falls back to PCI vendor ids
/// under `/sys/class/drm` when `lspci` is unavailable.
pub fn detect() -> Vec<Gpu> {
    if let Ok(out) = Command::new("lspci").output() {
        if out.status.success() {
            let text = String::from_utf8_lossy(&out.stdout);
            let mut gpus = Vec::new();
            for line in text.lines() {
                let is_gpu = line.contains("VGA compatible controller")
                    || line.contains("3D controller")
                    || line.contains("Display controller");
                if !is_gpu {
                    continue;
                }
                // Description follows the class label after ": ".
                let name = line
                    .splitn(2, ": ")
                    .nth(1)
                    .unwrap_or(line)
                    .trim()
                    .to_string();
                gpus.push(Gpu {
                    vendor: vendor_of(&name),
                    name,
                });
            }
            if !gpus.is_empty() {
                return gpus;
            }
        }
    }
    detect_sys()
}

/// `/sys/class/drm` fallback: read the PCI vendor id of each render node.
fn detect_sys() -> Vec<Gpu> {
    let mut gpus = Vec::new();
    let Ok(rd) = std::fs::read_dir("/sys/class/drm") else {
        return gpus;
    };
    for e in rd.flatten() {
        let name = e.file_name();
        let name = name.to_string_lossy();
        // Only primary card nodes (card0), not connectors (card0-DP-1).
        if !name.starts_with("card") || name.contains('-') {
            continue;
        }
        let vpath = e.path().join("device/vendor");
        let vendor = match std::fs::read_to_string(&vpath).map(|s| s.trim().to_string()) {
            Ok(v) if v == "0x10de" => Vendor::Nvidia,
            Ok(v) if v == "0x1002" => Vendor::Amd,
            Ok(v) if v == "0x8086" => Vendor::Intel,
            _ => continue,
        };
        gpus.push(Gpu {
            vendor,
            name: format!("{} GPU", vendor.label()),
        });
    }
    gpus
}

/// The driver-install command for a vendor via the primary package manager, or
/// None if we don't have a mapping (e.g. Flatpak-only, or an unknown vendor).
pub fn install_cmd(v: Vendor) -> Option<ElevatedCmd> {
    let m = pkg::primary()?;
    use pkg::Manager::*;
    let (summary, argv): (&str, Vec<&str>) = match (v, m) {
        (Vendor::Nvidia, Apt) => (
            "Install the NVIDIA driver (ubuntu-drivers autoinstall)",
            vec!["ubuntu-drivers", "autoinstall"],
        ),
        (Vendor::Nvidia, Dnf) => (
            "Install the NVIDIA driver (requires RPM Fusion enabled)",
            vec![
                "dnf",
                "install",
                "-y",
                "akmod-nvidia",
                "xorg-x11-drv-nvidia-cuda",
            ],
        ),
        (Vendor::Nvidia, Pacman) => (
            "Install the NVIDIA driver + PRIME",
            vec![
                "pacman",
                "-S",
                "--noconfirm",
                "nvidia",
                "nvidia-utils",
                "nvidia-prime",
            ],
        ),
        (Vendor::Amd, Apt) => (
            "Install AMD Vulkan userspace",
            vec!["apt-get", "install", "-y", "mesa-vulkan-drivers"],
        ),
        (Vendor::Amd, Dnf) => (
            "Install AMD Vulkan userspace",
            vec!["dnf", "install", "-y", "mesa-vulkan-drivers"],
        ),
        (Vendor::Amd, Pacman) => (
            "Install AMD Vulkan + VA-API userspace",
            vec![
                "pacman",
                "-S",
                "--noconfirm",
                "mesa",
                "vulkan-radeon",
                "libva-mesa-driver",
            ],
        ),
        (Vendor::Intel, Apt) => (
            "Install Intel Vulkan + media userspace",
            vec![
                "apt-get",
                "install",
                "-y",
                "mesa-vulkan-drivers",
                "intel-media-va-driver",
            ],
        ),
        (Vendor::Intel, Dnf) => (
            "Install Intel Vulkan + media userspace",
            vec![
                "dnf",
                "install",
                "-y",
                "mesa-vulkan-drivers",
                "intel-media-driver",
            ],
        ),
        (Vendor::Intel, Pacman) => (
            "Install Intel Vulkan + media userspace",
            vec![
                "pacman",
                "-S",
                "--noconfirm",
                "mesa",
                "vulkan-intel",
                "intel-media-driver",
            ],
        ),
        _ => return None,
    };
    Some(ElevatedCmd::new(summary, &argv))
}

/// Best-effort: is the NVIDIA proprietary driver already loaded?
pub fn nvidia_loaded() -> bool {
    std::path::Path::new("/proc/driver/nvidia").exists()
        || Command::new("nvidia-smi")
            .arg("-L")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vendor_detection() {
        assert_eq!(vendor_of("NVIDIA Corporation GA104"), Vendor::Nvidia);
        assert_eq!(
            vendor_of("Advanced Micro Devices, Inc. [AMD/ATI]"),
            Vendor::Amd
        );
        assert_eq!(vendor_of("Intel Corporation UHD Graphics"), Vendor::Intel);
    }

    #[test]
    fn detect_does_not_panic() {
        let _ = detect();
    }
}
