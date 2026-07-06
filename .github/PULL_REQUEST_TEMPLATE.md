<!-- Thanks for contributing! See CONTRIBUTING.md. -->

## What does this change?

<!-- A short summary of the change and why. Link any related issue. -->

## Platforms tested

- [ ] Windows
- [ ] Linux (GUI)
- [ ] Linux (TUI)
- [ ] Docs only

## Checklist

- [ ] `cargo fmt --all` is clean
- [ ] `cargo clippy --all-targets` is clean
- [ ] `cargo test --all` passes
- [ ] New system tweaks/actions are reversible and privileged work goes through
      the broker / `pkexec` / `sudo` (not inline from the UI)
- [ ] Behavior-changing or security toggles carry a clear warning
- [ ] Docs updated if needed (README / LINUX.md / CHANGELOG)
