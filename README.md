# NeonPrime

> One machine. Three modes. It counts all the way to three.

A holographic system control deck for Windows — like [WinUtil](https://github.com/ChrisTitusTech/winutil), but more powerful and actually beautiful. Debloat it, tune it, watch it, and reshape it from a single cyan-and-ember HUD.

Built in **Rust**. Drawn in **Slint**.

---

## Why

WinUtil walked so this could fly. NeonPrime keeps the one-stop-Windows-control idea and adds the parts a PowerShell script in a WPF box never could:

- a live telemetry HUD that looks like it belongs on a starship
- every tweak reversible, with a rollback journal — debloat without the dread
- one-click system modes that swap your machine's whole personality
- your entire tuned setup exportable to a fresh install

## The three modes

The heart of NeonPrime. One click changes who your PC is:

| Mode | What it does |
|------|--------------|
| ◇ **AI / Inference** | GPU unleashed, background bloat suspended, VRAM cleared for models |
| ◇ **Game** | Game-process priority, latency-tuned networking, inference paused |
| ◇ **Work** | Balanced power, notifications tamed — the quiet profile |

*(Three of them. We counted.)*

## Features

- **Telemetry HUD** — live GPU load / VRAM / CPU / temps. Vendor-neutral GPU stats (NVIDIA / AMD / Intel) via DXGI + PDH; GPU temp via NVML; best-effort CPU temp via WMI. Cyan reports, ember warns.
- **System specs** — OS / CPU / GPU / RAM / live uptime strip on the dashboard.
- **Tweaks & debloat** — 24 reversible tweaks across Interface / Privacy / Performance, with **live search + category filter**.
- **Reversible everything** — an action journal with one-click **undo last**. Failures self-correct the toggle.
- **System modes** — one-click AI / Game / Work, persisted across restarts.
- **Quick Actions** — restart Explorer, flush DNS, clear temp, empty Recycle Bin, create restore point.
- **Startup manager** — enable/disable per-user startup apps (reversibly).
- **App installs** — a `winget`-backed picker.
- **Declarative config** — export your setup to TOML, replay it on a clean install.
- **Two themes** — Holographic (cyan) and HEV (Half-Life amber), the choice persisted.

## Aesthetic — "holographic glass, calm until it isn't"

| Role | Token |
|------|-------|
| Background | `#061119` |
| Primary · cyan | `#34D2FF` / soft `#8AE9FF` |
| Accent · ember | `#CE8A1F` / soft `#E6AE45` |

Cyan encodes nominal data. Ember encodes attention and the active mode — so your eye learns that amber means *this concerns you*.

## Architecture

Two processes, because a tool that edits your registry should be paranoid:

- **UI** — Slint, runs unelevated, renders the deck and never touches the system directly.
- **Broker** — a small elevated helper that executes a *whitelisted* set of reversible `Action`s over local IPC. The UI sends action IDs, never command strings.

Rollback, modes, and config-export are all the same primitive: a **reversible, declarative system action** (`apply()` / `revert()` / captured prior state). Telemetry is the one read-only pillar.

## Roadmap

- [x] **P0** — Slint shell + the holographic visual language
- [x] **P1** — Telemetry HUD (live GPU / VRAM / CPU / temp via NVML + sysinfo)
- [x] **P2** — Reversible action engine + rollback journal + elevated broker (IPC)
- [x] **P3** — Tweak catalog with live apply/revert + winget install panel
- [x] **P4** — System modes (AI / Game / Work), persisted active mode
- [x] **P5** — Declarative config export / import (TOML)

## Stack

`rust` · `slint` · `sysinfo` · `nvml-wrapper` · `windows` (DXGI / PDH) · `wmi` · `winreg` · `serde` + `toml`

## Status

Phases P0–P5 plus the Dashboard / Tweaks / Install / Modes / Config / Actions /
Startup panels are built and tested. Elevated work runs off the UI thread (no
freeze during UAC). 20 unit + integration tests pass.

**Known limitations / next steps:**
- The elevated broker (HKLM tweaks) needs an interactive UAC prompt, so the
  elevated end-to-end path hasn't been exercised headlessly — implemented and
  unit-tested; try the HKLM tweaks (e.g. "Disable telemetry") supervised.
- **CPU temperature** is best-effort via WMI ACPI thermal zones and reads `N/A`
  on machines that don't expose one; accurate per-core temps need a driver
  (LibreHardwareMonitor). **GPU temperature** is NVIDIA-only (NVML); AMD/Intel
  GPU temp would need a vendor SDK.
- Mode `actions` are currently benign markers; real power-plan / service / GPU /
  network actions plug into the same engine as new `Action` variants.
- IPC token is passed on the broker's command line — hardening TODO: named pipe
  with an explicit DACL.

## Hardware sensors (optional, accurate CPU temp)

Accurate CPU package temperature and motherboard sensors need ring-0 access (an
MSR/Super-I/O driver) — the same reason HWiNFO ships one. NeonPrime gets these by
embedding a small C# sidecar (`sensors/`) built on **LibreHardwareMonitor**, which
streams a JSON sensor snapshot to a temp file the app polls. Click **"Enable HW
sensors"** on the dashboard to launch it elevated (one UAC); CPU/board temps and
fans then light up. GPU temps work without elevation.

Build + stage the sidecar next to the app binary:

```
dotnet build sensors/NeonPrime.Sensors.csproj -c Release
# copy sensors/bin/Release/net9.0-windows/* → target/debug/sensors/
```

## Credits

- Crowbar icon (HEV theme) — ["Crowbar"](https://game-icons.net/1x1/delapouite/crowbar.html) by Delapouite, [game-icons.net](https://game-icons.net), licensed under [CC BY 3.0](https://creativecommons.org/licenses/by/3.0/). Recolored for theming. All other icons are original.
- Hardware sensors — [LibreHardwareMonitor](https://github.com/LibreHardwareMonitor/LibreHardwareMonitor) (`LibreHardwareMonitorLib`), licensed under [MPL-2.0](https://www.mozilla.org/en-US/MPL/2.0/), used unmodified via the `sensors/` sidecar.

---

<sub>NeonPrime borrows a name Valve filed and never shipped — seemed only fair to finish what they started, and to count past two.</sub>
