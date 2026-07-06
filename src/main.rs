// NeonPrime — a holographic system control deck.
//
// Windows keeps the full desktop UI (in `app_win.rs`, included below). The Linux
// port is scaffolded: the cross-platform backend lives in the library
// (`neonprime::core::linux` + `neonprime::platform`), and a Linux UI binary is
// the next slice. Until then, the default binary is Windows-only and this file
// is just the entry-point dispatcher.
//
// On Windows we don't want a console window tagging along with the GUI.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

// The entire Windows desktop app. `include!` splices it in at crate root so its
// `fn main`, module declarations, and `slint::include_modules!()` behave exactly
// as if written here, but only when targeting Windows.
#[cfg(windows)]
include!("app_win.rs");

#[cfg(not(windows))]
fn main() {
    // The Slint UI shell and the Linux backend both compile, but they are not
    // wired into a runnable Linux binary yet. Point users at what exists today.
    eprintln!(
        "NeonPrime {}: the desktop UI binary is currently Windows-only.\n\
         The Linux backend (telemetry, processes, network, services, packages, DNS) \
         lives in the library and is being wired into a native Linux UI. \
         See LINUX.md for status.",
        env!("CARGO_PKG_VERSION")
    );
    std::process::exit(2);
}
