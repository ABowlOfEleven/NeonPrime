//! Spawning and talking to the broker process from the UI.
//!
//! Two modes:
//!   * unelevated, a plain child process, used for HKCU-only work and tests;
//!   * elevated, launched via `Start-Process -Verb RunAs`, which triggers UAC.
//!
//! The elevated path needs an interactive UAC approval and so cannot be
//! exercised headlessly.

use super::hidden_command;
use std::io;
use std::net::TcpListener;
use std::time::{Duration, Instant};

use crate::core::ipc::{Client, Request, Response};

pub struct BrokerSession {
    pub client: Client,
    pub elevated: bool,
}

/// Path to `broker.exe` sitting beside the running executable.
fn broker_exe() -> io::Result<std::path::PathBuf> {
    let mut p = std::env::current_exe()?;
    p.pop();
    p.push("broker.exe");
    Ok(p)
}

/// Ephemeral, single-use handshake token: 128 bits from the OS CSPRNG, hex.
///
/// The loopback transport has no ACL, so any local process can `connect()` to
/// the broker's port; this token is the only thing standing between an
/// unprivileged (or low-integrity) local process and the elevated broker. It
/// must therefore be unguessable. The previous `np-<pid>-<nanos>` form was
/// neither secret nor high-entropy (pid is enumerable, the timestamp is
/// observable), so it could be reconstructed without even reading the command
/// line. (The token is still passed via argv, which is world-readable; closing
/// that residual is the named-pipe + DACL migration tracked separately.)
fn handshake_token() -> String {
    let mut buf = [0u8; 16];
    getrandom::getrandom(&mut buf).expect("OS CSPRNG unavailable");
    buf.iter().map(|b| format!("{b:02x}")).collect()
}

/// Grab a currently-free localhost port by binding then immediately releasing.
fn free_port() -> io::Result<u16> {
    let l = TcpListener::bind(("127.0.0.1", 0))?;
    Ok(l.local_addr()?.port())
}

impl BrokerSession {
    /// Spawn a broker and connect to it. `elevated` triggers a UAC prompt.
    pub fn spawn(elevated: bool) -> io::Result<Self> {
        let exe = broker_exe()?;
        let token = handshake_token();
        let port = free_port()?;
        // Pin the broker to this process: the broker refuses any connection whose
        // owning PID is not ours, so a same-user peer that read the token off the
        // (world-readable) command line still cannot stand in for the UI. The PID
        // is not a secret and cannot be forged by an attacker for its own socket.
        let client_pid = std::process::id();

        if elevated {
            // PowerShell RunAs raises the UAC prompt and launches the broker
            // elevated and detached.
            let arglist =
                format!("'--port','{port}','--token','{token}','--client-pid','{client_pid}'");
            let ps = format!(
                "Start-Process -FilePath '{}' -ArgumentList {arglist} -Verb RunAs -WindowStyle Hidden",
                exe.display()
            );
            hidden_command("powershell")
                .args(["-NoProfile", "-WindowStyle", "Hidden", "-Command", &ps])
                .spawn()?;
        } else {
            hidden_command(&exe)
                .args([
                    "--port",
                    &port.to_string(),
                    "--token",
                    &token,
                    "--client-pid",
                    &client_pid.to_string(),
                ])
                .spawn()?;
        }

        // Retry-connect: elevation + UAC can take a while. Bounded so a declined
        // UAC prompt doesn't hang the caller indefinitely.
        let deadline = Instant::now() + Duration::from_secs(12);
        loop {
            match Client::connect(port, &token) {
                Ok(client) => return Ok(BrokerSession { client, elevated }),
                Err(_) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(200));
                }
                Err(e) => return Err(e),
            }
        }
    }

    pub fn ping(&mut self) -> bool {
        matches!(self.client.call(&Request::Ping), Ok(Response::Pong))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_is_high_entropy_and_unpredictable() {
        let a = handshake_token();
        let b = handshake_token();
        assert_eq!(a.len(), 32, "128 bits as hex");
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(a, b, "each session token is unique");
        // Not the old reconstructable pid+timestamp format.
        assert!(!a.starts_with("np-"));
    }
}
