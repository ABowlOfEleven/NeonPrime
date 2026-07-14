//! Machine-store certificates and their expiry, for spotting soon-to-expire or
//! already-expired certs. Unelevated read of Cert:\LocalMachine\My.

use super::hidden_command;

pub struct Cert {
    pub subject: String,
    pub issuer: String,
    pub expires: String,
    pub days_left: i64,
    /// 1 ok, 2 expiring within 30 days, 3 expired.
    pub state: u8,
}

/// The common name from a DN, or the whole string if there is no CN.
fn cn(dn: &str) -> String {
    for part in dn.split(',') {
        if let Some(rest) = part.trim().strip_prefix("CN=") {
            return rest.to_string();
        }
    }
    dn.to_string()
}

/// Personal machine certificates, soonest expiry first (unelevated).
pub fn list() -> Vec<Cert> {
    let out = hidden_command("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "Get-ChildItem Cert:\\LocalMachine\\My | Sort-Object NotAfter | ForEach-Object { \
             $days = [int]($_.NotAfter - (Get-Date)).TotalDays; \
             \"$($_.Subject)`t$($_.Issuer)`t$($_.NotAfter.ToString('yyyy-MM-dd'))`t$days\" }",
        ])
        .output();
    let Ok(o) = out else { return Vec::new() };
    String::from_utf8_lossy(&o.stdout)
        .lines()
        .filter_map(|line| {
            let mut p = line.splitn(4, '\t');
            let subject = cn(p.next()?.trim());
            let issuer = cn(p.next().unwrap_or("").trim());
            let expires = p.next().unwrap_or("").trim().to_string();
            let days_left: i64 = p.next().unwrap_or("0").trim().parse().unwrap_or(0);
            if subject.is_empty() {
                return None;
            }
            let state = if days_left < 0 {
                3
            } else if days_left <= 30 {
                2
            } else {
                1
            };
            Some(Cert {
                subject,
                issuer,
                expires,
                days_left,
                state,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cn_extraction() {
        assert_eq!(cn("CN=web.example.com, O=Acme, C=US"), "web.example.com");
        assert_eq!(cn("no-cn-here"), "no-cn-here");
    }

    #[test]
    fn listing_runs() {
        // May legitimately be empty on a fresh box; just exercise the path.
        let _ = list();
    }
}
