# Changelog

## 0.3.0

- Added homogeneous schema v3 bundles with one through four ordered targets,
  scenario-authored cross-target probabilities, shared/independent charge
  groups, and initial/additional/absolute campaign counts.
- Added exact categorical propagation, deterministic serial ChaCha8 sampling,
  v3 trace/replay, target/prefix/terminal-set metrics, and result schema 3 while
  preserving the frozen schema-v2 paths and stream vectors.
- Added eleven-resource v3 ledgers and direct interval arithmetic for finite
  plus indefinitely repeating reward schedules under a finite scenario horizon.
- Added structural v3 provenance with only `provisional` and `source_backed`
  states, claim-level first-party coverage checks, and explicit scenario
  authority reporting.
- Added profile-aware catalog inspection/listing, schema-selectable templates,
  small three/four-target exact goldens, cross-profile exact oracles, mixed
  secure-catalog tests, and v3 release-archive smoke coverage.
- Kept shipped v3 mechanics provisional because the required first-party
  evidence for every numeric and limited-ticket claim is not yet complete.
- Parallel Monte Carlo, worker-count flags, automatic exact fallback, calendar
  modeling, tags, pushes, publication, and release creation remain deferred.

## 0.2.0

- Established schema v2 as the sole document and result profile, with
  provenance and separate behavior/document fingerprints.
- Added compiled sequential strategies with explicit horizons and selectable
  ticket-first or paid-first funding order.
- Removed the unused schema-v1 parser, nullable strategy, result projection,
  runtime data, and compatibility fixtures.
- Hardened catalog and explicit-document loading with pinned ambient roots,
  descriptor-relative no-follow descendants, and root-consistent catalog
  publication.
- Added catalog listing/inspection, scenario explanation/template generation,
  `--scenario-dir`, and optional structured validation diagnostics.
- Added shipped provisional data, schema-v2 authoring examples, synthetic test
  fixtures, benchmark coverage, threat-model documentation, and
  dual MIT/Apache-2.0 licensing.
- Fixed v2 scenario behavior fingerprints so identifier-only renames preserve
  fixed-seed streams while document fingerprints retain identity.
- Fixed exact propagation for nonzero long-tail branches below the ordinary
  `f64` exponent range, and escaped untrusted control characters in text output.
- Removed an undeclared `ripgrep` dependency from release checks and packaging
  so the Linux quality and release workflows run on stock GitHub-hosted runners.

## 0.1.0

- Initial local strategy engine development version.
