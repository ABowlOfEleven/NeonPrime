//! Linux telemetry: CPU%, memory, load average, and CPU temperature.
//!
//! CPU and memory come from `sysinfo` (same crate/version the Windows HUD uses).
//! CPU temperature is read straight from `/sys/class/hwmon`, preferring the
//! `coretemp` (Intel) / `k10temp` (AMD) package sensor. GPU telemetry is not part
//! of this scaffold yet (it needs vendor-specific probing: NVML for NVIDIA,
//! `/sys/class/drm` + amdgpu/i915 hwmon for AMD/Intel).

use std::fs;
use std::path::Path;

use sysinfo::System;

#[derive(Default, Clone)]
pub struct Sample {
    /// Global CPU utilization, 0..100.
    pub cpu: f32,
    pub mem_used: u64,
    pub mem_total: u64,
    /// 1-minute load average.
    pub load1: f32,
    /// CPU package temperature in Celsius, if a sensor was found.
    pub cpu_temp: Option<f32>,
}

pub struct Telemetry {
    sys: System,
    /// Cached path to the best CPU temperature input, resolved once.
    temp_path: Option<String>,
}

impl Default for Telemetry {
    fn default() -> Self {
        Self::new()
    }
}

impl Telemetry {
    pub fn new() -> Self {
        let mut sys = System::new();
        sys.refresh_cpu_usage();
        sys.refresh_memory();
        Telemetry {
            sys,
            temp_path: find_cpu_temp_input(),
        }
    }

    pub fn sample(&mut self) -> Sample {
        self.sys.refresh_cpu_usage();
        self.sys.refresh_memory();
        Sample {
            cpu: self.sys.global_cpu_usage(),
            mem_used: self.sys.used_memory(),
            mem_total: self.sys.total_memory(),
            load1: System::load_average().one as f32,
            cpu_temp: self.temp_path.as_deref().and_then(read_temp_c),
        }
    }
}

/// Read a hwmon `tempN_input` (millidegrees C) and convert to Celsius.
fn read_temp_c(path: &str) -> Option<f32> {
    let raw = fs::read_to_string(path).ok()?;
    let milli: f32 = raw.trim().parse().ok()?;
    Some(milli / 1000.0)
}

/// Locate the most CPU-like temperature input under `/sys/class/hwmon`.
/// Prefers a package/Tdie/Tctl label on a known CPU chip; falls back to the
/// first `temp1_input` of a CPU-named chip.
fn find_cpu_temp_input() -> Option<String> {
    let root = Path::new("/sys/class/hwmon");
    let mut fallback: Option<String> = None;
    for entry in fs::read_dir(root).ok()?.flatten() {
        let base = entry.path();
        let chip = fs::read_to_string(base.join("name"))
            .unwrap_or_default()
            .trim()
            .to_string();
        let is_cpu = matches!(chip.as_str(), "coretemp" | "k10temp" | "zenpower")
            || chip.to_lowercase().contains("cpu");
        if !is_cpu {
            continue;
        }
        // Walk tempN_input entries, preferring a package/Tctl/Tdie label.
        for i in 1..=16 {
            let input = base.join(format!("temp{i}_input"));
            if !input.exists() {
                continue;
            }
            let input_str = input.to_string_lossy().to_string();
            let label = fs::read_to_string(base.join(format!("temp{i}_label")))
                .unwrap_or_default()
                .to_lowercase();
            if label.contains("package") || label.contains("tctl") || label.contains("tdie") {
                return Some(input_str);
            }
            if fallback.is_none() {
                fallback = Some(input_str);
            }
        }
    }
    fallback
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_is_sane() {
        let mut t = Telemetry::new();
        let s = t.sample();
        assert!(s.cpu >= 0.0 && s.cpu <= 100.0);
        assert!(s.mem_total >= s.mem_used);
    }
}
