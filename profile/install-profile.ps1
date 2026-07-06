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
try {
    Set-ExecutionPolicy -Scope CurrentUser -ExecutionPolicy RemoteSigned -Force
    Write-Host '  Execution policy (CurrentUser) -> RemoteSigned' -ForegroundColor Green
} catch { Write-Host "  Could not set execution policy: $_" -ForegroundColor Yellow }

# 2. Strip any mark-of-the-web so the profile is not treated as downloaded.
try { Unblock-File -Path $src -ErrorAction SilentlyContinue } catch {}

function Install-WingetPkg($id) {
    Write-Host "  - $id" -ForegroundColor Gray
    try { winget install --id $id -e --accept-source-agreements --accept-package-agreements --silent 2>&1 | Out-Null } catch {}
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

Write-Host ''
Write-Host '  Done. Open a new PowerShell 7 (pwsh) window, or run: . $PROFILE' -ForegroundColor Cyan
Write-Host '  Tip: set a Nerd Font (CaskaydiaCove NF) in your terminal for prompt glyphs.' -ForegroundColor DarkGray
Write-Host ''
