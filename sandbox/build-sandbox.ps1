# Stage NeonPrime into sandbox\share for Windows Sandbox testing.
# Windows Sandbox is disposable: every launch is a clean Win11 VM, so it's the
# safe place to actually click harden / remove / DISM / powercfg / fixes buttons
# without touching the host. Run this, then double-click sandbox\neonprime.wsb.
[CmdletBinding()]
param([switch]$Release)

$ErrorActionPreference = 'Stop'
$root  = Split-Path $PSScriptRoot -Parent
$cfg   = if ($Release) { 'release' } else { 'debug' }
$share = Join-Path $PSScriptRoot 'share'

Write-Host "Building NeonPrime ($cfg)..." -ForegroundColor Cyan
if ($Release) {
    cargo build --release --manifest-path "$root\Cargo.toml"
} else {
    cargo build --manifest-path "$root\Cargo.toml"
}
if ($LASTEXITCODE -ne 0) { throw "cargo build failed ($LASTEXITCODE)" }

$bin = Join-Path $root "target\$cfg"
New-Item -ItemType Directory -Force -Path $share | Out-Null
Get-ChildItem $share | Remove-Item -Recurse -Force -ErrorAction SilentlyContinue

Copy-Item (Join-Path $bin 'neonprime.exe') $share -Force
Copy-Item (Join-Path $bin 'broker.exe')    $share -Force   # elevated action executor
if (Test-Path "$root\sensors\dist") { Copy-Item "$root\sensors\dist" "$share\sensors" -Recurse -Force }
if (Test-Path "$root\profile")      { Copy-Item "$root\profile"      "$share\profile" -Recurse -Force }

Write-Host "Staged $cfg build to $share" -ForegroundColor Green
Write-Host "Launch: sandbox\neonprime.wsb  (requires Windows Sandbox enabled + a reboot)" -ForegroundColor Yellow
