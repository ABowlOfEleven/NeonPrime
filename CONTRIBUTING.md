# Contributing to NeonPrime

Thanks for your interest. NeonPrime is a Rust system-control deck for Windows and
Linux, and it edits real systems, so the bar for changes is: reversible, honest,
and it must not surprise the user.

By participating you agree to the [Code of Conduct](CODE_OF_CONDUCT.md). For
anything security-related, see [SECURITY.md](SECURITY.md) and do **not** open a
public issue.

## Ways to help

- **Bug reports** and **feature requests**: open an issue (templates provided).
- **Docs**: fixes and clarifications are always welcome.
- **Code**: enhancements, new panels, better platform coverage, more tweaks.
- **Real-hardware testing** of the Linux builds (GUI and TUI) across desktops and
  distros is especially valuable right now.

## Development setup

You need a recent stable Rust toolchain.

```sh
# Windows (the full desktop deck)
cargo run --release

# Linux (GUI or headless TUI)
cargo run --release --bin neonprime-linux
cargo run --release --bin neonprime-tui
```

Building the Linux GUI needs Slint's usual dependencies: `fontconfig`, `xcb`, and
`xkbcommon` dev packages.

## Before you open a PR

Run the same checks CI runs. PRs must pass the Windows **and** Linux CI matrix.

```sh
cargo fmt --all
cargo clippy --all-targets
cargo test --all
```

## How the codebase is organized

One crate, split by platform with `cfg`:

- `src/core/` is the domain. Windows modules are gated `#[cfg(windows)]`; the
  Linux backend lives in `src/core/linux/` gated `#[cfg(target_os = "linux")]`.
- The UI is Slint. Windows uses `ui/app.slint` (spliced in via `src/app_win.rs`);
  Linux uses `ui/linux.slint` (`src/bin/neonprime-linux.rs`), plus a `ratatui`
  TUI in `src/bin/neonprime-tui.rs`. `build.rs` compiles the right `.slint` per
  target.
- See [LINUX.md](LINUX.md) for the full cross-platform architecture.

Because Slint's Linux stack needs system libraries, the Linux UI compiles only on
Linux (or a Linux CI runner). You can still validate a `.slint` file's syntax
from any host: temporarily point `build.rs` at it and run `cargo build`, then
revert. CI is the source of truth for the Linux build.

## Conventions that matter

- **Reversibility.** Every tweak carries an explicit apply/revert (Windows: an
  `on`/`off` action set plus a registry probe; Linux: an `on`/`off` value read
  back live). Do not add a tweak that cannot be cleanly undone.
- **Never mutate the system inline from the UI.** Privileged work goes through
  the elevated broker (Windows) or a `pkexec`/`sudo` command (Linux). The UI
  sends intent, not command strings.
- **Warn honestly.** Security or behavior-changing toggles must carry a `warn`
  explaining what they do and what could break.
- **Match the surrounding code.** Comment density, naming, and idiom should look
  like the file you are editing.

## Commits and PRs

- Keep commits focused with a clear subject line describing the outcome.
- Fill in the PR template. Describe what changed, how you tested it, and which
  platforms you verified.
- Small, reviewable PRs merge faster than large ones.

Thanks for helping NeonPrime count past two.
