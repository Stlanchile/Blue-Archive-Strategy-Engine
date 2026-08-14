# Data provenance and authority

Provenance is inert metadata attached to schema-v3 rulesets and reward
schedules. It affects document fingerprints and inspection output. It does not
affect behavior fingerprints, exact results, Monte Carlo run seeds, sampled
outcomes, trace, or replay.

References are never opened as paths or fetched as URLs. There is no runtime
network access, scraper, downloader, provenance directory, relaxed audit
loader, or `catalog verify` command.

## Status

V3 supports exactly:

```text
provisional
source_backed
```

`verified` is intentionally rejected. Historical schema-v2 provenance remains
unchanged and may still parse its historical status enum.

`provisional` may omit sources and may carry partial structurally valid
metadata. It makes no support claim.

`source_backed` means only that every required claim group has structurally
valid bindings and at least one first-party official source. It does not mean
that the engine independently established truth, reviewed source images,
resolved contradictions, or verified an analysis result. Result schema 3 has
no aggregate analysis-verification status.

## Sources and bindings

Each source has:

```text
source_id
source_category
label
reference
published_on (optional)
retrieved_on
content_sha256 (optional)
```

IDs are stable ASCII identifiers. Dates are valid Gregorian `YYYY-MM-DD` values
and are not compared with the runtime clock. SHA-256 values, when present, are
64 lowercase hexadecimal characters. Sources canonicalize by source ID;
bindings canonicalize by claim-group order and lexical source ID.

Ruleset claim groups are:

```text
recruitment_cost
ordinary_featured_target_probability
charge_thresholds
charge_reset_behavior
charge_carry_and_group_scope
atomic_ten_recruitment_continuation
limited_ticket_action_size_and_eligibility
```

Reward-schedule claim groups are:

```text
period_scope_and_reset
first_time_milestones
repeating_cycle
milestone_ticket_awards
```

The limited-ticket claim covers the intended ticket class, primitive action
size, eligible recruitment scope, and whether its primitives count toward the
modeled charge and recruitment-count rewards. Deferred activation until the
next strategy boundary is an engine execution protocol, not an external game
claim.

Cross-target probability values, banner topology, target order, initial state,
and strategy remain scenario-authored authority. Source-backed recruitment
mechanics do not turn those scenario inputs into official facts.

## Shipped v0.3 state

The repository ships `jp_2026_07_29_provisional_v3` and
`jp_2026_07_29_empty_v3`. It deliberately does not label a real v3 ruleset or
campaign reward schedule `source_backed`.

The official Japanese
[recruitment-renewal announcement](https://bluearchive.jp/news/newsJump/679)
and [July 29 maintenance notice](https://bluearchive.jp/news/newsJump/680)
provide partial first-party support, including threshold behavior and explicit
exclusions for certain select/guaranteed tickets. The accessible official text
reviewed during v0.3 implementation did not completely substantiate every
required numeric, reset/share, complete reward-table, atomic continuation, and
modeled campaign-ticket eligibility claim. Embedded image or in-game material
must be preserved and bound at claim level before source-backed runtime data is
added.

Secondary sources may aid discovery but cannot satisfy a required first-party
claim group. Missing evidence remains a release-readiness blocker and must not
be handled by weakening validation, silently downgrading a claim requirement,
or inventing gameplay values.
