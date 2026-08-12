# Exact-solver guard calibration

Calibration was run on every shipped scenario with exhaustive propagation,
without probability pruning or altered mechanics.

| Scenario | Peak boundary | Peak in-flight | Processed states | Emitted children |
|---|---:|---:|---:|---:|
| `campaign_dual_310` | 201 | 200 | 66,813 | 72,549 |
| `charge_199_one` | 1 | 1 | 3 | 1 |
| `charge_99_one` | 2 | 1 | 4 | 2 |
| `dual_independent_200` | 103 | 102 | 30,798 | 30,398 |
| `dual_shared_200` | 201 | 200 | 40,599 | 40,199 |
| `initial_success` | 0 | 0 | 0 | 0 |
| `single_target_200` | 2 | 1 | 600 | 399 |
| `ticket_atomic` | 10 | 9 | 57 | 91 |

Observed maxima are:

```text
frontier F = 201
processed P = 66,813
expansions X = 72,549
```

Four times those maxima round to powers of two below the required floors, so
v0.1 uses the frozen defaults:

```text
max_active_states = 65,536
max_processed_states = 1,048,576
max_transition_expansions = 2,097,152
conservation_tolerance = 1e-12
```

The defaults are safety limits, not policy inputs. Crossing one aborts the
analysis and returns no partial expectations or distributions.

Concrete execution has separate frozen availability limits:

```text
Monte Carlo runs = 1,000,000
primitive transitions per Monte Carlo run = 1,048,576
primitive transitions per Monte Carlo call = 100,000,000
materialized trace/replay primitive transitions = 100,000
```

These bounds are checked before excess work or trace growth and likewise return
no partial result.
