//! Thin, fault-tolerant registry read/write/delete over `winreg`.

use std::io;

use winreg::enums::*;
use winreg::RegKey;

use crate::core::action::{Hive, RegValue};

fn root(hive: Hive) -> RegKey {
    match hive {
        Hive::Hkcu => RegKey::predef(HKEY_CURRENT_USER),
        Hive::Hklm => RegKey::predef(HKEY_LOCAL_MACHINE),
    }
}

/// Read a value, returning `None` if the key or value is absent.
pub fn read(hive: Hive, path: &str, name: &str) -> io::Result<Option<RegValue>> {
    let key = match root(hive).open_subkey(path) {
        Ok(k) => k,
        Err(ref e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e),
    };
    // Probe DWORD first, then string.
    match key.get_value::<u32, _>(name) {
        Ok(v) => Ok(Some(RegValue::Dword(v))),
        Err(_) => match key.get_value::<String, _>(name) {
            Ok(s) => Ok(Some(RegValue::Sz(s))),
            Err(ref e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e),
        },
    }
}

/// Write a value, creating the subkey path if needed.
pub fn write(hive: Hive, path: &str, name: &str, value: &RegValue) -> io::Result<()> {
    let (key, _) = root(hive).create_subkey(path)?;
    match value {
        RegValue::Dword(v) => key.set_value(name, v),
        RegValue::Sz(s) => key.set_value(name, s),
    }
}

/// Delete a value. Absent key or value is treated as success (idempotent).
pub fn delete(hive: Hive, path: &str, name: &str) -> io::Result<()> {
    match root(hive).open_subkey_with_flags(path, KEY_SET_VALUE | KEY_QUERY_VALUE) {
        Ok(key) => match key.delete_value(name) {
            Ok(()) => Ok(()),
            Err(ref e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e),
        },
        Err(ref e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}
