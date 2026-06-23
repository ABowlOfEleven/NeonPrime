//! End-to-end test of the broker IPC + action engine, without elevation.
//! Exercises the full path: spawn broker → handshake → apply → verify → revert.
//! Targets the self-owned benign `HKCU\Software\NeonPrime\Test` key only.

use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};

use neonprime::core::action::{Action, Hive, RegValue};
use neonprime::core::ipc::{Client, Request, Response};
use neonprime::core::registry;

#[test]
fn broker_apply_then_revert_roundtrip() {
    let token = "integration-token";
    let mut child = Command::new(env!("CARGO_BIN_EXE_broker"))
        .args(["--port", "0", "--token", token])
        .stdout(Stdio::piped())
        .spawn()
        .expect("spawn broker");

    // Learn the ephemeral port from the broker's READY line.
    let stdout = child.stdout.take().unwrap();
    let mut reader = BufReader::new(stdout);
    let mut line = String::new();
    reader.read_line(&mut line).unwrap();
    let port: u16 = line
        .trim()
        .strip_prefix("READY ")
        .expect("READY line")
        .parse()
        .expect("port");

    let mut client = Client::connect(port, token).expect("connect");
    assert!(matches!(client.call(&Request::Ping).unwrap(), Response::Pong));

    let path = "Software\\NeonPrime\\Test";
    let name = "BrokerRoundtrip";
    let _ = registry::delete(Hive::Hkcu, path, name);

    let action = Action::SetReg {
        hive: Hive::Hkcu,
        path: path.into(),
        name: name.into(),
        value: RegValue::Dword(7),
    };
    let reversal = match client
        .call(&Request::Apply { label: "test".into(), action })
        .unwrap()
    {
        Response::Applied { reversal } => reversal,
        other => panic!("unexpected: {other:?}"),
    };
    assert_eq!(
        registry::read(Hive::Hkcu, path, name).unwrap(),
        Some(RegValue::Dword(7))
    );

    assert!(matches!(
        client.call(&Request::Revert { reversal }).unwrap(),
        Response::Reverted
    ));
    assert_eq!(registry::read(Hive::Hkcu, path, name).unwrap(), None);

    let _ = client.call(&Request::Shutdown);
    let _ = child.wait();
}

#[test]
fn broker_rejects_bad_token() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_broker"))
        .args(["--port", "0", "--token", "the-real-token"])
        .stdout(Stdio::piped())
        .spawn()
        .expect("spawn broker");

    let stdout = child.stdout.take().unwrap();
    let mut reader = BufReader::new(stdout);
    let mut line = String::new();
    reader.read_line(&mut line).unwrap();
    let port: u16 = line.trim().strip_prefix("READY ").unwrap().parse().unwrap();

    // Wrong token: the broker drops the connection, so the call fails.
    let mut client = Client::connect(port, "wrong-token").expect("tcp connect");
    assert!(client.call(&Request::Ping).is_err());

    let _ = child.kill();
    let _ = child.wait();
}
