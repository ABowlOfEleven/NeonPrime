# Installs the NeonPrime PowerShell profile + prerequisites and points the user's
# $PROFILE (PowerShell 7 first, then Windows PowerShell 5.1) at it.
#
# Run elevated by NeonPrime -> Quick Actions -> "Install PowerShell profile".
# The profile is a CTT/WinUtil-style profile aimed at PowerShell 7.
param([switch]$NoPrereqs)

$ErrorActionPreference = 'Stop'
Write-Host ''
Write-Host '  NeonPrime PowerShell profile installer' -ForegroundColor Cyan
Write-Host ''

$src = Join-Path $PSScriptRoot 'NeonPrime.profile.ps1'
if (-not (Test-Path $src)) { throw "NeonPrime.profile.ps1 not found next to this script ($PSScriptRoot)" }

# 1. Let profiles run. A Restricted/AllSigned policy is the usual reason the
#    profile "fails to load" after install. RemoteSigned allows local scripts.
#    If the *effective* policy already permits scripts (RemoteSigned/Unrestricted/
#    Bypass, often forced by a machine GPO), do nothing: trying to Set it there
#    just throws a noisy "overridden by a policy at a more specific scope" error
#    even though scripts already run fine.
$effective = Get-ExecutionPolicy
if ($effective -in @('RemoteSigned', 'Unrestricted', 'Bypass')) {
    Write-Host "  Execution policy already allows local scripts ($effective)." -ForegroundColor Green
} else {
    try {
        Set-ExecutionPolicy -Scope CurrentUser -ExecutionPolicy RemoteSigned -Force -ErrorAction Stop
        Write-Host '  Execution policy (CurrentUser) -> RemoteSigned' -ForegroundColor Green
    } catch {
        Write-Host "  Execution policy is '$effective' and could not be relaxed (a machine policy may enforce it):" -ForegroundColor Yellow
        Write-Host "    $($_.Exception.Message.Split([Environment]::NewLine)[0])" -ForegroundColor DarkYellow
        Write-Host '    If the profile fails to load, ask an admin to set RemoteSigned, or run pwsh with -ExecutionPolicy Bypass.' -ForegroundColor DarkGray
    }
}

# 2. Strip any mark-of-the-web so the profile is not treated as downloaded.
try { Unblock-File -Path $src -ErrorAction SilentlyContinue } catch {}

function Install-WingetPkg($id) {
    Write-Host "  - $id" -ForegroundColor Gray
    try { winget install --id $id -e --accept-source-agreements --accept-package-agreements --silent 2>&1 | Out-Null } catch {}
}

# Installing a Nerd Font is not enough: Windows Terminal must be told to USE it,
# or the Oh My Posh prompt glyphs render as blank boxes. Point WT's default font
# at an installed Nerd Font (unless the user already set a Nerd Font themselves).
function Set-TerminalNerdFont {
    $installed = @()
    foreach ($hive in 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Fonts',
        'HKCU:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Fonts') {
        $p = Get-ItemProperty $hive -ErrorAction SilentlyContinue
        if ($p) { $installed += $p.PSObject.Properties.Name }
    }
    # Prefer the family the prompt is designed for, then common fallbacks.
    $face = $null
    foreach ($f in 'CaskaydiaCove NF', 'CaskaydiaMono NF', 'JetBrainsMono NF', 'CascadiaCode NF', 'Cascadia Code NF') {
        if ($installed | Where-Object { $_ -like "$f*" }) { $face = $f; break }
    }
    if (-not $face) {
        $any = $installed | Where-Object { $_ -match '\bNF\b|Nerd Font' } | Select-Object -First 1
        if ($any) { $face = ($any -replace '\s+(Regular|Bold|Italic|Light|Medium|SemiBold|Semi|Thin|Extra\w*)\b.*$', '').Trim() }
    }
    if (-not $face) {
        Write-Host '  No Nerd Font found; prompt glyphs may not render until you install one.' -ForegroundColor Yellow
        return
    }

    $targets = @(
        "$env:LOCALAPPDATA\Packages\Microsoft.WindowsTerminal_8wekyb3d8bbwe\LocalState\settings.json",
        "$env:LOCALAPPDATA\Packages\Microsoft.WindowsTerminalPreview_8wekyb3d8bbwe\LocalState\settings.json",
        "$env:LOCALAPPDATA\Microsoft\Windows Terminal\settings.json"
    ) | Where-Object { Test-Path $_ }
    if (-not $targets) {
        Write-Host "  Windows Terminal not found. Set your terminal font to '$face' for prompt glyphs." -ForegroundColor DarkGray
        return
    }

    foreach ($t in $targets) {
        try {
            $cfg = Get-Content $t -Raw | ConvertFrom-Json
            $cur = $cfg.profiles.defaults.font.face
            if ($cur -and ($cur -match 'NF|Nerd')) { continue } # respect an existing Nerd Font choice
            Copy-Item $t "$t.neonprime-backup" -Force
            if (-not $cfg.profiles.defaults) {
                $cfg.profiles | Add-Member -NotePropertyName defaults -NotePropertyValue ([pscustomobject]@{}) -Force
            }
            if (-not $cfg.profiles.defaults.font) {
                $cfg.profiles.defaults | Add-Member -NotePropertyName font -NotePropertyValue ([pscustomobject]@{}) -Force
            }
            $cfg.profiles.defaults.font | Add-Member -NotePropertyName face -NotePropertyValue $face -Force
            $cfg | ConvertTo-Json -Depth 32 | Set-Content -Path $t -Encoding UTF8
            Write-Host "  Windows Terminal font -> $face (backed up settings.json)" -ForegroundColor Green
        } catch {
            Write-Host "  Could not auto-set the terminal font; set it to '$face' manually." -ForegroundColor Yellow
        }
    }
}

if (-not $NoPrereqs) {
    Write-Host '  Installing prerequisites...' -ForegroundColor Yellow
    if (-not (Get-Command pwsh -ErrorAction SilentlyContinue)) { Install-WingetPkg 'Microsoft.PowerShell' }
    Install-WingetPkg 'JanDeDobbeleer.OhMyPosh'      # prompt
    Install-WingetPkg 'ajeetdsouza.zoxide'           # smart cd
    Install-WingetPkg 'Microsoft.CascadiaCode'       # Nerd Font for glyphs

    # Modules must land where the shell that loads the profile will look. Install
    # them in PowerShell 7's scope (its module path differs from 5.1) and also in
    # the current 5.1 session for the 5.1 profile.
    $modScript = {
        Set-ExecutionPolicy -Scope CurrentUser -ExecutionPolicy RemoteSigned -Force -ErrorAction SilentlyContinue
        foreach ($mod in 'Terminal-Icons', 'PSReadLine') {
            if (-not (Get-Module -ListAvailable -Name $mod)) {
                try { Install-Module -Name $mod -Repository PSGallery -Scope CurrentUser -Force -SkipPublisherCheck } catch {}
            }
        }
    }
    Write-Host '  Installing modules (Windows PowerShell)...' -ForegroundColor Gray
    & $modScript
    $pwsh = Get-Command pwsh -ErrorAction SilentlyContinue
    if ($pwsh) {
        Write-Host '  Installing modules (PowerShell 7)...' -ForegroundColor Gray
        & $pwsh.Source -NoProfile -Command $modScript.ToString()
    }
}

# 3. Point both shells' profiles at ours (backing up anything already there).
#    PowerShell 7 first: it is the intended target.
$docs = [Environment]::GetFolderPath('MyDocuments')
$targets = @(
    (Join-Path $docs 'PowerShell\profile.ps1'),          # PowerShell 7
    (Join-Path $docs 'WindowsPowerShell\profile.ps1')     # Windows PowerShell 5.1
) | Select-Object -Unique

$line = ". `"$src`""
foreach ($t in $targets) {
    $dir = Split-Path $t
    if (-not (Test-Path $dir)) { New-Item -ItemType Directory -Force -Path $dir | Out-Null }
    if ((Test-Path $t) -and ((Get-Content $t -Raw) -notlike "*$src*")) {
        Copy-Item $t "$t.neonprime-backup" -Force
    }
    Set-Content -Path $t -Value $line -Encoding UTF8
    try { Unblock-File -Path $t -ErrorAction SilentlyContinue } catch {}
    Write-Host "  Wrote profile -> $t" -ForegroundColor Green
}

# 4. Point Windows Terminal at a Nerd Font so the prompt glyphs render.
Write-Host '  Configuring the terminal font...' -ForegroundColor Gray
Set-TerminalNerdFont

Write-Host ''
Write-Host '  Done. Open a NEW terminal tab (or run: . $PROFILE) for it to take effect.' -ForegroundColor Cyan
Write-Host '  If prompt icons still look blank, set your terminal font to a Nerd Font (e.g. CaskaydiaCove NF).' -ForegroundColor DarkGray
Write-Host ''
