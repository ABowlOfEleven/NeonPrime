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
