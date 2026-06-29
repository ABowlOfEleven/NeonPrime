# Sandbox testing

Windows Sandbox gives a **disposable, clean Windows 11 VM** that resets on close.
It's the safe place to actually exercise NeonPrime's destructive buttons —
harden, Appx removal, DISM features, power plans, fixes — without mutating the
host.

## One-time setup

1. Enable the optional feature (needs admin + a reboot). Either:
   - Use NeonPrime itself: **Features → Windows Sandbox → ENABLE**, or
   - Run elevated: `Enable-WindowsOptionalFeature -Online -FeatureName "Containers-DisposableClientVM" -All -NoRestart`
2. **Reboot.**

## Each test run

```pwsh
pwsh sandbox\build-sandbox.ps1        # debug build
pwsh sandbox\build-sandbox.ps1 -Release
```

Then double-click `sandbox\neonprime.wsb`. The sandbox opens with the staged
build mapped read-only at `C:\NeonPrime` (an Explorer window opens there);
launch `neonprime.exe`.

Notes:
- The mapped folder is read-only; the app writes its journal/settings to the
  sandbox's own `%APPDATA%`, thrown away on close.
- vGPU is enabled, but sandbox GPU telemetry is limited — the UI renders fine,
  some sensor values may read low/blank inside the VM.
- `share/` is git-ignored (build artifacts).
