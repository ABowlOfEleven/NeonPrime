//! NeonPrime core library.
//!
//! The reversible action engine, rollback journal, registry operations, and the
//! broker IPC protocol (Windows) plus the Linux system backend, shared by the UI
//! binary and the elevated broker binary.

pub mod core;
pub mod platform;
