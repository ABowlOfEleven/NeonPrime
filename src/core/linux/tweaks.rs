//! Tweaks, the Linux analog of the Windows Tweaks/Privacy panels.
//!
//! Linux has no single settings store, so this is desktop-environment aware. It
//! detects the DE and routes each tweak to the matching config tool:
//!
//! - GNOME family (GNOME/Cinnamon/Budgie/Unity/MATE/Pantheon) -> `gsettings`
//! - KDE Plasma -> `kwriteconfig6`/`kreadconfig6` (falls back to the `5` tools)
//! - XFCE -> `xfconf-query`
//! - kernel knobs (`sysctl`) -> universal, applied via `pkexec`
//!
//! Every tweak is reversible (an "on"/tweaked value and an "off"/stock value) and
//! its live state is read back, so the UI reflects reality, matching the Windows
//! registry-probe model. Only the current DE's tweaks (plus universal ones) are
//! shown.

use std::process::Command;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum De {
    Gnome,
    Kde,
    Xfce,
    Other,
}

impl De {
    pub fn label(self) -> &'static str {
        match self {
            De::Gnome => "GNOME / GTK",
            De::Kde => "KDE Plasma",
            De::Xfce => "XFCE",
            De::Other => "Unknown desktop",
        }
    }
}

fn have(bin: &str) -> bool {
    std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).any(|d| d.join(bin).is_file()))
        .unwrap_or(false)
}

/// Detect the desktop environment: `XDG_CURRENT_DESKTOP` first, then fall back to
/// which config tool is installed.
pub fn detect() -> De {
    let x = std::env::var("XDG_CURRENT_DESKTOP")
        .unwrap_or_default()
        .to_lowercase();
    if x.contains("kde") || x.contains("plasma") {
        return De::Kde;
    }
    if x.contains("xfce") {
        return De::Xfce;
    }
    let gnome_like = ["gnome", "unity", "budgie", "cinnamon", "mate", "pantheon"];
    if gnome_like.iter().any(|k| x.contains(k)) {
        return De::Gnome;
    }
    // No/unknown XDG hint: probe for a tool.
    if have("kreadconfig6") || have("kreadconfig5") {
        De::Kde
    } else if have("xfconf-query") {
        De::Xfce
    } else if have("gsettings") {
        De::Gnome
    } else {
        De::Other
    }
}

pub enum Op {
    /// dconf/gsettings key (GNOME family). Values are passed verbatim to
    /// `gsettings set`, so only quote-free GVariants (booleans) are used here.
    Gsettings {
        schema: &'static str,
        key: &'static str,
        on: &'static str,
        off: &'static str,
    },
    /// KConfig entry (KDE Plasma), via kreadconfig/kwriteconfig.
    Kconfig {
        file: &'static str,
        group: &'static str,
        key: &'static str,
        on: &'static str,
        off: &'static str,
    },
    /// Xfconf property (XFCE). `kind` is the xfconf type (bool/int/string).
    Xfconf {
        channel: &'static str,
        prop: &'static str,
        kind: &'static str,
        on: &'static str,
        off: &'static str,
    },
    /// Kernel `sysctl` parameter (runtime `-w`; needs root). DE-independent.
    Sysctl {
        param: &'static str,
        on: &'static str,
        off: &'static str,
    },
}

pub struct Tweak {
    pub id: &'static str,
    pub name: &'static str,
    pub desc: &'static str,
    pub category: &'static str, // "Interface" | "Performance" | "Privacy" | "Security"
    /// A caveat / what-could-break note (empty = none). Used on Security tweaks.
    pub warn: &'static str,
    pub op: Op,
}

pub fn catalog() -> &'static [Tweak] {
    use Op::*;
    &[
        // ── GNOME family (gsettings) ─────────────────────────────────
        Tweak {
            id: "gn-battery-pct",
            name: "Show battery percentage",
            desc: "Display the battery charge percentage in the top bar.",
            category: "Interface",
            warn: "",
            op: Gsettings {
                schema: "org.gnome.desktop.interface",
                key: "show-battery-percentage",
                on: "true",
                off: "false",
            },
        },
        Tweak {
            id: "gn-clock-seconds",
            name: "Show seconds in clock",
            desc: "Add seconds to the top-bar clock.",
            category: "Interface",
            warn: "",
            op: Gsettings {
                schema: "org.gnome.desktop.interface",
                key: "clock-show-seconds",
                on: "true",
                off: "false",
            },
        },
        Tweak {
            id: "gn-clock-weekday",
            name: "Show weekday in clock",
            desc: "Add the day of the week to the top-bar clock.",
            category: "Interface",
            warn: "",
            op: Gsettings {
                schema: "org.gnome.desktop.interface",
                key: "clock-show-weekday",
                on: "true",
                off: "false",
            },
        },
        Tweak {
            id: "gn-tap-click",
            name: "Tap to click",
            desc: "Enable tap-to-click on the touchpad.",
            category: "Interface",
            warn: "",
            op: Gsettings {
                schema: "org.gnome.desktop.peripherals.touchpad",
                key: "tap-to-click",
                on: "true",
                off: "false",
            },
        },
        Tweak {
            id: "gn-no-animations",
            name: "Disable animations",
            desc: "Turn off desktop animations for a snappier UI.",
            category: "Performance",
            warn: "",
            op: Gsettings {
                schema: "org.gnome.desktop.interface",
                key: "enable-animations",
                on: "false",
                off: "true",
            },
        },
        Tweak {
            id: "gn-no-recent",
            name: "Don't remember recent files",
            desc: "Stop tracking recently used files.",
            category: "Privacy",
            warn: "",
            op: Gsettings {
                schema: "org.gnome.desktop.privacy",
                key: "remember-recent-files",
                on: "false",
                off: "true",
            },
        },
        Tweak {
            id: "gn-no-app-usage",
            name: "Don't track app usage",
            desc: "Stop recording application usage history.",
            category: "Privacy",
            warn: "",
            op: Gsettings {
                schema: "org.gnome.desktop.privacy",
                key: "remember-app-usage",
                on: "false",
                off: "true",
            },
        },
        Tweak {
            id: "gn-no-usage-stats",
            name: "Disable software usage stats",
            desc: "Stop sending anonymous software usage statistics.",
            category: "Privacy",
            warn: "",
            op: Gsettings {
                schema: "org.gnome.desktop.privacy",
                key: "send-software-usage-stats",
                on: "false",
                off: "true",
            },
        },
        Tweak {
            id: "gn-no-tech-reports",
            name: "Disable technical problem reports",
            desc: "Stop automatically reporting technical problems.",
            category: "Privacy",
            warn: "",
            op: Gsettings {
                schema: "org.gnome.desktop.privacy",
                key: "report-technical-problems",
                on: "false",
                off: "true",
            },
        },
        // ── KDE Plasma (KConfig) ─────────────────────────────────────
        Tweak {
            id: "kde-no-animations",
            name: "Disable animations",
            desc: "Set the global animation speed to instant.",
            category: "Performance",
            warn: "",
            op: Kconfig {
                file: "kdeglobals",
                group: "KDE",
                key: "AnimationDurationFactor",
                on: "0",
                off: "1",
            },
        },
        Tweak {
            id: "kde-single-click",
            name: "Single-click to open",
            desc: "Open files and folders with a single click.",
            category: "Interface",
            warn: "",
            op: Kconfig {
                file: "kdeglobals",
                group: "KDE",
                key: "SingleClick",
                on: "true",
                off: "false",
            },
        },
        Tweak {
            id: "kde-no-baloo",
            name: "Disable file indexing (Baloo)",
            desc: "Turn off the desktop file-content indexer.",
            category: "Privacy",
            warn: "",
            op: Kconfig {
                file: "baloofilerc",
                group: "Basic Settings",
                key: "Indexing-Enabled",
                on: "false",
                off: "true",
            },
        },
        // ── XFCE (xfconf) ────────────────────────────────────────────
        Tweak {
            id: "xf-single-click",
            name: "Single-click to open",
            desc: "Open items in the file manager with a single click.",
            category: "Interface",
            warn: "",
            op: Xfconf {
                channel: "thunar",
                prop: "/misc-single-click",
                kind: "bool",
                on: "true",
                off: "false",
            },
        },
        Tweak {
            id: "xf-no-save-session",
            name: "Don't save session on exit",
            desc: "Stop XFCE from restoring apps from the last session.",
            category: "Privacy",
            warn: "",
            op: Xfconf {
                channel: "xfce4-session",
                prop: "/general/SaveOnExit",
                kind: "bool",
                on: "false",
                off: "true",
            },
        },
        // ── Universal (sysctl) ───────────────────────────────────────
        Tweak {
            id: "sys-swappiness",
            name: "Lower swappiness (10)",
            desc: "Prefer RAM over swap (vm.swappiness 10, default 60).",
            category: "Performance",
            warn: "",
            op: Sysctl {
                param: "vm.swappiness",
                on: "10",
                off: "60",
            },
        },
        // ── Security hardening (sysctl, universal). Real hardening, not just
        //    privacy. Each carries a warning about what it does / could break. ──
        Tweak {
            id: "sec-kptr",
            name: "Hide kernel addresses",
            desc: "kernel.kptr_restrict=2, hide kernel memory pointers from userspace.",
            category: "Security",
            warn: "Defeats a common exploit-development aid. Safe; only affects low-level debugging tools.",
            op: Sysctl {
                param: "kernel.kptr_restrict",
                on: "2",
                off: "0",
            },
        },
        Tweak {
            id: "sec-dmesg",
            name: "Restrict kernel log",
            desc: "kernel.dmesg_restrict=1, only root can read the kernel ring buffer.",
            category: "Security",
            warn: "Hides kernel info useful to attackers. Some diagnostics then need sudo to read dmesg.",
            op: Sysctl {
                param: "kernel.dmesg_restrict",
                on: "1",
                off: "0",
            },
        },
        Tweak {
            id: "sec-ptrace",
            name: "Restrict ptrace",
            desc: "kernel.yama.ptrace_scope=1, processes can't attach-debug unrelated processes.",
            category: "Security",
            warn: "Blocks a credential-theft technique. Debuggers still work on their own children; attaching to an unrelated running process then needs sudo.",
            op: Sysctl {
                param: "kernel.yama.ptrace_scope",
                on: "1",
                off: "0",
            },
        },
        Tweak {
            id: "sec-syncookies",
            name: "SYN-flood protection",
            desc: "net.ipv4.tcp_syncookies=1, mitigate TCP SYN-flood denial of service.",
            category: "Security",
            warn: "Standard network hardening. Safe.",
            op: Sysctl {
                param: "net.ipv4.tcp_syncookies",
                on: "1",
                off: "0",
            },
        },
        Tweak {
            id: "sec-rpfilter",
            name: "Reverse-path filtering",
            desc: "net.ipv4.conf.all.rp_filter=1, drop packets with spoofed source addresses.",
            category: "Security",
            warn: "Anti-spoofing. Safe on normal hosts; can break asymmetric or multi-homed routing.",
            op: Sysctl {
                param: "net.ipv4.conf.all.rp_filter",
                on: "1",
                off: "0",
            },
        },
        Tweak {
            id: "sec-no-redirects",
            name: "Ignore ICMP redirects",
            desc: "net.ipv4.conf.all.accept_redirects=0, reject ICMP redirect (MITM) messages.",
            category: "Security",
            warn: "Blocks a man-in-the-middle route-injection trick. Safe on a normal host (not a router).",
            op: Sysctl {
                param: "net.ipv4.conf.all.accept_redirects",
                on: "0",
                off: "1",
            },
        },
        Tweak {
            id: "sec-no-source-route",
            name: "Reject source routing",
            desc: "net.ipv4.conf.all.accept_source_route=0, drop source-routed packets.",
            category: "Security",
            warn: "Blocks a spoofing/bypass technique. Safe.",
            op: Sysctl {
                param: "net.ipv4.conf.all.accept_source_route",
                on: "0",
                off: "1",
            },
        },
        Tweak {
            id: "sec-no-bcast-ping",
            name: "Ignore broadcast pings",
            desc: "net.ipv4.icmp_echo_ignore_broadcasts=1, don't answer broadcast ICMP.",
            category: "Security",
            warn: "Stops your host amplifying smurf DoS attacks. Safe.",
            op: Sysctl {
                param: "net.ipv4.icmp_echo_ignore_broadcasts",
                on: "1",
                off: "0",
            },
        },
        Tweak {
            id: "sec-protect-symlinks",
            name: "Protect symlinks",
            desc: "fs.protected_symlinks=1, block symlink-following attacks in world-writable dirs.",
            category: "Security",
            warn: "Prevents a classic /tmp privilege-escalation trick. Usually already on. Safe.",
            op: Sysctl {
                param: "fs.protected_symlinks",
                on: "1",
                off: "0",
            },
        },
        Tweak {
            id: "sec-no-suid-dump",
            name: "No core dumps for setuid",
            desc: "fs.suid_dumpable=0, don't write crash dumps for privileged programs.",
            category: "Security",
            warn: "Stops secrets leaking into core dumps of setuid binaries. Safe.",
            op: Sysctl {
                param: "fs.suid_dumpable",
                on: "0",
                off: "1",
            },
        },
    ]
}

/// Which DE a tweak targets, or None for universal (sysctl) tweaks.
fn de_of(t: &Tweak) -> Option<De> {
    match t.op {
        Op::Gsettings { .. } => Some(De::Gnome),
        Op::Kconfig { .. } => Some(De::Kde),
        Op::Xfconf { .. } => Some(De::Xfce),
        Op::Sysctl { .. } => None,
    }
}

/// Tweaks relevant to the current desktop: the detected DE's tweaks plus the
/// universal ones.
pub fn for_current() -> Vec<&'static Tweak> {
    let de = detect();
    catalog()
        .iter()
        .filter(|t| de_of(t).is_none_or(|d| d == de))
        .collect()
}

/// Whether the detected DE's config tool is installed.
pub fn tools_available(de: De) -> bool {
    match de {
        De::Gnome => have("gsettings"),
        De::Kde => have("kreadconfig6") || have("kreadconfig5"),
        De::Xfce => have("xfconf-query"),
        De::Other => false,
    }
}

fn kread_bin() -> Option<&'static str> {
    if have("kreadconfig6") {
        Some("kreadconfig6")
    } else if have("kreadconfig5") {
        Some("kreadconfig5")
    } else {
        None
    }
}

fn kwrite_bin() -> &'static str {
    if have("kwriteconfig5") && !have("kwriteconfig6") {
        "kwriteconfig5"
    } else {
        "kwriteconfig6"
    }
}

fn output(cmd: &str, args: &[&str]) -> Option<String> {
    let o = Command::new(cmd).args(args).output().ok()?;
    if o.status.success() {
        Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
    } else {
        None
    }
}

/// Strip a single pair of surrounding single quotes (gsettings string output).
fn unquote(s: &str) -> &str {
    let s = s.trim();
    s.strip_prefix('\'')
        .and_then(|x| x.strip_suffix('\''))
        .unwrap_or(s)
}

/// Whether the tweak is currently in its "on" (tweaked) state.
pub fn is_applied(t: &Tweak) -> bool {
    match &t.op {
        Op::Gsettings {
            schema, key, on, ..
        } => output("gsettings", &["get", schema, key])
            .map(|v| unquote(&v) == *on)
            .unwrap_or(false),
        Op::Kconfig {
            file,
            group,
            key,
            on,
            ..
        } => kread_bin()
            .and_then(|b| output(b, &["--file", file, "--group", group, "--key", key]))
            .map(|v| v.trim() == *on)
            .unwrap_or(false),
        Op::Xfconf {
            channel, prop, on, ..
        } => output("xfconf-query", &["-c", channel, "-p", prop])
            .map(|v| v.trim() == *on)
            .unwrap_or(false),
        Op::Sysctl { param, on, .. } => output("sysctl", &["-n", param])
            .map(|v| v.trim() == *on)
            .unwrap_or(false),
    }
}

/// Does applying this tweak need a privilege prompt?
pub fn privileged(t: &Tweak) -> bool {
    matches!(t.op, Op::Sysctl { .. })
}

/// Command line to move the tweak to its on (`apply = true`) or off state.
pub fn apply_argv(t: &Tweak, apply: bool) -> Vec<String> {
    let pick = |on: &'static str, off: &'static str| if apply { on } else { off }.to_string();
    match &t.op {
        Op::Gsettings {
            schema,
            key,
            on,
            off,
        } => vec![
            "gsettings".into(),
            "set".into(),
            (*schema).into(),
            (*key).into(),
            pick(on, off),
        ],
        Op::Kconfig {
            file,
            group,
            key,
            on,
            off,
        } => vec![
            kwrite_bin().into(),
            "--file".into(),
            (*file).into(),
            "--group".into(),
            (*group).into(),
            "--key".into(),
            (*key).into(),
            pick(on, off),
        ],
        Op::Xfconf {
            channel,
            prop,
            kind,
            on,
            off,
        } => vec![
            "xfconf-query".into(),
            "-c".into(),
            (*channel).into(),
            "-p".into(),
            (*prop).into(),
            "--create".into(),
            "-t".into(),
            (*kind).into(),
            "-s".into(),
            pick(on, off),
        ],
        Op::Sysctl { param, on, off } => vec![
            "sysctl".into(),
            "-w".into(),
            format!("{param}={}", pick(on, off)),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_covers_every_backend() {
        let has = |f: fn(&Tweak) -> bool| catalog().iter().any(f);
        assert!(has(|t| matches!(t.op, Op::Gsettings { .. })));
        assert!(has(|t| matches!(t.op, Op::Kconfig { .. })));
        assert!(has(|t| matches!(t.op, Op::Xfconf { .. })));
        assert!(has(|t| matches!(t.op, Op::Sysctl { .. })));
    }

    #[test]
    fn for_current_includes_universal_sysctl() {
        // Whatever the DE, the universal sysctl tweak is always present.
        assert!(for_current().iter().any(|t| t.id == "sys-swappiness"));
    }

    #[test]
    fn gsettings_apply_is_unprivileged_set() {
        let t = catalog().iter().find(|t| t.id == "gn-battery-pct").unwrap();
        assert_eq!(
            apply_argv(t, true),
            vec![
                "gsettings",
                "set",
                "org.gnome.desktop.interface",
                "show-battery-percentage",
                "true"
            ]
        );
        assert!(!privileged(t));
    }

    #[test]
    fn kde_and_xfce_build_expected_commands() {
        let kde = catalog()
            .iter()
            .find(|t| t.id == "kde-single-click")
            .unwrap();
        let a = apply_argv(kde, true);
        assert_eq!(a[0].as_str(), kwrite_bin());
        assert!(a.contains(&"SingleClick".to_string()));

        let xf = catalog()
            .iter()
            .find(|t| t.id == "xf-single-click")
            .unwrap();
        let a = apply_argv(xf, false);
        assert_eq!(&a[0], "xfconf-query");
        assert_eq!(a.last().unwrap(), "false");
    }

    #[test]
    fn sysctl_is_privileged() {
        let t = catalog().iter().find(|t| t.id == "sys-swappiness").unwrap();
        assert!(privileged(t));
        assert_eq!(
            apply_argv(t, true),
            vec!["sysctl", "-w", "vm.swappiness=10"]
        );
    }

    #[test]
    fn unquote_strips_gsettings_quotes() {
        assert_eq!(unquote("'prefer-dark'"), "prefer-dark");
        assert_eq!(unquote("true"), "true");
    }
}
