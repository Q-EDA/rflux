# Security Policy

## Reporting a Vulnerability

The rflux maintainers take security reports seriously. **Please do not file
public GitHub issues for security problems.**

To report a vulnerability:

1. Email the maintainers at **security@q-eda.dev** (or, if unavailable, open a
   private security advisory via GitHub: **Security → Advisories → Report a
   vulnerability** on https://github.com/Q-EDA/rflux).
2. Include a clear description of the issue, the affected version/commit, a
   minimal reproduction, and any known impact.
3. You will receive an acknowledgement within **5 business days** and a
   triage decision (severity + fix plan) within **30 days**.

Please do not disclose the issue publicly until a fix has been released and
you have been given the all-clear.

## Scope

This policy covers the `rflux` source tree at https://github.com/Q-EDA/rflux,
including the Rust workspace (`crates/*`), the Python bindings
(`crates/py`, `python/rflux`), and the CI configuration. It does **not** cover
third-party dependencies — report those to their upstream maintainers first.

In particular, rflux contains an external-command surface in the simulation
path (calling `josim` / `josim-cli`). As documented in
[docs/external-command-policy.md](docs/external-command-policy.md) and
[docs/archive/security-compliance.md](docs/archive/security-compliance.md),
the `external_command` argument is treated as **trusted operator input**, not
as a public-facing security boundary. Reports rooted in attacker-controlled
`external_command` values are therefore considered out of scope for this
policy.

## Supported Versions

Only the latest release line receives security fixes.

| Version | Supported          |
|---------|--------------------|
| `0.1.x` | :white_check_mark: |
| < 0.1   | :x:                |

## Security Baseline

The currently enforced security/compliance baseline (advisory scanning,
license inventory, external-command allowlist, dependency review) is
documented in
[docs/archive/security-compliance.md](docs/archive/security-compliance.md).
Known gaps (no formal SBOM release process, no external-command sandbox, soft
advisory gates) are listed there and tracked as open work.
