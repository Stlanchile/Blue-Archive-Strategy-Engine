# ADR 0001: defer deterministic parallel Monte Carlo

## Status

Accepted — deferred.

## Context

Monte Carlo presently runs serially. Each run derives an independent ChaCha8
stream from the behavior fingerprints, master seed, and zero-based run index
using `mc-run-stream-v1`. This gives reproducible primitive outcomes and stable
aggregate behavior independent of provenance-only document changes.

Parallel execution could improve throughput, but it would add ordering,
aggregation, API, dependency, and reproducibility surfaces. It remains outside
the v0.3 scope and Rayon is intentionally absent.

## Decision

Keep serial Monte Carlo. Do not add `--jobs`, worker scheduling, parallel
aggregation, or a Rayon dependency in v0.3.

Any future implementation must preserve per-run stream derivation by run index,
make aggregate ordering deterministic, define numeric aggregation semantics,
retain trace/replay behavior, measure performance, and add compatibility tests
before changing the public interface.

Revisit only after measured serial throughput on representative v3
three/four-target scenarios is a demonstrated bottleneck and an implementation
can preserve byte-stable per-run streams, ascending aggregation semantics,
trace/replay behavior, and all existing v2 vectors.
