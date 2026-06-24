# Build a SELF-CONTAINED sensor sidecar (bundles the .NET runtime so target
# machines don't need .NET installed) and stage it beside the app binary.
#
#   ./publish-sensors.ps1                 # stages into target/debug/sensors
#   ./publish-sensors.ps1 -AppDir target/release
param(
    [string]$Config = "Release",
    [string]$AppDir = "target/debug"
)
$ErrorActionPreference = "Stop"
$root = $PSScriptRoot

dotnet publish "$root/sensors/NeonPrime.Sensors.csproj" `
    -c $Config -r win-x64 --self-contained true `
    -p:PublishSingleFile=false -p:PublishTrimmed=false | Out-Null

$pub = "$root/sensors/bin/$Config/net9.0-windows/win-x64/publish"
$dst = "$root/$AppDir/sensors"
New-Item -ItemType Directory -Force -Path $dst | Out-Null
Copy-Item "$pub/*" $dst -Recurse -Force
Write-Host "Staged self-contained sidecar -> $dst"
