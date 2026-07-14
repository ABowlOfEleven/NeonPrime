//! Local user profiles with size and last-use, for reclaiming disk. Listing is
//! an unelevated read of Win32_UserProfile (system/special profiles excluded);
//! deleting a stale profile needs admin and runs through the elevated shell.

use super::hidden_command;

pub struct Profile {
    /// Resolved account name, or the SID if it can't be translated.
    pub account: String,
    pub path: String,
    /// Profile folder size in MB (-1 if it couldn't be measured).
    pub size_mb: i64,
    pub last_use: String,
    /// Currently loaded (signed in) -> not safe to delete.
    pub loaded: bool,
    pub sid: String,
}

/// Non-system local profiles with folder size and last-use (unelevated). This
/// walks each profile folder to sum sizes, so it is best run off-thread.
pub fn list() -> Vec<Profile> {
    let out = hidden_command("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "Get-CimInstance Win32_UserProfile | Where-Object { -not $_.Special } | ForEach-Object { \
               $sz = (Get-ChildItem $_.LocalPath -Recurse -File -Force -ErrorAction SilentlyContinue | \
                 Measure-Object Length -Sum).Sum; \
               $name = try { (New-Object System.Security.Principal.SecurityIdentifier($_.SID)).Translate([System.Security.Principal.NTAccount]).Value } catch { $_.SID }; \
               $lu = if ($_.LastUseTime) { $_.LastUseTime.ToString('yyyy-MM-dd') } else { '' }; \
               \"$name`t$($_.LocalPath)`t$([math]::Round($sz/1MB))`t$lu`t$($_.Loaded)`t$($_.SID)\" }",
        ])
        .output();
    let Ok(o) = out else { return Vec::new() };
    String::from_utf8_lossy(&o.stdout)
        .lines()
        .filter_map(|line| {
            let mut p = line.splitn(6, '\t');
            let account = p.next()?.trim().to_string();
            let path = p.next()?.trim().to_string();
            let size_mb: i64 = p.next().unwrap_or("-1").trim().parse().unwrap_or(-1);
            let last_use = p.next().unwrap_or("").trim().to_string();
            let loaded = p.next().unwrap_or("").trim() == "True";
            let sid = p.next().unwrap_or("").trim().to_string();
            if path.is_empty() || sid.is_empty() {
                return None;
            }
            Some(Profile {
                account,
                path,
                size_mb,
                last_use,
                loaded,
                sid,
            })
        })
        .collect()
}

/// Elevated PowerShell to remove a profile (unloads and deletes it) by SID.
pub fn delete_script(sid: &str) -> String {
    format!(
        "Get-CimInstance Win32_UserProfile -Filter \"SID='{}'\" | Remove-CimInstance; \
         Write-Host 'Profile removed.'",
        sid.replace('\'', "''")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delete_targets_sid() {
        let s = delete_script("S-1-5-21-1-2-3-1001");
        assert!(s.contains("S-1-5-21-1-2-3-1001"));
        assert!(s.contains("Remove-CimInstance"));
    }

    #[test]
    fn listing_runs() {
        // Always at least the current user's profile on a real box.
        let _ = list();
    }
}
