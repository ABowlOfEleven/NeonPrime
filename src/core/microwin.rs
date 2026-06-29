//! MicroWin — build a slimmed, debloated Windows ISO from a stock one.
//!
//! The pipeline mirrors WinUtil's: mount the source ISO, copy it out, mount the
//! `install.wim` (converting from ESD if needed), remove provisioned Appx
//! packages, apply offline registry tweaks, commit, drop in an `autounattend.xml`
//! (requirement-bypass + OOBE skip + local account), then repack with `oscdimg`.
//!
//! Every step needs admin and a lot of disk + time, so the generated script runs
//! in an elevated, visible console. `oscdimg` ships with the Windows ADK
//! Deployment Tools; [`oscdimg_path`] locates it.

use std::path::Path;
use std::process::Command;

pub struct Options {
    pub iso: String,
    pub output: String,
    pub scratch: String,
    pub index: u32,
    pub debloat: bool,
    pub privacy: bool,
    pub bypass: bool,
}

/// Locate `oscdimg.exe` (ADK Deployment Tools), or None if not installed.
pub fn oscdimg_path() -> Option<String> {
    let candidates = [
        "C:\\Program Files (x86)\\Windows Kits\\10\\Assessment and Deployment Kit\\Deployment Tools\\amd64\\Oscdimg\\oscdimg.exe",
        "C:\\Program Files\\Windows Kits\\10\\Assessment and Deployment Kit\\Deployment Tools\\amd64\\Oscdimg\\oscdimg.exe",
    ];
    for c in candidates {
        if Path::new(c).exists() {
            return Some(c.to_string());
        }
    }
    if let Ok(o) = Command::new("where").arg("oscdimg.exe").output() {
        if o.status.success() {
            if let Some(line) = String::from_utf8_lossy(&o.stdout).lines().next() {
                let p = line.trim();
                if !p.is_empty() {
                    return Some(p.to_string());
                }
            }
        }
    }
    None
}

/// Default scratch directory on the system drive.
pub fn default_scratch() -> String {
    let drive = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into());
    format!("{drive}\\NeonPrime-MicroWin")
}

/// Default output ISO path: alongside the source, suffixed `-NeonPrime`.
pub fn default_output(iso: &str) -> String {
    let p = Path::new(iso);
    let dir = p.parent().map(|d| d.to_string_lossy().to_string()).unwrap_or_default();
    let stem = p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "windows".into());
    if dir.is_empty() {
        format!("{stem}-NeonPrime.iso")
    } else {
        format!("{dir}\\{stem}-NeonPrime.iso")
    }
}

/// Provisioned-package DisplayName fragments removed when debloat is on.
const BLOAT: &[&str] = &[
    "Microsoft.BingNews",
    "Microsoft.BingWeather",
    "Microsoft.GamingApp",
    "Microsoft.GetHelp",
    "Microsoft.Getstarted",
    "Microsoft.MicrosoftSolitaireCollection",
    "Microsoft.People",
    "Microsoft.PowerAutomateDesktop",
    "Microsoft.Todos",
    "Microsoft.WindowsAlarms",
    "Microsoft.WindowsFeedbackHub",
    "Microsoft.WindowsMaps",
    "Microsoft.Xbox",
    "Microsoft.ZuneMusic",
    "Microsoft.ZuneVideo",
    "MicrosoftTeams",
    "Microsoft.Copilot",
    "Clipchamp.Clipchamp",
];

/// The autounattend.xml written into the ISO when "bypass requirements" is on:
/// skips TPM/SecureBoot/RAM/CPU checks, the MS-account/OOBE prompts, and creates
/// a local Administrator account named `User` (blank password).
pub const AUTOUNATTEND: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<unattend xmlns="urn:schemas-microsoft-com:unattend" xmlns:wcm="http://schemas.microsoft.com/WMIConfig/2002/State">
  <settings pass="windowsPE">
    <component name="Microsoft-Windows-Setup" processorArchitecture="amd64" publicKeyToken="31bf3856ad364e35" language="neutral" versionScope="nonSxS">
      <RunSynchronous>
        <RunSynchronousCommand wcm:action="add"><Order>1</Order><Path>reg add HKLM\System\Setup\LabConfig /v BypassTPMCheck /t REG_DWORD /d 1 /f</Path></RunSynchronousCommand>
        <RunSynchronousCommand wcm:action="add"><Order>2</Order><Path>reg add HKLM\System\Setup\LabConfig /v BypassSecureBootCheck /t REG_DWORD /d 1 /f</Path></RunSynchronousCommand>
        <RunSynchronousCommand wcm:action="add"><Order>3</Order><Path>reg add HKLM\System\Setup\LabConfig /v BypassRAMCheck /t REG_DWORD /d 1 /f</Path></RunSynchronousCommand>
        <RunSynchronousCommand wcm:action="add"><Order>4</Order><Path>reg add HKLM\System\Setup\LabConfig /v BypassStorageCheck /t REG_DWORD /d 1 /f</Path></RunSynchronousCommand>
        <RunSynchronousCommand wcm:action="add"><Order>5</Order><Path>reg add HKLM\System\Setup\LabConfig /v BypassCPUCheck /t REG_DWORD /d 1 /f</Path></RunSynchronousCommand>
      </RunSynchronous>
      <UserData>
        <ProductKey><Key></Key></ProductKey>
        <AcceptEula>true</AcceptEula>
      </UserData>
    </component>
  </settings>
  <settings pass="oobeSystem">
    <component name="Microsoft-Windows-Shell-Setup" processorArchitecture="amd64" publicKeyToken="31bf3856ad364e35" language="neutral" versionScope="nonSxS">
      <OOBE>
        <HideEULAPage>true</HideEULAPage>
        <HideOEMRegistrationScreen>true</HideOEMRegistrationScreen>
        <HideOnlineAccountScreens>true</HideOnlineAccountScreens>
        <HideWirelessSetupInOOBE>true</HideWirelessSetupInOOBE>
        <ProtectYourPC>3</ProtectYourPC>
      </OOBE>
      <UserAccounts>
        <LocalAccounts>
          <LocalAccount wcm:action="add">
            <Name>User</Name>
            <Group>Administrators</Group>
            <Password><Value></Value><PlainText>true</PlainText></Password>
          </LocalAccount>
        </LocalAccounts>
      </UserAccounts>
    </component>
  </settings>
</unattend>
"#;

/// Generate the elevated PowerShell that performs the full build. `unattend` is
/// the path to the autounattend.xml NeonPrime wrote (used when `bypass` is on).
pub fn build_script(o: &Options, oscdimg: &str, unattend: &str) -> String {
    let mut s = String::new();
    s.push_str("$ErrorActionPreference = 'Stop'\n");
    s.push_str(&format!("$iso = '{}'\n", o.iso.replace('\'', "''")));
    s.push_str(&format!("$out = '{}'\n", o.output.replace('\'', "''")));
    s.push_str(&format!("$work = '{}'\n", o.scratch.replace('\'', "''")));
    s.push_str(&format!("$index = {}\n", o.index));
    s.push_str("$src = Join-Path $work 'src'\n");
    s.push_str("$mnt = Join-Path $work 'mount'\n");
    s.push_str("Write-Host 'NeonPrime MicroWin — preparing workspace...' -ForegroundColor Cyan\n");
    s.push_str("Remove-Item $work -Recurse -Force -ErrorAction SilentlyContinue\n");
    s.push_str("New-Item -ItemType Directory -Force -Path $src,$mnt | Out-Null\n");

    // Mount + copy the source ISO.
    s.push_str("Write-Host 'Mounting source ISO...'\n");
    s.push_str("$img = Mount-DiskImage -ImagePath $iso -PassThru; Start-Sleep 2\n");
    s.push_str("$dl = ($img | Get-Volume).DriveLetter\n");
    s.push_str("Write-Host 'Copying install media (a few minutes)...'\n");
    s.push_str("Copy-Item -Path \"$($dl):\\*\" -Destination $src -Recurse -Force\n");
    s.push_str("Dismount-DiskImage -ImagePath $iso | Out-Null\n");

    // ESD -> WIM if needed.
    s.push_str("$wim = Join-Path $src 'sources\\install.wim'\n");
    s.push_str("if (-not (Test-Path $wim)) {\n");
    s.push_str("  $esd = Join-Path $src 'sources\\install.esd'\n");
    s.push_str("  Write-Host 'Exporting ESD -> WIM...'\n");
    s.push_str("  dism /Export-Image /SourceImageFile:$esd /SourceIndex:$index /DestinationImageFile:$wim /Compress:max /CheckIntegrity\n");
    s.push_str("  $index = 1\n");
    s.push_str("}\n");
    s.push_str("attrib -r $wim\n");

    // Mount the image.
    s.push_str("Write-Host 'Mounting Windows image (slow)...'\n");
    s.push_str("dism /Mount-Image /ImageFile:$wim /Index:$index /MountDir:$mnt\n");

    if o.debloat {
        s.push_str("Write-Host 'Removing bundled apps...' -ForegroundColor Cyan\n");
        let patterns = BLOAT.iter().map(|b| format!("'{b}'")).collect::<Vec<_>>().join(",");
        s.push_str(&format!("$bloat = @({patterns})\n"));
        s.push_str("Get-AppxProvisionedPackage -Path $mnt | ForEach-Object { $p=$_; if ($bloat | Where-Object { $p.DisplayName -like \"*$_*\" }) { dism /Image:$mnt /Remove-ProvisionedAppxPackage /PackageName:$($p.PackageName) } }\n");
    }

    if o.privacy {
        s.push_str("Write-Host 'Applying offline privacy tweaks...' -ForegroundColor Cyan\n");
        s.push_str("reg load HKLM\\zSOFTWARE \"$mnt\\Windows\\System32\\config\\SOFTWARE\" | Out-Null\n");
        s.push_str("reg add \"HKLM\\zSOFTWARE\\Policies\\Microsoft\\Windows\\DataCollection\" /v AllowTelemetry /t REG_DWORD /d 0 /f\n");
        s.push_str("reg add \"HKLM\\zSOFTWARE\\Policies\\Microsoft\\Windows\\WindowsCopilot\" /v TurnOffWindowsCopilot /t REG_DWORD /d 1 /f\n");
        s.push_str("reg add \"HKLM\\zSOFTWARE\\Policies\\Microsoft\\Windows\\CloudContent\" /v DisableWindowsConsumerFeatures /t REG_DWORD /d 1 /f\n");
        s.push_str("reg unload HKLM\\zSOFTWARE | Out-Null\n");
    }

    // Commit + unmount.
    s.push_str("Write-Host 'Committing image (slow)...'\n");
    s.push_str("dism /Unmount-Image /MountDir:$mnt /Commit\n");

    if o.bypass {
        s.push_str("Write-Host 'Injecting autounattend.xml...'\n");
        s.push_str(&format!("Copy-Item '{}' (Join-Path $src 'autounattend.xml') -Force\n", unattend.replace('\'', "''")));
    }

    // Repack with oscdimg (BIOS + UEFI dual boot).
    s.push_str("Write-Host 'Building ISO with oscdimg...' -ForegroundColor Cyan\n");
    s.push_str("$boot = Join-Path $src 'boot\\etfsboot.com'\n");
    s.push_str("$efi = Join-Path $src 'efi\\microsoft\\boot\\efisys.bin'\n");
    s.push_str(&format!(
        "& '{}' -m -o -u2 -udfver102 -bootdata:\"2#p0,e,b$boot#pEF,e,b$efi\" $src $out\n",
        oscdimg.replace('\'', "''")
    ));
    s.push_str("Write-Host \"Done -> $out\" -ForegroundColor Green\n");
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts() -> Options {
        Options {
            iso: "D:\\win11.iso".into(),
            output: "D:\\win11-NeonPrime.iso".into(),
            scratch: "C:\\NeonPrime-MicroWin".into(),
            index: 1,
            debloat: true,
            privacy: true,
            bypass: true,
        }
    }

    #[test]
    fn default_output_suffixes() {
        assert_eq!(default_output("D:\\iso\\win11.iso"), "D:\\iso\\win11-NeonPrime.iso");
    }

    #[test]
    fn script_covers_the_pipeline() {
        let s = build_script(&opts(), "C:\\adk\\oscdimg.exe", "C:\\t\\unattend.xml");
        assert!(s.contains("Mount-DiskImage"));
        assert!(s.contains("dism /Mount-Image"));
        assert!(s.contains("Remove-ProvisionedAppxPackage")); // debloat on
        assert!(s.contains("AllowTelemetry")); // privacy on
        assert!(s.contains("autounattend.xml")); // bypass on
        assert!(s.contains("oscdimg.exe"));
        assert!(s.contains("/Commit"));
    }

    #[test]
    fn options_gate_sections() {
        let mut o = opts();
        o.debloat = false;
        o.privacy = false;
        o.bypass = false;
        let s = build_script(&o, "x", "y");
        assert!(!s.contains("Remove-ProvisionedAppxPackage"));
        assert!(!s.contains("AllowTelemetry"));
        assert!(!s.contains("autounattend.xml"));
    }

    #[test]
    fn autounattend_has_bypass_keys() {
        assert!(AUTOUNATTEND.contains("BypassTPMCheck"));
        assert!(AUTOUNATTEND.contains("BypassSecureBootCheck"));
        assert!(AUTOUNATTEND.contains("LocalAccount"));
    }
}
