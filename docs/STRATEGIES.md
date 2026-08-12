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

## Schema v1

V1 compiles to `LegacySequentialV1`. Its legacy horizon field is required and
accepts `null` or a positive integer; `null` retains the existing unlimited
semantics. Its funding order is permanently ticket-ten first, then paid-single.

## Schema v2

V2 strategy schema version 1 compiles to `SequentialTargetsV2`:

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
