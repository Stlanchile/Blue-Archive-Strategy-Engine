# Blue Archive Strategy Engine

`ba-strategy` v0.1 is a local Rust 2024 probability engine for one or two
ordered Blue Archive recruitment targets. It provides exhaustive branch
analysis, deterministic seeded Monte Carlo, single-run traces, and exact versus
simulation comparisons.

The bundled mechanics are the explicit provisional model
`jp_2026_07_29_provisional_v1`. They are not represented as independently
verified or official Japanese-server mechanics. Runtime behavior comes only
from the validated external ruleset in
[`data/rulesets`](data/rulesets/jp_2026_07_29_provisional_v1.json); the solver
does not keep a second threshold or cost table.

## Build and run

Rust 1.95.0, rustfmt, and Clippy are selected by
[`rust-toolchain.toml`](rust-toolchain.toml). The root is a virtual workspace,
and [`Cargo.lock`](Cargo.lock) is part of the application source.

```text
cargo build --workspace --locked

cargo run -p ba-cli --bin ba-strategy -- \
  validate data/rulesets/jp_2026_07_29_provisional_v1.json

cargo run -p ba-cli --bin ba-strategy -- \
  analyze single_target_200 --format json

cargo run -p ba-cli --bin ba-strategy -- \
  simulate dual_shared_200 --runs 10000 --seed 42

cargo run -p ba-cli --bin ba-strategy -- \
  simulate ticket_atomic --runs 1 --seed 42 --trace --format json

cargo run -p ba-cli --bin ba-strategy -- \
  compare dual_shared_200 --runs 10000 --seed 42
```

`--data-dir` defaults to `./data`. A scenario argument may be a path or the ID
of a fixture under `scenarios/golden/` next to that data directory.

Successful output goes to stdout. Errors go to stderr, with no authoritative
result on stdout:

| Exit | Meaning |
|---:|---|
| 0 | Success, including normal domain terminal outcomes |
| 2 | Command-line usage |
| 3 | JSON, schema, or domain validation |
| 4 | Catalog/filesystem I/O or rejection limit |
| 5 | Engine guard, arithmetic, transition, or probability invariant |
| 70 | Unexpected typed internal failure |

## Model semantics

The shipped v1 ruleset declares:

- paid single cost and action size;
- ticket-funded action size;
- ordinary pickup probability;
- maximum pre-recruitment charge;
- hit reset and miss increment;
- ordered threshold probability overrides.

Schema-v1 validates the provisional fixture values, but transitions,
strategies, affordability, horizon fit, and metric reconstruction all read the
compiled ruleset. Charge belongs to charge groups rather than banners, so two
banners can share a counter or use independent counters.

A paid or ticket action is atomic. Its banner is locked for every primitive
draw, it cannot stop after an early pickup, and policy runs again only after
the action completes. Tickets earned during an action are deferred until that
completion boundary. V1 tickets are universal and never expire.

The strategy `sequential_targets_prefer_tickets` selects the first unowned
ordered target, prefers a complete affordable ticket action, then a complete
affordable paid action. A configured horizon is a domain rule: an action must
fit in full. Solver limits are safety guards and never affect policy
feasibility.

## What “exact” means

Exact analysis exhaustively propagates every modeled nonzero branch in fixed
order and never probability-prunes. Equivalent future states are aggregated
in deterministic `BTreeMap` frontiers with compensated `f64` probability
accumulation. “Exact” therefore means exhaustive enumeration, not rational or
arbitrary-precision arithmetic.

First-success time is not future-relevant state. Its probability mass is
recorded when ownership first completes and retained in a separate PMF while
world states merge. Aggregate fields are explicitly named `expected_*`.
Concrete trace fields instead describe the one realized path.

The finite initial resource inventory plus the finite non-pyroxene milestone
schedule bounds every schema-v1 run. Exact guard calibration and headroom are
recorded in [`docs/CALIBRATION.md`](docs/CALIBRATION.md).

Concrete execution is also fail-closed: v0.1 defaults reject more than
1,000,000 Monte Carlo runs, more than 1,048,576 primitive transitions in one
run, more than 100,000,000 primitive transitions across one simulation call,
or more than 100,000 primitive transitions in a materialized trace/replay.
These are engine work limits, not strategy horizons.

## Reproducibility and input hardening

Every source is read once into a complete buffer of at most 1 MiB. A recursive
token scan rejects duplicate decoded keys at any object depth and nesting over
64 before strict typed parsing rejects unknown fields and trailing data.
Catalogs inspect at most 256 immediate `.json` regular files per directory.
One malformed, unsupported, oversized, unreadable, duplicate-ID, symlink, or
otherwise invalid candidate rejects the complete catalog; nothing is
truncated or partially published.

The v0.1 secure file-open implementation is enabled on Linux and Android.
Other targets fail closed with a path-policy error instead of falling back to
a symlink-following or potentially blocking open.

Validated rulesets, reward schedules, and scenarios receive versioned
canonical semantic SHA-256 fingerprints. Formatting, source paths, mtimes, and
machine/build state are excluded. Monte Carlo uses independent per-run
ChaCha8 streams derived from those raw fingerprints, the master seed, and the
zero-based run index. It reports Wilson intervals for probability support
points and approximate 95% mean intervals plus standard errors for numeric
expectations; a one-observation mean interval is `null`. See
[`docs/PROTOCOLS.md`](docs/PROTOCOLS.md).

Input shape and cross-document rules are documented in
[`docs/SCHEMA_V1.md`](docs/SCHEMA_V1.md).

## Workspace boundaries

```text
ba-cli -> ba-engine -> ba-core
```

- `ba-core`: strict input, validation, catalogs, fingerprints, state, strategy,
  and the pure RNG-free action/transition kernel.
- `ba-engine`: exhaustive propagation, aggregate metrics, Monte Carlo,
  confidence intervals, trace/replay, comparison, and provenance DTOs.
- `ba-cli`: argument handling, catalog resolution, rendering, and exit mapping.

There is no async runtime, frontend, server, database, scraper, calendar,
character database, valuation layer, optimizer, MDP, policy DSL, plugin
framework, parallel Monte Carlo, or support for more than two targets in v0.1.
