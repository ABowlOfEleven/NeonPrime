//! Resultant Set of Policy (RSoP): which Group Policy Objects apply and when
//! policy last refreshed, via `gpresult`. Unelevated (current user scope). The
//! full HTML report is produced with `gpresult /h`.

use std::process::Command;

pub struct GpoInfo {
    pub last_refresh: String,
    /// Names of the applied GPOs (computer + user).
    pub applied: Vec<String>,
    /// Full gpresult text (fallback / detail).
    pub raw: String,
}

pub fn info() -> GpoInfo {
    let raw = Command::new("gpresult")
        .args(["/r"])
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default();

    let last_refresh = raw
        .lines()
        .find(|l| l.contains("Last time Group Policy was applied"))
        .and_then(|l| {
            l.find("applied:")
                .map(|i| l[i + "applied:".len()..].trim().to_string())
        })
        .unwrap_or_default();

    GpoInfo {
        last_refresh,
        applied: parse_applied(&raw),
        raw,
    }
}

/// Pull the applied-GPO names out of gpresult /r text. Each "Applied Group Policy
/// Objects" block lists indented names until a blank line.
fn parse_applied(raw: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut collecting = false;
    for line in raw.lines() {
        let t = line.trim();
        if t.starts_with("Applied Group Policy Objects") {
            collecting = true;
            continue;
        }
        if collecting {
            if t.is_empty() {
                collecting = false;
                continue;
            }
            if t.starts_with("---") {
                continue;
            }
            out.push(t.to_string());
        }
    }
    out.sort();
    out.dedup();
    out
}

/// The command + args to write the full RSoP HTML report to `path`.
pub fn export_argv(path: &str) -> Vec<String> {
    vec!["/h".into(), path.into(), "/f".into()]
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
Created on 7/8/2026 at 10:00:00 AM

COMPUTER SETTINGS
------------------
    Last time Group Policy was applied: 7/8/2026 at 9:30:12 AM
    Applied Group Policy Objects
    -----------------------------
        Default Domain Policy
        Security Baseline

    The following GPOs were not applied because they are filtered out
    -----------------------------
        Local Group Policy
";

    #[test]
    fn parses_refresh_and_gpos() {
        let i = {
            // Exercise the pure parsers directly on sample text.
            let last = SAMPLE
                .lines()
                .find(|l| l.contains("Last time Group Policy was applied"))
                .and_then(|l| l.find("applied:").map(|x| l[x + 8..].trim().to_string()))
                .unwrap_or_default();
            (last, parse_applied(SAMPLE))
        };
        assert!(i.0.contains("9:30:12"));
        assert_eq!(i.1, vec!["Default Domain Policy", "Security Baseline"]);
    }

    #[test]
    fn export_argv_shape() {
        let a = export_argv("C:\\r.html");
        assert_eq!(a, vec!["/h", "C:\\r.html", "/f"]);
    }
}
