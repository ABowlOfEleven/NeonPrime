# Security Policy

NeonPrime makes privileged changes to the systems it runs on (registry, services,
packages, firewall, and more), so security is taken seriously. This document
covers how to report a vulnerability and the security model you are trusting.

## Supported versions

| Version | Supported |
| --- | --- |
| 3.1.x | Yes |
| < 3.1 | No (please update) |

## Reporting a vulnerability

**Please do not open a public issue for security vulnerabilities.**

Report privately through GitHub's **[private vulnerability reporting](https://github.com/ABowlOfEleven/NeonPrime/security/advisories/new)**
(the repository's Security tab → "Report a vulnerability"). Include:

- affected platform and version,
- a description of the issue and its impact,
- steps to reproduce (a proof of concept if you have one).

You can expect an acknowledgement within a few days. We will work with you on a
fix and coordinate disclosure. Please give us a reasonable window to release a
patch before any public disclosure.

## Security model

Understanding the design helps you assess risk and report meaningful issues.

- **Least privilege.** The UI runs unelevated and never touches the system
  directly. On Windows, privileged operations go to a small **elevated broker**
  that executes a *whitelisted* set of reversible actions over local IPC; the UI
  sends action identifiers, never raw command strings. On Linux, privileged
  operations are run as explicit commands through `pkexec` (GUI) or `sudo` (TUI),
  after the user confirms them.
- **Reversibility.** System tweaks are declarative and reversible, backed by a
  rollback journal, so a change can be undone deterministically.
- **No hidden network activity.** NeonPrime does not phone home. The network
  panel exists to show you what *other* processes are doing.

### Known limitations (by design, tracked)

- On Windows the broker's one-time IPC token is currently passed on its command
  line; a hardening pass to a named pipe with an explicit DACL is planned.
- The Linux binaries are a **preview**: they compile and pass CI on every push,
  but have not yet had extensive real-hardware runtime testing.

## Scope

In scope: privilege escalation, injection into the broker/IPC path, actions that
run outside the intended whitelist, and anything that causes NeonPrime to make an
unintended or irreversible system change. Out of scope: issues that require an
already-compromised machine or physical access, and the inherent risk of the
system-modification features being used as designed.
