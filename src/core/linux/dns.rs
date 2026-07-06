//! DNS switching via systemd-resolved (`resolvectl`), the Linux analog of the
//! Windows netsh DNS switcher. Same provider set (Auto/Cloudflare/Google/Quad9).
//!
//! Changes apply to the default-route link and are returned as [`ElevatedCmd`]s.

use std::fs;

use super::ElevatedCmd;

pub struct Provider {
    pub name: &'static str,
    /// IPv4 servers; empty means "revert to DHCP".
    pub servers: &'static [&'static str],
}

pub fn providers() -> &'static [Provider] {
    &[
        Provider {
            name: "AUTO",
            servers: &[],
        },
        Provider {
            name: "CLOUDFLARE",
            servers: &["1.1.1.1", "1.0.0.1"],
        },
        Provider {
            name: "GOOGLE",
            servers: &["8.8.8.8", "8.8.4.4"],
        },
        Provider {
            name: "QUAD9",
            servers: &["9.9.9.9", "149.112.112.112"],
        },
    ]
}

/// Interface backing the default IPv4 route, from `/proc/net/route`
/// (Destination `00000000`). Returns None if there is no default route.
pub fn default_link() -> Option<String> {
    let text = fs::read_to_string("/proc/net/route").ok()?;
    for line in text.lines().skip(1) {
        let mut f = line.split_whitespace();
        let iface = f.next()?;
        let dest = f.next()?;
        if dest == "00000000" {
            return Some(iface.to_string());
        }
    }
    None
}

/// Command to apply provider `idx` to `link` (e.g. the value from
/// [`default_link`]). Provider 0 (AUTO) reverts the link to its DHCP-supplied
/// servers; the rest set explicit resolvers.
pub fn set_cmd(idx: usize, link: &str) -> Option<ElevatedCmd> {
    let p = providers().get(idx)?;
    if p.servers.is_empty() {
        return Some(ElevatedCmd::new(
            format!("Revert DNS on {link} to DHCP"),
            &["resolvectl", "revert", link],
        ));
    }
    let mut argv = vec!["resolvectl", "dns", link];
    argv.extend(p.servers.iter().copied());
    Some(ElevatedCmd::new(
        format!("Set DNS on {link} to {}", p.name),
        &argv,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cloudflare_sets_both_servers() {
        let c = set_cmd(1, "eth0").unwrap();
        assert_eq!(
            c.argv,
            vec!["resolvectl", "dns", "eth0", "1.1.1.1", "1.0.0.1"]
        );
    }

    #[test]
    fn auto_reverts() {
        let c = set_cmd(0, "wlan0").unwrap();
        assert_eq!(c.argv, vec!["resolvectl", "revert", "wlan0"]);
    }
}
