//! Core domain.
//!
//! The Windows backend (everything privileged-but-reversible: registry ops, the
//! rollback journal, the elevated broker IPC, telemetry helpers) is gated to
//! `cfg(windows)`. The Linux backend is a separate, read-mostly module tree.
//! Keeping them split by `cfg` lets the crate compile on both targets from one
//! source; the shared surface (the Slint UI, `sysinfo`, config) sits above this.

// ── Windows backend ─────────────────────────────────────────────────────────
#[cfg(windows)]
pub mod action;
#[cfg(windows)]
pub mod cleanup;
#[cfg(windows)]
pub mod config;
#[cfg(windows)]
pub mod debloat;
#[cfg(windows)]
pub mod dns;
#[cfg(windows)]
pub mod engine;
#[cfg(windows)]
pub mod eventlog;
#[cfg(windows)]
pub mod features;
#[cfg(windows)]
pub mod firewall;
#[cfg(windows)]
pub mod installs;
#[cfg(windows)]
pub mod ipc;
#[cfg(windows)]
pub mod journal;
#[cfg(windows)]
pub mod localusers;
#[cfg(windows)]
pub mod microwin;
#[cfg(windows)]
pub mod modes;
#[cfg(windows)]
pub mod netmon;
#[cfg(windows)]
pub mod power;
#[cfg(windows)]
pub mod privacy;
#[cfg(windows)]
pub mod procmon;
#[cfg(windows)]
pub mod quick;
#[cfg(windows)]
pub mod registry;
#[cfg(windows)]
pub mod repair;
#[cfg(windows)]
pub mod services;
#[cfg(windows)]
pub mod session;
#[cfg(windows)]
pub mod settings;
#[cfg(windows)]
pub mod startup;
#[cfg(windows)]
pub mod tweaks;

// ── Linux backend (scaffold) ────────────────────────────────────────────────
#[cfg(target_os = "linux")]
pub mod linux;
