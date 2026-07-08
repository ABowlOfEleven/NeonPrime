//! Local user accounts and Administrators membership for the Users panel.
//!
//! Listing is unelevated via `Get-LocalUser` / `Get-LocalGroupMember`. Changes
//! (enable/disable, admin membership, password-never-expires) need admin and run
//! through the elevated shell. Resetting a password is delegated to a `net user`
//! console prompt so the app never handles the password itself.

use std::process::Command;

pub struct LocalUser {
    pub name: String,
    pub full_name: String,
    pub description: String,
    pub enabled: bool,
    /// Member of the local Administrators group.
    pub is_admin: bool,
    pub never_expires: bool,
}

fn esc(s: &str) -> String {
    s.replace('\'', "''")
}

/// All local user accounts, with Administrators membership flagged (unelevated).
pub fn list() -> Vec<LocalUser> {
    let admins = admin_members();
    let out = Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "Get-LocalUser | ForEach-Object { \
             \"$($_.Name)`t$($_.FullName)`t$($_.Enabled)`t$($null -eq $_.PasswordExpires)`t$($_.Description)\" }",
        ])
        .output();
    let Ok(o) = out else { return Vec::new() };
    if !o.status.success() {
        return Vec::new();
    }
    String::from_utf8_lossy(&o.stdout)
        .lines()
        .filter_map(|line| {
            let mut p = line.splitn(5, '\t');
            let name = p.next()?.trim().to_string();
            let full_name = p.next().unwrap_or("").trim().to_string();
            let enabled = p.next().unwrap_or("").trim() == "True";
            let never_expires = p.next().unwrap_or("").trim() == "True";
            let description = p.next().unwrap_or("").trim().to_string();
            if name.is_empty() {
                return None;
            }
            let is_admin = admins.iter().any(|a| a.eq_ignore_ascii_case(&name));
            Some(LocalUser {
                name,
                full_name,
                description,
                enabled,
                is_admin,
                never_expires,
            })
        })
        .collect()
}

/// Short names of the current Administrators-group members (unelevated read).
fn admin_members() -> Vec<String> {
    let out = Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "Get-LocalGroupMember -Group Administrators -ErrorAction SilentlyContinue | \
             ForEach-Object { ($_.Name -split '\\\\')[-1] }",
        ])
        .output();
    let Ok(o) = out else { return Vec::new() };
    String::from_utf8_lossy(&o.stdout)
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect()
}

/// Elevated PowerShell to enable or disable an account.
pub fn enable_script(name: &str, enable: bool) -> String {
    let verb = if enable {
        "Enable-LocalUser"
    } else {
        "Disable-LocalUser"
    };
    format!("{verb} -Name '{}'; Write-Host 'Done.'", esc(name))
}

/// Elevated PowerShell to add or remove an account from Administrators.
pub fn admin_script(name: &str, admin: bool) -> String {
    let verb = if admin {
        "Add-LocalGroupMember"
    } else {
        "Remove-LocalGroupMember"
    };
    format!(
        "{verb} -Group Administrators -Member '{}'; Write-Host 'Done.'",
        esc(name)
    )
}

/// Elevated PowerShell to toggle the password-never-expires flag.
pub fn expiry_script(name: &str, never: bool) -> String {
    format!(
        "Set-LocalUser -Name '{}' -PasswordNeverExpires ${never}; Write-Host 'Done.'",
        esc(name)
    )
}

/// Script for an elevated, visible console that prompts for a new password with
/// `net user`, so the password is entered into Windows, never into the app.
pub fn reset_password_script(name: &str) -> String {
    format!("net user \"{}\" *", name.replace('"', ""))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scripts_target_and_quote() {
        assert!(enable_script("guest", true).contains("Enable-LocalUser"));
        assert!(enable_script("guest", false).contains("Disable-LocalUser"));
        assert!(admin_script("bob", true).contains("Add-LocalGroupMember"));
        assert!(admin_script("bob", false).contains("Remove-LocalGroupMember"));
        assert!(expiry_script("bob", true).contains("$true"));
        assert!(expiry_script("bob", false).contains("$false"));
        assert!(reset_password_script("bob").contains("net user"));
    }

    #[test]
    fn quotes_are_doubled() {
        assert!(enable_script("o'brien", true).contains("o''brien"));
    }

    #[test]
    fn listing_returns_users() {
        // The built-in Administrator/DefaultAccount always exist on Windows.
        let v = list();
        assert!(!v.is_empty());
    }
}
