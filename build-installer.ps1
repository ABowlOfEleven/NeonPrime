# Build the NeonPrime MSI installer.
#   ./build-installer.ps1
#
# Produces NeonPrime-3.0.0-Setup.msi bundling: neonprime.exe, broker.exe, and the
# self-contained LibreHardwareMonitor sidecar (sensors/). Requires the WiX dotnet
# tool: dotnet tool install --global wix
param([string]$Version = "3.0.0")
$ErrorActionPreference = "Stop"
$root = $PSScriptRoot
$dist = Join-Path $root "dist"
$wix = Join-Path $env:USERPROFILE ".dotnet\tools\wix.exe"

Write-Host "[1/4] Building release binaries..."
cargo build --release --manifest-path "$root\Cargo.toml" | Out-Null
if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }

Write-Host "[2/4] Staging payload -> dist\"
Remove-Item $dist -Recurse -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force -Path $dist | Out-Null
Copy-Item "$root\target\release\neonprime.exe" $dist
Copy-Item "$root\target\release\broker.exe" $dist
Copy-Item "$root\assets\neonprime.ico" $dist
Copy-Item "$root\README.md" $dist
Copy-Item "$root\LICENSE" $dist
Copy-Item "$root\profile" $dist -Recurse   # PowerShell profile + installer

Write-Host "[3/4] Publishing self-contained sensor sidecar..."
& "$root\publish-sensors.ps1" -AppDir "dist" | Out-Null

Write-Host "[4/4] Building MSI..."
$msi = Join-Path $root "NeonPrime-$Version-Setup.msi"
& $wix build "$root\installer\NeonPrime.wxs" -d DistDir="$dist" -arch x64 -o $msi
if ($LASTEXITCODE -ne 0) { throw "wix build failed ($LASTEXITCODE)" }
Write-Host "Installer ready: $msi"
