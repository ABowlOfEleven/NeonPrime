# Changelog

All notable changes to NeonPrime. Dates are UTC.

## 3.1.1 — 2026-07-06

Windows bug fixes reported from driving 3.1.0.

### Fixed

- **Version display** now shows the real build version (from `CARGO_PKG_VERSION`)
  instead of a hardcoded string, so it tracks each release.
- **App install** ran `winget` hidden and unelevated (the GUI has no console), so
  installs silently did nothing for machine-scope packages. It now runs in a
  visible, elevated console so you see progress and can approve UAC.
- **App removal**: the Install panel now has a Remove button
  (`winget uninstall`), which it previously lacked.
- **PowerShell profile installer**: now runs elevated, sets the CurrentUser
  execution policy to RemoteSigned (so the installed profile actually loads
  instead of being blocked), unblocks the profile file, targets PowerShell 7
  first, and installs the modules in PowerShell 7's scope as well as 5.1's. Also
  fixed the installer window flashing and closing after the UAC prompt: the
  elevated launcher passed the script path unquoted, so with the app under
  `C:\Program Files\` it split on the space; the path is now quoted.
- **Profile now loads under Windows PowerShell 5.1**: it used
  `Set-PSReadLineOption -PredictionSource`, which needs PSReadLine 2.1+, but 5.1
  ships 2.0 and errored on every shell start. The prediction options are now
  gated behind a PSReadLine version check; history de-dup and key handlers still
  apply everywhere.
- **Quieter execution-policy handling**: the installer no longer tries to change
  the execution policy when the effective policy already allows local scripts
  (RemoteSigned / Unrestricted / Bypass, often forced by a machine GPO). That
  attempt used to print a scary "overridden by a policy at a more specific scope"
  error even though scripts already ran fine.

### Added

- **NeonPrime author's public Rust projects in the Install catalog** (search
  "ABowlOfEleven"): hopscout (via its winget package), plus GenomeForge and
  Formant, which download and install their latest GitHub-release MSI. Their
  installed state is detected too — winget packages via the winget scan, the
  GitHub-release apps via Add/Remove Programs.
- **First-launch state indicators** on Install, Features, and Startup, so you no
  longer have to guess what's already done:
  - Install: a background `winget` scan flags each app `● INSTALLED` or
    `AVAILABLE`, with a RECHECK button to re-scan after changes.
  - Features: each optional feature shows `● ENABLED` / `DISABLED`, detected
    unelevated from file/registry probes (no UAC just to look). The two whose
    payload ships even when off (WSL, VM Platform) show `state: admin`.
  - Startup: each entry now shows an explicit `● ON` / `OFF` pill.
- **28 more WinUtil-parity tweaks**, all fully reversible through the same undo /
  rollback journal as the existing ones (ported with exact registry paths and
  on/off values from WinUtil's config):
  - Interface: show battery %, End Task on taskbar, always-show scrollbars, Num
    Lock on startup, verbose sign-in, detailed BSoD, hide Home/Gallery, hide
    Start recommendations, disable sign-in blur, disable lock screen, suppress
    unsigned-RDP warning.
  - Privacy: disable Activity History, location tracking, Delivery Optimization,
    background apps, notifications, Notepad AI, plus Edge and Brave debloat.
  - Performance: adjust visuals for best performance, Game Mode, disable
    fullscreen optimizations, disable Storage Sense, disable hibernation, UTC
    clock, prefer IPv4.
  - Hardening score: block firmware-injected software (WPBT) and block automatic
    driver-software installs.
  The aggressive, non-reversible removals (remove Edge / OneDrive, disable
  BitLocker) are intentionally left out.
- **Windows feature + system-tool parity**: NFS client (DISM), and new Quick
  Actions to enable the OpenSSH server, fix the clock over NTP, back up the
  registry, toggle the legacy F8 boot menu, set up auto sign-in (netplwiz), and
  open the classic control-panel applets (Sound, Power, Region, Network, System,
  Date & Time, Printers, Computer Management, System Restore).
- **Remove PowerShell profile** quick action: restores your previous `$PROFILE`
  from the backup NeonPrime made, or clears it if NeonPrime created it, undoing
  the profile install.
- **PowerShell profile now matches Chris Titus Tech's WinUtil profile** and adds
  our own on top. New: `Show-Help` (a grouped command menu), `Update-Profile`,
  the `winutil` / `winutildev` launchers, `ff` / `sed` / `k9` / `trash` /
  `docs` / `dtop`, the CTT git shortcuts (`gs` `ga` `gp` `gpush` `gpull` `gcl`
  `gcom` `lazyg` `g`), the PSReadLine syntax-color scheme, and the extra edit
  keybinds (Ctrl+D/W, Alt+D, Ctrl+arrows, Ctrl+Z/Y). The installer also opens
  its console in PowerShell 7 when present, so you land in the modern shell.

## 3.1.0 — 2026-07-06

The cross-platform release: NeonPrime now runs on Linux too, as both a GUI and a
headless terminal UI, alongside the unchanged Windows deck.

### Added

- **Linux GUI (`neonprime-linux`)** — a Slint window sharing the holographic look
  of the Windows deck, with sixteen panels: Dashboard, Processes, Network,
  Tweaks, Debloat, Packages (with a curated app catalog), Services (systemd),
  Firewall (ufw), DNS (resolvectl), Power (power-profiles-daemon), Autostart
  (XDG), Cleanup, Restore Points (Timeshift/Snapper), Quick Actions, Graphics,
  and Servers.
- **Linux TUI (`neonprime-tui`)** — a headless, SSH-friendly `ratatui` terminal
  UI over the same backend. No display or GUI libraries needed at runtime;
  privileged actions run through `sudo`.
- **Desktop-environment-aware tweaks** — routes to GNOME `gsettings`, KDE
  `kwriteconfig`, or XFCE `xfconf-query`, plus `sysctl`.
- **Security hardening** on both platforms, each toggle with a warning:
  - Windows (in the Hardening Score panel): disable SMBv1, block AutoRun,
    require SmartScreen, disable LLMNR, stop WDigest caching, Defender PUA
    protection, disable Windows Script Host, no LM hash, block RDP, disable
    Remote Assistance.
  - Linux (Tweaks → Security): kernel-hardening sysctls (kptr/dmesg/ptrace
    restrict, SYN cookies, reverse-path filter, ignore ICMP redirects, reject
    source routing, ignore broadcast pings, protect symlinks, no setuid dumps).
- **Graphics panel (Linux)** — GPU detection + per-vendor driver install, and
  multi-GPU game setup: auto-detected hybrid graphics, correct dGPU launch
  options for Steam (NVIDIA PRIME offload or `DRI_PRIME=1`), one-click
  `switcheroo-control`, and GameMode/MangoHud/Gamescope (or the CachyOS meta).
- **Servers panel (Linux)** — install + enable OpenSSH and Samba.
- **Windows enhancements** — sortable + filterable Process manager, reverse-DNS
  in the Network monitor, per-target Cleanup size bars, and a fuzzy command
  palette with recents.
- **Packaging** — Linux AppImage, tarball, `.deb`, and `.rpm`; a Windows +
  Linux CI matrix; the release pipeline now publishes Linux artifacts alongside
  the Windows MSI.

### Notes

- The Linux binaries are a preview: they compile and pass CI (fmt / clippy /
  test) on every push, but real-hardware runtime testing is ongoing.

## 3.0.1 — 2026-07-05

- Statically link the MSVC CRT so the binaries start on a clean Windows.
- Broker and sensor sidecar exit promptly on a bare run (installer-validator
  friendly).
- Submitted to winget (`ABowlOfEleven.NeonPrime`).

## 3.0.0 — 2026-06-29

- First public release: the Windows deck with full WinUtil parity plus telemetry
  HUD, reversible tweaks + rollback, system modes, privacy/hardening score,
  services/process/firewall management, MicroWin, and more. MSI + portable zip,
  GitHub Actions CI and tag-driven releases.
