// NeonPrime privileged broker (Windows-only body; `include!`d by src/bin/broker.rs).
//
// Runs (ideally elevated), serves exactly one local client that proves knowledge
// of a one-time token, executes a whitelisted set of reversible Actions, and
// exits. It holds no state; the UI owns the journal.
//
//   broker --port <port> --token <token>
//
// `--port 0` binds an ephemeral port and prints `READY <port>` to stdout.

use std::io::{BufRead, BufReader, Read};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::time::{Duration, Instant};

use neonprime::core::action::Action;
use neonprime::core::engine;
use neonprime::core::ipc::{self, Request, Response};

fn main() {
    let (mut port, mut token) = (0u16, String::new());
    // The PID of the launching UI process. The loopback transport has no ACL, so
    // even a same-user process that read the token off our (world-readable)
    // command line could connect; pinning the accepted connection to the UI's
    // owning PID means such a peer is refused even with a correct token.
    let mut client_pid: u32 = 0;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--port" => port = args.next().and_then(|s| s.parse().ok()).unwrap_or(0),
            "--token" => token = args.next().unwrap_or_default(),
            "--client-pid" => client_pid = args.next().and_then(|s| s.parse().ok()).unwrap_or(0),
            _ => {}
        }
    }
    if token.is_empty() {
        // Bare run (no token): print usage and exit cleanly. Exiting 0 keeps
        // installer/AV validators happy; the real launch always passes a token.
        println!("NeonPrime broker. Launched by the app with --port <port> --token <token>.");
        return;
    }

    let listener = match TcpListener::bind(("127.0.0.1", port)) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("bind failed: {e}");
            std::process::exit(1);
        }
    };
    if let Ok(addr) = listener.local_addr() {
        println!("READY {}", addr.port());
    }

    // Serve the first client that (a) originates from the launching UI process
    // and (b) passes the token handshake, then exit. A peer that fails either
    // check is dropped and we keep accepting until the deadline, so a same-user
    // process that read the argv token cannot stand in for the UI, and a stalled
    // peer cannot pin this elevated process (bounded handshake read in serve).
    let deadline = Instant::now() + Duration::from_secs(30);
    while Instant::now() < deadline {
        match listener.accept() {
            Ok((stream, _)) => {
                if client_pid != 0 && !peer_is(&stream, client_pid) {
                    continue;
                }
                let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
                if let Ok(true) = serve(stream, &token) {
                    break;
                }
            }
            Err(_) => break,
        }
    }
}

/// Returns `Ok(true)` once a peer authenticates and its session ends; `Ok(false)`
/// if the peer stalled or presented a bad token (the caller keeps accepting).
fn serve(stream: TcpStream, token: &str) -> std::io::Result<bool> {
    let mut writer = stream.try_clone()?;
    let mut reader = BufReader::new(stream);

    // Handshake: first line must equal the token, else drop the connection.
    // Bounded in size AND time so an unauthenticated local peer can't stream an
    // endless line or trickle bytes to pin this (elevated) process before the
    // token is even checked.
    let mut line = String::new();
    if (&mut reader)
        .take(ipc::MAX_MSG_BYTES)
        .read_line(&mut line)
        .is_err()
    {
        return Ok(false);
    }
    if line.trim_end() != token {
        return Ok(false);
    }

    // Authenticated: clear the handshake timeout so an idle-but-legitimate UI is
    // not dropped between requests.
    let _ = reader.get_ref().set_read_timeout(None);

    while let Some(req) = ipc::read_msg::<_, Request>(&mut reader)? {
        let resp = handle(req);
        ipc::write_msg(&mut writer, &resp)?;
    }
    Ok(true)
}

/// True if the remote end of `stream` is owned by `expect_pid`. Fail-OPEN: if the
/// peer's owning PID cannot be resolved, the connection is allowed so a resolver
/// hiccup can never lock out the legitimate UI; only a positive mismatch rejects.
/// This is defence in depth layered on top of the token handshake and `vet()`.
fn peer_is(stream: &TcpStream, expect_pid: u32) -> bool {
    match stream.peer_addr() {
        Ok(SocketAddr::V4(a)) => match peer_owner_pid(a.port()) {
            Some(pid) => pid == expect_pid,
            None => true,
        },
        _ => true,
    }
}

/// Resolve the owning PID of the local IPv4 TCP connection whose local port is
/// `local_port` (the peer's source port, from our perspective) via the TCP table.
fn peer_owner_pid(local_port: u16) -> Option<u32> {
    use windows::Win32::NetworkManagement::IpHelper::{
        GetExtendedTcpTable, MIB_TCPTABLE_OWNER_PID, TCP_TABLE_OWNER_PID_CONNECTIONS,
    };
    use windows::Win32::Networking::WinSock::AF_INET;

    unsafe {
        let mut size: u32 = 0;
        // First call sizes the buffer (returns ERROR_INSUFFICIENT_BUFFER).
        let _ = GetExtendedTcpTable(
            None,
            &mut size,
            false,
            AF_INET.0 as u32,
            TCP_TABLE_OWNER_PID_CONNECTIONS,
            0,
        );
        if size == 0 {
            return None;
        }
        // u32-aligned backing store: MIB_TCPROW_OWNER_PID is all u32 fields.
        let mut buf = vec![0u32; (size as usize).div_ceil(4)];
        let rc = GetExtendedTcpTable(
            Some(buf.as_mut_ptr().cast()),
            &mut size,
            false,
            AF_INET.0 as u32,
            TCP_TABLE_OWNER_PID_CONNECTIONS,
            0,
        );
        if rc != 0 {
            return None;
        }
        let table = &*(buf.as_ptr() as *const MIB_TCPTABLE_OWNER_PID);
        let rows = std::slice::from_raw_parts(table.table.as_ptr(), table.dwNumEntries as usize);
        for r in rows {
            // dwLocalPort holds the port big-endian in its low 16 bits.
            if u16::from_be((r.dwLocalPort & 0xffff) as u16) == local_port {
                return Some(r.dwOwningPid);
            }
        }
        None
    }
}

#[cfg(test)]
mod pin_tests {
    use super::*;

    #[test]
    fn peer_owner_pid_resolves_a_local_connection_to_this_process() {
        // Both ends live in the test process, so the client's owning PID must
        // resolve to our own PID. Validates the TCP-table FFI on the real OS.
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let client = TcpStream::connect(("127.0.0.1", port)).unwrap();
        let (server, _) = listener.accept().unwrap();
        let client_src_port = server.peer_addr().unwrap().port();
        assert_eq!(peer_owner_pid(client_src_port), Some(std::process::id()));
        assert!(peer_is(&server, std::process::id()));
        drop(client);
    }
}

fn handle(req: Request) -> Response {
    match req {
        Request::Ping => Response::Pong,
        Request::Apply { action, .. } => {
            if let Err(why) = vet(&action) {
                return Response::Error(format!("rejected: {why}"));
            }
            match engine::apply(&action) {
                Ok(reversal) => Response::Applied { reversal },
                Err(e) => Response::Error(e.to_string()),
            }
        }
        Request::Revert { reversal } => match engine::revert(&reversal) {
            Ok(()) => Response::Reverted,
            Err(e) => Response::Error(e.to_string()),
        },
        Request::Shutdown => std::process::exit(0),
    }
}

/// HKLM subtrees the broker is permitted to modify, derived from the shipped
/// tweak catalog (`core::tweaks` / `core::modes`). Anything outside these is
/// refused, so a peer that completes the handshake still cannot reach
/// autostart/exec keys such as `...\CurrentVersion\Run`, Image File Execution
/// Options, or a service ImagePath. Prefixes ending in `\` match a whole subtree;
/// the rest match an exact key.
const HKLM_ALLOW: &[&str] = &[
    "SOFTWARE\\Microsoft\\PolicyManager\\current\\device\\Start",
    "SOFTWARE\\Microsoft\\Windows Script Host\\Settings",
    "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\CapabilityAccessManager\\ConsentStore\\location",
    "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Device Installer",
    "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\DriverSearching",
    "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Explorer\\FlyoutMenuSettings",
    "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Policies\\Explorer",
    "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Policies\\System",
    "SOFTWARE\\Policies\\",
    "SYSTEM\\CurrentControlSet\\Control\\",
    "SYSTEM\\CurrentControlSet\\Services\\",
    "SYSTEM\\Maps",
];

/// Case-insensitive prefix test (registry paths are ASCII, case-insensitive).
fn starts_with_ci(hay: &str, prefix: &str) -> bool {
    hay.len() >= prefix.len()
        && hay.as_bytes()[..prefix.len()].eq_ignore_ascii_case(prefix.as_bytes())
}

/// Server-side guard on an incoming action. The elevated broker does not trust
/// its caller: it enforces the same allowlist the UI is supposed to, so a peer
/// that completes the handshake still cannot make an arbitrary elevated write.
fn vet(action: &Action) -> Result<(), &'static str> {
    let path = action.reg_path();
    if path.is_empty() {
        return Err("empty registry path");
    }
    if path.contains("..") {
        return Err("path traversal");
    }
    // Never accept a value name that yields code execution (IFEO debugger,
    // service image path, logon shell/userinit), even inside an allowed subtree.
    let name = match action {
        Action::SetReg { name, .. } | Action::DeleteReg { name, .. } => name.as_str(),
    };
    const DANGEROUS_NAMES: &[&str] =
        &["debugger", "imagepath", "servicedll", "shell", "userinit"];
    if DANGEROUS_NAMES.iter().any(|d| name.eq_ignore_ascii_case(d)) {
        return Err("dangerous value name");
    }
    // HKLM writes must land inside the shipped tweak catalog's subtrees. HKCU
    // actions do not need elevation and are left to the caller's own rights.
    if action.needs_elevation() && !HKLM_ALLOW.iter().any(|p| starts_with_ci(path, p)) {
        return Err("registry path outside HKLM allowlist");
    }
    Ok(())
}

#[cfg(test)]
mod vet_tests {
    use super::*;
    use neonprime::core::action::{Action, Hive, RegValue};

    fn hklm(path: &str, name: &str) -> Action {
        Action::SetReg {
            hive: Hive::Hklm,
            path: path.into(),
            name: name.into(),
            value: RegValue::Sz("C:\\Users\\Public\\evil.exe".into()),
        }
    }

    #[test]
    fn refuses_hklm_autostart_and_exec_hijacks() {
        // A peer that completed the handshake must not be able to plant these.
        assert!(vet(&hklm(
            "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Run",
            "NeonPwn"
        ))
        .is_err());
        assert!(vet(&hklm(
            "SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion\\Image File Execution Options\\sethc.exe",
            "Debugger"
        ))
        .is_err());
        assert!(vet(&hklm(
            "SYSTEM\\CurrentControlSet\\Services\\Spooler",
            "ImagePath"
        ))
        .is_err());
    }

    #[test]
    fn allows_shipped_tweak_subtrees() {
        // Representative HKLM paths the shipped tweak catalog actually writes.
        for p in [
            "SOFTWARE\\Policies\\Microsoft\\Windows\\DataCollection",
            "SYSTEM\\CurrentControlSet\\Services\\DiagTrack",
            "SYSTEM\\CurrentControlSet\\Control\\Lsa",
            "SYSTEM\\Maps",
            "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Policies\\System",
        ] {
            assert!(vet(&hklm(p, "Start")).is_ok(), "legit tweak path rejected: {p}");
        }
    }
}
