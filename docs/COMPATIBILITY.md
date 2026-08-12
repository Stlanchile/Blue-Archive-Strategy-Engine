# Compatibility

## Frozen schema-v1 contract

Schema-v1 remains the compatibility baseline: source bytes, canonical encoding,
semantic fingerprints, numeric goldens, derived Monte Carlo seeds, fixed-seed
simulation, trace/replay behavior, DTO field sets, nullable horizon semantics,
and validation first-failure precedence are preserved. The only intentional
user-visible v1 exceptions are package version `0.2.0` and the version-neutral
CLI description `Blue Archive Strategy Engine`.

The pre-change Linux baseline was recorded with
`cargo test --workspace --locked`: 61 passed, zero failed, one ignored, zero
filtered. The ignored test is the explicitly statistical smoke test
`monte_carlo_interval_contains_the_dual_exact_golden_at_scale`. Counts are
evidence, not a wire protocol.

| Test target | Passed | Failed | Ignored | Measured | Filtered |
|---|---:|---:|---:|---:|---:|
| `ba-cli` library | 2 | 0 | 0 | 0 | 0 |
| `ba-strategy` binary | 0 | 0 | 0 | 0 | 0 |
| CLI integration | 9 | 0 | 0 | 0 | 0 |
| `ba-core` library | 5 | 0 | 0 | 0 | 0 |
| core domain | 4 | 0 | 0 | 0 | 0 |
| core strict inputs | 12 | 0 | 0 | 0 | 0 |
| `ba-engine` library | 5 | 0 | 0 | 0 | 0 |
| calibration | 1 | 0 | 0 | 0 | 0 |
| DTO contract | 2 | 0 | 0 | 0 | 0 |
| goldens | 3 | 0 | 0 | 0 | 0 |
| rewards | 2 | 0 | 0 | 0 | 0 |
| ruleset authority | 4 | 0 | 0 | 0 | 0 |
| simulation | 8 | 0 | 1 | 0 | 0 |
| solver regressions | 4 | 0 | 0 | 0 | 0 |
| doctests (three targets) | 0 | 0 | 0 | 0 | 0 |

The v1 raw-file SHA-256 vectors are:

| Fixture | SHA-256 |
|---|---|
| `data/rulesets/jp_2026_07_29_provisional_v1.json` | `71e28f0b082cd8aab8ac42cc4ecd7cd1ec8fc72a006901c53d43c329c7c22c0e` |
| `data/rewards/empty_v1.json` | `2f00ac378cc2d34e0c8bd0358afbd2827a29ceb2e5fa928bf1dc770783d84cf7` |
| `data/rewards/jp_2026_07_29_campaign_v1.json` | `859d23812f8567bdcefee5497226f9f5c2e10581b91956f8e2baa7c173b3ad48` |
| `scenarios/golden/campaign_dual_310.json` | `8a9b5daf320f73344ec00b22a0ff299111f4d50d2d12682dcbf17cae9c9e8777` |
| `scenarios/golden/charge_199_one.json` | `fd405d31e517e1bf465ce3626a76a9721a7528092b061fab3658d94f8d3a229f` |
| `scenarios/golden/charge_99_one.json` | `3616e040407c8bb36bce9a4b2f58741d64f5bc387848ae9cf6bda74b81defb68` |
| `scenarios/golden/dual_independent_200.json` | `f8a6d954347bf98ba5254be40f02aa6cc98b508880c705984800f249507daca8` |
| `scenarios/golden/dual_shared_200.json` | `325193383a78b6f085b7432a0fceb599076c48ad3ab52067ac09a0714f6fa45c` |
| `scenarios/golden/initial_success.json` | `f34a85e1b63c5453caffbe3bc4299b328206f73ec622cb2b013e2685b83dd079` |
| `scenarios/golden/single_target_200.json` | `61a98ae4860103719f57cec19dd4982059fe70a480199254f9f681ce03e35461` |
| `scenarios/golden/ticket_atomic.json` | `8876cc9c21cb641198ee25161d0d08dfab03b530293a7b9c3495d0d45ffaf997` |

V1 ruleset conversion order is fixed: header/typed parsing, ID, ordinary
probability, threshold probabilities in source order, mechanics construction,
provisional-v1 authority validation, then generic mechanics validation. Thus an
authority mismatch remains the first error even if a generic failure such as
zero paid cost is also present.

## Schema profiles

The scenario version alone chooses semantics and result shape.

| Scenario | Ruleset | Reward schedule | Result |
|---|---|---|---|
| V1 | V1 | V1 | valid; semantics/result schema 1 |
| V1 | V2 | V1 | rejected |
| V1 | V1 | V2 | rejected |
| V2 | V1 | V1 | valid; semantics/result schema 2 |
| V2 | V2 | V1 | valid when reward compatibility permits |
| V2 | V1 | V2 | valid when reward compatibility permits |
| V2 | V2 | V2 | valid when reward compatibility permits |

Invalid v1 cross-version references return
`incompatible_schema_reference` at `/ruleset_id` or `/reward_schedule_id`.
Unreferenced v2 catalog entries do not change a v1 scenario’s profile or
output. V2 output exposes input schema versions, behavior/document
fingerprints, declared provenance, and compiled strategy context; it is not an
additive modification of the v1 wire shape.
