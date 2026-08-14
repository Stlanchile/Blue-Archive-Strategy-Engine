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
