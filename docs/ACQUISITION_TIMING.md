# First-acquisition timing (v0.4)

Schema-v3 campaigns can report each configured target's first-acquisition PMF
and CDF. This is an opt-in report; existing invocations keep result schemas 2/3.
Inputs, strategy decisions, engine semantics, state keys, fingerprints and RNG
streams do not change. Schema-v2 timing reports are unsupported.

## Meaning

For target i, T_i is zero if initially owned, otherwise the **additional primitive
recruitment count** where its ownership bit first becomes set. An execution
ending without the target contributes to the separate unacquired outcome.
Time zero means available at observation start; it does not date a historical
acquisition. Absolute count is checked initial campaign count plus additional
count. An acquisition during a ticket action is dated at that primitive draw,
even though the atomic action continues to completion.

PMF_i(n) = P(T_i = n); CDF_i(n) = P(T_i <= n). These probabilities are
unconditional, including runs that fail to acquire every target. They describe
the configured strategy until its actual terminal boundary, including resource
exhaustion, action-fit limits and early completion. No hypothetical recruiting
continues after termination. Duplicate acquisitions do not change T_i;
cross-target hits record the acquired target's time.

Marginals cannot be multiplied to recover joint completion. In the synthetic
four-draw, two-target test with per-draw probabilities 1/4, 1/4, 1/2, each
marginal acquisition probability is 175/256, but all-target completion is 110/256.
No joint distributions, conditional timing means, quantiles or valuations are
reported.

## CLI

```bash
ba-strategy analyze scenarios/golden/v3_atomic_cross_target.json --acquisition-timing --format json
ba-strategy simulate scenarios/examples/four_target_independent_simulation_v3.json --runs 10000 --seed 42 --acquisition-timing --format json
ba-strategy compare scenarios/golden/v3_atomic_cross_target.json --runs 1000 --seed 42 --acquisition-timing --format text
```

`--acquisition-timing` is available only on analyze/simulate/compare. A validated
v2 bundle returns usage exit 2 with `--acquisition-timing requires a schema-v3
scenario`, before entropy acquisition. Malformed bundles retain ordinary
validation precedence. `--trace` conflicts at argument parsing (exit 2). Trace
and replay formats are unchanged. Omitted seeds retain OS-entropy behavior;
explicit seeds reproduce the same legacy aggregate and primitive outcomes with
collection enabled or disabled.

Text retains the legacy summary and appends target-ordered rows. Comparison
text includes exact and sampled rows, followed by aligned differences. All
probability intervals are **pointwise 95% Wilson intervals**, not simultaneous
confidence bands. Statistical disagreement is informational (exit 0).

## Rust API

The three exported functions return concrete schema-4 DTOs:

```rust,ignore
analyze_exact_v3_with_acquisition_timing(bundle, exact_options, timing_options)
simulate_monte_carlo_v3_with_acquisition_timing(bundle, runs, master_seed, limits, timing_options)
compare_v3_with_acquisition_timing(bundle, runs, master_seed, exact_options, limits, timing_options)
```

`bundle` is `&ValidatedScenarioBundleV3`, `runs` is `NonZeroU64`, and the seed is
`u64`. The other arguments are `ExactSolverOptions`, `SimulationLimits`, and
`AcquisitionTimingOptions`. All return `Result<..., EngineError>` with result
DTOs `ExactAcquisitionTimingResultV4`, `MonteCarloAcquisitionTimingResultV4`, and
`ComparisonAcquisitionTimingResultV4`, respectively.

`AcquisitionTimingOptions::default()` allows 65,536 distinct support keys.
`AcquisitionTimingOptions::new(NonZeroUsize)` permits smaller positive limits;
values above 65,536 return `InvalidAcquisitionTimingOptions`. The private field
is available through `max_support_points()`. These report options are excluded
from both fingerprints and seed derivation.

## Wire contract

Declaration order is serialization order. Exact and sampled envelopes contain:

| Field | Meaning |
| --- | --- |
| result_schema_version | 4 |
| engine_kind | `exact_acquisition_timing` or `monte_carlo_acquisition_timing` |
| analysis | Unchanged legacy result-schema-3 analysis |
| acquisition_timing | `{ options: { max_support_points }, targets: [...] }` |

The outer version describes the complete report. Embedded
`analysis.provenance.result_schema_version = 3` describes the embedded object;
engine semantics remain 3. Package version is 0.4.0.

Exact targets have fields, in order: `target_index` (zero-based), `target_id`,
`initially_owned`, `acquired_by_terminal_probability`,
`not_acquired_by_terminal_probability`, `pmf`, `cdf`.
Sampled targets insert `acquired_sample_count`, `not_acquired_sample_count`
between the probability fields and arrays.

Exact points contain `additional_recruitment_count`,
`absolute_campaign_recruitment_count`, `probability`. Sampled points append
`sample_count`, `confidence_interval_95: { lower, upper }`. For PMF, count means
bin observations; for CDF, cumulative observations. Each sampled probability
is computed once from its integer count divided by total runs. Acquired plus
unacquired sample counts equal total runs exactly.

Targets follow configured order; points have strictly increasing additional
counts. Missing PMF counts mean zero. Between CDF points, hold the preceding
value; before the first point use zero. Empty arrays mean no finite acquisition
support. Initially owned targets have exactly one point at zero, probability
one. Unacquired mass is never represented by infinity or a fake count. Internally
positive exact bins remain present even when projection to f64 rounds to zero.
Exact unacquired mass is accumulated from terminal states independently, not
computed by subtracting a rounded marginal from one. Conservation uses the
existing exact tolerance, with finite bounded probabilities and nondecreasing
CDFs.

Comparison uses `engine_kind: acquisition_timing_comparison`, an embedded
`ComparisonResultV3`, and `acquisition_timing: { exact, monte_carlo, comparisons }`.
Each target comparison contains `target_index`, `target_id`,
`not_acquired_simulation_minus_exact`,
`exact_not_acquired_within_monte_carlo_interval`, `points`.
Each point contains the two count coordinates, `pmf_simulation_minus_exact`,
`cdf_simulation_minus_exact`, `exact_pmf_within_monte_carlo_interval`, and
`exact_cdf_within_monte_carlo_interval`.

Comparison support is the sorted union of exact PMF keys, sampled PMF keys,
zero, and the configured additional horizon. Absent PMF bins have zero count;
CDFs hold their last value. Zero-sample bins still receive Wilson intervals.
Exact runs first, then simulation; the legacy comparison is assembled from those
results without executing either solver again.

## Bounds and failure

Each dataset allows at most 65,536 distinct `(target_index, additional_count)`
keys in total. Comparison support has its own identical cap, including zero and
horizon. Smaller caller limits apply to all three datasets. Existing-key updates
consume no extra slot. Allocation is sparse, never horizon-sized, and no run
history or primitive traces are retained for distribution reporting.

Support exhaustion returns `AcquisitionTimingSupportLimitExceeded` (CLI code
`acquisition_timing_support_limit_exceeded`). Invalid options use
`invalid_acquisition_timing_options`. Both are engine class, exit 5. Inconsistent
observer/kernel facts are internal errors, exit 70. Existing execution and
checked-arithmetic guards remain in force. Failure discards the entire report;
there is no legacy-only fallback.

New JSON and text are rendered through a bounded writer with a 64 MiB UTF-8
limit, including the final newline. Exceeding it returns
`output_size_limit_exceeded`, engine class, exit 5, before authoritative stdout.
Existing renderers keep their contracts. An OS stdout write failure can occur
after a prefix has reached the external stream and retains exit 70.

Timing inherits the bundle's authority. Shipped v3 rules and empty rewards are
provisional; mathematical precision adds no gameplay source qualification.
