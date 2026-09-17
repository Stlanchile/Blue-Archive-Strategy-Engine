# Calibration and benchmarks

Exact-solver calibration exhaustively propagates every shipped golden scenario
without probability pruning or altered mechanics.

| Scenario | Peak boundary | Peak in-flight | Processed states | Expansions |
|---|---:|---:|---:|---:|
| `campaign_dual_310` | 201 | 200 | 66,813 | 72,549 |
| `charge_199_one` | 1 | 1 | 3 | 1 |
| `charge_99_one` | 2 | 1 | 4 | 2 |
| `dual_independent_200` | 103 | 102 | 30,798 | 30,398 |
| `dual_shared_200` | 201 | 200 | 40,599 | 40,199 |
| `initial_success` | 0 | 0 | 0 | 0 |
| `single_target_200` | 2 | 1 | 600 | 399 |
| `ticket_atomic` | 10 | 9 | 57 | 91 |
| `v3_three_target_exact_small` | 1 | 1 | 7 | 3 |
| `v3_four_target_exact_small` | 1 | 1 | 9 | 4 |
| `v3_atomic_cross_target` | 21 | 19 | 121 | 296 |

These v3 goldens are deliberately small proofs of three/four-target and atomic
cross-target behavior; they are not performance claims for realistic long
horizons. The observed table maxima remain frontier 201, processed states
66,813, and expansions 72,549. The frozen default safety guards are:

```text
max_active_states = 65,536
max_processed_states = 1,048,576
max_transition_expansions = 2,097,152
conservation_tolerance = 1e-12
```

These are guards, not strategy inputs. Crossing one returns no partial exact
result. Concrete execution also limits Monte Carlo runs to 1,000,000, primitive
transitions per run to 1,048,576, transitions per simulation call to
100,000,000, and materialized trace/replay transitions to 100,000.

Exact probability mass is propagated with a normalized binary scale and a
compensated significand. This keeps mathematically nonzero branches alive when
their magnitude is below the ordinary `f64` exponent range. Public result
fields remain `f64`; values smaller than its representable range round to zero
only at the output boundary.

## Benchmark observations

The `ba-engine` benchmark executable measures representative operations without
changing guards. These figures are observations from one implementation run,
not performance promises or CI thresholds; hardware, kernel, compiler, and
load materially affect them.

| Operation | Observed elapsed time |
|---|---:|
| Shipped v2 ruleset read and validation | 89.710 us |
| Complete shipped catalog load | 126.114 us |
| `single_target_200` exact | 874.728 us |
| `dual_shared_200` exact | 9.334484 ms |
| `campaign_dual_310` exact | 18.048731 ms |
| Fixed-seed serial Monte Carlo, 10,000 runs | 124.653361 ms |
| Synthetic custom exact | 13.611 us |
| Near-guard exact success | 16.644478 ms |
| Over-guard exact failure | 16.160215 ms |

Additional v0.3 observations from the same class of optimized local benchmark
run:

| Operation | Observed elapsed time |
|---|---:|
| Mixed shipped v2/v3 catalog load | 263.939 us |
| Shipped provisional v3 ruleset read and validation | 41.404 us |
| V3 categorical compilation | 1.686 us |
| `v3_three_target_exact_small` exact | 27.531 us |
| `v3_four_target_exact_small` exact | 17.242 us |
| `v3_atomic_cross_target` exact | 134.272 us |
| V3 fixed-seed serial Monte Carlo, 10,000 runs | 19.555357 ms |
| Large-initial-count repeat interval accumulation | 326 ns |

The synthetic operation stages `tests/fixtures/schema_v2/` into a temporary
catalog; fictional mechanics are not installed in `data/`. The Monte Carlo
benchmark is intentionally serial. No Rayon, parallel Monte Carlo, or
wall-clock pass/fail threshold is introduced.

V3 calibration additionally exercises categorical compilation/sampling,
repeat lookup and interval accumulation, mixed-profile catalog loading,
three/four-target exact propagation, and fixed-seed serial simulation. These
measurements do not justify raising a guard when a larger scenario is rejected.

## v0.4 acquisition timing qualification (2026-09-17)

The historical `every_shipped_scenario_matches_frozen_calibration_and_headroom`
case covers eight v2 goldens. The new
`shipped_v3_scenarios_match_qualified_baseline_counts` covers all nine shipped v3
scenarios, using vectors captured before solver edits. Timing-enabled exact
results also compare the complete embedded analysis with the disabled path;
fixed-seed MC aggregates and trace/replay observations are compared separately.

Measured on Rust 1.95.0, Intel Core i9-14900HX, Linux
6.18.33.2-microsoft-standard-WSL2. The in-process release harness warms each
variant, then alternates order for three measured pairs and reports medians.
The two numbers below mean **disabled / enabled**; serialization is measured
separately as byte counts. These observations are machine/load dependent, not
performance promises or CI thresholds. The package-version update does not
change these execution paths. No solver guard was raised.

| Case | Exact | MC 1 | MC 10,000 | MC 100,000 |
| --- | --- | --- | --- | --- |
| single_target_v3 | 855.539µs / 884.518µs | 15.321µs / 15.597µs | 109.000103ms / 109.044894ms | 1.162694619s / 1.182660472s |
| dual_target_shared_paid_first_v3 | 11.903131ms / 12.355978ms | 27.99µs / 28.068µs | 175.91056ms / 178.351211ms | 1.661028245s / 1.6633209s |
| dual_target_independent_ticket_first_v3 | 13.370586ms / 13.784169ms | 21.622µs / 42.027µs | 139.070248ms / 164.588493ms | 1.306603991s / 1.281617269s |
| three_target_mixed_cross_acquisition_v3 | 47.999155ms / 50.434597ms | 26.733µs / 27.14µs | 287.217773ms / 286.195759ms | 2.782588181s / 2.875252545s |
| four_target_independent_simulation_v3 | 82.926104ms / 84.343989ms | 59.082µs / 61.211µs | 406.412578ms / 406.366725ms | 4.408255054s / 4.127323326s |
| atomic_cross_target | 45.919µs / 48.389µs | 4.444µs / 4.567µs | 10.545176ms / 10.528564ms | 105.354246ms / 107.731196ms |
| merge_oracle | 12.266µs / 13.281µs | 3.773µs / 3.972µs | 6.912887ms / 7.007463ms | 74.109829ms / 68.949678ms |
| large_initial_small_repeat_window | 35.578µs / 36.665µs | 4.522µs / 4.922µs | 10.18003ms / 10.08429ms | 106.805547ms / 105.525483ms |
| four_targets_five_categories | 52.896µs / 57.795µs | 8.301µs / 8.669µs | 9.716569ms / 9.370365ms | 93.392015ms / 92.357822ms |

The exact overhead in this sample is small but measurable, as expected from
histogram insertion and projection. MC differences vary: the independent-dual
10,000-run pair increased from 139 to 165 ms, while some other pairs were nearly
unchanged or faster within measurement variation. This does not establish a
speedup. The 100,000-run cases provide a larger-workload observation; serial
execution remains adequate for these examples. Scheduling noise and different
sampled key counts preclude a single portable overhead percentage.

Deterministic workload and UTF-8 pretty-JSON byte counts (including newline):

```text
counts single_target_v3 exact: boundary=2 in_flight=1 processed=600 expansions=399 keys=200 json_bytes=118822
counts single_target_v3 MC 1: primitives=100 keys=1 json_bytes=11223
counts single_target_v3 MC 10000: primitives=896356 keys=200 json_bytes=183815
counts single_target_v3 MC 100000: primitives=9022015 keys=200 json_bytes=185011
counts single_target_v3 comparison: points=201 json_bytes=214627
counts dual_target_shared_paid_first_v3 exact: boundary=202 in_flight=201 processed=40997 expansions=60893 keys=400 json_bytes=190125
counts dual_target_shared_paid_first_v3 MC 1: primitives=200 keys=1 json_bytes=12835
counts dual_target_shared_paid_first_v3 MC 10000: primitives=1328713 keys=400 json_bytes=320928
counts dual_target_shared_paid_first_v3 MC 100000: primitives=13254756 keys=400 json_bytes=322856
counts dual_target_shared_paid_first_v3 comparison: points=402 json_bytes=367856
counts dual_target_independent_ticket_first_v3 exact: boundary=300 in_flight=290 processed=32670 expansions=57299 keys=390 json_bytes=185188
counts dual_target_independent_ticket_first_v3 MC 1: primitives=200 keys=1 json_bytes=12939
counts dual_target_independent_ticket_first_v3 MC 10000: primitives=1586980 keys=390 json_bytes=312931
counts dual_target_independent_ticket_first_v3 MC 100000: primitives=15933610 keys=390 json_bytes=314886
counts dual_target_independent_ticket_first_v3 comparison: points=392 json_bytes=359143
counts three_target_mixed_cross_acquisition_v3 exact: boundary=400 in_flight=399 processed=150696 expansions=224745 keys=798 json_bytes=353857
counts three_target_mixed_cross_acquisition_v3 MC 1: primitives=179 keys=3 json_bytes=17083
counts three_target_mixed_cross_acquisition_v3 MC 10000: primitives=2183157 keys=787 json_bytes=603303
counts three_target_mixed_cross_acquisition_v3 MC 100000: primitives=21832212 keys=795 json_bytes=613927
counts three_target_mixed_cross_acquisition_v3 comparison: points=802 json_bytes=697588
counts four_target_independent_simulation_v3 exact: boundary=599 in_flight=598 processed=319597 expansions=318202 keys=1394 json_bytes=587238
counts four_target_independent_simulation_v3 MC 1: primitives=400 keys=2 json_bytes=20752
counts four_target_independent_simulation_v3 MC 10000: primitives=3295295 keys=1305 json_bytes=968997
counts four_target_independent_simulation_v3 MC 100000: primitives=32971077 keys=1364 json_bytes=1021808
counts four_target_independent_simulation_v3 comparison: points=1399 json_bytes=1175460
counts atomic_cross_target exact: boundary=21 in_flight=19 processed=121 expansions=296 keys=20 json_bytes=16164
counts atomic_cross_target MC 1: primitives=10 keys=1 json_bytes=12798
counts atomic_cross_target MC 10000: primitives=100000 keys=20 json_bytes=28838
counts atomic_cross_target MC 100000: primitives=1000000 keys=20 json_bytes=28960
counts atomic_cross_target comparison: points=22 json_bytes=40838
counts merge_oracle exact: boundary=10 in_flight=8 processed=29 expansions=54 keys=8 json_bytes=10562
counts merge_oracle MC 1: primitives=4 keys=1 json_bytes=12759
counts merge_oracle MC 10000: primitives=40000 keys=8 json_bytes=19650
counts merge_oracle MC 100000: primitives=400000 keys=8 json_bytes=19741
counts merge_oracle comparison: points=10 json_bytes=30292
counts large_initial_small_repeat_window exact: boundary=14 in_flight=15 processed=85 expansions=180 keys=16 json_bytes=14728
counts large_initial_small_repeat_window MC 1: primitives=8 keys=2 json_bytes=13853
counts large_initial_small_repeat_window MC 10000: primitives=62780 keys=16 json_bytes=26360
counts large_initial_small_repeat_window MC 100000: primitives=629276 keys=16 json_bytes=26485
counts large_initial_small_repeat_window comparison: points=18 json_bytes=39014
counts four_targets_five_categories exact: boundary=33 in_flight=25 processed=123 expansions=225 keys=16 json_bytes=16361
counts four_targets_five_categories MC 1: primitives=4 keys=1 json_bytes=20045
counts four_targets_five_categories MC 10000: primitives=40000 keys=16 json_bytes=32431
counts four_targets_five_categories MC 100000: primitives=400000 keys=16 json_bytes=32551
counts four_targets_five_categories comparison: points=20 json_bytes=51562
```

Primitive totals are reconstructed by rounding the existing mean times run
count; the work guard bounds them at 100,000,000, so f64 rounding is well below
half a primitive. These measurements do not change the wire schema. Exact
state/frontier/expansion counts match the frozen vectors with collection on or
off. Comparison uses one sampled run here, exposing sparse unequal supports.

The synthetic merge fixture has eight timing keys and passes at a caller limit
of eight; seven rejects. The harness separately measures these outcomes (about
12 and 4 microseconds in this run; a rejection is not a speed comparison). A
full-cap unit case inserts 65,536 keys across four targets, updates an existing
key at the cap, then verifies that key 65,537 rejects before insertion. Exact
integration also retains 1,200 positive internal bins including public f64
underflow. The existing exact terminal conservation scans make enormous
single-target horizons an expensive way to test storage alone, so the full
storage-cap test isolates the budget while smaller engine tests cover failure
propagation. A u64::MAX authored horizon with no resources remains sparse.

Additional observer complexity is O(A log B) for exact acquisition observations,
O(N K log B) for sampled histogram updates (K <= 4), and O(B) storage. These are
additional costs, not a new complexity claim for the entire legacy solver.
Neither arrays proportional to the authored horizon nor retained run histories
are used. Comparison support is independently capped, including zero/horizon.

The production JSON renderer is exercised with a large concrete schema-4 DTO
that exceeds 64 MiB; it returns the typed output limit error. Smaller UTF-8
boundary tests cover JSON and text, including newline bytes. Report assembly
never writes authoritative stdout before rendering succeeds.

Peak process RSS for the four-target example, three release CLI executions per
variant, JSON rendered and discarded (10,000 runs for sampled/comparison):

| Command | Disabled RSS KiB range | Enabled RSS KiB range |
| --- | --- | --- |
| analyze | 5036–5304 | 5796–5808 |
| simulate | 4784–4784 | 5532–5872 |
| compare | 5284–5360 | 7344–7344 |

RSS includes loading, legacy results, timing results and rendering; these are
observations, not portable memory bounds. The elapsed times from this RSS batch
are excluded because workspace checks were running concurrently. The serial
in-process alternating harness above is the timing comparison.

Retained v2 benchmark observations from the same harness: single-target exact
0.92 ms; shared-dual 9.23 ms; campaign exact 16.77 ms; 10,000-run MC 120.65 ms.
Existing near-guard success and one-step-over failure remain covered. The new
v3 merge case passed at 29 processed states / 54 expansions and rejected at
53 expansions. The five-category four-target and large-initial-count repeating
fixtures are test-only; they are never packaged as runtime data.
