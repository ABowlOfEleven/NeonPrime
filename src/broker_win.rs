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
use std::net::{TcpListener, TcpStream};

use neonprime::core::action::Action;
use neonprime::core::engine;
use neonprime::core::ipc::{self, Request, Response};

fn main() {
    let (mut port, mut token) = (0u16, String::new());
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--port" => port = args.next().and_then(|s| s.parse().ok()).unwrap_or(0),
            "--token" => token = args.next().unwrap_or_default(),
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

    // Serve a single client, then exit.
    if let Ok((stream, _)) = listener.accept() {
        let _ = serve(stream, &token);
    }
}

fn serve(stream: TcpStream, token: &str) -> std::io::Result<()> {
    let mut writer = stream.try_clone()?;
    let mut reader = BufReader::new(stream);

    // Handshake: first line must equal the token, else drop the connection.
    // Bounded so an unauthenticated local peer can't stream an endless line and
    // exhaust this (elevated) process's memory before the token is even checked.
    let mut line = String::new();
    (&mut reader).take(ipc::MAX_MSG_BYTES).read_line(&mut line)?;
    if line.trim_end() != token {
        return Ok(());
    }

    while let Some(req) = ipc::read_msg::<_, Request>(&mut reader)? {
        let resp = handle(req);
        ipc::write_msg(&mut writer, &resp)?;
    }
    Ok(())
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
