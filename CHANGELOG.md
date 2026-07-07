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

### Added

- **Remove PowerShell profile** quick action: restores your previous `$PROFILE`
  from the backup NeonPrime made, or clears it if NeonPrime created it, undoing
  the profile install.

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
