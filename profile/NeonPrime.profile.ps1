# ============================================================================
#  NeonPrime PowerShell profile
#  A fast, modern shell (CTT / WinUtil-style) + NeonPrime integration + extras.
#  Installed by NeonPrime -> Quick Actions -> "Install PowerShell profile".
#  Your previous $PROFILE is backed up as $PROFILE.neonprime-backup.
#  Type `Show-Help` to list every command this profile adds.
# ============================================================================

# Prompt - Oh My Posh (if installed)
if (Get-Command oh-my-posh -ErrorAction SilentlyContinue) {
    $npTheme = Join-Path $env:POSH_THEMES_PATH 'atomic.omp.json'
    if (Test-Path $npTheme) { oh-my-posh init pwsh --config $npTheme | Invoke-Expression }
    else { oh-my-posh init pwsh | Invoke-Expression }
}

# Pretty file/folder icons
if (Get-Module -ListAvailable -Name Terminal-Icons) { Import-Module Terminal-Icons }

# PSReadLine - history, colors, prediction, and keybinds.
# Colors and the extra key handlers work on every PSReadLine version; the
# prediction options need 2.1+ (PowerShell 7, or a manually updated module), so
# they are gated - Windows PowerShell 5.1 ships 2.0 and would otherwise throw on
# load.
if (Get-Module -ListAvailable -Name PSReadLine) {
    Import-Module PSReadLine
    Set-PSReadLineOption -HistoryNoDuplicates -EditMode Windows
    Set-PSReadLineOption -Colors @{
        Command   = '#87CEEB'
        Parameter = '#98FB98'
        Operator  = '#FFB6C1'
        Variable  = '#DDA0DD'
        String    = '#FFDAB9'
        Number    = '#B0E0E6'
        Type      = '#F0E68C'
        Comment   = '#D3D3D3'
        Keyword   = '#8367c7'
        Error     = '#FF6347'
    }
    if ((Get-Module PSReadLine).Version -ge [version]'2.1.0') {
        try {
            Set-PSReadLineOption -PredictionSource History
            Set-PSReadLineOption -PredictionViewStyle ListView
        } catch {}
    }
    # Navigation / editing
    Set-PSReadLineKeyHandler -Key UpArrow            -Function HistorySearchBackward
    Set-PSReadLineKeyHandler -Key DownArrow          -Function HistorySearchForward
    Set-PSReadLineKeyHandler -Key Tab                -Function MenuComplete
    Set-PSReadLineKeyHandler -Chord 'Ctrl+d'         -Function DeleteChar
    Set-PSReadLineKeyHandler -Chord 'Ctrl+w'         -Function BackwardDeleteWord
    Set-PSReadLineKeyHandler -Chord 'Alt+d'          -Function DeleteWord
    Set-PSReadLineKeyHandler -Chord 'Ctrl+LeftArrow' -Function BackwardWord
    Set-PSReadLineKeyHandler -Chord 'Ctrl+RightArrow' -Function ForwardWord
    Set-PSReadLineKeyHandler -Chord 'Ctrl+z'         -Function Undo
    Set-PSReadLineKeyHandler -Chord 'Ctrl+y'         -Function Redo
}

# zoxide - smart `cd` that learns your habits (use `z <part-of-path>`)
if (Get-Command zoxide -ErrorAction SilentlyContinue) {
    Invoke-Expression (& { (zoxide init --cmd z powershell | Out-String) })
}

# ───────────────────────── File / directory helpers ────────────────────────
function touch($file) {
    if (Test-Path $file) { (Get-Item $file).LastWriteTime = Get-Date }
    else { New-Item -ItemType File -Path $file | Out-Null }
}
function ff($name) { Get-ChildItem -Recurse -Filter $name -File | Select-Object -ExpandProperty FullName }
function ll { Get-ChildItem -Force @args | Format-Table -AutoSize }
function la { Get-ChildItem @args | Format-Table -AutoSize }
function which($name) { (Get-Command $name -ErrorAction SilentlyContinue).Source }
function grep { $input | Select-String @args }
function head($path, $n = 10) { Get-Content $path -TotalCount $n }
function tail($path, $n = 10) { Get-Content $path -Tail $n }
function sed($file, $find, $replace) { (Get-Content $file).Replace($find, $replace) | Set-Content $file }
function mkcd($dir) { New-Item -ItemType Directory -Force -Path $dir | Out-Null; Set-Location $dir }
function df { Get-PSDrive -PSProvider FileSystem | Format-Table -AutoSize }
function unzip($file) { Expand-Archive -Path $file -DestinationPath (Get-Location) }
function docs { Set-Location ([Environment]::GetFolderPath('MyDocuments')) }
function dtop { Set-Location ([Environment]::GetFolderPath('Desktop')) }
# Move to the Recycle Bin instead of deleting outright.
function trash($path) {
    Add-Type -AssemblyName Microsoft.VisualBasic -ErrorAction SilentlyContinue
    $full = (Resolve-Path $path).Path
    if (Test-Path $full -PathType Container) {
        [Microsoft.VisualBasic.FileIO.FileSystem]::DeleteDirectory($full, 'OnlyErrorDialogs', 'SendToRecycleBin')
    } else {
        [Microsoft.VisualBasic.FileIO.FileSystem]::DeleteFile($full, 'OnlyErrorDialogs', 'SendToRecycleBin')
    }
}

# ───────────────────────────── Processes ───────────────────────────────────
function pgrep($name) { Get-Process $name -ErrorAction SilentlyContinue }
function pkill($name) { Get-Process $name -ErrorAction SilentlyContinue | Stop-Process -Force }
function k9($name) { pkill $name }

# ───────────────────────────── Clipboard ───────────────────────────────────
function cpy { $input | Set-Clipboard }
function pst { Get-Clipboard }

# ───────────────────────────── Shell misc ──────────────────────────────────
function export($name, $value) { Set-Item -Force -Path "env:$name" -Value $value }
function reload { . $PROFILE }
function ep { if ($env:EDITOR) { & $env:EDITOR $PROFILE } else { notepad $PROFILE } }
function su { Start-Process wt -Verb RunAs -ErrorAction SilentlyContinue; if (-not $?) { Start-Process pwsh -Verb RunAs } }

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

# ─────────────────────────── Sysadmin toolkit ──────────────────────────────
# Recent errors/warnings from an event log (Critical/Error/Warning).
function errlog {
    param([string]$Log = 'System', [int]$Count = 20)
    Get-WinEvent -FilterHashtable @{ LogName = $Log; Level = 1, 2, 3 } -MaxEvents $Count -ErrorAction SilentlyContinue |
        Select-Object TimeCreated,
            @{ n = 'Level'; e = { $_.LevelDisplayName } },
            Id, ProviderName,
            @{ n = 'Message'; e = { ($_.Message -split "`r?`n")[0] } } |
        Format-Table -AutoSize -Wrap
}
function syserr { errlog System @args }
function apperr { errlog Application @args }

# Which process owns a TCP port; list listeners; kill by port.
function port {
    param([Parameter(Mandatory)][int]$Number)
    Get-NetTCPConnection -LocalPort $Number -ErrorAction SilentlyContinue |
        Select-Object LocalAddress, LocalPort, RemoteAddress, State,
            @{ n = 'PID'; e = { $_.OwningProcess } },
            @{ n = 'Process'; e = { (Get-Process -Id $_.OwningProcess -ErrorAction SilentlyContinue).ProcessName } } |
        Format-Table -AutoSize
}
function ports {
    Get-NetTCPConnection -State Listen -ErrorAction SilentlyContinue |
        Select-Object LocalAddress, LocalPort,
            @{ n = 'PID'; e = { $_.OwningProcess } },
            @{ n = 'Process'; e = { (Get-Process -Id $_.OwningProcess -ErrorAction SilentlyContinue).ProcessName } } |
        Sort-Object LocalPort | Format-Table -AutoSize
}
function portkill {
    param([Parameter(Mandatory)][int]$Number)
    $pids = Get-NetTCPConnection -LocalPort $Number -ErrorAction SilentlyContinue |
        Select-Object -ExpandProperty OwningProcess -Unique
    if (-not $pids) { "nothing listening on port $Number"; return }
    foreach ($procId in $pids) {
        Stop-Process -Id $procId -Force -ErrorAction SilentlyContinue
        "killed PID $procId on port $Number"
    }
}
function Test-Port {
    param([Parameter(Mandatory)][string]$ComputerName, [Parameter(Mandatory)][int]$Number)
    Test-NetConnection -ComputerName $ComputerName -Port $Number -InformationLevel Detailed
}

# Services: find, restart, list auto-services that are stopped.
function svc { param([string]$Name = '') Get-Service -Name "*$Name*" -ErrorAction SilentlyContinue | Format-Table -AutoSize Status, Name, DisplayName }
function svcrestart { param([Parameter(Mandatory)][string]$Name) Restart-Service -Name $Name -Force -ErrorAction Stop; "restarted $Name" }
function svcfailed { Get-Service | Where-Object { $_.Status -eq 'Stopped' -and $_.StartType -eq 'Automatic' } | Format-Table -AutoSize Status, Name, DisplayName }

# Is a reboot pending?
function Test-PendingReboot {
    $pending = $false
    if (Test-Path 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Component Based Servicing\RebootPending') { $pending = $true }
    if (Test-Path 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\WindowsUpdate\Auto Update\RebootRequired') { $pending = $true }
    $pfro = (Get-ItemProperty 'HKLM:\SYSTEM\CurrentControlSet\Control\Session Manager' -ErrorAction SilentlyContinue).PendingFileRenameOperations
    if ($pfro) { $pending = $true }
    if ($pending) { Write-Host 'Reboot PENDING' -ForegroundColor Yellow } else { Write-Host 'No reboot pending' -ForegroundColor Green }
}

# Network config + reset + DNS lookup.
function ipinfo { Get-NetIPConfiguration | Format-List }
function Reset-Network {
    ipconfig /release | Out-Null
    ipconfig /renew | Out-Null
    ipconfig /flushdns | Out-Null
    'Released, renewed, flushed. For a full winsock reset run NeonPrime -> Quick Actions -> Network reset (elevated).'
}
function Resolve-Name { param([Parameter(Mandatory)][string]$Name) Resolve-DnsName $Name -ErrorAction SilentlyContinue | Format-Table -AutoSize }

# Group Policy refresh.
function gpup { gpupdate /force }

# Disk usage of a folder's immediate children; biggest files under a path.
function duf {
    param([string]$Path = '.')
    Get-ChildItem -LiteralPath $Path -Directory -Force -ErrorAction SilentlyContinue | ForEach-Object {
        $bytes = (Get-ChildItem -LiteralPath $_.FullName -Recurse -File -Force -ErrorAction SilentlyContinue |
            Measure-Object Length -Sum).Sum
        [pscustomobject]@{ Folder = $_.Name; 'Size(MB)' = [math]::Round(($bytes) / 1MB, 1) }
    } | Sort-Object 'Size(MB)' -Descending | Format-Table -AutoSize
}
function Get-LargeFiles {
    param([string]$Path = '.', [int]$Top = 20)
    Get-ChildItem -LiteralPath $Path -Recurse -File -Force -ErrorAction SilentlyContinue |
        Sort-Object Length -Descending | Select-Object -First $Top FullName,
            @{ n = 'Size(MB)'; e = { [math]::Round($_.Length / 1MB, 1) } } | Format-Table -AutoSize
}

# Installed updates / hotfixes (most recent first).
function patches { Get-HotFix | Sort-Object InstalledOn -Descending | Select-Object -First 25 HotFixID, Description, InstalledOn | Format-Table -AutoSize }

# Logged-on sessions.
function who { quser 2>$null }

# Remote desktop to a host.
function rdp { param([Parameter(Mandatory)][string]$ComputerName) mstsc /v:$ComputerName }

# Elevated SFC + DISM repair in a new admin console.
function Repair-System {
    Start-Process powershell -Verb RunAs -ArgumentList '-NoExit', '-Command', 'sfc /scannow; DISM /Online /Cleanup-Image /RestoreHealth'
}

# Restart / shut down (optional delay in seconds).
function reboot { param([int]$Seconds = 0) shutdown /r /t $Seconds }
function poweroff { param([int]$Seconds = 0) shutdown /s /t $Seconds }

# ─────────────────────────────── Git ───────────────────────────────────────
function gs { git status @args }
function ga { git add . }
function gp { git push @args }
function gpush { git push @args }
function gpull { git pull @args }
function gcl { git clone @args }
function glg { git log --oneline --graph --decorate -n 20 @args }
function gcom { git add .; git commit -m "$args" }
function lazyg { git add .; git commit -m "$args"; git push }
# Jump to your GitHub working dir (needs zoxide to have learned it).
function g { __zoxide_z github }

# ───────────────────────── WinUtil (Chris Titus Tech) ───────────────────────
function winutil { Invoke-RestMethod https://christitus.com/win | Invoke-Expression }
function winutildev { Invoke-RestMethod https://christitus.com/windev | Invoke-Expression }

# ───────────────────────── NeonPrime integration ───────────────────────────
# Launch the NeonPrime app (it sits next to this profile when installed).
function np { Start-Process (Join-Path $PSScriptRoot 'neonprime.exe') }

# Read live temps from NeonPrime's sensor sidecar (run NeonPrime -> Enable HW sensors).
function Get-Temps {
    $f = Join-Path $env:TEMP 'neonprime-sensors.json'
    if (-not (Test-Path $f)) { Write-Host 'NeonPrime sensors not running - open NeonPrime and click "Enable HW sensors".' -ForegroundColor Yellow; return }
    Get-Content $f -Raw | ConvertFrom-Json |
        Where-Object { $_.type -eq 'Temperature' -and $_.value -gt 0 } |
        ForEach-Object { '{0,-34} {1,5:N0} °C' -f "$($_.hw) / $($_.name)", $_.value }
}
Set-Alias temps Get-Temps

# Update this profile by updating NeonPrime (the app ships the profile).
function Update-Profile {
    Write-Host 'Updating NeonPrime (which ships this profile) via winget...' -ForegroundColor Cyan
    winget upgrade --id ABowlOfEleven.NeonPrime -e --accept-source-agreements --accept-package-agreements
    Write-Host 'Done. Open a new shell (or run: . $PROFILE) to load the new profile.' -ForegroundColor Green
}

# ─────────────────────────────── Help ──────────────────────────────────────
function Show-Help {
    $ok = $null -ne $PSStyle
    $t = if ($ok) { $PSStyle.Foreground.BrightCyan } else { '' }     # title
    $s = if ($ok) { $PSStyle.Foreground.BrightBlue } else { '' }     # section
    $c = if ($ok) { $PSStyle.Foreground.BrightGreen } else { '' }    # command
    $d = if ($ok) { $PSStyle.Foreground.BrightWhite } else { '' }    # desc
    $a = if ($ok) { $PSStyle.Foreground.BrightYellow } else { '' }   # accent
    $m = if ($ok) { $PSStyle.Foreground.BrightBlack } else { '' }    # dim
    $r = if ($ok) { $PSStyle.Reset } else { '' }
    Write-Host @"

${t}◢ NEONPRIME PowerShell profile${r}
${m}──────────────────────────────────────────────────────────${r}

${s}NeonPrime${r}
  ${c}np${r}                 ${a}->${r} ${d}Launch the NeonPrime app${r}
  ${c}temps${r}              ${a}->${r} ${d}Live CPU/GPU temps (needs HW sensors on)${r}
  ${c}Update-Profile${r}     ${a}->${r} ${d}Update NeonPrime (and this profile)${r}
  ${c}winutil${r}            ${a}->${r} ${d}Run Chris Titus Tech's WinUtil${r}
  ${c}winutildev${r}         ${a}->${r} ${d}Run WinUtil (dev branch)${r}

${s}Git${r}
  ${c}gs${r}   ${d}status${r}    ${c}ga${r}   ${d}add .${r}    ${c}gp / gpush${r} ${d}push${r}    ${c}gpull${r} ${d}pull${r}
  ${c}gcl <repo>${r} ${d}clone${r}   ${c}gcom <msg>${r} ${d}add+commit${r}   ${c}lazyg <msg>${r} ${d}add+commit+push${r}
  ${c}glg${r} ${d}graph log${r}   ${c}g${r} ${d}jump to GitHub dir (zoxide)${r}

${s}Files / directories${r}
  ${c}touch <f>${r}  ${c}ff <name>${r}  ${c}sed <f> <find> <rep>${r}  ${c}mkcd <d>${r}  ${c}trash <p>${r}
  ${c}ll${r} ${d}list (all)${r}  ${c}la${r} ${d}list${r}  ${c}head/tail <f> [n]${r}  ${c}grep${r}  ${c}unzip <f>${r}  ${c}df${r}
  ${c}docs${r} ${d}Documents${r}  ${c}dtop${r} ${d}Desktop${r}  ${c}which <cmd>${r}

${s}Processes${r}
  ${c}pgrep <name>${r} ${d}find${r}   ${c}pkill / k9 <name>${r} ${d}kill${r}

${s}System / net${r}
  ${c}uptime${r}  ${c}sysinfo${r}  ${c}cleanup${r} ${d}clear %TEMP%${r}  ${c}update-all${r} ${d}winget upgrade${r}
  ${c}Get-PubIP${r}  ${c}flushdns${r}  ${c}weather [loc]${r}  ${c}cpy${r} ${d}copy${r}  ${c}pst${r} ${d}paste${r}
  ${c}export <n> <v>${r}  ${c}reload${r} ${d}re-source${r}  ${c}ep${r} ${d}edit profile${r}  ${c}su${r} ${d}elevated shell${r}

${s}Sysadmin${r}
  ${c}syserr / apperr [n]${r} ${d}recent event-log errors${r}   ${c}errlog <log> [n]${r}
  ${c}port <n>${r} ${d}who owns a port${r}  ${c}ports${r} ${d}listeners${r}  ${c}portkill <n>${r}  ${c}Test-Port <host> <n>${r}
  ${c}svc [name]${r} ${d}status${r}  ${c}svcrestart <name>${r}  ${c}svcfailed${r} ${d}stopped auto-services${r}
  ${c}Test-PendingReboot${r}  ${c}ipinfo${r}  ${c}Reset-Network${r}  ${c}Resolve-Name <host>${r}  ${c}gpup${r} ${d}gpupdate /force${r}
  ${c}duf [path]${r} ${d}folder sizes${r}  ${c}Get-LargeFiles [path] [n]${r}  ${c}patches${r} ${d}hotfixes${r}  ${c}who${r} ${d}sessions${r}
  ${c}rdp <host>${r}  ${c}Repair-System${r} ${d}sfc + DISM (admin)${r}  ${c}reboot [s]${r}  ${c}poweroff [s]${r}

${m}──────────────────────────────────────────────────────────${r}
"@
}

# ─────────────────────────────── Welcome ───────────────────────────────────
Write-Host ''
Write-Host '  ◢ NEONPRIME' -ForegroundColor Cyan -NoNewline
Write-Host ' shell ready.' -ForegroundColor DarkCyan
Write-Host "  Type " -ForegroundColor DarkGray -NoNewline
Write-Host 'Show-Help' -ForegroundColor Yellow -NoNewline
Write-Host ' for the full command list.' -ForegroundColor DarkGray
Write-Host ''
