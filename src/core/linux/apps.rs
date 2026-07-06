//! Curated application catalog, the Linux analog of the Windows Install catalog.
//!
//! Each app carries per-manager package names (apt/dnf/pacman) and a Flathub id,
//! so the Packages panel can offer one-click installs whichever manager the user
//! picks. An empty field means "not available via that manager".

use super::pkg::Manager;

pub struct App {
    pub name: &'static str,
    pub desc: &'static str,
    pub category: &'static str, // Browsers|Development|Media|Communication|Utilities|Gaming
    pub apt: &'static str,
    pub dnf: &'static str,
    pub pacman: &'static str,
    pub flatpak: &'static str,
}

macro_rules! app {
    ($name:expr, $cat:expr, $desc:expr, $apt:expr, $dnf:expr, $pac:expr, $flat:expr) => {
        App {
            name: $name,
            category: $cat,
            desc: $desc,
            apt: $apt,
            dnf: $dnf,
            pacman: $pac,
            flatpak: $flat,
        }
    };
}

pub fn catalog() -> &'static [App] {
    &[
        // Browsers
        app!(
            "Firefox",
            "Browsers",
            "Mozilla web browser.",
            "firefox",
            "firefox",
            "firefox",
            "org.mozilla.firefox"
        ),
        app!(
            "Chromium",
            "Browsers",
            "Open-source Chrome base.",
            "chromium",
            "chromium",
            "chromium",
            "org.chromium.Chromium"
        ),
        app!(
            "Brave",
            "Browsers",
            "Privacy-focused Chromium browser.",
            "",
            "",
            "",
            "com.brave.Browser"
        ),
        // Development
        app!(
            "Git",
            "Development",
            "Distributed version control.",
            "git",
            "git",
            "git",
            ""
        ),
        app!(
            "Neovim",
            "Development",
            "Hyperextensible Vim-based editor.",
            "neovim",
            "neovim",
            "neovim",
            "io.neovim.nvim"
        ),
        app!(
            "VS Code",
            "Development",
            "Microsoft's code editor.",
            "",
            "",
            "",
            "com.visualstudio.code"
        ),
        app!(
            "Docker",
            "Development",
            "Container engine.",
            "docker.io",
            "docker",
            "docker",
            ""
        ),
        app!(
            "Alacritty",
            "Development",
            "GPU-accelerated terminal.",
            "alacritty",
            "alacritty",
            "alacritty",
            ""
        ),
        app!(
            "Kitty",
            "Development",
            "Fast, feature-rich terminal.",
            "kitty",
            "kitty",
            "kitty",
            ""
        ),
        app!(
            "tmux",
            "Development",
            "Terminal multiplexer.",
            "tmux",
            "tmux",
            "tmux",
            ""
        ),
        // Media
        app!(
            "VLC",
            "Media",
            "Plays nearly any media file.",
            "vlc",
            "vlc",
            "vlc",
            "org.videolan.VLC"
        ),
        app!(
            "mpv",
            "Media",
            "Minimal, scriptable media player.",
            "mpv",
            "mpv",
            "mpv",
            "io.mpv.Mpv"
        ),
        app!(
            "OBS Studio",
            "Media",
            "Streaming and screen recording.",
            "obs-studio",
            "obs-studio",
            "obs-studio",
            "com.obsproject.Studio"
        ),
        app!(
            "GIMP",
            "Media",
            "Raster image editor.",
            "gimp",
            "gimp",
            "gimp",
            "org.gimp.GIMP"
        ),
        app!(
            "Inkscape",
            "Media",
            "Vector graphics editor.",
            "inkscape",
            "inkscape",
            "inkscape",
            "org.inkscape.Inkscape"
        ),
        app!(
            "Blender",
            "Media",
            "3D creation suite.",
            "blender",
            "blender",
            "blender",
            "org.blender.Blender"
        ),
        app!(
            "Krita",
            "Media",
            "Digital painting.",
            "krita",
            "krita",
            "krita",
            "org.kde.krita"
        ),
        // Communication
        app!(
            "Discord",
            "Communication",
            "Voice and text chat.",
            "",
            "",
            "discord",
            "com.discordapp.Discord"
        ),
        app!(
            "Telegram",
            "Communication",
            "Messaging app.",
            "telegram-desktop",
            "telegram-desktop",
            "telegram-desktop",
            "org.telegram.desktop"
        ),
        app!(
            "Signal",
            "Communication",
            "Private messenger.",
            "",
            "",
            "signal-desktop",
            "org.signal.Signal"
        ),
        app!(
            "Thunderbird",
            "Communication",
            "Email client.",
            "thunderbird",
            "thunderbird",
            "thunderbird",
            "org.mozilla.Thunderbird"
        ),
        // Utilities
        app!(
            "htop",
            "Utilities",
            "Interactive process viewer.",
            "htop",
            "htop",
            "htop",
            ""
        ),
        app!(
            "Fastfetch",
            "Utilities",
            "System info in the terminal.",
            "fastfetch",
            "fastfetch",
            "fastfetch",
            ""
        ),
        app!(
            "GParted",
            "Utilities",
            "Partition editor.",
            "gparted",
            "gparted",
            "gparted",
            ""
        ),
        app!(
            "Timeshift",
            "Utilities",
            "System restore snapshots.",
            "timeshift",
            "timeshift",
            "timeshift",
            ""
        ),
        app!(
            "LibreOffice",
            "Utilities",
            "Office suite.",
            "libreoffice",
            "libreoffice",
            "libreoffice-fresh",
            "org.libreoffice.LibreOffice"
        ),
        // Gaming
        app!(
            "Steam",
            "Gaming",
            "Valve's game store.",
            "steam",
            "steam",
            "steam",
            "com.valvesoftware.Steam"
        ),
        app!(
            "Lutris",
            "Gaming",
            "Open gaming platform.",
            "lutris",
            "lutris",
            "lutris",
            "net.lutris.Lutris"
        ),
        app!(
            "Heroic",
            "Gaming",
            "Epic/GOG game launcher.",
            "",
            "",
            "",
            "com.heroicgameslauncher.hgl"
        ),
        app!(
            "ProtonUp-Qt",
            "Gaming",
            "Manage Proton-GE versions.",
            "",
            "",
            "",
            "net.davidotek.pupgui2"
        ),
    ]
}

/// The package id to install `a` with manager `m`, or None if unavailable there.
pub fn pkg_id(a: &App, m: Manager) -> Option<&'static str> {
    let id = match m {
        Manager::Apt => a.apt,
        Manager::Dnf | Manager::Zypper => a.dnf,
        Manager::Pacman => a.pacman,
        Manager::Flatpak => a.flatpak,
    };
    (!id.is_empty()).then_some(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_is_populated_and_categorized() {
        assert!(catalog().len() >= 20);
        let cats: std::collections::HashSet<_> = catalog().iter().map(|a| a.category).collect();
        assert!(cats.contains("Browsers"));
        assert!(cats.contains("Gaming"));
    }

    #[test]
    fn pkg_id_resolves_per_manager() {
        let vlc = catalog().iter().find(|a| a.name == "VLC").unwrap();
        assert_eq!(pkg_id(vlc, Manager::Apt), Some("vlc"));
        assert_eq!(pkg_id(vlc, Manager::Flatpak), Some("org.videolan.VLC"));
        let brave = catalog().iter().find(|a| a.name == "Brave").unwrap();
        assert_eq!(pkg_id(brave, Manager::Apt), None); // flatpak-only
        assert_eq!(pkg_id(brave, Manager::Flatpak), Some("com.brave.Browser"));
    }
}
