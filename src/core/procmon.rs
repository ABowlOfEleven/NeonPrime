//! Process & resource monitor — top processes by CPU/RAM plus per-process GPU%
//! and dedicated VRAM, the latter via the same PDH counters Task Manager uses
//! (parsed per `pid_*` instance). Read-only except for `kill`.

use std::collections::HashMap;
use std::process::Command;

use sysinfo::{ProcessesToUpdate, System};
use windows::core::PCWSTR;
use windows::Win32::System::Performance::*;

pub struct Proc {
    pub name: String,
    pub pid: u32,
    pub cpu: f32,  // percent of total CPU (0..100)
    pub mem: u64,  // bytes
    pub gpu: f32,  // percent (0..100, summed 3D engines)
    pub vram: u64, // dedicated bytes
}

const PDH_MORE_DATA_U32: u32 = 0x800007D2;

fn pid_from_instance(name: &str) -> Option<u32> {
    let rest = name.strip_prefix("pid_")?;
    let end = rest.find('_').unwrap_or(rest.len());
    rest[..end].parse().ok()
}

/// Read a PDH per-instance counter, summing values per owning PID.
unsafe fn read_by_pid(counter: isize) -> HashMap<u32, f64> {
    let mut map = HashMap::new();
    let mut size = 0u32;
    let mut count = 0u32;
    if PdhGetFormattedCounterArrayW(counter, PDH_FMT_DOUBLE, &mut size, &mut count, None)
        != PDH_MORE_DATA_U32
    {
        return map;
    }
    let mut buf = vec![0u8; size as usize];
    if PdhGetFormattedCounterArrayW(
        counter,
        PDH_FMT_DOUBLE,
        &mut size,
        &mut count,
        Some(buf.as_mut_ptr() as *mut PDH_FMT_COUNTERVALUE_ITEM_W),
    ) != 0
    {
        return map;
    }
    let items = std::slice::from_raw_parts(
        buf.as_ptr() as *const PDH_FMT_COUNTERVALUE_ITEM_W,
        count as usize,
    );
    for it in items {
        let name = it.szName.to_string().unwrap_or_default();
        if let Some(pid) = pid_from_instance(&name) {
            let v = it.FmtValue.Anonymous.doubleValue;
            if v.is_finite() && v > 0.0 {
                *map.entry(pid).or_insert(0.0) += v;
            }
        }
    }
    map
}

/// Persistent per-process GPU counters (utilization needs deltas between samples).
struct GpuByPid {
    query: isize,
    util: isize,
    mem: isize,
    ready: bool,
}

impl GpuByPid {
    fn new() -> Self {
        unsafe {
            let mut query = 0isize;
            let mut util = 0isize;
            let mut mem = 0isize;
            let mut ready = false;
            if PdhOpenQueryW(PCWSTR::null(), 0, &mut query) == 0 {
                let up: Vec<u16> = "\\GPU Engine(*engtype_3D)\\Utilization Percentage\0"
                    .encode_utf16()
                    .collect();
                let mp: Vec<u16> = "\\GPU Process Memory(*)\\Dedicated Usage\0"
                    .encode_utf16()
                    .collect();
                let u = PdhAddEnglishCounterW(query, PCWSTR(up.as_ptr()), 0, &mut util) == 0;
                let m = PdhAddEnglishCounterW(query, PCWSTR(mp.as_ptr()), 0, &mut mem) == 0;
                if u || m {
                    PdhCollectQueryData(query);
                    ready = true;
                }
            }
            GpuByPid {
                query,
                util,
                mem,
                ready,
            }
        }
    }

    fn sample(&self) -> HashMap<u32, (f32, u64)> {
        let mut out: HashMap<u32, (f32, u64)> = HashMap::new();
        if !self.ready {
            return out;
        }
        unsafe {
            if PdhCollectQueryData(self.query) != 0 {
                return out;
            }
            for (pid, v) in read_by_pid(self.util) {
                out.entry(pid).or_default().0 = (v as f32).min(100.0);
            }
            for (pid, v) in read_by_pid(self.mem) {
                out.entry(pid).or_default().1 = v as u64;
            }
        }
        out
    }
}

pub struct ProcMonitor {
    sys: System,
    gpu: GpuByPid,
}

impl ProcMonitor {
    pub fn new() -> Self {
        let mut sys = System::new();
        sys.refresh_processes(ProcessesToUpdate::All, true);
        ProcMonitor {
            sys,
            gpu: GpuByPid::new(),
        }
    }

    /// Refresh and return the top `limit` processes sorted by CPU.
    pub fn snapshot(&mut self, limit: usize) -> Vec<Proc> {
        self.sys.refresh_processes(ProcessesToUpdate::All, true);
        let gmap = self.gpu.sample();
        let ncpu = self.sys.cpus().len().max(1) as f32;
        let mut procs: Vec<Proc> = self
            .sys
            .processes()
            .values()
            .map(|p| {
                let pid = p.pid().as_u32();
                let (gpu, vram) = gmap.get(&pid).copied().unwrap_or((0.0, 0));
                Proc {
                    name: p.name().to_string_lossy().to_string(),
                    pid,
                    cpu: p.cpu_usage() / ncpu,
                    mem: p.memory(),
                    gpu,
                    vram,
                }
            })
            .collect();
        procs.sort_by(|a, b| {
            b.cpu
                .partial_cmp(&a.cpu)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        procs.truncate(limit);
        procs
    }
}

/// Force-terminate a process by PID. Returns false on failure (e.g. protected).
pub fn kill(pid: u32) -> bool {
    Command::new("taskkill")
        .args(["/F", "/PID", &pid.to_string()])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_pid_from_instance() {
        assert_eq!(
            pid_from_instance("pid_1234_luid_0x0_0x1_phys_0_eng_0_engtype_3D"),
            Some(1234)
        );
        assert_eq!(pid_from_instance("nope"), None);
    }

    #[test]
    fn snapshot_returns_processes() {
        let mut m = ProcMonitor::new();
        let v = m.snapshot(20);
        assert!(!v.is_empty());
        assert!(v.len() <= 20);
    }
}
