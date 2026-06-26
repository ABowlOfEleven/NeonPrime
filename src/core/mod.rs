//! Core domain: everything privileged-but-reversible flows through here.

pub mod action;
pub mod config;
pub mod engine;
pub mod features;
pub mod installs;
pub mod ipc;
pub mod journal;
pub mod modes;
pub mod quick;
pub mod registry;
pub mod repair;
pub mod session;
pub mod settings;
pub mod startup;
pub mod tweaks;
