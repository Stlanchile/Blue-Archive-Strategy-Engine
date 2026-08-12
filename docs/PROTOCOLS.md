# Reproducibility protocols

## Canonical semantic fingerprints

The semantic encoding version is `canonical-json-v1`.

```text
validated semantic document
-> versioned semantic node tree
-> compact canonical JSON bytes
-> SHA-256
```

The writer emits object keys in lexical order, canonical unsigned integers,
compact UTF-8, and no trailing newline. Probability ratios are reduced before
encoding. Set-like or ID-indexed arrays are sorted; ordered targets,
milestones, and threshold overrides retain their semantic order.

Source paths, raw formatting, comments/descriptions, mtimes, Git state,
timestamps, hostnames, and build identifiers are excluded. External digests
are lowercase 64-character hex; internal stream derivation uses the raw 32
bytes.

Fixed writer vector:

```text
{"a":1,"b":["x",2],"c":{"y":true,"z":null}}
ef7399b9e14e5bc9393892927aff176ede3c1416d3af75cc0e44eaa6312a133d
```

The shipped minimal v1 bundle vectors are frozen in
`crates/ba-core/tests/strict_inputs.rs`. V2 uses the same canonical writer but
exposes separate behavior and document projections. Provenance is excluded from
behavior fingerprints and included in v2 document fingerprints. A v1 document
referenced by a v2 scenario uses its unchanged legacy fingerprint for both
roles.

## Monte Carlo streams

The stream derivation version is `mc-run-stream-v1`. Run indices are processed
serially in ascending zero-based order.

At the CLI boundary, `--seed <u64>` supplies the master seed explicitly. When
the option is omitted, the CLI obtains one `u64` from the operating system's
cryptographically secure entropy source. Failure to acquire entropy aborts the
command without simulation. The resolved master seed is always present in RNG
provenance, allowing an entropy-seeded result to be reproduced explicitly.

```text
SHA-256(
  UTF8("ba-strategy/mc-run-stream/v1\0")
  || master_seed as 8 little-endian bytes
  || run_index as 8 little-endian bytes
  || raw scenario behavior fingerprint
  || raw ruleset behavior fingerprint
  || raw reward-schedule behavior fingerprint
)
```

The 32-byte result seeds one `ChaCha8Rng`. Each run therefore has an independent
stream that does not depend on earlier run lengths or scheduler behavior.
Deterministic one-branch distributions consume no RNG. Non-deterministic
rational probabilities use rejection sampling over unbiased bounded `u64`
values.

Monte Carlo PMFs and CDFs are derived from checked integer sample counts. In
particular, a CDF divides the cumulative integer count once at each support
point instead of repeatedly adding rounded floating-point fractions.

Reports identify:

```text
rng_algorithm = chacha8
stream_derivation_version = mc-run-stream-v1
run_index_contract = zero-based ascending indices 0..runs-1
```

Fixed run-seed vectors and repeatability are tested in
`crates/ba-engine/tests/simulation.rs`.

## Result versions

Schema-v1 successful result provenance remains:

```text
engine_version
engine_semantics_version = 1
result_schema_version = 1
semantic_encoding_version = canonical-json-v1
scenario/ruleset/reward-schedule IDs and fingerprints
```

Schema-v2 scenarios select engine semantics 2 and result schema 2 regardless of
the referenced document versions. Their provenance includes each input schema
version, behavior/document fingerprint roles, declared ruleset/reward
verification status and provenance, and compiled-strategy context. A
provenance-only mutation can therefore change complete serialized v2 result
bytes without changing any behavioral metric, event, state transition, or run
seed.

Mechanics or strategy behavior changes require an engine semantics and package
version change. Aggregate wire changes require a result schema change.
Canonical encoding changes require a semantic encoding version change. Stream
derivation changes require a stream derivation version change.
