**English** | [简体中文](README.zh-CN.md)

# Blue Archive Strategy Engine

`ba-strategy` 0.2.0 is a local Rust probability engine for one or two ordered
Blue Archive recruitment targets. It supports exhaustive analysis, reproducible
serial Monte Carlo, trace/replay, and exact-versus-simulation comparison.

This is an unofficial fan/research tool. It is not affiliated with, endorsed
by, or a source of official information from Nexon, Yostar, Blue Archive, or
their affiliates. The repository contains no copyrighted game assets.

The shipped data is explicitly provisional. In particular,
`jp_2026_07_29_provisional_v2` is not an independently verified or official
statement of game rules.

## Build and run

Rust 1.95.0, rustfmt, and Clippy are selected by
[`rust-toolchain.toml`](rust-toolchain.toml). Use the checked-in lockfile.

```text
cargo build --workspace --locked

cargo run --locked -p ba-cli --bin ba-strategy -- \
  catalog list all --format json

cargo run --locked -p ba-cli --bin ba-strategy -- \
  --scenario-dir scenarios/examples analyze example_single_target_v2 --format json

cargo run --locked -p ba-cli --bin ba-strategy -- \
  scenario template --scenario-id generated_v2 \
  --ruleset jp_2026_07_29_provisional_v2 \
  --reward-schedule jp_2026_07_29_empty_v2 --target-count 2
```

Existing commands remain available: `validate`, `analyze`, `simulate`, and
`compare`. New local-inspection commands are `catalog list`, `catalog inspect`,
`scenario explain`, and `scenario template`. JSON catalog, inspection, and
explanation outputs include an explicit output schema version. Template output
is valid schema-v2 JSON on stdout and defaults to ticket-first funding with a
200-recruitment horizon.

`--data-dir` defaults to `./data`. With `--scenario-dir <PATH>`, a bare name
such as `foo` or `foo.json` resolves to `<PATH>/foo.json`; `./foo.json`,
`../foo.json`, nested paths, and absolute paths remain explicit paths. Without
`--scenario-dir`, bare names resolve against `scenarios/golden/`.

`validate --diagnostics` emits a diagnostics-schema-v1 error envelope on
failure, including a stable class/code/message and, when available, a pointer,
line/column, and corrective hint. Successful validation reports include
behavior and document fingerprints. Successful output is written to stdout;
failures are written to stderr with no authoritative stdout result.

| Exit | Meaning |
|---:|---|
| 0 | Success, including normal domain terminal outcomes |
| 2 | Command-line usage |
| 3 | JSON, schema, or domain validation |
| 4 | Catalog/filesystem/entropy I/O or rejection limit |
| 5 | Engine guard, arithmetic, transition, or probability invariant |
| 70 | Unexpected typed internal failure |

## Inputs and compatibility

`data/` is shipped provisional runtime data; `scenarios/examples/` contains
authoring examples using only shipped data; `scenarios/golden/` contains frozen
regression scenarios; and `tests/fixtures/` is synthetic/adversarial test data.
The `synthetic_custom_*` fixtures are deliberately non-gameplay mechanics and
are never runtime catalog data or release-facing examples.

All input documents use schema version 2. Schema version 1 was an unused
development format and is not accepted. Successful analysis uses engine
semantics 2 and result schema 2. Details are in
[`docs/SCHEMA_V2.md`](docs/SCHEMA_V2.md).

V2 results distinguish behavior fingerprints from document fingerprints.
Changing only v2 provenance changes document identity/provenance output but
does not change mechanics, strategy decisions, exact or Monte Carlo behavior,
or per-run seed derivation. See [`docs/SCHEMA_V2.md`](docs/SCHEMA_V2.md).

## Security model

On Linux and Android, a user-selected ambient root (`--data-dir`,
`--scenario-dir`, or an explicit document parent) may be followed once and is
then pinned. Descendants are resolved descriptor-relatively with no symlink
following. Rulesets and rewards load as one checked data-root generation, so a
concurrent observable replacement fails closed with
`catalog_generation_changed` rather than returning a mixed catalog. Other
platforms fail closed for secure loading. This is a filesystem consistency
boundary, not cryptographic integrity against a sufficiently privileged actor.
See [`docs/THREAT_MODEL.md`](docs/THREAT_MODEL.md).

## Model and limits

Actions are atomic: a locked banner continues for every primitive draw even
after an early pickup, and policy is reevaluated only at the action boundary.
Every scenario requires an explicit positive horizon and an exact permutation
of `ticket_ten` and `paid_single` as funding priority. See
[`docs/STRATEGIES.md`](docs/STRATEGIES.md).

Document limits are 1 MiB, JSON depth 64, 512 inspected immediate directory
entries, and 256 retained JSON candidates. Exact analysis enumerates all
modeled nonzero branches without probability pruning. Guard calibration and
environment-specific benchmark observations are recorded in
[`docs/CALIBRATION.md`](docs/CALIBRATION.md).

## Workspace and releases

```text
ba-cli -> ba-engine -> ba-core
```

- `ba-core`: strict input, secure catalogs, validation, fingerprints, strategy,
  and the pure transition kernel.
- `ba-engine`: exact propagation, Monte Carlo, trace/replay, comparison, and
  result projections.
- `ba-cli`: argument parsing, resolution, command execution, rendering, and
  error mapping.

The project is not published to crates.io and this implementation does not tag,
push, publish, or create releases. Release readiness and its least-privilege
boundaries are described in [`docs/RELEASING.md`](docs/RELEASING.md).

Contributing, security reporting, and the dual MIT/Apache-2.0 terms are in
[`CONTRIBUTING.md`](CONTRIBUTING.md), [`SECURITY.md`](SECURITY.md),
[`LICENSE-MIT`](LICENSE-MIT), and [`LICENSE-APACHE`](LICENSE-APACHE).
