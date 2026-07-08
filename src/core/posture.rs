//! Read-only security / compliance posture for the Compliance panel and the
//! Support Bundle: Defender, firewall, BitLocker, TPM, Secure Boot, UAC, and how
//! stale the last update is.
//!
//! Everything is a best-effort unelevated read via one PowerShell pass that emits
//! tab-delimited lines (`name \t status \t state \t detail`), so a single process
//! spawn covers the whole board. `state`: 0 unknown, 1 good, 2 warn, 3 bad.

use std::process::Command;

pub struct PostureItem {
    pub name: String,
    /// Short human value, e.g. "On", "Encrypted", "3 profiles on".
    pub status: String,
    /// 0 unknown, 1 good, 2 warn, 3 bad.
    pub state: u8,
    pub detail: String,
}

// One PowerShell pass. Each check is wrapped so a failure emits an "unknown" line
// rather than aborting the batch. `Row name value state detail` helper keeps the
// tab-delimited contract in one place.
const SCRIPT: &str = r#"
$ErrorActionPreference = 'SilentlyContinue'
function Row($n,$v,$s,$d){ "$n`t$v`t$s`t$d" }

# Defender real-time protection + signature age.
try {
  $mp = Get-MpComputerStatus
  if ($mp) {
    $rtp = [bool]$mp.RealTimeProtectionEnabled
    Row 'Defender real-time' ($(if($rtp){'On'}else{'Off'})) ($(if($rtp){1}else{3})) $mp.AMRunningMode
    $age = [int]$mp.AntivirusSignatureAge
    Row 'Defender signatures' ("$age day(s) old") ($(if($age -le 3){1}elseif($age -le 14){2}else{3})) ("version " + $mp.AntivirusSignatureVersion)
  } else { Row 'Defender real-time' 'Unknown' 0 'Get-MpComputerStatus unavailable' }
} catch { Row 'Defender real-time' 'Unknown' 0 'not available' }

# Firewall: how many of the three profiles are enabled.
try {
  $fw = Get-NetFirewallProfile
  $on = ($fw | Where-Object Enabled).Count
  Row 'Firewall' ("$on of 3 profiles on") ($(if($on -eq 3){1}elseif($on -ge 1){2}else{3})) (($fw | ForEach-Object { "$($_.Name)=$([bool]$_.Enabled)" }) -join ', ')
} catch { Row 'Firewall' 'Unknown' 0 'not available' }

# BitLocker on the system drive.
try {
  $bl = Get-BitLockerVolume -MountPoint $env:SystemDrive
  if ($bl) {
    $prot = "$($bl.ProtectionStatus)"
    Row 'BitLocker (system)' ($(if($prot -eq 'On'){'Encrypted'}else{'Off'})) ($(if($prot -eq 'On'){1}else{2})) ("$($bl.VolumeStatus), $($bl.EncryptionMethod)")
  } else { Row 'BitLocker (system)' 'Unknown' 0 'not reported' }
} catch { Row 'BitLocker (system)' 'Unknown' 0 'needs admin or not present' }

# TPM presence / readiness.
try {
  $tpm = Get-Tpm
  if ($tpm) {
    $ok = ([bool]$tpm.TpmPresent -and [bool]$tpm.TpmReady)
    Row 'TPM' ($(if($ok){'Present, ready'}elseif([bool]$tpm.TpmPresent){'Present, not ready'}else{'Not present'})) ($(if($ok){1}elseif([bool]$tpm.TpmPresent){2}else{3})) ''
  } else { Row 'TPM' 'Unknown' 0 'not reported' }
} catch { Row 'TPM' 'Unknown' 0 'not available' }

# Secure Boot (throws on legacy/BIOS boot).
try {
  $sb = Confirm-SecureBootUEFI
  Row 'Secure Boot' ($(if($sb){'On'}else{'Off'})) ($(if($sb){1}else{3})) ''
} catch { Row 'Secure Boot' 'Off / legacy' 2 'not a UEFI Secure Boot system' }

# UAC (EnableLUA).
try {
  $lua = (Get-ItemProperty 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\System').EnableLUA
  Row 'UAC' ($(if($lua -eq 1){'On'}else{'Off'})) ($(if($lua -eq 1){1}else{3})) ''
} catch { Row 'UAC' 'Unknown' 0 'not available' }

# How stale the most recent update is (cheap proxy for patch level).
try {
  $hf = Get-HotFix | Sort-Object InstalledOn -Descending | Select-Object -First 1
  if ($hf -and $hf.InstalledOn) {
    $days = [int]((Get-Date) - $hf.InstalledOn).TotalDays
    Row 'Last update' ("$days day(s) ago") ($(if($days -le 45){1}elseif($days -le 90){2}else{3})) ("$($hf.HotFixID) on " + $hf.InstalledOn.ToString('yyyy-MM-dd'))
  } else { Row 'Last update' 'Unknown' 0 'no hotfix date' }
} catch { Row 'Last update' 'Unknown' 0 'not available' }
"#;

/// Run the posture board. Best-effort: a check that fails shows as "Unknown".
pub fn scan() -> Vec<PostureItem> {
    let out = Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", SCRIPT])
        .output();
    let Ok(o) = out else { return Vec::new() };
    String::from_utf8_lossy(&o.stdout)
        .lines()
        .filter_map(parse_line)
        .collect()
}

fn parse_line(line: &str) -> Option<PostureItem> {
    let mut p = line.splitn(4, '\t');
    let name = p.next()?.trim().to_string();
    let status = p.next()?.trim().to_string();
    let state: u8 = p.next()?.trim().parse().unwrap_or(0);
    let detail = p.next().unwrap_or("").trim().to_string();
    if name.is_empty() {
        return None;
    }
    Some(PostureItem {
        name,
        status,
        state: state.min(3),
        detail,
    })
}

fn esc_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// A self-contained HTML compliance report (audit evidence). `machine` and
/// `stamp` are supplied by the caller so this stays dependency-free.
pub fn report_html(items: &[PostureItem], machine: &str, stamp: &str) -> String {
    let (good, warn, bad) = summary(items);
    let rows = items
        .iter()
        .map(|i| {
            let (label, color) = match i.state {
                1 => ("OK", "#2e9e5b"),
                2 => ("WARN", "#c98a1f"),
                3 => ("RISK", "#d24b4a"),
                _ => ("?", "#888"),
            };
            format!(
                "<tr><td><b style=\"color:{color}\">{label}</b></td><td>{}</td><td>{}</td><td class=\"d\">{}</td></tr>",
                esc_html(&i.name),
                esc_html(&i.status),
                esc_html(&i.detail),
            )
        })
        .collect::<String>();
    format!(
        "<!doctype html><meta charset=\"utf-8\"><title>NeonPrime compliance report</title>\
         <style>body{{font-family:Segoe UI,system-ui,sans-serif;margin:2rem;color:#1b1b1b}}\
         h1{{font-size:1.3rem;margin:0}}.sub{{color:#666;margin:.2rem 0 1.2rem}}\
         table{{border-collapse:collapse;width:100%;font-size:.95rem}}\
         th,td{{text-align:left;padding:.5rem .6rem;border-bottom:1px solid #e3e3e3}}\
         th{{color:#666;font-weight:600;font-size:.8rem;text-transform:uppercase;letter-spacing:.04em}}\
         td.d{{color:#666}}.pill{{display:inline-block;padding:.15rem .5rem;border-radius:.4rem;font-size:.8rem;margin-right:.4rem}}</style>\
         <h1>NeonPrime compliance report</h1>\
         <div class=\"sub\">{machine} &middot; {stamp} &middot; \
         <span class=\"pill\" style=\"background:#e5f4ea;color:#2e9e5b\">{good} OK</span>\
         <span class=\"pill\" style=\"background:#faf1dd;color:#c98a1f\">{warn} warn</span>\
         <span class=\"pill\" style=\"background:#fadedd;color:#d24b4a\">{bad} risk</span></div>\
         <table><tr><th>State</th><th>Check</th><th>Status</th><th>Detail</th></tr>{rows}</table>",
        machine = esc_html(machine),
        stamp = esc_html(stamp),
    )
}

/// A one-line summary: counts of good / warn / bad for headers and reports.
pub fn summary(items: &[PostureItem]) -> (usize, usize, usize) {
    let mut good = 0;
    let mut warn = 0;
    let mut bad = 0;
    for i in items {
        match i.state {
            1 => good += 1,
            2 => warn += 1,
            3 => bad += 1,
            _ => {}
        }
    }
    (good, warn, bad)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_row() {
        let i = parse_line("Firewall\t3 of 3 profiles on\t1\tDomain=True").unwrap();
        assert_eq!(i.name, "Firewall");
        assert_eq!(i.state, 1);
        assert!(i.detail.contains("Domain"));
    }

    #[test]
    fn clamps_and_rejects() {
        assert_eq!(parse_line("X\tY\t9\t").unwrap().state, 3);
        assert!(parse_line("nope").is_none());
    }

    #[test]
    fn summary_counts() {
        let items = vec![
            PostureItem {
                name: "a".into(),
                status: String::new(),
                state: 1,
                detail: String::new(),
            },
            PostureItem {
                name: "b".into(),
                status: String::new(),
                state: 3,
                detail: String::new(),
            },
            PostureItem {
                name: "c".into(),
                status: String::new(),
                state: 2,
                detail: String::new(),
            },
        ];
        assert_eq!(summary(&items), (1, 1, 1));
    }

    #[test]
    fn scan_returns_items() {
        // On any Windows box the batch yields several rows (Defender/firewall/etc.).
        let v = scan();
        assert!(!v.is_empty());
    }
}
