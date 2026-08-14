# Strategies

The engine compiles strategy configuration into the validated scenario bundle;
exact analysis, serial Monte Carlo, trace, and replay all dispatch through that
compiled strategy. There are no plugins, policy DSLs, optimizers, or executable
user policies.

## Shared action semantics

The policy always selects the first unowned ordered target. It is reevaluated
only at action boundaries. A selected ticket or paid action locks the banner and
continues for its full primitive action size even if an early draw acquires the
target. Tickets earned during that action become available only once it ends.
An action must fit the remaining configured horizon in full.

## Schema-v2 strategy

Strategy schema version 1 compiles to `SequentialTargetsV2`:

```json
"strategy": {
  "strategy_schema_version": 1,
  "strategy_id": "sequential",
  "kind": "sequential_targets",
  "funding_priority": ["ticket_ten", "paid_single"],
  "max_total_recruitments": 200
}
```

`max_total_recruitments` is required and must be a positive integer; missing,
`null`, zero, negative, fractional, and overflowing values are invalid. There
is no inferred, unlimited, budget-derived, or conservative default. The funding
array must be exactly one permutation of `ticket_ten` and `paid_single`:
ticket-first and paid-first are both valid and observable. If neither complete
affordable action fits, the strategy stops for horizon; if neither is affordable,
it stops for resources.

## Schema-v3 strategy

Strategy schema version 2 compiles to the v3 sequential strategy:

```json
"strategy": {
  "strategy_schema_version": 2,
  "strategy_id": "sequential_targets_v3",
  "kind": "sequential_targets",
  "funding_priority": ["ticket_ten", "paid_single"],
  "max_additional_recruitments": 400
}
```

V3 uses one through four ordered targets. At each boundary it skips every
already-owned target, including a later target acquired cross-banner before it
became current. The horizon counts additional primitive recruitments only.
All-target completion has priority over horizon and resource terminal reasons.

`PaidSingle` spends the compiled cost and performs the compiled primitive action
size (one in the shipped provisional ruleset). `LimitedTicketTen` represents
only the modeled eligible limited ten-recruitment ticket class and performs the
compiled action size (ten in the shipped provisional ruleset). It is not a
universal representation of select, guaranteed, paid, special, archive, or free
recruitment categories.

There are no target-specific budgets, conditional policies, target-order
optimization, policy search, executable plugins, or automatic exact fallback.
