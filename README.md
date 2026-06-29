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

- **Telemetry HUD** — live GPU load / VRAM / CPU / temps with rolling **CPU + GPU sparklines**. Vendor-neutral GPU stats (NVIDIA / AMD / Intel) via DXGI + PDH; GPU temp via NVML; best-effort CPU temp via WMI. Cyan reports, ember warns.
- **System specs** — OS / CPU / GPU / RAM / live uptime strip on the dashboard.
- **Privacy Shield** — a live **hardening score** gauge that reads your real registry/service state across 11 privacy checks and hardens any exposed item with one click (all reversible).
- **Tweaks & debloat** — 29 reversible tweaks across Interface / Privacy / Performance (including service-control tweaks), with **live search + category filter** and a one-click **Essential Tweaks** preset.
- **Reversible everything** — an action journal with a full **History timeline**: revert any past change (or all of them), not just the last. Failures self-correct the toggle.
- **System modes & power plans** — one-click AI / Game / Work that actually *do* things: each applies a reversible bundle (Game DVR, background apps, notifications) and switches the power plan, all undoable; click an active mode to turn it off. Plus a Balanced / High Performance / Ultimate switcher.
- **Process manager** — top processes by CPU/RAM with **per-process GPU% and VRAM**, plus a kill button.
- **Network monitor** — live outbound TCP connections per process (remote IP:port + state), refreshing while open — see what's phoning home, then **block any app** at the firewall in one click. Plus a DNS switcher (Cloudflare / Google / Quad9 / automatic).
- **Services manager** — searchable list of every service with start/stop and start-type (auto/manual/disabled).
- **MicroWin** — build a slimmed, debloated Windows ISO: strip bundled apps, apply offline privacy tweaks, and inject an autounattend that bypasses TPM/SecureBoot/RAM and skips OOBE.
- **Disk cleanup** — scan reclaimable space (temp, Recycle Bin, thumbnails, system/update caches) and clear it per-target.
- **Command palette** — Ctrl+K to fuzzy-jump to any panel or run any action.
- **Accessible** — WCAG-AA contrast, screen-reader roles/labels (UIA), and full keyboard navigation (Tab / Enter / Space with focus rings).
- **Quick Actions** — restart Explorer, flush DNS, clear temp, empty Recycle Bin, create restore point, install the NeonPrime PowerShell profile.
- **Startup manager** — enable/disable per-user startup apps (reversibly).
- **App installs** — a `winget`-backed picker with **194 apps** imported from WinUtil's catalog + live search.
- **Debloat** — remove preinstalled UWP apps (Copilot, Xbox Game Bar, etc.) per-user with live installed/removed state, plus one-click telemetry scheduled-task disabling.
- **Fixes** — elevated SFC + DISM repair, network-stack reset, Windows Update reset.
- **Windows Update** — Default / Security-only / Disabled mode selector.
- **Windows Features** — DISM enable/disable for .NET 3.5, Hyper-V, Sandbox, WSL, IIS, and more.
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

16 panels — Dashboard / Network / Processes / Tweaks / Privacy / Debloat /
Cleanup / Startup / Install / Features / MicroWin / Modes / Services / Actions /
Config / History — grouped into Monitor / Optimize / Software / System in a
scrollable sidebar. Elevated work runs off the UI thread (no freeze during UAC).
47 unit + integration tests pass.

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
streams a JSON sensor snapshot to a temp file the app polls.

- **GPU temps (all vendors)** work without elevation. On AMD/Intel the sidecar is
  auto-started in the background so GPU temperature just works; on NVIDIA, NVML
  already covers it.
- **CPU package + motherboard temps + fans** need the LHM driver (ring-0), so they
  need admin — click **"Enable HW sensors"** on the dashboard to launch the sidecar
  elevated (one UAC).

Build a **self-contained** sidecar (bundles the .NET runtime — nothing to install
on the target) and stage it next to the app binary:

```
./publish-sensors.ps1                       # → target/debug/sensors
./publish-sensors.ps1 -AppDir target/release
```

## Installer

```
./build-installer.ps1          # → NeonPrime-3.0.0-Setup.msi
```

Produces a Windows MSI (via WiX 5 — `dotnet tool install --global wix --version 5.0.2`)
that installs `neonprime.exe`, the elevated `broker.exe`, and the **self-contained**
sensor sidecar to `Program Files\NeonPrime`, with a Start-Menu shortcut, an
uninstaller, and major-upgrade handling. No runtime prerequisites on the target.

## Credits

- Crowbar icon (HEV theme) — ["Crowbar"](https://game-icons.net/1x1/delapouite/crowbar.html) by Delapouite, [game-icons.net](https://game-icons.net), licensed under [CC BY 3.0](https://creativecommons.org/licenses/by/3.0/). Recolored for theming. All other icons are original.
- Hardware sensors — [LibreHardwareMonitor](https://github.com/LibreHardwareMonitor/LibreHardwareMonitor) (`LibreHardwareMonitorLib`), licensed under [MPL-2.0](https://www.mozilla.org/en-US/MPL/2.0/), used unmodified via the `sensors/` sidecar.

---

<sub>NeonPrime borrows a name Valve filed and never shipped — seemed only fair to finish what they started, and to count past two.</sub>
