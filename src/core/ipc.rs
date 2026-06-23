//! Broker IPC: newline-delimited JSON over a localhost TCP connection.
//!
//! The transport is loopback TCP guarded by a one-time token handshake — it
//! works across integrity levels (unelevated UI ↔ elevated broker) without
//! named-pipe DACL plumbing. Token is passed on the broker's command line and
//! is single-use per session.
//!
//! NOTE (hardening TODO): command-line args are world-readable; a future pass
//! should move to a named pipe with an explicit DACL, or pass the token via an
//! inherited handle / stdin instead of argv.

use std::io::{self, BufRead, BufReader, Write};
use std::net::TcpStream;

use serde::{Deserialize, Serialize};

use crate::core::action::{Action, Reversal};

/// UI → broker.
#[derive(Serialize, Deserialize, Debug)]
pub enum Request {
    Ping,
    Apply { label: String, action: Action },
    Revert { reversal: Reversal },
    Shutdown,
}

/// Broker → UI.
#[derive(Serialize, Deserialize, Debug)]
pub enum Response {
    Pong,
    Applied { reversal: Reversal },
    Reverted,
    Error(String),
}

/// Write one JSON message followed by a newline, and flush.
pub fn write_msg<W: Write, T: Serialize>(w: &mut W, msg: &T) -> io::Result<()> {
    let s = serde_json::to_string(msg).map_err(io::Error::other)?;
    w.write_all(s.as_bytes())?;
    w.write_all(b"\n")?;
    w.flush()
}

/// Read one newline-delimited JSON message. `Ok(None)` on clean EOF.
pub fn read_msg<R: BufRead, T: for<'de> Deserialize<'de>>(r: &mut R) -> io::Result<Option<T>> {
    let mut line = String::new();
    if r.read_line(&mut line)? == 0 {
        return Ok(None);
    }
    let msg = serde_json::from_str(line.trim_end()).map_err(io::Error::other)?;
    Ok(Some(msg))
}

/// Client end of the broker connection, held by the UI.
pub struct Client {
    writer: TcpStream,
    reader: BufReader<TcpStream>,
}

impl Client {
    /// Connect to a broker on `127.0.0.1:port` and complete the token handshake.
    pub fn connect(port: u16, token: &str) -> io::Result<Client> {
        let stream = TcpStream::connect(("127.0.0.1", port))?;
        let mut writer = stream.try_clone()?;
        writer.write_all(token.as_bytes())?;
        writer.write_all(b"\n")?;
        writer.flush()?;
        let reader = BufReader::new(stream);
        Ok(Client { writer, reader })
    }

    /// Send a request and await the single response.
    pub fn call(&mut self, req: &Request) -> io::Result<Response> {
        write_msg(&mut self.writer, req)?;
        match read_msg::<_, Response>(&mut self.reader)? {
            Some(r) => Ok(r),
            None => Err(io::Error::other("broker closed the connection")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::action::{Action, Hive, RegValue};

    #[test]
    fn request_roundtrips_through_json() {
        let req = Request::Apply {
            label: "demo".into(),
            action: Action::SetReg {
                hive: Hive::Hklm,
                path: "Software\\X".into(),
                name: "Y".into(),
                value: RegValue::Dword(3),
            },
        };
        let s = serde_json::to_string(&req).unwrap();
        let back: Request = serde_json::from_str(&s).unwrap();
        match back {
            Request::Apply { action, .. } => assert!(action.needs_elevation()),
            _ => panic!("wrong variant"),
        }
    }
}
