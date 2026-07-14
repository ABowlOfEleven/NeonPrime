//! Support Bundle: a one-click machine snapshot to attach to a helpdesk ticket.
//!
//! Gathers security posture, recent event-log errors, services, specs, network
//! config, installed apps, top processes, and drivers into a timestamped folder
//! with an HTML index. Reuses the posture / eventlog / services collectors; the
//! raw text dumps run through a single PowerShell pass.

use super::hidden_command;
use std::path::{Path, PathBuf};

use super::{asset, eventlog, posture, services};

pub struct BundleResult {
    pub path: String,
    pub files: usize,
}

fn ps_capture(cmd: &str) -> String {
    hidden_command("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", cmd])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default()
}

fn write_file(dir: &Path, name: &str, content: &str, count: &mut usize) {
    if std::fs::write(dir.join(name), content).is_ok() {
        *count += 1;
    }
}

fn esc_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Build the whole bundle under the user's profile folder. Returns the folder
/// path and how many files were written.
pub fn generate() -> Result<BundleResult, String> {
    let machine = std::env::var("COMPUTERNAME").unwrap_or_else(|_| "this-pc".into());
    let fs_stamp = {
        let s = ps_capture("(Get-Date).ToString('yyyyMMdd-HHmmss')");
        if s.is_empty() {
            "snapshot".to_string()
        } else {
            s
        }
    };
    let disp_stamp = ps_capture("(Get-Date).ToString('yyyy-MM-dd HH:mm')");

    let base = PathBuf::from(std::env::var("USERPROFILE").unwrap_or_else(|_| ".".into()));
    let dir = base.join(format!("NeonPrime-Support-{machine}-{fs_stamp}"));
    std::fs::create_dir_all(&dir).map_err(|e| format!("create folder: {e}"))?;

    let mut files = 0usize;

    // Reused collectors (in-process).
    let posture = posture::scan();
    write_file(
        &dir,
        "posture.html",
        &posture::report_html(&posture, &machine, &disp_stamp),
        &mut files,
    );

    let events = eventlog::recent(250);
    let mut ev_txt = String::new();
    for e in &events {
        let lvl = match e.level {
            2 => "ERROR",
            1 => "WARN",
            _ => "INFO",
        };
        ev_txt.push_str(&format!(
            "{}  [{lvl}]  {} (id {})  {}\n",
            e.time, e.source, e.id, e.message
        ));
    }
    write_file(&dir, "events.txt", &ev_txt, &mut files);

    let svcs = services::list();
    let mut svc_txt = String::new();
    for s in &svcs {
        let run = if s.running { "RUNNING" } else { "stopped" };
        let st = match s.startup {
            0 => "Auto",
            1 => "Manual",
            2 => "Disabled",
            _ => "Other",
        };
        svc_txt.push_str(&format!("{run:8}  {st:9}  {}  ({})\n", s.display, s.name));
    }
    write_file(&dir, "services.txt", &svc_txt, &mut files);

    // Raw text dumps, written straight into the folder by one PowerShell pass.
    let d = dir.to_string_lossy().replace('\'', "''");
    let dump = format!(
        "$d = '{d}'; \
         systeminfo | Out-File -Encoding utf8 (Join-Path $d 'specs.txt'); \
         ipconfig /all | Out-File -Encoding utf8 (Join-Path $d 'network.txt'); \
         Get-Process | Sort-Object WorkingSet -Descending | Select-Object -First 40 Name, Id, \
           @{{n='Mem(MB)';e={{[math]::Round($_.WorkingSet/1MB)}}}} | Format-Table -AutoSize | \
           Out-File -Encoding utf8 (Join-Path $d 'processes.txt'); \
         Get-ItemProperty 'HKLM:\\Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\*', \
           'HKLM:\\Software\\WOW6432Node\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\*' \
           -ErrorAction SilentlyContinue | Where-Object DisplayName | \
           Select-Object DisplayName, DisplayVersion, Publisher | Sort-Object DisplayName | \
           Format-Table -AutoSize | Out-File -Encoding utf8 (Join-Path $d 'installed-apps.txt'); \
         driverquery /v /fo table | Out-File -Encoding utf8 (Join-Path $d 'drivers.txt')"
    );
    let _ = hidden_command("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &dump])
        .output();
    for f in [
        "specs.txt",
        "network.txt",
        "processes.txt",
        "installed-apps.txt",
        "drivers.txt",
    ] {
        if dir.join(f).exists() {
            files += 1;
        }
    }

    // HTML index (the thing you open).
    let asset = asset::info();
    let index = index_html(&machine, &disp_stamp, &asset, &posture, &events);
    write_file(&dir, "report.html", &index, &mut files);

    Ok(BundleResult {
        path: dir.to_string_lossy().to_string(),
        files,
    })
}

fn index_html(
    machine: &str,
    stamp: &str,
    asset: &asset::AssetInfo,
    posture: &[posture::PostureItem],
    events: &[eventlog::EventEntry],
) -> String {
    let (good, warn, bad) = posture::summary(posture);

    let posture_rows = posture
        .iter()
        .map(|i| {
            let color = match i.state {
                1 => "#2e9e5b",
                2 => "#c98a1f",
                3 => "#d24b4a",
                _ => "#888",
            };
            let label = match i.state {
                1 => "OK",
                2 => "WARN",
                3 => "RISK",
                _ => "?",
            };
            format!(
                "<tr><td><b style=\"color:{color}\">{label}</b></td><td>{}</td><td>{}</td></tr>",
                esc_html(&i.name),
                esc_html(&i.status)
            )
        })
        .collect::<String>();

    let event_rows = events
        .iter()
        .take(15)
        .map(|e| {
            let color = if e.level == 2 { "#d24b4a" } else { "#c98a1f" };
            let lvl = if e.level == 2 { "ERROR" } else { "WARN" };
            format!(
                "<tr><td class=\"d\">{}</td><td><b style=\"color:{color}\">{lvl}</b></td><td>{}</td><td>{}</td></tr>",
                esc_html(&e.time),
                esc_html(&e.source),
                esc_html(&e.message)
            )
        })
        .collect::<String>();

    let files = [
        ("specs.txt", "Full systeminfo"),
        ("network.txt", "ipconfig /all"),
        ("installed-apps.txt", "Installed programs"),
        ("processes.txt", "Top processes by memory"),
        ("services.txt", "All services"),
        ("events.txt", "Recent errors + warnings"),
        ("drivers.txt", "Driver inventory"),
        ("posture.html", "Full security posture report"),
    ]
    .iter()
    .map(|(f, desc)| format!("<li><a href=\"{f}\">{f}</a> &mdash; {desc}</li>"))
    .collect::<String>();

    format!(
        "<!doctype html><meta charset=\"utf-8\"><title>NeonPrime support bundle - {machine}</title>\
         <style>body{{font-family:Segoe UI,system-ui,sans-serif;margin:2rem;color:#1b1b1b;max-width:60rem}}\
         h1{{font-size:1.35rem;margin:0}}h2{{font-size:1rem;margin:1.6rem 0 .5rem;color:#333}}\
         .sub{{color:#666;margin:.2rem 0 1rem}}table{{border-collapse:collapse;width:100%;font-size:.9rem}}\
         th,td{{text-align:left;padding:.4rem .6rem;border-bottom:1px solid #e3e3e3;vertical-align:top}}\
         th{{color:#666;font-weight:600;font-size:.75rem;text-transform:uppercase;letter-spacing:.04em}}\
         td.d{{color:#666;white-space:nowrap}}.pill{{display:inline-block;padding:.15rem .5rem;border-radius:.4rem;font-size:.8rem;margin-right:.4rem}}\
         ul{{line-height:1.7}}a{{color:#2456c9}}</style>\
         <h1>NeonPrime support bundle</h1>\
         <div class=\"sub\"><b>{machine}</b> &middot; {stamp} &middot; {model} \
         <span class=\"pill\" style=\"background:#e5f4ea;color:#2e9e5b\">{good} OK</span>\
         <span class=\"pill\" style=\"background:#faf1dd;color:#c98a1f\">{warn} warn</span>\
         <span class=\"pill\" style=\"background:#fadedd;color:#d24b4a\">{bad} risk</span></div>\
         <h2>Asset</h2><table>\
         <tr><td class=\"d\">Manufacturer</td><td>{manu}</td></tr>\
         <tr><td class=\"d\">Model</td><td>{model}</td></tr>\
         <tr><td class=\"d\">Serial / service tag</td><td>{serial}</td></tr></table>\
         <h2>Security posture</h2><table><tr><th>State</th><th>Check</th><th>Status</th></tr>{posture_rows}</table>\
         <h2>Recent errors + warnings (top 15)</h2><table><tr><th>When</th><th>Level</th><th>Source</th><th>Message</th></tr>{event_rows}</table>\
         <h2>Included files</h2><ul>{files}</ul>",
        manu = esc_html(&asset.manufacturer),
        model = esc_html(&asset.model),
        serial = esc_html(&asset.serial),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn index_html_has_sections() {
        let a = asset::AssetInfo {
            manufacturer: "Dell Inc.".into(),
            model: "Latitude 7420".into(),
            serial: "ABC123".into(),
            warranty_url: String::new(),
        };
        let html = index_html("PC1", "2026-07-08 10:00", &a, &[], &[]);
        assert!(html.contains("support bundle"));
        assert!(html.contains("Latitude 7420"));
        assert!(html.contains("Security posture"));
    }
}
