//! Linux "phoning home" monitor: active outbound IPv4 TCP connections with the
//! owning process, parsed from `/proc/net/tcp` and joined to PIDs via the socket
//! inode -> `/proc/<pid>/fd` symlink map. Read-only; no privilege needed for the
//! current user's own sockets (a full system view needs root, handled later).
//!
//! IPv6 (`/proc/net/tcp6`) is a follow-up; this scaffold covers IPv4.

use std::collections::HashMap;
use std::fs;
use std::net::{IpAddr, Ipv4Addr};
use std::sync::{Arc, Mutex};

pub struct Conn {
    pub proc_name: String,
    /// Owning pid, or 0 if the socket could not be attributed.
    pub pid: u32,
    pub remote: String,
    pub remote_ip: Ipv4Addr,
    pub host: String,
    pub state: String,
}

fn state_name(hex: &str) -> &'static str {
    match hex {
        "01" => "ESTABLISHED",
        "02" => "SYN-SENT",
        "03" => "SYN-RECV",
        "04" => "FIN-WAIT1",
        "05" => "FIN-WAIT2",
        "06" => "TIME-WAIT",
        "07" => "CLOSE",
        "08" => "CLOSE-WAIT",
        "09" => "LAST-ACK",
        "0A" => "LISTEN",
        "0B" => "CLOSING",
        _ => "?",
    }
}

/// Parse a `/proc/net/tcp` hex `addr:port` field into (Ipv4Addr, port).
/// The address is a little-endian u32 in hex; the port is big-endian hex.
fn parse_addr(field: &str) -> Option<(Ipv4Addr, u16)> {
    let (addr_hex, port_hex) = field.split_once(':')?;
    let addr = u32::from_str_radix(addr_hex, 16).ok()?;
    let port = u16::from_str_radix(port_hex, 16).ok()?;
    Some((Ipv4Addr::from(addr.to_le_bytes()), port))
}

/// Active outbound IPv4 TCP connections. Listeners, loopback, and unconnected
/// sockets are filtered out. Sorted by process name.
pub fn connections() -> Vec<Conn> {
    let text = match fs::read_to_string("/proc/net/tcp") {
        Ok(t) => t,
        Err(_) => return Vec::new(),
    };
    let inode_map = socket_inode_to_pid();
    let mut out = Vec::new();
    for line in text.lines().skip(1) {
        let f: Vec<&str> = line.split_whitespace().collect();
        if f.len() < 10 {
            continue;
        }
        let state = f[3];
        if state == "0A" {
            continue; // LISTEN
        }
        let Some((remote_ip, remote_port)) = parse_addr(f[2]) else {
            continue;
        };
        if remote_port == 0 || remote_ip.is_unspecified() || remote_ip.is_loopback() {
            continue;
        }
        let inode: u64 = f[9].parse().unwrap_or(0);
        let (pid, name) = inode_map
            .get(&inode)
            .cloned()
            .unwrap_or((0, "—".to_string()));
        out.push(Conn {
            proc_name: name,
            pid,
            remote: format!("{remote_ip}:{remote_port}"),
            remote_ip,
            host: String::new(),
            state: state_name(state).into(),
        });
    }
    out.sort_by(|a, b| {
        a.proc_name
            .to_lowercase()
            .cmp(&b.proc_name.to_lowercase())
            .then(a.remote.cmp(&b.remote))
    });
    out
}

/// Build a map of socket inode -> (pid, process name) by scanning `/proc/<pid>/fd`
/// for `socket:[inode]` symlinks. Best-effort: unreadable pids are skipped.
fn socket_inode_to_pid() -> HashMap<u64, (u32, String)> {
    let mut map = HashMap::new();
    let Ok(procs) = fs::read_dir("/proc") else {
        return map;
    };
    for entry in procs.flatten() {
        let name = entry.file_name();
        let Some(pid) = name.to_str().and_then(|s| s.parse::<u32>().ok()) else {
            continue;
        };
        let comm = fs::read_to_string(format!("/proc/{pid}/comm"))
            .map(|s| s.trim().to_string())
            .unwrap_or_default();
        let Ok(fds) = fs::read_dir(format!("/proc/{pid}/fd")) else {
            continue;
        };
        for fd in fds.flatten() {
            if let Ok(target) = fs::read_link(fd.path()) {
                let t = target.to_string_lossy();
                if let Some(rest) = t.strip_prefix("socket:[") {
                    if let Some(num) = rest.strip_suffix(']') {
                        if let Ok(inode) = num.parse::<u64>() {
                            map.entry(inode).or_insert((pid, comm.clone()));
                        }
                    }
                }
            }
        }
    }
    map
}

/// Lazy, cached reverse-DNS resolver (identical contract to the Windows one):
/// `host()` never blocks; it returns a cached name or "" and resolves in the
/// background. Failures and private ranges are cached as "".
#[derive(Clone, Default)]
pub struct Resolver {
    cache: Arc<Mutex<HashMap<Ipv4Addr, String>>>,
    inflight: Arc<Mutex<std::collections::HashSet<Ipv4Addr>>>,
}

impl Resolver {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn host(&self, ip: Ipv4Addr) -> String {
        if let Some(h) = self.cache.lock().unwrap().get(&ip) {
            return h.clone();
        }
        if ip.is_private() || ip.is_loopback() || ip.is_link_local() {
            self.cache.lock().unwrap().insert(ip, String::new());
            return String::new();
        }
        if self.inflight.lock().unwrap().insert(ip) {
            let cache = self.cache.clone();
            let inflight = self.inflight.clone();
            std::thread::spawn(move || {
                let name = match dns_lookup::lookup_addr(&IpAddr::V4(ip)) {
                    Ok(n) if n != ip.to_string() => n,
                    _ => String::new(),
                };
                cache.lock().unwrap().insert(ip, name);
                inflight.lock().unwrap().remove(&ip);
            });
        }
        String::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_hex_addr_little_endian() {
        // 0100007F:0050 -> 127.0.0.1:80
        assert_eq!(
            parse_addr("0100007F:0050"),
            Some((Ipv4Addr::new(127, 0, 0, 1), 80))
        );
    }

    #[test]
    fn state_names_cover_common_set() {
        assert_eq!(state_name("01"), "ESTABLISHED");
        assert_eq!(state_name("0A"), "LISTEN");
        assert_eq!(state_name("ff"), "?");
    }

    #[test]
    fn resolver_short_circuits_private() {
        let r = Resolver::new();
        assert_eq!(r.host(Ipv4Addr::new(10, 0, 0, 1)), "");
    }

    #[test]
    fn enumerating_connections_does_not_panic() {
        let _ = connections();
    }
}
