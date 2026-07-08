//! Disk & volume health: per-volume free space and physical-disk SMART/health.
//! Unelevated reads via Get-Volume and Get-PhysicalDisk.

use std::process::Command;

pub struct Volume {
    pub name: String, // drive letter, e.g. "C:"
    pub label: String,
    pub fs: String,
    pub total_gb: i64,
    pub free_gb: i64,
    /// Fraction used (0..1).
    pub used_frac: f32,
}

pub struct PhysDisk {
    pub model: String,
    pub media: String,
    pub size_gb: i64,
    pub health: String,
    /// 0 unknown, 1 healthy, 2 warning, 3 unhealthy.
    pub state: u8,
}

fn ps(cmd: &str) -> String {
    Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", cmd])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default()
}

/// Fixed volumes with a drive letter (unelevated).
pub fn volumes() -> Vec<Volume> {
    let out = ps(
        "Get-Volume | Where-Object DriveLetter | ForEach-Object { \
         \"$($_.DriveLetter)`t$($_.FileSystemLabel)`t$($_.FileSystem)`t\
         $([math]::Round($_.Size/1GB))`t$([math]::Round($_.SizeRemaining/1GB))\" }",
    );
    out.lines()
        .filter_map(|line| {
            let mut p = line.splitn(5, '\t');
            let letter = p.next()?.trim().to_string();
            if letter.is_empty() {
                return None;
            }
            let label = p.next().unwrap_or("").trim().to_string();
            let fs = p.next().unwrap_or("").trim().to_string();
            let total_gb: i64 = p.next().unwrap_or("0").trim().parse().unwrap_or(0);
            let free_gb: i64 = p.next().unwrap_or("0").trim().parse().unwrap_or(0);
            let used_frac = if total_gb > 0 {
                ((total_gb - free_gb) as f32 / total_gb as f32).clamp(0.0, 1.0)
            } else {
                0.0
            };
            Some(Volume {
                name: format!("{letter}:"),
                label,
                fs,
                total_gb,
                free_gb,
                used_frac,
            })
        })
        .collect()
}

/// Physical disks with SMART/health status (unelevated).
pub fn physical() -> Vec<PhysDisk> {
    let out = ps(
        "Get-PhysicalDisk | ForEach-Object { \
         \"$($_.FriendlyName)`t$($_.MediaType)`t$([math]::Round($_.Size/1GB))`t$($_.HealthStatus)\" }",
    );
    out.lines()
        .filter_map(|line| {
            let mut p = line.splitn(4, '\t');
            let model = p.next()?.trim().to_string();
            if model.is_empty() {
                return None;
            }
            let media = p.next().unwrap_or("").trim().to_string();
            let size_gb: i64 = p.next().unwrap_or("0").trim().parse().unwrap_or(0);
            let health = p.next().unwrap_or("").trim().to_string();
            let state = match health.as_str() {
                "Healthy" => 1,
                "Warning" => 2,
                "Unhealthy" => 3,
                _ => 0,
            };
            Some(PhysDisk {
                model,
                media,
                size_gb,
                health,
                state,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_volumes() {
        // Any Windows box has at least the system volume.
        assert!(!volumes().is_empty());
    }

    #[test]
    fn used_fraction_is_sane() {
        for v in volumes() {
            assert!((0.0..=1.0).contains(&v.used_frac));
        }
    }
}
