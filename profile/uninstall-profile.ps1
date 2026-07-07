# Removes the NeonPrime PowerShell profile hook and restores your previous
# $PROFILE (from the backup NeonPrime made), or resets it to the default if
# NeonPrime created it. Undoes install-profile.ps1. No elevation needed.
$ErrorActionPreference = 'Continue'
Write-Host ''
Write-Host '  Removing the NeonPrime PowerShell profile...' -ForegroundColor Cyan
Write-Host ''

$src = Join-Path $PSScriptRoot 'NeonPrime.profile.ps1'
$docs = [Environment]::GetFolderPath('MyDocuments')
$targets = @(
    (Join-Path $docs 'PowerShell\profile.ps1'),          # PowerShell 7
    (Join-Path $docs 'WindowsPowerShell\profile.ps1')     # Windows PowerShell 5.1
) | Select-Object -Unique

$touched = $false
foreach ($t in $targets) {
    $backup = "$t.neonprime-backup"
    if (Test-Path $backup) {
        # There was a profile before ours; put it back.
        Move-Item -Path $backup -Destination $t -Force
        Write-Host "  Restored your previous profile -> $t" -ForegroundColor Green
        $touched = $true
        continue
    }
    if (-not (Test-Path $t)) { continue }
    $content = Get-Content $t -Raw
    if ($content -like "*$src*") {
        # We own this file (no backup means we created it). Drop our line; if
        # nothing meaningful remains, delete the file so the default is restored.
        $kept = Get-Content $t | Where-Object { $_ -notlike "*$src*" -and $_.Trim() -ne '' }
        if ($kept) {
            Set-Content -Path $t -Value $kept -Encoding UTF8
            Write-Host "  Removed the NeonPrime line -> $t" -ForegroundColor Green
        } else {
            Remove-Item -Path $t -Force
            Write-Host "  Reset to default (deleted NeonPrime-created profile) -> $t" -ForegroundColor Green
        }
        $touched = $true
    } else {
        Write-Host "  No NeonPrime hook in $t, left unchanged." -ForegroundColor DarkGray
    }
}

Write-Host ''
if ($touched) {
    Write-Host '  Done. Open a new terminal for it to take effect.' -ForegroundColor Cyan
} else {
    Write-Host '  Nothing to remove, the NeonPrime profile was not installed.' -ForegroundColor Yellow
}
Write-Host ''
