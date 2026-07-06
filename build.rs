fn main() {
    // Each platform has its own Slint UI: the full Windows deck (ui/app.slint)
    // or the Linux UI (ui/linux.slint). Key off the *target* (CARGO_CFG_*), not
    // host cfg, so cross-compiling / `cargo check --target` picks the right one.
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os == "linux" {
        slint_build::compile("ui/linux.slint").unwrap();
    } else {
        slint_build::compile("ui/app.slint").unwrap();
    }

    // Embed the app icon into the Windows executable (taskbar / Explorer).
    // winresource is a Windows-host-only build dep, so gate the code by host cfg
    // and only actually run it when building *for* Windows.
    #[cfg(windows)]
    if target_os == "windows" {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/neonprime.ico");
        if let Err(e) = res.compile() {
            println!("cargo:warning=app icon embed skipped: {e}");
        }
    }
}
