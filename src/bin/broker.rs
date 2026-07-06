//! NeonPrime privileged broker (Windows-only).
//!
//! The elevated broker is part of the Windows reversible-tweak/UAC model. Its
//! body lives in `src/broker_win.rs` and is spliced in on Windows only; on other
//! platforms this compiles to an inert stub so the crate still builds.
#[cfg(windows)]
include!("../broker_win.rs");

#[cfg(not(windows))]
fn main() {}
