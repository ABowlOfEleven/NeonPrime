//! Core domain: everything privileged-but-reversible flows through here.

pub mod action;
pub mod cleanup;
pub mod config;
pub mod dns;
pub mod debloat;
pub mod engine;
pub mod features;
pub mod firewall;
pub mod installs;
pub mod ipc;
pub mod journal;
pub mod microwin;
pub mod modes;
pub mod netmon;
pub mod power;
pub mod privacy;
pub mod procmon;
pub mod quick;
pub mod registry;
pub mod repair;
pub mod services;
pub mod session;
pub mod settings;
pub mod startup;
pub mod tweaks;
