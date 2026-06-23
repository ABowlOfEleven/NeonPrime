//! Spawning and talking to the broker process from the UI.
//!
//! Two modes:
//!   * unelevated — a plain child process, used for HKCU-only work and tests;
//!   * elevated — launched via `Start-Process -Verb RunAs`, which triggers UAC.
//!
//! The elevated path needs an interactive UAC approval and so cannot be
//! exercised headlessly.

use std::io;
use std::net::TcpListener;
use std::process::Command;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

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

/// Ephemeral, non-cryptographic handshake token (single-use per session).
fn handshake_token() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("np-{}-{}", std::process::id(), nanos)
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

        if elevated {
            // PowerShell RunAs raises the UAC prompt and launches the broker
            // elevated and detached.
            let arglist = format!("'--port','{port}','--token','{token}'");
            let ps = format!(
                "Start-Process -FilePath '{}' -ArgumentList {arglist} -Verb RunAs -WindowStyle Hidden",
                exe.display()
            );
            Command::new("powershell")
                .args(["-NoProfile", "-WindowStyle", "Hidden", "-Command", &ps])
                .spawn()?;
        } else {
            Command::new(&exe)
                .args(["--port", &port.to_string(), "--token", &token])
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
