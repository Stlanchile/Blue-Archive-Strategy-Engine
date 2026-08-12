# Changelog

## 0.2.0

- Added schema-v2 rulesets, rewards, scenarios, provenance, behavior/document
  fingerprints, and schema-v2 result projections.
- Added compiled v2 sequential strategies with explicit horizons and selectable
  ticket-first or paid-first funding order; retained v1 strategy semantics.
- Hardened catalog and explicit-document loading with pinned ambient roots,
  descriptor-relative no-follow descendants, and root-consistent catalog
  publication.
- Added catalog listing/inspection, scenario explanation/template generation,
  `--scenario-dir`, and optional structured validation diagnostics.
- Added shipped provisional v2 data, v2 authoring examples, synthetic v2 test
  fixtures, benchmark coverage, compatibility/threat-model documentation, and
  dual MIT/Apache-2.0 licensing.
- Fixed v2 scenario behavior fingerprints so identifier-only renames preserve
  fixed-seed streams while document fingerprints retain identity.
- Fixed exact propagation for nonzero long-tail branches below the ordinary
  `f64` exponent range, and escaped untrusted control characters in text output.
- Removed an undeclared `ripgrep` dependency from release checks and packaging
  so the Linux quality and release workflows run on stock GitHub-hosted runners.

## 0.1.0

- Initial local strategy engine release.
