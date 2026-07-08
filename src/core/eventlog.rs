//! Recent Windows event-log entries for the Event Viewer panel.
//!
//! Reads the System and Application logs unelevated via `Get-WinEvent` (the
//! Security log needs admin and is intentionally left out). Results come back
//! tab-delimited and are parsed into [`EventEntry`]. Runs off the UI thread like
//! the other scanners.

use std::process::Command;

pub struct EventEntry {
    /// Short local timestamp, e.g. "07-08 03:21".
    pub time: String,
    /// Normalized severity: 0 = info/other, 1 = warning, 2 = error (or critical).
    pub level: u8,
    pub source: String,
    pub id: i64,
    /// Which log it came from ("System" / "Application").
    pub log: String,
    /// First line of the event message.
    pub message: String,
}

/// The most recent Critical/Error/Warning entries across System + Application,
/// newest first. `max` caps how many events to pull.
pub fn recent(max: u32) -> Vec<EventEntry> {
    // One line per event: time \t level \t source \t id \t log \t first-message-line.
    let script = format!(
        "$ErrorActionPreference='SilentlyContinue'; \
         Get-WinEvent -FilterHashtable @{{ LogName='System','Application'; Level=1,2,3 }} \
         -MaxEvents {max} | ForEach-Object {{ \
           $m = ($_.Message -split \"`r?`n\")[0]; \
           \"$($_.TimeCreated.ToString('MM-dd HH:mm'))`t$($_.Level)`t$($_.ProviderName)`t$($_.Id)`t$($_.LogName)`t$m\" }}"
    );
    let out = Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .output();
    let Ok(o) = out else { return Vec::new() };
    String::from_utf8_lossy(&o.stdout)
        .lines()
        .filter_map(parse_line)
        .collect()
}

fn parse_line(line: &str) -> Option<EventEntry> {
    let mut p = line.splitn(6, '\t');
    let time = p.next()?.trim().to_string();
    let level_raw: i32 = p.next()?.trim().parse().ok()?;
    let source = p.next()?.trim().to_string();
    let id: i64 = p.next()?.trim().parse().unwrap_or(0);
    let log = p.next()?.trim().to_string();
    let message = p.next().unwrap_or("").trim().to_string();
    // Get-WinEvent Level: 1 Critical, 2 Error, 3 Warning, 4 Info, 5 Verbose.
    let level = match level_raw {
        1 | 2 => 2,
        3 => 1,
        _ => 0,
    };
    if source.is_empty() && message.is_empty() {
        return None;
    }
    Some(EventEntry {
        time,
        level,
        source,
        id,
        log,
        message,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_tab_line() {
        let e = parse_line(
            "07-08 03:21\t2\tService Control Manager\t7009\tSystem\tA timeout was reached.",
        )
        .expect("parsed");
        assert_eq!(e.level, 2);
        assert_eq!(e.id, 7009);
        assert_eq!(e.log, "System");
        assert_eq!(e.source, "Service Control Manager");
        assert!(e.message.contains("timeout"));
    }

    #[test]
    fn maps_warning_and_critical() {
        assert_eq!(parse_line("t\t3\ts\t1\tSystem\tm").unwrap().level, 1);
        assert_eq!(parse_line("t\t1\ts\t1\tSystem\tm").unwrap().level, 2);
        assert_eq!(parse_line("t\t4\ts\t1\tSystem\tm").unwrap().level, 0);
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse_line("not enough fields").is_none());
    }
}
