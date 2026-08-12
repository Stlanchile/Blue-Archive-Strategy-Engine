# Schema v2

Schema v2 adds provenance-aware rulesets and reward schedules, explicit
compiled strategies, and result schema 2 while retaining the one/two-target
finite model. All documents use `schema_version: 2` and their existing
`document_type` (`ruleset`, `reward_schedule`, or `scenario`). Unknown
version/type pairs are rejected as unsupported documents.

## Shipped provisional data

- `data/rulesets/jp_2026_07_29_provisional_v2.json`
- `data/rewards/jp_2026_07_29_empty_v2.json`

The ruleset exactly mirrors the behavior-affecting values in the shipped v1
provisional ruleset and is marked `provisional` with no sources. It is not
independently verified, official, or an endorsement by the engine. The empty
v2 reward schedule is also provisional and declares compatibility with both
shipped provisional ruleset IDs.

`tests/fixtures/schema_v2/non_v1_*` deliberately demonstrates generic
mechanics outside the v1 authority table. Those `synthetic_*` documents are
test-only: they do not appear in runtime `data/`, normal catalog listings,
authoring examples, or runtime archives.

## Rulesets, rewards, and provenance

A v2 ruleset declares the same mechanics as v1, but generic validation is used
without the v1 provisional authority lock. It requires positive costs/action
sizes and miss increment; normalized valid probabilities; unique strictly
increasing thresholds within the maximum charge; an in-range hit reset; checked
derived arithmetic; and a probability-one override at every charge where a
miss would exceed the maximum. At most 4,096 threshold overrides are allowed.

A v2 reward schedule has 1–256 compatible ruleset IDs and at most 4,096
milestones, with at most seven rewards in each milestone. Empty milestone
schedules are valid. Pyroxene rewards, empty reward lists, duplicate resource
kinds, zero quantities, unordered counts, and arithmetic overflow are rejected.

Rulesets and reward schedules require:

```json
"provenance": {
  "verification_status": "provisional",
  "sources": []
}
```

`verification_status` is `provisional`, `source_backed`, or `verified`.
`source_backed` and `verified` require at least one source; `provisional` may
have none. There are at most 32 sources. Labels are limited to 256 UTF-8 bytes,
references to 2,048 UTF-8 bytes, optional `retrieved_on` must be a Gregorian
`YYYY-MM-DD`, and optional `content_sha256` must be 64 lowercase hexadecimal
characters. The engine validates declarations only: `verified` is not engine
endorsement or a claim that a source is official or true.

## Scenarios and compatibility

A v2 scenario still has exactly one or two ordered targets, distinct target
students and banners, shared or independent charge groups, one initial charge
per used group, complete explicit resources, zero initial cumulative count, and
initial ownership limited to configured targets. It may reference either v1 or
v2 rulesets and rewards, provided the reward schedule declares compatibility.
V1 scenarios may reference only v1 documents. The scenario version—not the
referenced document versions—selects semantics/result profile.

The authoring examples are in `scenarios/examples/`. `scenario template`
produces a valid v2 scenario with a 200-recruitment ticket-first strategy and
is suitable for a validation round trip.

## Fingerprints and reproducibility

V2 exposes two semantic SHA-256 surfaces for rulesets, rewards, and scenarios:

- **Behavior fingerprint:** normalized behavior-affecting fields. It feeds the
  unchanged `mc-run-stream-v1` per-run derivation.
- **Document fingerprint:** behavior plus semantically relevant provenance and
  other document identity fields.

Consequently, a provenance-only change preserves compiled mechanics, strategy
decisions, exact metrics, solver diagnostics, state transitions, trace/replay
events, serial Monte Carlo primitive outcomes, aggregate behavioral metrics,
and per-run seeds for a fixed master seed. It changes the document fingerprint
and corresponding declared provenance/status in v2 output. Full serialized v2
results may therefore differ even when their behavioral projections are equal.

When a v2 scenario references a v1 document, the legacy fingerprint is used for
both v2 fingerprint roles and the declared verification status is `null`.
