# Changelog

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
