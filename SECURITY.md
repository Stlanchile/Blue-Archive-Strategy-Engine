# Security policy

## Supported runtime

The secure local loader is supported on Linux x86_64. Android keeps guarded
library behavior; macOS and Windows are compile-checked but secure loading
fails closed rather than using a weaker pathname fallback. See
[`docs/THREAT_MODEL.md`](docs/THREAT_MODEL.md).

## Reporting a vulnerability

Do not open a public issue for a suspected vulnerability. Send a concise report
to `admin@xhz.email` with affected version, reproduction steps, impact, and any
suggested mitigation. Please allow time for acknowledgement and coordinated
remediation before public disclosure.

Reports about path traversal, symlink handling, descriptor races, unsafe file
types, malformed JSON bypasses, resource exhaustion, result reproducibility,
or release-artifact integrity are in scope. Gameplay-data disagreements are
normally data-quality issues, not security vulnerabilities.

## Dependency checks

Security review uses a fresh online `cargo audit --deny warnings` for release
verification. A network or advisory-database failure is an environment failure,
not a clean audit. Local investigation may use `cargo audit --no-fetch`, but it
does not replace a fresh online release check.
