//! Best-effort CPU temperature via WMI (`MSAcpi_ThermalZoneTemperature`).
//!
//! WMI needs COM, and COM apartment rules clash with the GUI thread, so the
//! query runs on its own dedicated thread that owns its COM init. The latest
//! reading is cached and read lock-free-ish by the UI.
//!
//! Note: ACPI thermal zones are motherboard-reported and not always the CPU
//! package; accurate per-core temps need a driver (LibreHardwareMonitor). This
//! degrades to `None` when no usable zone is exposed.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Deserialize;
use wmi::{COMLibrary, WMIConnection};

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ThermalZone {
    /// Tenths of a Kelvin.
    current_temperature: u32,
}

pub struct CpuTempMonitor {
    latest: Arc<Mutex<Option<f32>>>,
}

impl CpuTempMonitor {
    pub fn start() -> Self {
        let latest = Arc::new(Mutex::new(None));
        let shared = latest.clone();

        std::thread::spawn(move || {
            // COM is initialized for THIS thread only — no clash with the UI.
            let Ok(com) = COMLibrary::new() else { return };
            let Ok(conn) = WMIConnection::with_namespace_path("root\\WMI", com) else { return };

            loop {
                let reading = conn
                    .raw_query::<ThermalZone>(
                        "SELECT CurrentTemperature FROM MSAcpi_ThermalZoneTemperature",
                    )
                    .ok()
                    .and_then(|zones| zones.into_iter().next())
                    .map(|z| z.current_temperature as f32 / 10.0 - 273.15)
                    .filter(|c| c.is_finite() && *c > 0.0 && *c < 150.0);

                if let Ok(mut g) = shared.lock() {
                    *g = reading;
                }
                std::thread::sleep(Duration::from_secs(3));
            }
        });

        CpuTempMonitor { latest }
    }

    pub fn get(&self) -> Option<f32> {
        self.latest.lock().ok().and_then(|g| *g)
    }
}
