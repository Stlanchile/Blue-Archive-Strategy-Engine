# v0.4 compatibility baseline

The baseline was qualified from `6de33d8fa790b52a2409cb48473946fdceb9866c`
plus seven pre-existing working-file corrections on 2026-09-17. All 147 existing
tests passed (one statistical test intentionally ignored) before solver edits.
The corrections preserve exact Wilson endpoints at 0/1 in both profiles, avoid
false v3 terminal-ticket overflow after spending, and construct unused kernel
errors lazily. Four regression tests cover the corrected outcomes. These are
the only behavioral differences in that qualified baseline.

Subsequent review corrected premature underflow in both exact profiles: positive
subnormal probabilities now survive projection whenever representable in f64.
This intentionally changes previously zero long-tail values; the frozen shipped
scenario outputs remain unchanged. Schema-4 timing probabilities also clamp
endpoint roundoff to [0, 1] after validation, preventing false interval failures.

Before timing loop changes, complete serialized v2/v3 exact, sampled, comparison
and trace outputs and multiply-invalid diagnostic outputs were captured in
`crates/ba-cli/tests/fixtures/compatibility/`. `compatibility.rs` compares bytes,
including field order, nulls and omissions. It normalizes only `engine_version`
and the workspace path in diagnostics. No numeric values, fingerprints, event
sequences, error codes or other version fields are normalized. V3 exact solver
counts for all nine shipped scenarios are frozen separately in
`crates/ba-engine/tests/fixtures/v3_calibration.json`.

| Surface | Retained contract |
| --- | --- |
| Runtime input bytes | Three v2 and two v3 documents are SHA-256 pinned by verify-shipped-data.sh; existing golden scenarios are unchanged |
| Parsing and validation | Whole-document duplicate/depth scanning, decoded duplicate keys, strict typed fields, numeric bounds, homogeneous bundles and existing precedence |
| Diagnostics and exit codes | Existing class/code, pointer/location availability, hints; exits 0/2/3/4/5/70 |
| V2 mechanics | One/two targets, seven resources, implicit zero initial progress, strategy schema 1, binary branch order and action atomicity |
| V3 mechanics | One/four targets, arbitrary valid ownership subsets, featured/other/none branch order, joint GCD, featured-only charge reset, eleven resources, strategy schema 2 |
| Campaign coordinates | Additional and absolute counts remain separate; no historical reward replay |
| Rewards | First-time and repeating formula, bounded future materialization, deferred ticket activation, terminal precedence |
| Optional cycle | Omitted `repeating_cycle` equals explicit null, including document identity |
| Provenance | Component claim bindings, six scenario authority fields, v3 verified rejection, behavior/document separation |
| Identity | Canonical-json-v1, existing projections and fingerprints; timing options excluded |
| Exact | Ordered propagation/merge arithmetic, exhaustive branches, scaled mass, state keys, all guards and solver counts |
| Sampling | ChaCha8, mc-run-stream-v1, zero-RNG deterministic outcomes, rejection rules, ascending serial run/aggregate/moment order |
| Trace/replay | Existing event/outcome shapes and order, replay rejection of impossible/missing/surplus outcomes |
| Existing results | Schema-2/3 field sets, declaration/map ordering, omission/null behavior and default rendering |
| CLI | Existing syntax/defaults, loading before execution, path resolution, entropy behavior, no partial application-error stdout |
| Catalogs | Profile grouping then ID order; scenario schema then ID order; inspection schemas 1/2 unchanged |
| Filesystem | Selected-root symlink support, no-follow descendants, descriptor generation checks, whole-catalog rejection and byte/entry bounds |
| Packaging | Root naming, normalized ordering/modes/ownership/timestamps, manifest and checksum validation, fixture exclusion |

Package-version fields now say 0.4.0; help gains `--acquisition-timing`.
New APIs and schema-4 output are opt-in and require v3. No automatic migration,
input schema 4, engine-semantics bump, seed change, trace schema change or new
dependency is introduced. Lower timing support limits may reject a new report
while the legacy invocation remains accepted. See [timing](ACQUISITION_TIMING.md)
for its exact field sets and bounds.

The compatibility corpus complements the existing parser, security, numerical,
stream, replay, and cross-profile tests. The one-target v3 timing distribution
is checked against v2 first-success output; sampled outcomes are not required
to match across profiles because their fingerprints and samplers differ.
