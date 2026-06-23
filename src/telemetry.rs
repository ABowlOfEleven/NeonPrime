// Telemetry sampling for the NeonPrime HUD.
//
// CPU + system RAM come from `sysinfo` (cross-platform). GPU load, VRAM, and
// temperature come from NVIDIA's NVML via `nvml-wrapper`. NVML is loaded lazily
// and every call is fault-tolerant: on a machine without an NVIDIA GPU (or with
// the driver missing) the GPU fields simply report "N/A" instead of failing.

use nvml_wrapper::enum_wrappers::device::TemperatureSensor;
use nvml_wrapper::Nvml;
use sysinfo::System;

/// Bytes per binary gigabyte (GiB) — what users mean by "64 GB of RAM".
const GIB: f64 = 1024.0 * 1024.0 * 1024.0;

/// One snapshot of system vitals, pre-formatted for the UI.
#[derive(Default, Clone)]
pub struct Sample {
    pub cpu_ratio: f32,
    pub cpu_text: String,
    pub ram_ratio: f32,
    pub ram_text: String,
    pub gpu_available: bool,
    pub gpu_ratio: f32,
    pub gpu_text: String,
    pub vram_ratio: f32,
    pub vram_text: String,
    pub temp_ratio: f32,
    pub temp_text: String,
    pub temp_warn: bool,
}

pub struct Telemetry {
    sys: System,
    nvml: Option<Nvml>,
}

impl Telemetry {
    pub fn new() -> Self {
        let mut sys = System::new_all();
        sys.refresh_cpu_usage();
        sys.refresh_memory();
        // NVML may be absent (no NVIDIA GPU / driver) — that's fine, we degrade.
        let nvml = Nvml::init().ok();
        Self { sys, nvml }
    }

    pub fn sample(&mut self) -> Sample {
        let mut s = Sample::default();

        self.sys.refresh_cpu_usage();
        self.sys.refresh_memory();

        let cpu = self.sys.global_cpu_usage(); // 0..100
        s.cpu_ratio = (cpu / 100.0).clamp(0.0, 1.0);
        s.cpu_text = format!("{cpu:.0}%");

        let used = self.sys.used_memory() as f64; // bytes
        let total = self.sys.total_memory() as f64; // bytes
        if total > 0.0 {
            s.ram_ratio = (used / total) as f32;
            s.ram_text = format!("{:.1} / {:.0}G", used / GIB, total / GIB);
        }

        if let Some(nvml) = &self.nvml {
            if let Ok(dev) = nvml.device_by_index(0) {
                s.gpu_available = true;

                if let Ok(u) = dev.utilization_rates() {
                    s.gpu_ratio = (u.gpu as f32 / 100.0).clamp(0.0, 1.0);
                    s.gpu_text = format!("{}", u.gpu);
                }
                if let Ok(mem) = dev.memory_info() {
                    let (vu, vt) = (mem.used as f64, mem.total as f64);
                    if vt > 0.0 {
                        s.vram_ratio = (vu / vt) as f32;
                        s.vram_text = format!("{:.1} / {:.0}G", vu / GIB, vt / GIB);
                    }
                }
                if let Ok(t) = dev.temperature(TemperatureSensor::Gpu) {
                    s.temp_ratio = (t as f32 / 100.0).clamp(0.0, 1.0);
                    s.temp_text = format!("{t}°C");
                    s.temp_warn = t >= 80;
                }
            }
        }

        if !s.gpu_available {
            s.gpu_text = "N/A".into();
            s.vram_text = "N/A".into();
            s.temp_text = "N/A".into();
        }

        s
    }
}
