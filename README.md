# NeonPrime

[![CI](https://github.com/ABowlOfEleven/NeonPrime/actions/workflows/ci.yml/badge.svg)](https://github.com/ABowlOfEleven/NeonPrime/actions/workflows/ci.yml)
![Platform](https://img.shields.io/badge/platform-Windows%2011%20%C2%B7%20Linux-0a84ff)
![Built with](https://img.shields.io/badge/built%20with-Rust%20%2B%20Slint-CE8A1F)
![License](https://img.shields.io/badge/license-MIT-34D2FF)

> One machine. Three modes. It counts all the way to three.

A holographic system control deck for **Windows and Linux**. Think [WinUtil](https://github.com/ChrisTitusTech/winutil) and [linutil](https://github.com/ChrisTitusTech/linutil), but more powerful and actually beautiful: debloat it, tune it, watch it, harden it, and reshape it from a single cyan-and-ember HUD. On Linux there is also a headless terminal UI for servers and SSH.

Built in **Rust**, drawn in **Slint** (with a **ratatui** terminal UI on Linux).

<div align="center">
  <em>Cyan reports. Ember warns. Everything reverts.</em>
</div>

<div align="center">
  <img src="assets/screenshots/dashboard.png" width="880" alt="NeonPrime dashboard: a live telemetry HUD with a GPU-load gauge, CPU / RAM / temperature meters, and rolling CPU and GPU sparklines">
</div>

---

## Download

Grab the latest build from **[Releases](https://github.com/ABowlOfEleven/NeonPrime/releases)**:

| File | Platform | What it is |
|------|----------|------------|
| `NeonPrime-x.y.z-Setup.msi` | Windows | Installer. Drops the app, the elevated broker, and the bundled sensor sidecar into `Program Files`. No prerequisites. |
| `NeonPrime-x.y.z-portable.zip` | Windows | Unzip and run `neonprime.exe`. |
| `NeonPrime-x.y.z-x86_64.AppImage` | Linux | Portable. `chmod +x` and run. The GUI. |
| `NeonPrime-x.y.z-linux-x86_64.tar.gz` | Linux | Portable tarball (`neonprime` GUI + `neonprime-tui`). |
| `neonprime_x.y.z_amd64.deb` / `.rpm` | Linux | Native packages. Installs `neonprime` and `neonprime-tui`. |

> The Linux binaries are a preview: they compile and pass CI on every push, but they have not yet been through extensive real-hardware testing. Feedback welcome.

## Why

WinUtil walked so this could fly. NeonPrime keeps the one-stop Windows-control idea and adds what a PowerShell script in a WPF box never could:

- a live telemetry HUD that looks like it belongs on a starship
- every tweak reversible, backed by a rollback journal (debloat without the dread)
- one-click system modes that swap your machine's whole personality
- your entire tuned setup exportable to a fresh install
- a proper IT toolkit on top: security posture, one-click support bundles, event logs, users, printers, disks, drivers, certs, and Group Policy

## The three modes

The heart of NeonPrime. One click changes who your PC is:

| Mode | What it does |
|------|--------------|
| ◇ **AI / Inference** | High-performance power, GPU freed (Game DVR off), background apps suspended |
| ◇ **Game** | High-performance power, Game Mode on, Game DVR and background recording off |
| ◇ **Work** | Balanced power, notifications silenced. The quiet profile. |

Every mode is a reversible bundle. Click the active one again to turn it back off.

*(Three of them. We counted.)*

<div align="center">
  <img src="assets/screenshots/modes.png" width="880" alt="System modes panel: AI / Inference, Game, and Work cards with a power-plan switcher">
</div>

---

## Features

**Monitor**

- **Telemetry HUD:** live GPU load, VRAM, CPU, and temps with rolling CPU and GPU sparklines. Vendor-neutral GPU stats (NVIDIA, AMD, Intel) via DXGI and PDH; GPU temp via NVML; best-effort CPU temp via WMI. Plus an OS / CPU / GPU / RAM / uptime strip.
- **Network monitor:** live outbound TCP connections per process (remote IP:port and state), refreshing while open, so you can see what is phoning home. One click blocks any app at the firewall. Includes a DNS switcher (Cloudflare, Google, Quad9, or automatic).
- **Process manager:** top processes by CPU and RAM, with per-process GPU% and VRAM, plus a kill button.
- **Event viewer:** recent System and Application errors and warnings from the Windows event log, filterable by level and text, each with an expandable message.

**Optimize**

- **Privacy & hardening score:** a live gauge that reads your real registry and service state and hardens any exposed item in one click (all reversible). Beyond telemetry, it includes real security hardening, each with a plain warning about what it does and what could break: disable SMBv1, block AutoRun, require SmartScreen, disable LLMNR, stop WDigest credential caching, Defender PUA protection, disable Windows Script Host, no LM hash, block inbound RDP, disable Remote Assistance, block firmware-injected software (WPBT), and block automatic driver-software installs.
- **Tweaks:** over 50 reversible tweaks across Interface, Privacy, and Performance, ported to match WinUtil (Activity History, location, hibernation, Game Mode, background apps, notifications, Edge/Brave debloat, and many more), each undoable through the rollback journal, with live search, category filters, and a one-click Essential Tweaks preset.
- **Debloat:** remove preinstalled UWP apps (Copilot, Xbox Game Bar, and friends) per-user with live installed/removed state, plus one-click telemetry scheduled-task disabling.
- **Cleanup:** scan reclaimable space (temp, Recycle Bin, thumbnails, system and update caches) and clear it per target.
- **Startup manager:** enable or disable per-user startup apps, reversibly, each showing its current on/off state.

**Software**

- **Install:** a picker with 197 apps (WinUtil's `winget` catalog plus the author's own public Rust projects), live search, an "update all", and a first-launch scan that flags each app installed or available (with a recheck button).
- **Windows Features:** DISM enable/disable for .NET 3.5, Hyper-V, Sandbox, WSL, IIS, NFS, and more, each showing its detected enabled/disabled state (read without a UAC prompt).
- **MicroWin:** build a slimmed, debloated Windows ISO. Strip bundled apps, apply offline privacy tweaks, and inject an autounattend that bypasses TPM/SecureBoot/RAM and skips OOBE.

**System**

- **System modes & power plans:** one-click AI, Game, and Work that actually do things (see above), plus a Balanced / High-Performance / Ultimate switcher.
- **Services manager:** a searchable list of every service, with start/stop and start-type control (auto, manual, disabled).
- **Quick Actions:** restart Explorer, flush DNS, clear temp, empty the Recycle Bin, create a restore point, run repairs (SFC, DISM, chkdsk, gpupdate, network reset), enable Remote Desktop or the OpenSSH server, generate a battery or system-info report, fix the clock over NTP, back up the registry, and open any of the management consoles (Event Viewer, Services, Task Scheduler, Disk Management, Device Manager, Local Users, Group Policy). Plus install a WinUtil/CTT-style PowerShell profile that now ships a full sysadmin toolkit: event-log tails, port-to-process, service restart, pending-reboot check, network reset, disk usage, hotfix list, and more.
- **Config & provisioning:** export your whole setup to a TOML profile (tweaks, hardening, active mode, and your installed app set) and replay it on a clean machine, applying the tweaks and installing the apps in one step. Plus repair fixes (SFC, DISM, network reset, Windows Update reset), Windows Update modes, and restore points.
- **History:** a full timeline over the action journal. Revert any past change, or all of them, not just the last.

**IT & helpdesk** *(new in 3.3)*

- **Compliance & posture:** a read-only security board covering Defender (real-time and signature age), firewall profiles, BitLocker, TPM, Secure Boot, UAC, and patch age, each rated OK / warn / risk, with a one-click HTML compliance report for audits.
- **Support bundle:** a one-click machine snapshot to attach to a ticket. It gathers specs, security posture, recent event-log errors, services, network config, installed apps, top processes, and drivers into a timestamped folder with an HTML index, and shows the asset (make, model, serial) with a vendor warranty-lookup link.
- **Users:** local accounts and Administrators membership, with enable/disable, admin and password-never-expires toggles, and a password reset that is typed into a Windows console, never into the app.
- **Printers:** every printer with its queue depth, a one-click "clear a stuck queue", and a spooler restart.
- **Profiles:** local user profiles with their size and last-use date, so you can delete stale ones and reclaim disk.
- **Disks:** physical-disk SMART/health with per-volume free-space bars.
- **Drivers:** the signed-driver inventory with version and date, problem (yellow-bang) devices flagged and sorted first, with filter and export.
- **Certificates:** the machine certificate store sorted by soonest expiry, flagged OK / soon / expired.
- **Group Policy:** the applied GPOs and when policy last refreshed (RSoP), plus a full HTML report export.

<div align="center">
  <img src="assets/screenshots/tweaks.png" width="430" alt="Tweaks panel: reversible toggles with search and category filters">
  <img src="assets/screenshots/actions.png" width="430" alt="Quick Actions: one-click maintenance and sysadmin tasks">
</div>

**Everywhere**

- **Command palette:** press `Ctrl+K` to fuzzy-jump to any panel or run any action.
- **Accessible:** WCAG-AA contrast, screen-reader roles and labels (UIA), and full keyboard navigation (Tab, Enter, Space, with focus rings).
- **Two themes:** Holographic (cyan) and HEV (Half-Life amber), your choice persisted.

---

## On Linux

The same holographic deck, distro-agnostic, in two front ends over one native backend (no bundled shell scripts):

- **`neonprime` (GUI):** a Slint window with sixteen panels grouped Monitor / Optimize / System / Maintenance:
  - **Dashboard** (CPU / RAM / temp / load, CPU sparkline, specs), **Processes** (sort + filter + kill), **Network** (outbound connections with reverse-DNS).
  - **Tweaks:** desktop-environment aware, so it drives GNOME `gsettings`, KDE `kwriteconfig`, or XFCE `xfconf-query` as appropriate, plus `sysctl`, across Interface / Performance / Privacy / **Security** (kernel hardening: kptr/dmesg/ptrace restrict, SYN cookies, reverse-path filter, and more, each with a warning).
  - **Debloat** (remove snaps + preinstalled apps), **Packages** (apt / dnf / pacman / zypper / flatpak, with a curated app catalog), **Services** (systemd), **Firewall** (ufw), **DNS** (resolvectl), **Power** (power-profiles-daemon), **Autostart** (XDG), **Cleanup**, **Restore Points** (Timeshift / Snapper), **Quick Actions**.
  - **Graphics:** detect your GPUs and install the right driver/userspace; hybrid (iGPU + dGPU) laptops/desktops are auto-detected and get the correct dGPU launch options for Steam (NVIDIA PRIME render-offload or `DRI_PRIME=1`), one-click `switcheroo-control` for per-app "Run using dedicated graphics card", and one-click GameMode / MangoHud / Gamescope (or the CachyOS gaming meta).
  - **Servers:** install + enable OpenSSH and Samba.
- **`neonprime-tui` (headless):** a `ratatui` terminal UI over the same backend, for servers and SSH sessions. No display or GUI libraries needed at runtime. Privileged actions run through `sudo` (in-terminal prompt) instead of the GUI's `pkexec`. Enable SSH and set up the GPU over an existing SSH connection.

Privileged actions never run inline: the GUI hands them to `pkexec` and the TUI to `sudo`, the Linux analog of the Windows elevated broker. See [LINUX.md](LINUX.md) for the full architecture and status.

## Aesthetic: "holographic glass, calm until it isn't"

| Role | Token |
|------|-------|
| Background | `#061119` |
| Primary, cyan | `#34D2FF` / soft `#8AE9FF` |
| Accent, ember | `#CE8A1F` / soft `#E6AE45` |

Cyan encodes nominal data. Ember encodes attention and the active mode, so your eye learns that amber means *this concerns you*.

## Architecture

Two processes, because a tool that edits your registry should be paranoid:

- **UI:** Slint, runs unelevated, renders the deck, and never touches the system directly.
- **Broker:** a small elevated helper that executes a *whitelisted* set of reversible `Action`s over local IPC. The UI sends action IDs, never command strings.

Rollback, modes, and config export are all the same primitive: a **reversible, declarative system action** (`apply()` / `revert()` / captured prior state). Telemetry is the one read-only pillar.

---

## Build from source

```sh
# Windows (the desktop deck)
cargo run --release

# Linux (GUI or headless TUI)
cargo run --release --bin neonprime-linux
cargo run --release --bin neonprime-tui

cargo test --all      # 82 unit + integration tests
```

One source tree, split by `cfg`: the Windows backend is gated to `cfg(windows)`, the Linux backend to `cfg(target_os = "linux")`, and each platform compiles its own Slint UI (`ui/app.slint` vs `ui/linux.slint`). Building the Linux GUI needs Slint's usual deps (`fontconfig`, `xcb`, `xkbcommon`).

### Installer (MSI)

```sh
./build-installer.ps1 -Version 3.3.0   # -> NeonPrime-3.3.0-Setup.msi
```

Produces a Windows MSI (via WiX 5: `dotnet tool install --global wix --version 5.0.2`) that installs `neonprime.exe`, the elevated `broker.exe`, and the self-contained sensor sidecar to `Program Files\NeonPrime`, with a Start-Menu shortcut, an uninstaller, and major-upgrade handling. No runtime prerequisites on the target.

### Hardware sensors (optional, accurate CPU temp)

Accurate CPU package temperature and motherboard sensors need ring-0 access (an MSR / Super-I/O driver), the same reason HWiNFO ships one. NeonPrime gets these by embedding a small C# sidecar (`sensors/`) built on **LibreHardwareMonitor**, which streams a JSON sensor snapshot to a temp file the app polls.

- **GPU temps (all vendors)** work without elevation. On AMD and Intel the sidecar auto-starts in the background; on NVIDIA, NVML already covers it.
- **CPU package, motherboard temps, and fans** need the LHM driver (ring-0), so they need admin. Click **Enable HW sensors** on the dashboard to launch the sidecar elevated (one UAC).

Build a self-contained sidecar (bundles the .NET runtime, nothing to install on the target):

```sh
./publish-sensors.ps1                        # -> target/debug/sensors
./publish-sensors.ps1 -AppDir target/release
```

---

## Status

**Windows:** 26 panels (Dashboard, Network, Processes, Events, Tweaks, Privacy, Debloat, Cleanup, Startup, Install, Features, MicroWin, Modes, Services, Users, Compliance, Actions, Config, History, plus an IT section: Support, Printers, Profiles, Disks, Drivers, Certificates, Group Policy) grouped Monitor / Optimize / Software / System / IT. Elevated work runs off the UI thread, so a UAC prompt never freezes the window.

**Linux:** a 16-panel GUI plus a headless TUI over the same backend. Compiles and passes CI (fmt / clippy / test) on every push; runtime testing on real hardware is ongoing.

82 tests pass, with a Windows + Linux CI matrix on every push (clippy runs with warnings denied on both).

**Notes and caveats:**

- The elevated broker (HKLM tweaks) needs an interactive UAC prompt, so the elevated end-to-end path is best tried supervised.
- CPU temperature is best-effort via WMI ACPI thermal zones and reads `N/A` on machines that do not expose one; accurate per-core temps come from the LibreHardwareMonitor sidecar.
- MicroWin's ISO build is heavy (admin, ~20 GB, several minutes) and is best validated in Windows Sandbox or a throwaway VM.
- The IPC token is passed on the broker's command line; a future pass moves it to a named pipe with an explicit DACL.

## Stack

`rust` · `slint` · `sysinfo` · `dns-lookup` · shared, then per platform:
- **Windows:** `nvml-wrapper` · `windows` (DXGI / PDH) · `wmi` · `winreg` · `serde` + `toml`
- **Linux:** `ratatui` + `crossterm` (TUI) · `/proc` + `/sys` + `systemctl` / `gsettings` / `resolvectl` / `pkexec`

## Credits

- Crowbar icon (HEV theme): ["Crowbar"](https://game-icons.net/1x1/delapouite/crowbar.html) by Delapouite, [game-icons.net](https://game-icons.net), licensed under [CC BY 3.0](https://creativecommons.org/licenses/by/3.0/). Recolored for theming. All other icons are original.
- Hardware sensors: [LibreHardwareMonitor](https://github.com/LibreHardwareMonitor/LibreHardwareMonitor) (`LibreHardwareMonitorLib`), licensed under [MPL-2.0](https://www.mozilla.org/en-US/MPL/2.0/), used unmodified via the `sensors/` sidecar.

---

<sub>NeonPrime borrows a name Valve filed and never shipped. Seemed only fair to finish what they started, and to count past two.</sub>
