//! Printer & spooler tools: list printers with status + queued jobs, clear a
//! stuck queue, and restart the Print Spooler. Listing is unelevated; clearing a
//! queue and restarting the spooler run through the elevated shell.

use std::process::Command;

pub struct Printer {
    pub name: String,
    pub status: String,
    pub jobs: i64,
    pub is_default: bool,
}

fn esc(s: &str) -> String {
    s.replace('\'', "''")
}

/// All printers with their status and current queue depth (unelevated).
pub fn list() -> Vec<Printer> {
    let out = Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "$def = (Get-CimInstance Win32_Printer -Filter \"Default=True\" -ErrorAction SilentlyContinue).Name; \
             Get-Printer | ForEach-Object { \
               $j = (Get-PrintJob -PrinterName $_.Name -ErrorAction SilentlyContinue | Measure-Object).Count; \
               \"$($_.Name)`t$($_.PrinterStatus)`t$j`t$($_.Name -eq $def)\" }",
        ])
        .output();
    let Ok(o) = out else { return Vec::new() };
    String::from_utf8_lossy(&o.stdout)
        .lines()
        .filter_map(|line| {
            let mut p = line.splitn(4, '\t');
            let name = p.next()?.trim().to_string();
            let status = p.next().unwrap_or("").trim().to_string();
            let jobs: i64 = p.next().unwrap_or("0").trim().parse().unwrap_or(0);
            let is_default = p.next().unwrap_or("").trim() == "True";
            if name.is_empty() {
                return None;
            }
            Some(Printer {
                name,
                status,
                jobs,
                is_default,
            })
        })
        .collect()
}

/// Elevated PowerShell to purge a printer's queue.
pub fn clear_queue_script(name: &str) -> String {
    format!(
        "Get-PrintJob -PrinterName '{}' -ErrorAction SilentlyContinue | Remove-PrintJob; \
         Write-Host 'Queue cleared.'",
        esc(name)
    )
}

/// Elevated PowerShell to restart the Print Spooler service.
pub fn restart_spooler_script() -> String {
    "Restart-Service -Name Spooler -Force; Write-Host 'Spooler restarted.'".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scripts_target_the_printer() {
        assert!(clear_queue_script("HP LaserJet").contains("HP LaserJet"));
        assert!(clear_queue_script("HP LaserJet").contains("Remove-PrintJob"));
        assert!(restart_spooler_script().contains("Spooler"));
    }

    #[test]
    fn quotes_doubled() {
        assert!(clear_queue_script("O'Brien's").contains("O''Brien''s"));
    }
}
