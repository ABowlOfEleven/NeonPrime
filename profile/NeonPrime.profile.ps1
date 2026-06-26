# ============================================================================
#  NeonPrime PowerShell profile
#  A fast, modern shell setup (CTT-style) + NeonPrime integration + extras.
#  Installed by NeonPrime → Quick Actions → "Install PowerShell profile".
#  Your previous $PROFILE is backed up as $PROFILE.neonprime-backup.
# ============================================================================

# Prompt — Oh My Posh (if installed)
if (Get-Command oh-my-posh -ErrorAction SilentlyContinue) {
    $npTheme = Join-Path $env:POSH_THEMES_PATH 'atomic.omp.json'
    if (Test-Path $npTheme) { oh-my-posh init pwsh --config $npTheme | Invoke-Expression }
    else { oh-my-posh init pwsh | Invoke-Expression }
}

# Pretty file/folder icons
if (Get-Module -ListAvailable -Name Terminal-Icons) { Import-Module Terminal-Icons }

# PSReadLine — history prediction + sane keys
if (Get-Module -ListAvailable -Name PSReadLine) {
    Import-Module PSReadLine
    Set-PSReadLineOption -PredictionSource History -HistoryNoDuplicates -EditMode Windows
    try { Set-PSReadLineOption -PredictionViewStyle ListView } catch {}
    Set-PSReadLineKeyHandler -Key UpArrow   -Function HistorySearchBackward
    Set-PSReadLineKeyHandler -Key DownArrow -Function HistorySearchForward
    Set-PSReadLineKeyHandler -Key Tab       -Function MenuComplete
}

# zoxide — smart `cd` that learns your habits (use `z <part-of-path>`)
if (Get-Command zoxide -ErrorAction SilentlyContinue) {
    Invoke-Expression (& { (zoxide init powershell | Out-String) })
}

# ───────────────────────── Unix-style helpers ──────────────────────────────
function touch($file) {
    if (Test-Path $file) { (Get-Item $file).LastWriteTime = Get-Date }
    else { New-Item -ItemType File -Path $file | Out-Null }
}
function ll { Get-ChildItem -Force @args }
function la { Get-ChildItem -Force -Hidden @args }
function which($name) { (Get-Command $name -ErrorAction SilentlyContinue).Source }
function grep { $input | Select-String @args }
function head($path, $n = 10) { Get-Content $path -TotalCount $n }
function tail($path, $n = 10) { Get-Content $path -Tail $n }
function mkcd($dir) { New-Item -ItemType Directory -Force -Path $dir | Out-Null; Set-Location $dir }
function df { Get-PSDrive -PSProvider FileSystem | Format-Table -AutoSize }
function unzip($file) { Expand-Archive -Path $file -DestinationPath (Get-Location) }
function pgrep($name) { Get-Process $name -ErrorAction SilentlyContinue }
function pkill($name) { Get-Process $name -ErrorAction SilentlyContinue | Stop-Process -Force }
function export($name, $value) { Set-Item -Force -Path "env:$name" -Value $value }
function reload { . $PROFILE }
function ep { if ($env:EDITOR) { & $env:EDITOR $PROFILE } else { notepad $PROFILE } }
function su { Start-Process wt -Verb RunAs -ErrorAction SilentlyContinue; if (-not $?) { Start-Process pwsh -Verb RunAs } }

# Clipboard
function cpy { $input | Set-Clipboard }
function pst { Get-Clipboard }

# ───────────────────────────── Networking ──────────────────────────────────
function Get-PubIP { (Invoke-RestMethod -Uri 'https://api.ipify.org?format=json').ip }
function flushdns { Clear-DnsClientCache; 'DNS cache flushed.' }
function weather($loc = '') { (Invoke-WebRequest "https://wttr.in/$loc`?format=3" -UseBasicParsing).Content }

# ─────────────────────────────── System ────────────────────────────────────
function uptime {
    $b = (Get-CimInstance Win32_OperatingSystem).LastBootUpTime
    $u = (Get-Date) - $b
    "Up {0}d {1}h {2}m  (since {3:g})" -f $u.Days, $u.Hours, $u.Minutes, $b
}
function sysinfo { Get-ComputerInfo | Select-Object CsName, WindowsProductName, OsVersion, CsProcessors, @{n='RAM(GB)';e={[math]::Round($_.CsTotalPhysicalMemory/1GB)}} }
function cleanup { Remove-Item "$env:TEMP\*" -Recurse -Force -ErrorAction SilentlyContinue; 'Temp cleared.' }
function update-all { winget upgrade --all --accept-source-agreements --accept-package-agreements }

# ─────────────────────────────── Git ───────────────────────────────────────
function gst { git status @args }
function gad { git add @args }
function gco { git commit -m @args }
function gpu { git push @args }
function gpl { git pull @args }
function glg { git log --oneline --graph --decorate -n 20 @args }
function lazyg($msg) { git add .; git commit -m $msg; git push }

# ───────────────────────── NeonPrime integration ───────────────────────────
# Launch the NeonPrime app (it sits next to this profile when installed).
function np { Start-Process (Join-Path $PSScriptRoot 'neonprime.exe') }

# Read live temps from NeonPrime's sensor sidecar (run NeonPrime → Enable HW sensors).
function Get-Temps {
    $f = Join-Path $env:TEMP 'neonprime-sensors.json'
    if (-not (Test-Path $f)) { Write-Host 'NeonPrime sensors not running — open NeonPrime and click "Enable HW sensors".' -ForegroundColor Yellow; return }
    Get-Content $f -Raw | ConvertFrom-Json |
        Where-Object { $_.type -eq 'Temperature' -and $_.value -gt 0 } |
        ForEach-Object { '{0,-34} {1,5:N0} °C' -f "$($_.hw) / $($_.name)", $_.value }
}
Set-Alias temps Get-Temps

# ─────────────────────────────── Welcome ───────────────────────────────────
Write-Host ''
Write-Host '  ◢ NEONPRIME' -ForegroundColor Cyan -NoNewline
Write-Host ' shell ready.' -ForegroundColor DarkCyan
Write-Host '  temps · np · update-all · weather · gst/gad/gco/gpu · z <dir> · ll · which' -ForegroundColor DarkGray
Write-Host ''
