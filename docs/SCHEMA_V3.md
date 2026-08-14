# Schema v3

Schema v3 is the source-aware multi-target campaign profile. It is additive:
schema v2 remains a frozen compatibility profile with separate raw parsing,
canonicalization, binary sampling, result DTOs, and finite termination logic.
A scenario, ruleset, and reward schedule must all use the same profile.

## Versions

```text
document schema = 3
strategy schema = 2
engine semantics = 3
result schema = 3
semantic encoding = canonical-json-v1
Monte Carlo stream derivation = mc-run-stream-v1
RNG = ChaCha8
```

Schema-v3 result and trace field names distinguish additional recruitment
progress from the absolute campaign count. V2 field names and bytes are not
reinterpreted.

## Ruleset

The v3 ruleset has explicit featured/non-featured terminology:

```json
{
  "schema_version": 3,
  "document_type": "ruleset",
  "ruleset_id": "example_v3",
  "provenance": {
    "provenance_status": "provisional",
    "sources": [],
    "claim_bindings": []
  },
  "paid_single_cost": 120,
  "paid_single_action_size": 1,
  "ticket_action_size": 10,
  "ordinary_featured_target_probability": {
    "numerator": 7,
    "denominator": 1000
  },
  "maximum_pre_recruitment_charge": 199,
  "featured_hit_reset_charge": 0,
  "non_featured_increment": 1,
  "threshold_overrides": [
    {
      "pre_charge": 99,
      "featured_target_probability": {
        "numerator": 1,
        "denominator": 2
      }
    },
    {
      "pre_charge": 199,
      "featured_target_probability": {
        "numerator": 1,
        "denominator": 1
      }
    }
  ]
}
```

Ratios are reduced. Thresholds are unique and strictly increasing. Every charge
from which a non-featured increment would exceed the maximum must have a
probability-one featured override.

## Scenario

A v3 scenario has one through four ordered targets and exactly one corresponding
featured banner per target. All six authority fields are required and accept
only `user_authored` in v0.3:

```text
scenario
banner_topology
target_order
initial_state
cross_target_probabilities
strategy
```

This prevents scenario-authored topology and probabilities from being presented
as source-backed ruleset facts.

The count relationship is:

```text
absolute_campaign_recruitment_count
  = initial_recruitment_count
  + additional_recruitments_performed
```

Milestones at or below the initial count are already processed. Historical
rewards retained by the account belong in `initial_resources`; they are not
granted again. The strategy horizon limits only additional recruitments.

All eleven resources are required, including explicit zeroes:

```text
pyroxene
limited_ten_recruitment_tickets
eligma
advanced_bd_selectors
advanced_tech_note_selectors
superior_tech_note_selectors
gift_boxes
keystone_fragments
secret_tech_notes
superior_bd_selectors
high_grade_gift_boxes
```

Only pyroxene and the modeled eligible limited ten-recruitment ticket can fund
actions. V3 milestone rewards cannot award pyroxene.

## Cross-target probability tables

Every configured banner has one ordinary row and threshold rows corresponding
exactly to the ruleset thresholds. `other_target_weights` contains every target
other than the banner's featured target, in configured target order. Explicit
zeroes are required.

Other-target weights are absolute probabilities, not probabilities conditional
on missing the featured target. For featured probability `n_f / d_f` and row
denominator `D`, `D` must be divisible by `d_f`:

```text
featured_weight = n_f * (D / d_f)
residual_weight = D - featured_weight - sum(other weights)
```

The complete distribution is reduced by the joint GCD of the denominator and
all weights. Runtime branch order is:

```text
current featured target
other configured targets in ascending target index
no configured target
```

Zero-weight outcomes are omitted. Each nonzero branch occupies the half-open
interval ending at its `upper_exclusive` endpoint. Equivalent input scales
therefore have the same behavior fingerprint, run seeds, and sampled outcomes.

## Ownership and charge

For a draw on banner A:

- acquiring A updates A ownership and resets A's active charge group;
- acquiring configured target B updates B ownership and applies A's
  non-featured increment;
- acquiring no configured target applies the same non-featured increment;
- duplicate acquisition has the same charge classification as first
  acquisition.

Only the active banner's charge group changes. Shared, independent, and mixed
group arrangements are supported.

## Atomic actions and strategy

The only strategy is `sequential_targets`:

```json
{
  "strategy_schema_version": 2,
  "strategy_id": "sequential_targets_v3",
  "kind": "sequential_targets",
  "funding_priority": ["ticket_ten", "paid_single"],
  "max_additional_recruitments": 400
}
```

At an action boundary it selects the first unowned target, then the first
affordable funding kind in the exact two-kind permutation that fits the
remaining additional horizon. The banner, funding kind, and action size remain
locked until completion. A ten-recruitment action continues after early target
completion. Tickets earned during the action activate only at completion.

## Reward schedule

`initial_milestones` is finite, strictly increasing, and may be empty.
`repeating_cycle` is required but may be `null`. For start `S`, period `P`,
offset `o`, and zero-based cycle `k`:

```text
generated count = S + k * P + o
1 <= o <= P
```

Offset `P` is the final milestone in one cycle. For example, `S=390`, `P=200`
produces offset 200 at 590 and offset 20 of the next cycle at 610.

Rewards for `(a, b]` are accumulated directly. Repeat occurrences are counted
analytically for each offset; the implementation does not subtract two
cumulative ledgers. This lets a future interval succeed even if the complete
historical total would overflow.

The checked scenario endpoint and the positive additional horizon make a
repeating schedule finite for execution. More than 65,536 reachable milestones
is rejected before allocation; no partial prefix is returned.

## Results and execution

Exact analysis enumerates every positive canonical branch without pruning. It
aggregates success, terminal reasons, terminal masks, per-target ownership,
ordered prefixes, and first completion from scaled probability mass before
public `f64` conversion. First-completion history stays outside the active state
key, so atomic paths may merge after completing on different primitive draws.

Monte Carlo remains serial in ascending zero-based run index. A deterministic
one-branch distribution consumes no RNG; other distributions use unbiased
bounded sampling and half-open endpoints. Trace and replay consume the same
compiled acquisition branches and pure transition functions.

There is no automatic exact-to-simulation fallback, parallel execution,
worker-count flag, calendar model, policy optimizer, remote data access, or
runtime provenance fetch.
