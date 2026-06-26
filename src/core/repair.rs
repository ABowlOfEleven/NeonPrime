//! System repair fixes and Windows Update modes — elevated command scripts run
//! by the Config panel. These aren't part of the reversible action model (they
//! invoke SFC/DISM/netsh/reg), so they run in an elevated PowerShell.

/// `(label, powershell-script)` for the Fixes section. Run in a *visible*
/// elevated console so the user can watch progress (SFC/DISM take minutes).
pub fn fixes() -> &'static [(&'static str, &'static str)] {
    &[
        (
            "Repair system files",
            "Write-Host 'Running DISM then SFC (this takes a while)...'; \
             DISM /Online /Cleanup-Image /RestoreHealth; \
             sfc /scannow; \
             Write-Host 'Repair complete.'",
        ),
        (
            "Reset network",
            "Write-Host 'Resetting the network stack...'; \
             netsh winsock reset; netsh int ip reset; ipconfig /flushdns; \
             Write-Host 'Done. A reboot is recommended.'",
        ),
        (
            "Reset Windows Update",
            "Write-Host 'Resetting Windows Update...'; \
             Stop-Service wuauserv,bits,cryptsvc -Force -ErrorAction SilentlyContinue; \
             Remove-Item \"$env:SystemRoot\\SoftwareDistribution\" -Recurse -Force -ErrorAction SilentlyContinue; \
             Remove-Item \"$env:SystemRoot\\System32\\catroot2\" -Recurse -Force -ErrorAction SilentlyContinue; \
             Start-Service wuauserv,bits,cryptsvc -ErrorAction SilentlyContinue; \
             Write-Host 'Windows Update reset.'",
        ),
    ]
}

/// `(label, powershell-script)` for the Windows Update mode selector. Run hidden
/// (just registry/service changes). "Default" undoes the others.
pub fn update_modes() -> &'static [(&'static str, &'static str)] {
    &[
        (
            "Default",
            "reg delete \"HKLM\\SOFTWARE\\Policies\\Microsoft\\Windows\\WindowsUpdate\" /f 2>$null; \
             sc.exe config wuauserv start= demand",
        ),
        (
            "Security only",
            "reg add \"HKLM\\SOFTWARE\\Policies\\Microsoft\\Windows\\WindowsUpdate\" /v DeferFeatureUpdates /t REG_DWORD /d 1 /f; \
             reg add \"HKLM\\SOFTWARE\\Policies\\Microsoft\\Windows\\WindowsUpdate\" /v DeferFeatureUpdatesPeriodInDays /t REG_DWORD /d 365 /f; \
             reg add \"HKLM\\SOFTWARE\\Policies\\Microsoft\\Windows\\WindowsUpdate\" /v DeferQualityUpdates /t REG_DWORD /d 0 /f",
        ),
        (
            "Disabled",
            "reg add \"HKLM\\SOFTWARE\\Policies\\Microsoft\\Windows\\WindowsUpdate\\AU\" /v NoAutoUpdate /t REG_DWORD /d 1 /f; \
             sc.exe config wuauserv start= disabled; sc.exe stop wuauserv",
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn catalogs_nonempty() {
        assert_eq!(fixes().len(), 3);
        assert_eq!(update_modes().len(), 3);
        for (l, s) in fixes().iter().chain(update_modes()) {
            assert!(!l.is_empty() && !s.is_empty());
        }
    }
}
