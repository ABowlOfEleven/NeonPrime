//! Network "phoning home" monitor — active IPv4 TCP connections with the owning
//! process, via `GetExtendedTcpTable`. Read-only; no elevation needed.

use std::collections::HashMap;
use std::ffi::c_void;
use std::net::Ipv4Addr;

use windows::Win32::Foundation::BOOL;
use windows::Win32::NetworkManagement::IpHelper::{
    GetExtendedTcpTable, MIB_TCPTABLE_OWNER_PID, TCP_TABLE_OWNER_PID_ALL,
};
use windows::Win32::Networking::WinSock::AF_INET;

pub struct Conn {
    pub proc_name: String,
    pub pid: u32,
    pub remote: String,
    pub state: String,
    /// Full executable path (for firewall rules); empty if inaccessible.
    pub path: String,
}

fn state_name(s: u32) -> &'static str {
    match s {
        1 => "CLOSED",
        2 => "LISTEN",
        3 => "SYN-SENT",
        4 => "SYN-RCVD",
        5 => "ESTABLISHED",
        6 => "FIN-WAIT1",
        7 => "FIN-WAIT2",
        8 => "CLOSE-WAIT",
        9 => "CLOSING",
        10 => "LAST-ACK",
        11 => "TIME-WAIT",
        12 => "DELETE-TCB",
        _ => "?",
    }
}

/// Active outbound IPv4 TCP connections with the owning process. Listeners,
/// loopback, and unconnected sockets are filtered out. Sorted by process name.
pub fn connections() -> Vec<Conn> {
    let mut size: u32 = 0;
    unsafe {
        GetExtendedTcpTable(None, &mut size, BOOL(0), AF_INET.0 as u32, TCP_TABLE_OWNER_PID_ALL, 0);
    }
    if size == 0 {
        return Vec::new();
    }
    let mut buf = vec![0u8; size as usize];
    let ret = unsafe {
        GetExtendedTcpTable(
            Some(buf.as_mut_ptr() as *mut c_void),
            &mut size,
            BOOL(0),
            AF_INET.0 as u32,
            TCP_TABLE_OWNER_PID_ALL,
            0,
        )
    };
    if ret != 0 {
        return Vec::new();
    }

    let table = buf.as_ptr() as *const MIB_TCPTABLE_OWNER_PID;
    let n = unsafe { (*table).dwNumEntries } as usize;
    let rows = unsafe { (*table).table.as_ptr() };

    let names = process_names();
    let mut out = Vec::new();
    for i in 0..n {
        let row = unsafe { &*rows.add(i) };
        // Address DWORDs are stored as in-memory network-order bytes a.b.c.d.
        let remote_ip = Ipv4Addr::from(row.dwRemoteAddr.to_ne_bytes());
        let remote_port = (row.dwRemotePort as u16).swap_bytes();
        // Skip listeners / unconnected / loopback.
        if row.dwState == 2 || remote_port == 0 || remote_ip.is_unspecified() || remote_ip.is_loopback() {
            continue;
        }
        let pid = row.dwOwningPid;
        let (name, path) = names.get(&pid).cloned().unwrap_or_else(|| ("—".into(), String::new()));
        out.push(Conn {
            proc_name: name,
            pid,
            remote: format!("{remote_ip}:{remote_port}"),
            state: state_name(row.dwState).into(),
            path,
        });
    }
    out.sort_by(|a, b| a.proc_name.to_lowercase().cmp(&b.proc_name.to_lowercase()).then(a.remote.cmp(&b.remote)));
    out
}

/// Map every running PID to its process `(name, exe-path)` (best-effort).
fn process_names() -> HashMap<u32, (String, String)> {
    use sysinfo::{ProcessesToUpdate, System};
    let mut sys = System::new();
    sys.refresh_processes(ProcessesToUpdate::All, true);
    sys.processes()
        .iter()
        .map(|(pid, p)| {
            let name = p.name().to_string_lossy().to_string();
            let path = p.exe().map(|e| e.to_string_lossy().to_string()).unwrap_or_default();
            (pid.as_u32(), (name, path))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_names_cover_the_common_set() {
        assert_eq!(state_name(5), "ESTABLISHED");
        assert_eq!(state_name(2), "LISTEN");
        assert_eq!(state_name(99), "?");
    }

    #[test]
    fn enumerating_connections_does_not_panic() {
        // On any networked machine this returns Ok; just exercise the FFI path.
        let _ = connections();
    }
}
