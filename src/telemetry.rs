//! Telemetry sampling for the NeonPrime HUD.
//!
//! CPU + RAM come from `sysinfo`. GPU VRAM + name come from DXGI (vendor-neutral
//! — NVIDIA / AMD / Intel). GPU utilization comes from the PDH "GPU Engine"
//! counter (also vendor-neutral), falling back to NVML. GPU temperature comes
//! from NVML (NVIDIA only). CPU temperature is a best-effort WMI reading on a
//! background thread.

use nvml_wrapper::enum_wrappers::device::TemperatureSensor;
use nvml_wrapper::Nvml;
use sysinfo::System;

use crate::cputemp::CpuTempMonitor;
use crate::gpu::{self, GpuCounters};

/// Bytes per binary gigabyte (GiB) — what users mean by "64 GB of RAM".
const GIB: f64 = 1024.0 * 1024.0 * 1024.0;

#[derive(Default, Clone)]
pub struct Sample {
    pub cpu_ratio: f32,
    pub cpu_text: String,
    pub cpu_temp_ratio: f32,
    pub cpu_temp_text: String,
    pub cpu_temp_warn: bool,
    pub ram_ratio: f32,
    pub ram_text: String,
    pub gpu_available: bool,
    pub gpu_name: String,
    pub gpu_ratio: f32,
    pub gpu_text: String,
    pub vram_ratio: f32,
    pub vram_text: String,
    pub temp_ratio: f32,
    pub temp_text: String,
    pub temp_warn: bool,
    pub uptime_text: String,
}

fn fmt_uptime(secs: u64) -> String {
    let (d, h, m) = (secs / 86400, (secs % 86400) / 3600, (secs % 3600) / 60);
    if d > 0 {
        format!("{d}d {h}h {m}m")
    } else if h > 0 {
        format!("{h}h {m}m")
    } else {
        format!("{m}m")
    }
}

pub struct Telemetry {
    sys: System,
    nvml: Option<Nvml>,
    gpu_counters: GpuCounters,
    cpu_temp: CpuTempMonitor,
}

impl Telemetry {
    pub fn new() -> Self {
        let mut sys = System::new_all();
        sys.refresh_cpu_usage();
        sys.refresh_memory();
        Self {
            sys,
            nvml: Nvml::init().ok(),
            gpu_counters: GpuCounters::new(),
            cpu_temp: CpuTempMonitor::start(),
        }
    }

    pub fn sample(&mut self) -> Sample {
        let mut s = Sample::default();

        // ── CPU + RAM ────────────────────────────────────────────────
        self.sys.refresh_cpu_usage();
        self.sys.refresh_memory();

        let cpu = self.sys.global_cpu_usage();
        s.cpu_ratio = (cpu / 100.0).clamp(0.0, 1.0);
        s.cpu_text = format!("{cpu:.0}%");

        let used = self.sys.used_memory() as f64;
        let total = self.sys.total_memory() as f64;
        if total > 0.0 {
            s.ram_ratio = (used / total) as f32;
            s.ram_text = format!("{:.1} / {:.0}G", used / GIB, total / GIB);
        }

        // ── GPU: name + total via DXGI; util + VRAM-used via PDH ──────
        let dxgi = gpu::query();
        let (mut util, pdh_vram_used) = self.gpu_counters.sample();

        let mut vram_used: Option<u64> = pdh_vram_used.filter(|&u| u > 0);
        let mut vram_total: Option<u64> = None;
        if let Some(info) = &dxgi {
            s.gpu_available = true;
            s.gpu_name = info.name.clone();
            if info.vram_total > 0 {
                vram_total = Some(info.vram_total);
            }
        }

        // ── NVML: temperature, plus name/util/VRAM fallback (NVIDIA) ──
        if let Some(nvml) = &self.nvml {
            if let Ok(dev) = nvml.device_by_index(0) {
                s.gpu_available = true;
                if s.gpu_name.is_empty() {
                    if let Ok(name) = dev.name() {
                        s.gpu_name = name;
                    }
                }
                if util.is_none() {
                    if let Ok(u) = dev.utilization_rates() {
                        util = Some((u.gpu as f32 / 100.0).clamp(0.0, 1.0));
                    }
                }
                if let Ok(mem) = dev.memory_info() {
                    if vram_total.is_none() && mem.total > 0 {
                        vram_total = Some(mem.total);
                    }
                    if vram_used.is_none() {
                        vram_used = Some(mem.used);
                    }
                }
                if let Ok(t) = dev.temperature(TemperatureSensor::Gpu) {
                    s.temp_ratio = (t as f32 / 100.0).clamp(0.0, 1.0);
                    s.temp_text = format!("{t}°C");
                    s.temp_warn = t >= 80;
                }
            }
        }

        match util {
            Some(u) => {
                s.gpu_ratio = u;
                s.gpu_text = format!("{}", (u * 100.0).round() as u32);
            }
            None => s.gpu_text = "N/A".into(),
        }

        if let (Some(u), Some(tot)) = (vram_used, vram_total) {
            s.vram_ratio = (u as f64 / tot as f64) as f32;
            s.vram_text = format!("{:.1} / {:.0}G", u as f64 / GIB, tot as f64 / GIB);
        }

        if !s.gpu_available {
            s.gpu_name = "No GPU".into();
            s.gpu_text = "N/A".into();
            s.vram_text = "N/A".into();
            s.temp_text = "N/A".into();
        } else {
            if s.vram_text.is_empty() {
                s.vram_text = "N/A".into();
            }
            if s.temp_text.is_empty() {
                s.temp_text = "N/A".into(); // non-NVIDIA: no temp source yet
            }
        }

        // ── CPU temperature (best-effort, WMI) ───────────────────────
        match self.cpu_temp.get() {
            Some(c) => {
                s.cpu_temp_ratio = (c / 100.0).clamp(0.0, 1.0);
                s.cpu_temp_text = format!("{c:.0}°C");
                s.cpu_temp_warn = c >= 85.0;
            }
            None => s.cpu_temp_text = "N/A".into(),
        }

        s.uptime_text = fmt_uptime(System::uptime());
        s
    }
}
