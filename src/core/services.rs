//! Windows services manager. Listing (name / display / running / start-type) is
//! unelevated via `Get-Service`; start/stop and changing start-type need admin,
//! so those run through the elevated shell.

use std::process::Command;

pub struct Svc {
    pub name: String,
    pub display: String,
    pub running: bool,
    /// 0 = automatic, 1 = manual, 2 = disabled, 3 = other (boot/system).
    pub startup: u8,
}

fn startup_code(s: &str) -> u8 {
    match s.trim() {
        "Automatic" | "Auto" => 0,
        "Manual" => 1,
        "Disabled" => 2,
        _ => 3,
    }
}

/// All services, sorted by display name (unelevated). Tab-delimited parse.
pub fn list() -> Vec<Svc> {
    let out = Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "Get-Service | Sort-Object DisplayName | ForEach-Object { \
             \"$($_.Name)`t$($_.DisplayName)`t$($_.Status)`t$($_.StartType)\" }",
        ])
        .output();
    let Ok(o) = out else { return Vec::new() };
    if !o.status.success() {
        return Vec::new();
    }
    String::from_utf8_lossy(&o.stdout)
        .lines()
        .filter_map(|line| {
            let mut p = line.split('\t');
            let name = p.next()?.trim().to_string();
            let display = p.next()?.trim().to_string();
            let status = p.next()?.trim();
            let startup = startup_code(p.next().unwrap_or(""));
            if name.is_empty() {
                return None;
            }
            Some(Svc { name, display, running: status == "Running", startup })
        })
        .collect()
}

fn ps_startup(code: i32) -> &'static str {
    match code {
        0 => "Automatic",
        1 => "Manual",
        _ => "Disabled",
    }
}

/// Elevated PowerShell to start / stop a service.
pub fn start_script(name: &str) -> String {
    format!("Start-Service -Name '{}'; Write-Host 'Started.'", name.replace('\'', "''"))
}
pub fn stop_script(name: &str) -> String {
    format!("Stop-Service -Name '{}' -Force; Write-Host 'Stopped.'", name.replace('\'', "''"))
}

/// Elevated PowerShell to change a service's start-type (code 0/1/2).
pub fn startup_script(name: &str, code: i32) -> String {
    format!(
        "Set-Service -Name '{}' -StartupType {}; Write-Host 'Start-type updated.'",
        name.replace('\'', "''"),
        ps_startup(code)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn startup_codes_map() {
        assert_eq!(startup_code("Automatic"), 0);
        assert_eq!(startup_code("Manual"), 1);
        assert_eq!(startup_code("Disabled"), 2);
        assert_eq!(startup_code("Boot"), 3);
    }

    #[test]
    fn scripts_quote_and_target() {
        assert!(start_script("wuauserv").contains("Start-Service"));
        assert!(stop_script("wuauserv").contains("-Force"));
        assert!(startup_script("wuauserv", 2).contains("Disabled"));
    }

    #[test]
    fn listing_returns_services() {
        let v = list();
        assert!(!v.is_empty());
    }
}
