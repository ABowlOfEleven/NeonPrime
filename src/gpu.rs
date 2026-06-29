//! Vendor-neutral GPU info via DXGI (NVIDIA / AMD / Intel, no vendor SDK) plus
//! GPU utilization via the PDH "GPU Engine" performance counter (also
//! vendor-neutral — the same source Task Manager uses).

use windows::core::PCWSTR;
use windows::Win32::Graphics::Dxgi::*;
use windows::Win32::System::Performance::*;

#[derive(Default, Clone)]
pub struct GpuInfo {
    pub name: String,
    pub vram_total: u64,
}

/// Query the primary GPU (the adapter with the most dedicated VRAM) via DXGI:
/// name and total dedicated VRAM. (Live *usage* comes from PDH below —
/// `QueryVideoMemoryInfo` only reports the calling process's usage.)
pub fn query() -> Option<GpuInfo> {
    unsafe {
        let factory: IDXGIFactory1 = CreateDXGIFactory1().ok()?;
        let mut best: Option<(IDXGIAdapter1, DXGI_ADAPTER_DESC1)> = None;

        let mut i = 0u32;
        loop {
            let adapter = match factory.EnumAdapters1(i) {
                Ok(a) => a,
                Err(_) => break,
            };
            i += 1;

            let desc = match adapter.GetDesc1() {
                Ok(d) => d,
                Err(_) => continue,
            };
            // Skip the software/WARP adapter.
            if (desc.Flags & DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32) != 0 {
                continue;
            }
            let better = match &best {
                Some((_, bd)) => desc.DedicatedVideoMemory > bd.DedicatedVideoMemory,
                None => true,
            };
            if better {
                best = Some((adapter, desc));
            }
        }

        let (_adapter, desc) = best?;
        let name = String::from_utf16_lossy(&desc.Description)
            .trim_end_matches('\0')
            .trim()
            .to_string();
        Some(GpuInfo {
            name,
            vram_total: desc.DedicatedVideoMemory as u64,
        })
    }
}

const PDH_MORE_DATA_U32: u32 = 0x800007D2;

/// Sum a PDH counter's instances as doubles. Returns `Some(0.0)` when there are
/// no instances (idle), `None` on error.
unsafe fn read_sum(counter: isize) -> Option<f64> {
    let mut size: u32 = 0;
    let mut count: u32 = 0;
    let probe = PdhGetFormattedCounterArrayW(counter, PDH_FMT_DOUBLE, &mut size, &mut count, None);
    if probe != PDH_MORE_DATA_U32 {
        return Some(0.0);
    }
    let mut buf = vec![0u8; size as usize];
    let res = PdhGetFormattedCounterArrayW(
        counter,
        PDH_FMT_DOUBLE,
        &mut size,
        &mut count,
        Some(buf.as_mut_ptr() as *mut PDH_FMT_COUNTERVALUE_ITEM_W),
    );
    if res != 0 {
        return None;
    }
    let items = std::slice::from_raw_parts(
        buf.as_ptr() as *const PDH_FMT_COUNTERVALUE_ITEM_W,
        count as usize,
    );
    let mut sum = 0.0f64;
    for it in items {
        let v = it.FmtValue.Anonymous.doubleValue;
        if v.is_finite() && v > 0.0 {
            sum += v;
        }
    }
    Some(sum)
}

/// Live PDH counters for vendor-neutral GPU load + dedicated VRAM usage — the
/// same sources Task Manager reads, so they work for NVIDIA / AMD / Intel.
pub struct GpuCounters {
    query: isize,
    util: isize,
    mem: isize,
    ready: bool,
}

impl GpuCounters {
    pub fn new() -> Self {
        unsafe {
            let mut query: isize = 0;
            let mut util: isize = 0;
            let mut mem: isize = 0;
            let mut ready = false;

            if PdhOpenQueryW(PCWSTR::null(), 0, &mut query) == 0 {
                let util_path: Vec<u16> = "\\GPU Engine(*engtype_3D)\\Utilization Percentage\0"
                    .encode_utf16()
                    .collect();
                let mem_path: Vec<u16> = "\\GPU Adapter Memory(*)\\Dedicated Usage\0"
                    .encode_utf16()
                    .collect();
                let u_ok =
                    PdhAddEnglishCounterW(query, PCWSTR(util_path.as_ptr()), 0, &mut util) == 0;
                let m_ok =
                    PdhAddEnglishCounterW(query, PCWSTR(mem_path.as_ptr()), 0, &mut mem) == 0;
                if u_ok || m_ok {
                    PdhCollectQueryData(query); // prime
                    ready = true;
                }
            }
            GpuCounters {
                query,
                util,
                mem,
                ready,
            }
        }
    }

    /// `(utilization 0..1, dedicated VRAM used in bytes)` — either may be `None`.
    pub fn sample(&self) -> (Option<f32>, Option<u64>) {
        if !self.ready {
            return (None, None);
        }
        unsafe {
            if PdhCollectQueryData(self.query) != 0 {
                return (None, None);
            }
            let util = read_sum(self.util).map(|s| (s as f32 / 100.0).clamp(0.0, 1.0));
            let vram = read_sum(self.mem).map(|s| s as u64);
            (util, vram)
        }
    }
}
