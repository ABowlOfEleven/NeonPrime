# NeonPrime on Linux

NeonPrime started as a Windows-first system control deck. This document tracks
the Linux port: what works, how the code is split, and what is left before a
native Linux release.

## Status

| Area | State |
| --- | --- |
| Cross-platform compile (one source tree) | Done. The crate and Linux backend build on Linux; the Windows build is unchanged. |
| Linux system backend (`src/core/linux`) | Done (scaffold). Telemetry, processes, network, systemd, packages, DNS, with unit tests. |
| Linux desktop UI binary | Not started. The default binary is a stub on Linux; the real UI is next. |
| Packaging (AppImage, tarball, deb, rpm) | Scripts + CI in place; produce validation artifacts until the UI lands. |

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

## Building and packaging locally

```sh
# Compile + test the Linux backend
cargo test --lib

# Portable tarball, deb, rpm, AppImage
packaging/linux/build-tarball.sh
cargo deb            # needs: cargo install cargo-deb
cargo generate-rpm   # needs: cargo install cargo-generate-rpm
packaging/linux/build-appimage.sh   # needs: rsvg-convert or ImageMagick
```

CI runs the Windows and Linux build/test matrix on every push
(`.github/workflows/ci.yml`). `.github/workflows/package-linux.yml` builds the
Linux artifacts on demand; it stays manual until the Linux UI exists, at which
point it should trigger on version tags and publish to the release.

## What is next

1. A Linux UI binary (`src/bin/neonprime-linux.rs` or a `cfg`-selected `main`)
   that brings up the Slint shell against the Linux backend, starting with the
   monitor panels (dashboard, processes, network) and then the management panels
   (services, packages, DNS).
2. Promote shared UI/backend traits into `src/platform.rs`.
3. GPU telemetry on Linux (NVML for NVIDIA, `/sys/class/drm` + amdgpu/i915 hwmon
   for AMD/Intel), and IPv6 connections (`/proc/net/tcp6`).
4. Flip `package-linux.yml` to publish real release artifacts.
