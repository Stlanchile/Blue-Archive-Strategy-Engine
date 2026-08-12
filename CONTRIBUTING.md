# Contributing

Thank you for improving this local research tool. Please keep changes narrow,
deterministic, and compatible with the documented schema-v1 contract.

## Development

Use Rust 1.95.0 and the checked-in lockfile. Before proposing a change, run:

```text
cargo fmt --all -- --check
cargo build --workspace --all-targets --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked
cargo bench --workspace --no-run --locked
```

Do not modify `Cargo.lock` incidentally. Dependency changes must be intentional,
reviewed with their manifests and resolved lockfile, and then all ordinary Cargo
commands must continue to use `--locked`.

## Data and compatibility

- Put shipped provisional runtime documents only under `data/`.
- Put user-facing examples under `scenarios/examples/`; they must reference only
  shipped data.
- Keep frozen regression scenarios under `scenarios/golden/`.
- Put fictional, adversarial, or schema-boundary data only under
  `tests/fixtures/`; never present it as gameplay data.
- Preserve schema-v1 bytes, validation precedence, result fields, fingerprints,
  seeds, traces, and command behavior unless an explicitly documented exception
  applies.

Please add focused tests for new behavior, including behavioral-versus-document
fingerprint assertions when changing schema-v2 provenance. Do not add remote
ingestion, executable policies, plugins, parallel Monte Carlo, or more than two
targets without a separately reviewed design.

## Security and conduct

Do not weaken descriptor-relative loading, no-follow descendant policy, limits,
or fail-closed behavior. Report security issues privately as described in
[`SECURITY.md`](SECURITY.md), rather than opening a public issue first.

By contributing, you agree that your contribution may be distributed under the
MIT and Apache-2.0 licenses, without assigning copyright to a named individual.
