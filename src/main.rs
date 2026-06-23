// NeonPrime — a holographic system control deck for Windows.
//
// Phase 0: the app shell + the two-theme visual language (Holographic / HEV),
// proving the Slint stack and the live theme toggle before any privileged work.
//
// On Windows we don't want a console window tagging along with the GUI.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

slint::include_modules!();

fn main() -> Result<(), slint::PlatformError> {
    let app = AppWindow::new()?;
    app.run()
}
