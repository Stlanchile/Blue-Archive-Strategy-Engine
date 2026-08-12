# Schema v1

Every input is one top-level JSON object containing exactly one
`schema_version: 1` and one supported `document_type`: `ruleset`,
`reward_schedule`, or `scenario`. Unknown fields, duplicate decoded keys,
trailing values, invalid UTF-8, nesting deeper than 64, and documents larger
than 1,048,576 bytes are rejected.

Behavior IDs are case-sensitive ASCII values matching:

```text
[A-Za-z0-9][A-Za-z0-9._-]{0,127}
```

Probabilities are `{ "numerator": u64, "denominator": nonzero-u64 }`.
Validation requires numerator not greater than denominator and normalizes the
ratio by its greatest common divisor.

## Ruleset

The v1 fields are:

```text
schema_version
document_type
ruleset_id
paid_single_cost
paid_single_action_size
ticket_action_size
ordinary_pickup_probability
maximum_pre_recruitment_charge
hit_reset_charge
miss_increment
threshold_overrides[]
```

Costs and action sizes are positive. Threshold charges are unique, strictly
increasing, and inside the maximum. Every pre-charge from which a miss would
exceed the maximum must have a probability-one override. Schema-v1 additionally
requires the exact mechanics shipped as
`jp_2026_07_29_provisional_v1`; validated compiled fields remain the runtime
authority.

## Reward schedule

A schedule declares `reward_schedule_id`, a nonempty unique
`compatible_ruleset_ids` array, and a finite ordered `milestones` array.
Milestone counts are positive and strictly increasing. Each contains nonempty,
positive reward entries with no repeated resource kind. Reward entry order is
set-like and canonicalized before runtime traces and fingerprints are produced.
The cumulative quantity of every resource kind must fit in `u64`.

V1 rejects pyroxene and recurring rewards. The supported passive resource
kinds are:

```text
limited_ten_recruitment_tickets
eligma
advanced_bd_selectors
advanced_tech_note_selectors
superior_tech_note_selectors
gift_boxes
```

The repository ships both an empty schedule and the complete provisional
campaign schedule in [`data/rewards`](../data/rewards).

## Scenario

A scenario contains:

```text
scenario_id
ruleset_id
reward_schedule_id
students[]
banners[]
initial_charges[]
initial_resources
initial_owned_targets[]
strategy
targets[]
```

Every resource field is required, including explicit zeroes:

```text
pyroxene
limited_ten_recruitment_tickets
eligma
advanced_bd_selectors
advanced_tech_note_selectors
superior_tech_note_selectors
gift_boxes
```

`strategy.max_total_recruitments` is also required. Its value is either `null`
or a positive integer. Initial cumulative recruitment count is always zero in
v1.

A scenario has exactly one or two unique ordered targets. Its student set,
featured-student set, banner set, and target references must describe exactly
the same reachable targets. Every used charge group has one initial charge,
and unused students, banners, groups, or initial charges are rejected.
Initially owned students must be targets.

The only v1 strategy kind is:

```text
sequential_targets_prefer_tickets
```

The strategy horizon is not an engine guard and the scenario contains no
conservation tolerance or solver-limit fields.

## Catalog policy

`<data-dir>/rulesets/` and `<data-dir>/rewards/` must be actual,
non-symlink directories. Only immediate entries are inspected; loading never
recurses.

At most 512 immediate directory entries are inspected; the 513th fails with
`CatalogDirectoryEntryLimitExceeded`. Within that scan budget, non-JSON entries
are ignored. Every `.json` entry must be a non-symlink regular file. At most 256
JSON candidates are retained: the 257th fails immediately with
`CatalogEntryLimitExceeded`, before metadata inspection or parsing. Every
accepted candidate must validate, even when the scenario does not reference it.
A catalog object becomes visible only after the entire accepted candidate set
succeeds.
