//! Driver & device inventory: signed drivers with version and date, and a flag
//! for problem (error) devices. Unelevated via CIM. Supports a plain-text export.

use std::process::Command;

pub struct Device {
    pub name: String,
    pub class: String,
    pub version: String,
    pub date: String,
    /// Device is reporting a Configuration Manager error (yellow-bang).
    pub problem: bool,
}

/// All signed drivers with version/date, flagging any device currently in error.
pub fn list() -> Vec<Device> {
    let script = "$err = @(Get-CimInstance Win32_PnPEntity | \
         Where-Object { $_.ConfigManagerErrorCode -ne $null -and $_.ConfigManagerErrorCode -ne 0 } | \
         Select-Object -ExpandProperty Name); \
         Get-CimInstance Win32_PnPSignedDriver | Where-Object DeviceName | ForEach-Object { \
           $d = if ($_.DriverDate) { $_.DriverDate.ToString('yyyy-MM-dd') } else { '' }; \
           $p = $err -contains $_.DeviceName; \
           \"$($_.DeviceName)`t$($_.DeviceClass)`t$($_.DriverVersion)`t$d`t$p\" }";
    let out = Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .output();
    let Ok(o) = out else { return Vec::new() };
    let mut devs: Vec<Device> = String::from_utf8_lossy(&o.stdout)
        .lines()
        .filter_map(|line| {
            let mut p = line.splitn(5, '\t');
            let name = p.next()?.trim().to_string();
            if name.is_empty() {
                return None;
            }
            let class = p.next().unwrap_or("").trim().to_string();
            let version = p.next().unwrap_or("").trim().to_string();
            let date = p.next().unwrap_or("").trim().to_string();
            let problem = p.next().unwrap_or("").trim() == "True";
            Some(Device {
                name,
                class,
                version,
                date,
                problem,
            })
        })
        .collect();
    // Problem devices first, then by class, then name.
    devs.sort_by(|a, b| {
        b.problem
            .cmp(&a.problem)
            .then(a.class.cmp(&b.class))
            .then(a.name.cmp(&b.name))
    });
    devs
}

/// Render the inventory as tab-aligned plain text for export.
pub fn to_text(devs: &[Device]) -> String {
    let mut s = String::from("PROBLEM\tCLASS\tVERSION\tDATE\tDEVICE\n");
    for d in devs {
        s.push_str(&format!(
            "{}\t{}\t{}\t{}\t{}\n",
            if d.problem { "YES" } else { "" },
            d.class,
            d.version,
            d.date,
            d.name
        ));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_has_header_and_rows() {
        let devs = vec![Device {
            name: "NVIDIA GPU".into(),
            class: "Display".into(),
            version: "1.2.3".into(),
            date: "2026-01-01".into(),
            problem: true,
        }];
        let t = to_text(&devs);
        assert!(t.starts_with("PROBLEM"));
        assert!(t.contains("YES"));
        assert!(t.contains("NVIDIA GPU"));
    }

    #[test]
    fn listing_runs() {
        // A real box has many signed drivers.
        assert!(!list().is_empty());
    }
}
