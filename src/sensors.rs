//! Bridge to the `neonprime-sensors` sidecar (LibreHardwareMonitor).
//!
//! The sidecar writes a JSON snapshot of all sensors to a temp file each second.
//! We poll that file (works even when the sidecar is elevated and we aren't) and
//! pull out the readings we care about — chiefly the CPU package temperature,
//! which needs the elevated LHM driver and so is unavailable any other way.

use std::path::PathBuf;
use std::time::Duration;

use serde::Deserialize;

#[derive(Deserialize)]
struct Reading {
    #[serde(rename = "hwType")]
    hw_type: String,
    name: String,
    #[serde(rename = "type")]
    kind: String,
    value: f32,
}

#[derive(Default)]
pub struct Sensors {
    pub cpu_temp: Option<f32>,
    pub gpu_hotspot: Option<f32>,
}

/// Shared snapshot file, written by the sidecar and read here.
pub fn snapshot_path() -> PathBuf {
    std::env::temp_dir().join("neonprime-sensors.json")
}

/// The sidecar executable, staged in a `sensors` folder beside the app binary.
fn sidecar_exe() -> PathBuf {
    let mut p = std::env::current_exe().unwrap_or_default();
    p.pop();
    p.push("sensors");
    p.push("neonprime-sensors.exe");
    p
}

/// Read the latest sensors, if the snapshot is fresh (< 6s old).
pub fn read() -> Sensors {
    let path = snapshot_path();
    let fresh = std::fs::metadata(&path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.elapsed().ok())
        .map(|e| e < Duration::from_secs(6))
        .unwrap_or(false);
    if !fresh {
        return Sensors::default();
    }
    let Ok(data) = std::fs::read_to_string(&path) else {
        return Sensors::default();
    };
    let readings: Vec<Reading> = serde_json::from_str(&data).unwrap_or_default();

    let mut cpu_pkg: Option<f32> = None;
    let mut cpu_max: Option<f32> = None;
    let mut gpu_hotspot: Option<f32> = None;

    for r in &readings {
        if r.kind == "Temperature" && r.name.contains("Hot Spot") {
            gpu_hotspot = Some(r.value);
        }
        if r.hw_type == "Cpu" && r.kind == "Temperature" && r.value > 0.0 {
            if r.name.contains("Package") || r.name.contains("Tctl") || r.name.contains("Tdie") {
                cpu_pkg = Some(r.value);
            }
            cpu_max = Some(cpu_max.map_or(r.value, |m: f32| m.max(r.value)));
        }
    }

    Sensors {
        cpu_temp: cpu_pkg.or(cpu_max),
        gpu_hotspot,
    }
}

/// Launch the sidecar elevated (UAC) so it can load the LHM driver and report
/// CPU/board temps. It writes to [`snapshot_path`], which [`read`] then polls.
pub fn spawn_elevated() -> std::io::Result<()> {
    let exe = sidecar_exe();
    let out = snapshot_path();
    let ps = format!(
        "Start-Process -FilePath '{}' -ArgumentList '--out','{}' -Verb RunAs -WindowStyle Hidden",
        exe.display(),
        out.display()
    );
    std::process::Command::new("powershell")
        .args(["-NoProfile", "-WindowStyle", "Hidden", "-Command", &ps])
        .spawn()?;
    Ok(())
}
