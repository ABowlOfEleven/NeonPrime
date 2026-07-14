# NeonPrime PowerShell profile - online installer.
#
# Run via:  irm https://raw.githubusercontent.com/ABowlOfEleven/NeonPrime/main/profile/setup.ps1 | iex
#
# NeonPrime's "Install PowerShell profile" button launches this in a Windows
# Terminal pwsh tab (matching WinUtil's delivery). It is fetched, not run from a
# staged file, on purpose: Windows Terminal is a singleton, so a new-tab does not
# inherit the launcher's elevation, environment, or a local path. Running here, in
# the user's own terminal, also means the profile and modules land in the
# LOGGED-IN user's scope, not an elevating admin's (the cross-machine bug in the
# old staged, elevated installer).

$ErrorActionPreference = 'Stop'
$base = 'https://raw.githubusercontent.com/ABowlOfEleven/NeonPrime/main/profile'
Write-Host ''
Write-Host '  NeonPrime PowerShell profile installer' -ForegroundColor Cyan
Write-Host ''

# 1. Download the profile to a stable per-user location and de-MOTW it.
$dir = Join-Path $env:LOCALAPPDATA 'NeonPrime'
New-Item -ItemType Directory -Force -Path $dir | Out-Null
$dest = Join-Path $dir 'NeonPrime.profile.ps1'
Write-Host '  Downloading profile...' -ForegroundColor Gray
Invoke-WebRequest -Uri "$base/NeonPrime.profile.ps1" -OutFile $dest -UseBasicParsing
try { Unblock-File -Path $dest -ErrorAction SilentlyContinue } catch {}

# 2. Let profiles run. Skip if an effective policy already allows scripts (setting
#    it under a stricter machine GPO just throws a noisy override error).
$effective = Get-ExecutionPolicy
if ($effective -in @('RemoteSigned', 'Unrestricted', 'Bypass')) {
    Write-Host "  Execution policy already allows local scripts ($effective)." -ForegroundColor Green
} else {
    try {
        Set-ExecutionPolicy -Scope CurrentUser -ExecutionPolicy RemoteSigned -Force -ErrorAction Stop
        Write-Host '  Execution policy (CurrentUser) -> RemoteSigned' -ForegroundColor Green
    } catch {
        Write-Host "  Could not relax execution policy ($effective); a machine policy may enforce it." -ForegroundColor Yellow
    }
}

# 3. Prerequisites, all user scope (no elevation needed here).
function Install-WingetPkg($id) {
    Write-Host "  - $id" -ForegroundColor Gray
    try { winget install --id $id -e --source winget --accept-source-agreements --accept-package-agreements --silent 2>&1 | Out-Null } catch {}
}
Write-Host '  Installing prerequisites...' -ForegroundColor Yellow
Install-WingetPkg 'JanDeDobbeleer.OhMyPosh'   # prompt
Install-WingetPkg 'ajeetdsouza.zoxide'        # smart cd
Install-WingetPkg 'Microsoft.CascadiaCode'    # Nerd Font for glyphs
foreach ($mod in 'Terminal-Icons', 'PSReadLine') {
    if (-not (Get-Module -ListAvailable -Name $mod)) {
        try { Install-Module -Name $mod -Repository PSGallery -Scope CurrentUser -Force -SkipPublisherCheck } catch {}
    }
}

# 4. Point both shells' $PROFILE at ours, backing up anything already there.
$docs = [Environment]::GetFolderPath('MyDocuments')
$targets = @(
    (Join-Path $docs 'PowerShell\profile.ps1'),        # PowerShell 7 (the target)
    (Join-Path $docs 'WindowsPowerShell\profile.ps1')  # Windows PowerShell 5.1
) | Select-Object -Unique

$line = ". `"$dest`""
foreach ($t in $targets) {
    $d = Split-Path $t
    if (-not (Test-Path $d)) { New-Item -ItemType Directory -Force -Path $d | Out-Null }
    if ((Test-Path $t) -and ((Get-Content $t -Raw) -notlike "*$dest*")) {
        Copy-Item $t "$t.neonprime-backup" -Force
    }
    Set-Content -Path $t -Value $line -Encoding UTF8
    try { Unblock-File -Path $t -ErrorAction SilentlyContinue } catch {}
    Write-Host "  Wrote profile -> $t" -ForegroundColor Green
}

# 5. Point Windows Terminal at an installed Nerd Font so prompt glyphs render.
function Set-TerminalNerdFont {
    $installed = @()
    foreach ($hive in 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Fonts',
        'HKCU:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Fonts') {
        $p = Get-ItemProperty $hive -ErrorAction SilentlyContinue
        if ($p) { $installed += $p.PSObject.Properties.Name }
    }
    $face = $null
    foreach ($f in 'CaskaydiaCove NF', 'CaskaydiaMono NF', 'JetBrainsMono NF', 'CascadiaCode NF', 'Cascadia Code NF') {
        if ($installed | Where-Object { $_ -like "$f*" }) { $face = $f; break }
    }
    if (-not $face) {
        Write-Host '  No Nerd Font found yet; prompt glyphs may look blank until one is installed.' -ForegroundColor DarkGray
        return
    }
    $wtSettings = @(
        "$env:LOCALAPPDATA\Packages\Microsoft.WindowsTerminal_8wekyb3d8bbwe\LocalState\settings.json",
        "$env:LOCALAPPDATA\Packages\Microsoft.WindowsTerminalPreview_8wekyb3d8bbwe\LocalState\settings.json",
        "$env:LOCALAPPDATA\Microsoft\Windows Terminal\settings.json"
    ) | Where-Object { Test-Path $_ }
    foreach ($t in $wtSettings) {
        try {
            $cfg = Get-Content $t -Raw | ConvertFrom-Json
            $cur = $cfg.profiles.defaults.font.face
            if ($cur -and ($cur -match 'NF|Nerd')) { continue }
            Copy-Item $t "$t.neonprime-backup" -Force
            if (-not $cfg.profiles.defaults) {
                $cfg.profiles | Add-Member -NotePropertyName defaults -NotePropertyValue ([pscustomobject]@{}) -Force
            }
            if (-not $cfg.profiles.defaults.font) {
                $cfg.profiles.defaults | Add-Member -NotePropertyName font -NotePropertyValue ([pscustomobject]@{}) -Force
            }
            $cfg.profiles.defaults.font | Add-Member -NotePropertyName face -NotePropertyValue $face -Force
            $cfg | ConvertTo-Json -Depth 32 | Set-Content -Path $t -Encoding UTF8
            Write-Host "  Windows Terminal font -> $face" -ForegroundColor Green
        } catch {
            Write-Host "  Could not auto-set the terminal font; set it to '$face' manually." -ForegroundColor Yellow
        }
    }
}
Write-Host '  Configuring the terminal font...' -ForegroundColor Gray
Set-TerminalNerdFont

Write-Host ''
Write-Host '  Done. Open a NEW tab (or run: . $PROFILE) for it to take effect.' -ForegroundColor Cyan
Write-Host ''
