//! NeonPrime privileged broker.
//!
//! Runs (ideally elevated), serves exactly one local client that proves
//! knowledge of a one-time token, executes a whitelisted set of reversible
//! [`Action`]s, and exits. It holds no state — the UI owns the journal.
//!
//!   broker --port <port> --token <token>
//!
//! `--port 0` binds an ephemeral port and prints `READY <port>` to stdout.

use std::io::{BufRead, BufReader};
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
    let mut line = String::new();
    reader.read_line(&mut line)?;
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

/// Minimal guard against obviously-malformed requests. The real allowlist of
/// permitted tweaks is enforced UI-side today; this is defense in depth and
/// will grow into a path/key whitelist.
fn vet(action: &Action) -> Result<(), &'static str> {
    let path = action.reg_path();
    if path.is_empty() {
        return Err("empty registry path");
    }
    if path.contains("..") {
        return Err("path traversal");
    }
    Ok(())
}
