# Installs the NeonPrime PowerShell profile + its prerequisites, and points the
# user's $PROFILE (both Windows PowerShell and PowerShell 7) at it. Run by
# NeonPrime → Quick Actions → "Install PowerShell profile". No elevation needed.
param([switch]$NoPrereqs)

$ErrorActionPreference = 'Stop'
Write-Host ''
Write-Host '  ◢ NeonPrime PowerShell profile installer' -ForegroundColor Cyan
Write-Host ''

$src = Join-Path $PSScriptRoot 'NeonPrime.profile.ps1'
if (-not (Test-Path $src)) { throw "NeonPrime.profile.ps1 not found next to this script ($PSScriptRoot)" }

function Install-WingetPkg($id) {
    Write-Host "  • $id" -ForegroundColor Gray
    winget install --id $id -e --accept-source-agreements --accept-package-agreements --silent 2>&1 | Out-Null
}

if (-not $NoPrereqs) {
    Write-Host '  Installing prerequisites...' -ForegroundColor Yellow
    if (-not (Get-Command pwsh -ErrorAction SilentlyContinue)) { Install-WingetPkg 'Microsoft.PowerShell' }
    Install-WingetPkg 'JanDeDobbeleer.OhMyPosh'      # prompt
    Install-WingetPkg 'ajeetdsouza.zoxide'           # smart cd
    Install-WingetPkg 'Microsoft.CascadiaCode'       # Nerd Font for glyphs

    foreach ($mod in 'Terminal-Icons', 'PSReadLine') {
        if (-not (Get-Module -ListAvailable -Name $mod)) {
            Write-Host "  • module $mod" -ForegroundColor Gray
            try { Install-Module -Name $mod -Repository PSGallery -Scope CurrentUser -Force -SkipPublisherCheck } catch {}
        }
    }
}

# Point both shells' profiles at ours (back up anything already there).
$docs = [Environment]::GetFolderPath('MyDocuments')
$targets = @(
    (Join-Path $docs 'WindowsPowerShell\profile.ps1'),
    (Join-Path $docs 'PowerShell\profile.ps1')
) | Select-Object -Unique

$line = ". `"$src`""
foreach ($t in $targets) {
    $dir = Split-Path $t
    if (-not (Test-Path $dir)) { New-Item -ItemType Directory -Force -Path $dir | Out-Null }
    if (Test-Path $t) {
        $existing = Get-Content $t -Raw
        if ($existing -notlike "*$src*") { Copy-Item $t "$t.neonprime-backup" -Force }
    }
    Set-Content -Path $t -Value $line -Encoding UTF8
    Write-Host "  Wrote profile → $t" -ForegroundColor Green
}

Write-Host ''
Write-Host '  Done. Open a new terminal (or run: . $PROFILE) to load it.' -ForegroundColor Cyan
Write-Host '  Tip: set a Nerd Font (CaskaydiaCove NF) in your terminal for prompt glyphs.' -ForegroundColor DarkGray
Write-Host ''
