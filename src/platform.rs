//! Platform facade: the cross-platform seam.
//!
//! Windows keeps its established backend under [`crate::core`] (registry, WMI,
//! DXGI/PDH telemetry, the elevated broker). The Linux backend lives under
//! [`crate::core::linux`] and is surfaced here as [`backend`] so a future
//! OS-neutral UI can depend on one path regardless of target.
//!
//! This is intentionally thin for now. As the Linux UI is wired up, shared
//! traits (a `Telemetry`, `ProcessSource`, `ServiceManager` abstraction the two
//! backends both implement) will land here so the UI code stops being
//! `cfg`-littered.

/// The active OS backend. Only present on platforms that have one wired up.
#[cfg(target_os = "linux")]
pub use crate::core::linux as backend;

/// Short human name for the current platform, handy for UI/about strings.
pub const fn name() -> &'static str {
    if cfg!(windows) {
        "Windows"
    } else if cfg!(target_os = "linux") {
        "Linux"
    } else {
        "unsupported"
    }
}
