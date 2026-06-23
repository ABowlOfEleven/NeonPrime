// NeonPrime — a holographic system control deck for Windows.
//
// Phase 1: live telemetry HUD. A 1 Hz timer samples CPU / RAM / GPU / VRAM /
// temperature and pushes the values into the Slint `Sys` global, which the
// gauge and meters bind to.
//
// On Windows we don't want a console window tagging along with the GUI.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod telemetry;

use std::time::Duration;

use slint::{Timer, TimerMode};
use telemetry::{Sample, Telemetry};

slint::include_modules!();

/// Copy a telemetry sample into the UI's `Sys` global.
fn apply(app: &AppWindow, s: &Sample) {
    let sys = app.global::<Sys>();
    sys.set_cpu_ratio(s.cpu_ratio);
    sys.set_cpu_text(s.cpu_text.as_str().into());
    sys.set_ram_ratio(s.ram_ratio);
    sys.set_ram_text(s.ram_text.as_str().into());
    sys.set_gpu_available(s.gpu_available);
    sys.set_gpu_ratio(s.gpu_ratio);
    sys.set_gpu_text(s.gpu_text.as_str().into());
    sys.set_vram_ratio(s.vram_ratio);
    sys.set_vram_text(s.vram_text.as_str().into());
    sys.set_temp_ratio(s.temp_ratio);
    sys.set_temp_text(s.temp_text.as_str().into());
    sys.set_temp_warn(s.temp_warn);
}

fn main() -> Result<(), slint::PlatformError> {
    let app = AppWindow::new()?;

    let mut tele = Telemetry::new();
    // Paint a first reading immediately so the HUD isn't blank on launch.
    apply(&app, &tele.sample());

    let weak = app.as_weak();
    let timer = Timer::default();
    timer.start(TimerMode::Repeated, Duration::from_secs(1), move || {
        if let Some(app) = weak.upgrade() {
            let s = tele.sample();
            apply(&app, &s);
        }
    });

    app.run()
}
