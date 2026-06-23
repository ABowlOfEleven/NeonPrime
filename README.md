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

- **Telemetry HUD** — live GPU / VRAM / CPU / temp / net gauges. Cyan reports, ember warns.
- **Tweaks & debloat** — privacy, performance, telemetry-off, appx removal, service control.
- **Reversible everything** — an action journal with one-click undo and restore points.
- **App installs** — a `winget`-backed picker.
- **Declarative config** — export your setup to TOML, replay it on a clean install.

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

`rust` · `slint` · `sysinfo` · `nvml-wrapper` · `windows-rs` · `serde` + `toml`

## Status

All six phases (P0–P5) are built and tested — full HUD, two themes, the reversible
action engine + elevated broker, the Tweaks / Install / Modes / Config panels, and
declarative export/import. 17 unit + integration tests pass.

**Known limitations / next steps:**
- The elevated broker (HKLM tweaks) needs an interactive UAC prompt, so the
  elevated end-to-end path hasn't been exercised headlessly — it's implemented and
  unit-tested, but try the HKLM tweaks (e.g. "Disable telemetry") supervised.
- Mode `actions` are currently benign markers; real power-plan / service / GPU /
  network actions plug into the same engine as new `Action` variants.
- IPC token is passed on the broker's command line — hardening TODO: named pipe
  with an explicit DACL.

## Credits

- Crowbar icon (HEV theme) — ["Crowbar"](https://game-icons.net/1x1/delapouite/crowbar.html) by Delapouite, [game-icons.net](https://game-icons.net), licensed under [CC BY 3.0](https://creativecommons.org/licenses/by/3.0/). Recolored for theming. All other icons are original.

---

<sub>NeonPrime borrows a name Valve filed and never shipped — seemed only fair to finish what they started, and to count past two.</sub>
