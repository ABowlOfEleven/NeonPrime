# NeonPrime on Linux

NeonPrime started as a Windows-first system control deck. This document tracks
the Linux port: what works, how the code is split, and what is left before a
native Linux release.

## Status

| Area | State |
| --- | --- |
| Cross-platform compile (one source tree) | Done. The crate and Linux backend build on Linux; the Windows build is unchanged. |
| Linux system backend (`src/core/linux`) | Done. Telemetry, processes, network, systemd, packages, DNS, tweaks, cleanup, power, firewall, autostart, debloat, restore, quick, app catalog. Unit-tested. |
| Linux desktop UI (`neonprime-linux`) | Done. Slint window, fourteen panels; verified on Linux CI. |
| Linux terminal UI (`neonprime-tui`) | Done. Headless ratatui UI over the same backend; verified on Linux CI. |
| Packaging (AppImage, tarball, deb, rpm) | Builds and ships both binaries (`neonprime` GUI + `neonprime-tui`). |

The GUI is `ui/linux.slint` + `src/bin/neonprime-linux.rs`, sharing the
holographic look of the Windows deck. It has fourteen panels covering every
Windows feature with a real Linux analog:

- Dashboard: live CPU / RAM / CPU-temp meters, a CPU sparkline, and specs.
- Tweaks: desktop-environment aware (gsettings / KConfig / xfconf) + sysctl,
  Interface / Performance / Privacy, live applied-state, searchable.
- Debloat: remove snapd and preinstalled apps, with live installed-state.
- Processes: sortable (CPU / RAM / name) + name-filterable, kill.
- Network: outbound connections with cached reverse-DNS.
- Firewall: ufw enabled state + enable / disable / reset.
- Services: systemd list (lazy) + start / stop / enable / disable.
- Packages: detected managers (apt / dnf / pacman / zypper / flatpak), a curated
  app catalog (category chips + search), install / remove / update-all.
- DNS: provider switch on the default-route link (resolvectl).
- Cleanup: cache / Trash / package-cache / journald sizing, sorted with bars.
- Power: power-profiles-daemon (power-saver / balanced / performance).
- Autostart: XDG autostart entries, toggled via the Hidden key.
- Restore Points: create Timeshift / Snapper snapshots, open the tool GUI.
- Quick Actions: flush DNS, clear cache, empty Trash, drop memory caches,
  remove orphaned packages, trim SSDs, enable Flathub.

The TUI (`neonprime-tui`) is a headless / SSH-friendly ratatui front end over
the same backend, covering the same action panels plus a live Dashboard. It
needs no display or GUI libraries at runtime; privileged actions suspend the
TUI and run through `sudo` (in-terminal prompt) rather than the GUI's graphical
`pkexec`. The GUI binary prints a pointer to it if it cannot open a display.

Privileged GUI actions run through `pkexec`. The Windows-only panels (registry
tweaks beyond gsettings, DISM Features, Appx internals, MicroWin, the Privacy
score, and the reversible History over the elevated broker) have no Linux
equivalent by nature.

> Note: because Slint's Linux stack needs `fontconfig`/`xcb` at build time, the
> Linux UI can only be compiled on Linux (or a Linux CI runner), not
> cross-checked from Windows. The Linux *backend* (no Slint) still cross-checks
> from any host.

## How the split works

One crate, gated by `cfg`:

- `src/main.rs` is a thin dispatcher. On Windows it `include!`s `src/app_win.rs`
  (the full Slint desktop app). On other targets it is a stub `main` that points
  here. The broker splits the same way (`src/bin/broker.rs` +
  `src/broker_win.rs`).
- `src/core/mod.rs` gates every Windows module behind `cfg(windows)` and exposes
  `src/core/linux` behind `cfg(target_os = "linux")`.
- `src/platform.rs` is the OS-neutral seam. As the Linux UI is built, shared
  traits both backends implement will live here so UI code stops branching on OS.
- `Cargo.toml` target-gates the Windows-only crates (`windows`, `winreg`, `wmi`,
  `nvml-wrapper`, `winresource`, and for now `slint`, `serde`, `toml`,
  `single-instance`). The Linux library builds with just `sysinfo` + `dns-lookup`
  and the standard library, so it needs no GUI or system `-dev` packages to
  compile and test.

## Linux backend (`src/core/linux`)

Read-only for listing; privileged actions are returned as `ElevatedCmd` values
(a `pkexec`-ready argv plus a human summary) that the UI runs after explicit
confirmation. This mirrors the Windows model of handing scripts to the elevated
broker rather than mutating the system inline.

| Module | Source | Windows counterpart |
| --- | --- | --- |
| `telemetry` | `sysinfo` + `/sys/class/hwmon` | DXGI/PDH/NVML + `sysinfo` |
| `procmon` | `sysinfo`, `kill(1)` | PDH per-process GPU + `taskkill` |
| `netmon` | `/proc/net/tcp` + `/proc/<pid>/fd` | `GetExtendedTcpTable` |
| `services` | `systemctl` | Service Control Manager |
| `pkg` | apt / dnf / pacman / zypper / flatpak | winget |
| `dns` | `resolvectl` | `netsh` |
| `cleanup` | dir walk + `pkexec` package/journal clean | Cleanup panel |
| `power` | `powerprofilesctl` | power plans / System Modes |
| `quick` | one-shot maintenance commands | Quick Actions |
| `firewall` | ufw (`/etc/ufw/ufw.conf` + `pkexec ufw`) | Windows Firewall |
| `autostart` | XDG `~/.config/autostart` + `/etc/xdg/autostart` | Startup |
| `tweaks` | gsettings / KConfig / xfconf / sysctl | Tweaks + Privacy |
| `debloat` | dpkg/rpm/pacman probe + `pkexec` remove | Debloat |
| `restore` | Timeshift / Snapper | Restore Points |
| `apps` | per-manager names + Flathub ids | Install catalog |

## Building and packaging locally

```sh
# Compile + test the Linux backend
cargo test --lib

# Run the GUI, or the terminal UI (headless)
cargo run --bin neonprime-linux
cargo run --bin neonprime-tui

# Portable tarball, deb, rpm, AppImage (ship both binaries)
packaging/linux/build-tarball.sh
cargo deb            # needs: cargo install cargo-deb
cargo generate-rpm   # needs: cargo install cargo-generate-rpm
packaging/linux/build-appimage.sh   # needs: rsvg-convert or ImageMagick
```

CI runs the Windows and Linux build/test matrix on every push
(`.github/workflows/ci.yml`). `.github/workflows/package-linux.yml` builds the
Linux artifacts on demand; it stays manual until the Linux UI is confirmed
running on hardware, then it should trigger on version tags and publish to the
release.

## What is next

1. Run `neonprime-linux` on real hardware (CI compiles and tests it, but a GUI
   can't be exercised headlessly). Once confirmed, flip `package-linux.yml` to
   trigger on version tags and publish the AppImage / tarball / deb / rpm to the
   release alongside the Windows MSI.
2. Promote shared UI/backend traits into `src/platform.rs` so the two UIs stop
   duplicating structure.
3. GPU telemetry on Linux (NVML for NVIDIA, `/sys/class/drm` + amdgpu/i915 hwmon
   for AMD/Intel), and IPv6 connections (`/proc/net/tcp6`).
4. Nice-to-haves: per-process GPU on Linux, firewalld support alongside ufw, and
   a curated Packages catalog (the winget-catalog analog).
