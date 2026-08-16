# Release readiness

Version 0.3.0 is prepared for controlled Linux x86_64 CLI release verification.
It is not yet approved as a formal source-backed gameplay-data release. This is
not a crates.io publication workflow, and local implementation work must not
create a tag, push, publish a package, or create a GitHub release.

## Preconditions

- Use a full, non-shallow history; inspect contributor history and copied/vendor
  material before release.
- This repository’s reviewed history identifies only `Shirasu Hare
  <shirasu_hare@stu.abydos.ac>` and no tracked vendor tree, third-party notices,
  or copied license material. It therefore uses contributor-neutral dual
  MIT/Apache-2.0 terms. Re-open this assessment if history or imported material
  changes.
- Require a fresh online `cargo audit --deny warnings`; advisory/network failure
  is not a clean audit.
- Require first-party claim-level evidence before adding or approving
  source-backed v3 ruleset/campaign data. The current shipped v3 data remains
  provisional because this gate is incomplete.
- CI installs the reviewed `cargo-audit 0.22.2` with its published lockfile
  before running that online audit.
- Keep all crates `publish = false`, use the reviewed lockfile, and run locked
  formatting, build, Clippy, tests, documentation, and benchmark-compilation
  gates.

The intentional direct dependency additions are `rustix 1.1.4` with only
`fs,std` enabled (MSRV 1.63; MIT/Apache-2.0 family) and
`serde_path_to_error 0.1.20` (MSRV 1.61; MIT OR Apache-2.0). Both are compatible
with Rust 1.95.0. The reviewed tree contains no Rayon.

## Verification and publication boundary

A release verifier must have `contents: read` only. It checks the tag/version,
formats/builds/tests/docs/benchmarks/audit, packages the Linux binary with
runtime data and examples/goldens, produces checksums and a manifest, validates
a clean extraction, and smoke-tests from an unrelated working directory.

Only a separate protected-environment publisher may have `contents: write`.
It downloads verified artifacts, rechecks tag/commit/manifest/hashes, and only
creates/uploads the release. It must not check out the project, build it, run
Cargo, or execute project code. Use immutable full-SHA action pins with their
human-readable release noted next to each pin; never use mutable action tags.

Reviewed action pins:

| Action | Release | Immutable commit |
|---|---|---|
| `actions/checkout` | 5.0.0 | `08c6903cd8c0fde910a37f88322edcfb5dd907a8` |
| `actions/upload-artifact` | 4.6.2 | `ea165f8d65b6e75b540449e92b4886f43607fa02` |
| `actions/download-artifact` | 4.3.0 | `d3f86a106a0bac45b974a628896c90dbdf5c8093` |

The runtime archive must include `ba-strategy`, v2 and v3 `data/`,
`scenarios/golden/`, `scenarios/examples/`, core user-facing docs, README,
CHANGELOG, SECURITY, and both licenses. It must exclude `tests/fixtures/`.
Normalize archive ordering, modes, ownership, and timestamps before checksums.

Protect `v*` tags and the release environment, restrict tag creation, and
require reviewers for publication. A verification failure makes publication
impossible.

Packaging verification is not publication. It must not create a tag, push,
publish crates, create a GitHub release, or describe an incomplete source/audit
gate as complete.
