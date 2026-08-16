use std::collections::BTreeMap;
use std::num::NonZeroU64;

use ba_core::{
    PrimitiveAcquisition, ResourceLedger, ResourcesV3, StrategyDecision, TerminalReason,
    ValidatedScenarioBundleV3, WorldStateKey, apply_primitive_transition_v3, begin_action_v3,
    complete_action_v3, decide_v3, initial_world_v3, milestone_rewards_acquired_v3,
    outcome_distribution_v3, reconstruct_funding_v3, terminal_resources_v3,
};
use rand_chacha::ChaCha8Rng;
use rand_core::{RngCore, SeedableRng};
use sha2::{Digest, Sha256};

use crate::error::EngineError;
use crate::exact_v3::analyze_exact_v3;
use crate::options::{ExactSolverOptions, SimulationLimits};
use crate::result::{
    ConfidenceInterval, EstimateDiagnostics, RNG_ALGORITHM, RngProvenance,
    STREAM_DERIVATION_VERSION,
};
use crate::result_v3::{
    AbsoluteMilestoneReachProbabilityV3, AnalysisContextV3, AnalysisProvenanceV3,
    ComparisonResultV3, FirstCompletionProbabilityV3, IndicatorProbabilityIntervalV3,
    MonteCarloAnalysisResultV3, MonteCarloEstimationMetadataV3, MonteCarloSampleCountsV3,
    PrefixCompletionProbabilityV3, ProbabilityComparisonV3, ResourceEstimateDiagnosticsV3,
    RunTraceEventV3, RunTraceResultV3, TargetAcquisitionProbabilityV3,
    TerminalOwnedSetProbabilityIntervalV3, TerminalOwnedSetProbabilityV3,
    TerminalReasonProbabilityV3, TerminalSetComparisonV3, expected_from_ledger_sums, ledger_values,
};
use crate::sampling::uniform_below;

const STREAM_DOMAIN: &[u8] = b"ba-strategy/mc-run-stream/v1\0";

#[derive(Debug, Clone)]
struct ConcreteRunV3 {
    terminal: WorldStateKey,
    first_completion: Option<u64>,
    terminal_reason: TerminalReason,
    outcomes: Vec<PrimitiveAcquisition>,
    events: Vec<RunTraceEventV3>,
}

#[derive(Debug, Clone, Copy, Default)]
struct Moments {
    count: u64,
    mean: f64,
    m2: f64,
}

impl Moments {
    fn add(&mut self, value: f64) -> Result<(), EngineError> {
        if !value.is_finite() {
            return Err(EngineError::InternalInvariantViolation {
                message: "non-finite v3 Monte Carlo observation".to_owned(),
            });
        }
        self.count = self
            .count
            .checked_add(1)
            .ok_or(EngineError::ArithmeticOverflow {
                context: "counting v3 Monte Carlo moments",
            })?;
        let delta = value - self.mean;
        self.mean += delta / self.count as f64;
        let second_delta = value - self.mean;
        self.m2 += delta * second_delta;
        Ok(())
    }

    fn diagnostics(self) -> EstimateDiagnostics {
        let standard_error = if self.count <= 1 {
            0.0
        } else {
            let variance = self.m2 / (self.count - 1) as f64;
            (variance / self.count as f64).sqrt()
        };
        EstimateDiagnostics {
            standard_error,
            confidence_interval_95: (self.count > 1).then(|| {
                let half_width = 1.96 * standard_error;
                ConfidenceInterval {
                    lower: self.mean - half_width,
                    upper: self.mean + half_width,
                }
            }),
        }
    }
}

#[derive(Debug, Default)]
struct ResourceSumsV3 {
    values: [u128; 11],
}

impl ResourceSumsV3 {
    fn add(&mut self, ledger: ResourceLedger) -> Result<(), EngineError> {
        for (index, value) in ledger_values(ledger).into_iter().enumerate() {
            self.values[index] = self.values[index].checked_add(u128::from(value)).ok_or(
                EngineError::ArithmeticOverflow {
                    context: "accumulating v3 Monte Carlo integer resource totals",
                },
            )?;
        }
        Ok(())
    }

    fn expectation(&self, runs: u64) -> crate::ExpectedResourcesV3 {
        let divisor = runs as f64;
        expected_from_ledger_sums(self.values.map(|value| value as f64 / divisor))
    }
}

#[derive(Debug, Default)]
struct ResourceMomentsV3 {
    values: [Moments; 11],
}

impl ResourceMomentsV3 {
    fn add(&mut self, ledger: ResourceLedger) -> Result<(), EngineError> {
        for (index, value) in ledger_values(ledger).into_iter().enumerate() {
            self.values[index].add(value as f64)?;
        }
        Ok(())
    }

    fn diagnostics(self) -> ResourceEstimateDiagnosticsV3 {
        let values = self.values.map(Moments::diagnostics);
        ResourceEstimateDiagnosticsV3 {
            pyroxene: values[0].clone(),
            limited_ten_recruitment_tickets: values[1].clone(),
            eligma: values[2].clone(),
            advanced_bd_selectors: values[3].clone(),
            advanced_tech_note_selectors: values[4].clone(),
            superior_tech_note_selectors: values[5].clone(),
            gift_boxes: values[6].clone(),
            keystone_fragments: values[7].clone(),
            secret_tech_notes: values[8].clone(),
            superior_bd_selectors: values[9].clone(),
            high_grade_gift_boxes: values[10].clone(),
        }
    }
}

#[must_use]
pub fn derive_run_seed_v3(
    bundle: &ValidatedScenarioBundleV3,
    master_seed: u64,
    run_index: u64,
) -> [u8; 32] {
    let mut digest = Sha256::new();
    digest.update(STREAM_DOMAIN);
    digest.update(master_seed.to_le_bytes());
    digest.update(run_index.to_le_bytes());
    digest.update(bundle.fingerprints().scenario.as_bytes());
    digest.update(bundle.fingerprints().ruleset.as_bytes());
    digest.update(bundle.fingerprints().reward_schedule.as_bytes());
    digest.finalize().into()
}

pub fn simulate_monte_carlo_v3(
    bundle: &ValidatedScenarioBundleV3,
    runs: NonZeroU64,
    master_seed: u64,
) -> Result<MonteCarloAnalysisResultV3, EngineError> {
    simulate_monte_carlo_v3_with_limits(bundle, runs, master_seed, SimulationLimits::default())
}

pub fn simulate_monte_carlo_v3_with_limits(
    bundle: &ValidatedScenarioBundleV3,
    runs: NonZeroU64,
    master_seed: u64,
    limits: SimulationLimits,
) -> Result<MonteCarloAnalysisResultV3, EngineError> {
    let limits = limits.validate()?;
    let run_count = runs.get();
    if run_count > limits.max_runs {
        return Err(EngineError::SimulationRunLimitExceeded {
            requested: run_count,
            maximum: limits.max_runs,
        });
    }
    let mut successes = 0_u64;
    let mut terminal_reason_counts = BTreeMap::<TerminalReason, u64>::new();
    let mut owned_counts = BTreeMap::<u8, u64>::new();
    let mut first_completion_counts = BTreeMap::<u64, u64>::new();
    let mut terminal_count_counts = BTreeMap::<u64, u64>::new();
    let mut total_terminal_count = 0_u128;
    let mut successful_terminal_count = 0_u128;
    let mut total_first_completion_count = 0_u128;
    let mut total_paid_spend = 0_u128;
    let mut total_ticket_draws = 0_u128;
    let mut residual_sums = ResourceSumsV3::default();
    let mut reward_sums = ResourceSumsV3::default();
    let mut residual_moments = ResourceMomentsV3::default();
    let mut reward_moments = ResourceMomentsV3::default();
    let mut terminal_moments = Moments::default();
    let mut total_primitives = 0_u64;

    for run_index in 0..run_count {
        let mut rng = ChaCha8Rng::from_seed(derive_run_seed_v3(bundle, master_seed, run_index));
        let remaining_total = limits
            .max_total_primitive_transitions
            .checked_sub(total_primitives)
            .ok_or(EngineError::InternalInvariantViolation {
                message: "v3 Monte Carlo primitive total exceeded its validated limit".to_owned(),
            })?;
        let run_limit = limits
            .max_primitive_transitions_per_run
            .min(remaining_total);
        let scope = if run_limit == remaining_total
            && remaining_total < limits.max_primitive_transitions_per_run
        {
            "v3 Monte Carlo total"
        } else {
            "v3 Monte Carlo run"
        };
        let run = execute_run_v3(bundle, false, run_limit, scope, |distribution| {
            sample_outcome_v3(distribution, &mut rng)
        })?;
        total_primitives = total_primitives
            .checked_add(run.terminal.cumulative_primitive_recruitments)
            .ok_or(EngineError::ArithmeticOverflow {
                context: "counting v3 Monte Carlo primitive transitions",
            })?;
        let funding = reconstruct_funding_v3(bundle, &run.terminal)?;
        let residual = terminal_resources_v3(bundle, &run.terminal)?;
        let rewards =
            milestone_rewards_acquired_v3(bundle, run.terminal.cumulative_primitive_recruitments)?;

        increment_count(
            &mut terminal_reason_counts,
            run.terminal_reason,
            "counting v3 Monte Carlo terminal reasons",
        )?;
        increment_count(
            &mut owned_counts,
            run.terminal.owned_target_mask,
            "counting v3 Monte Carlo ownership masks",
        )?;
        increment_count(
            &mut terminal_count_counts,
            run.terminal.cumulative_primitive_recruitments,
            "counting v3 Monte Carlo terminal recruitment counts",
        )?;
        if run.terminal_reason == TerminalReason::TargetsAcquired {
            let first =
                run.first_completion
                    .ok_or_else(|| EngineError::InternalInvariantViolation {
                        message: "successful v3 run has no first-completion count".to_owned(),
                    })?;
            successes = successes
                .checked_add(1)
                .ok_or(EngineError::ArithmeticOverflow {
                    context: "counting successful v3 Monte Carlo runs",
                })?;
            successful_terminal_count = successful_terminal_count
                .checked_add(u128::from(run.terminal.cumulative_primitive_recruitments))
                .ok_or(EngineError::ArithmeticOverflow {
                    context: "summing successful v3 terminal counts",
                })?;
            total_first_completion_count = total_first_completion_count
                .checked_add(u128::from(first))
                .ok_or(EngineError::ArithmeticOverflow {
                    context: "summing v3 first-completion counts",
                })?;
        }
        if let Some(first) = run.first_completion {
            increment_count(
                &mut first_completion_counts,
                first,
                "counting v3 first-completion samples",
            )?;
        }
        total_terminal_count = total_terminal_count
            .checked_add(u128::from(run.terminal.cumulative_primitive_recruitments))
            .ok_or(EngineError::ArithmeticOverflow {
                context: "summing v3 terminal recruitment samples",
            })?;
        total_paid_spend = total_paid_spend
            .checked_add(u128::from(funding.paid_pyroxene_spent))
            .ok_or(EngineError::ArithmeticOverflow {
                context: "summing v3 paid-spend samples",
            })?;
        total_ticket_draws = total_ticket_draws
            .checked_add(u128::from(funding.ticket_funded_primitive_recruitments))
            .ok_or(EngineError::ArithmeticOverflow {
                context: "summing v3 ticket-funded samples",
            })?;
        residual_sums.add(residual)?;
        reward_sums.add(rewards)?;
        residual_moments.add(residual)?;
        reward_moments.add(rewards)?;
        terminal_moments.add(run.terminal.cumulative_primitive_recruitments as f64)?;
    }

    let divisor = run_count as f64;
    let all_mask = bundle.scenario().all_targets_mask();
    for mask in 0..=all_mask {
        owned_counts.entry(mask).or_insert(0);
    }
    let terminal_owned_set_probabilities = owned_counts
        .iter()
        .map(|(mask, count)| TerminalOwnedSetProbabilityV3 {
            owned_targets: owned_targets(bundle, *mask),
            probability: *count as f64 / divisor,
        })
        .collect::<Vec<_>>();
    let terminal_owned_set_probability_intervals_95 = owned_counts
        .iter()
        .map(|(mask, count)| TerminalOwnedSetProbabilityIntervalV3 {
            owned_targets: owned_targets(bundle, *mask),
            sample_count: *count,
            confidence_interval_95: wilson_interval(*count, run_count),
        })
        .collect::<Vec<_>>();
    let terminal_reason_probabilities = [
        TerminalReason::TargetsAcquired,
        TerminalReason::ResourcesExhausted,
        TerminalReason::StrategyHorizonReached,
    ]
    .into_iter()
    .map(|reason| TerminalReasonProbabilityV3 {
        terminal_reason: reason,
        probability: terminal_reason_counts.get(&reason).copied().unwrap_or(0) as f64 / divisor,
    })
    .collect();

    let target_counts = (0..bundle.scenario().targets().len())
        .map(|index| {
            owned_counts
                .iter()
                .filter(|(mask, _)| **mask & (1_u8 << index) != 0)
                .map(|(_, count)| *count)
                .try_fold(0_u64, |total, count| total.checked_add(count))
                .ok_or(EngineError::ArithmeticOverflow {
                    context: "counting v3 target acquisition samples",
                })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let prefix_counts = (0..bundle.scenario().targets().len())
        .map(|index| {
            let prefix_mask = (1_u8 << (index + 1)) - 1;
            owned_counts
                .iter()
                .filter(|(mask, _)| **mask & prefix_mask == prefix_mask)
                .map(|(_, count)| *count)
                .try_fold(0_u64, |total, count| total.checked_add(count))
                .ok_or(EngineError::ArithmeticOverflow {
                    context: "counting v3 prefix completion samples",
                })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let per_target_acquisition_probabilities = bundle
        .scenario()
        .targets()
        .iter()
        .zip(&target_counts)
        .map(|(target, count)| TargetAcquisitionProbabilityV3 {
            target_id: target.student_id.clone(),
            probability: *count as f64 / divisor,
        })
        .collect();
    let per_target_probability_intervals_95 = bundle
        .scenario()
        .targets()
        .iter()
        .zip(&target_counts)
        .map(|(target, count)| IndicatorProbabilityIntervalV3 {
            id: target.student_id.to_string(),
            sample_count: *count,
            confidence_interval_95: wilson_interval(*count, run_count),
        })
        .collect();
    let ordered_prefix_completion_probabilities = bundle
        .scenario()
        .targets()
        .iter()
        .enumerate()
        .zip(&prefix_counts)
        .map(|((index, _), count)| PrefixCompletionProbabilityV3 {
            prefix_length: index + 1,
            target_ids: bundle.scenario().targets()[..=index]
                .iter()
                .map(|target| target.student_id.clone())
                .collect(),
            probability: *count as f64 / divisor,
        })
        .collect();
    let ordered_prefix_probability_intervals_95 = prefix_counts
        .iter()
        .enumerate()
        .map(|(index, count)| IndicatorProbabilityIntervalV3 {
            id: format!("prefix_{}", index + 1),
            sample_count: *count,
            confidence_interval_95: wilson_interval(*count, run_count),
        })
        .collect();
    let absolute_campaign_milestone_reach_probabilities =
        milestone_reach_counts_v3(bundle, &terminal_count_counts)?
            .into_iter()
            .map(|(count, samples)| AbsoluteMilestoneReachProbabilityV3 {
                absolute_campaign_recruitment_count: count,
                probability: samples as f64 / divisor,
            })
            .collect();
    let first_all_target_completion_pmf = first_completion_counts
        .iter()
        .map(|(count, samples)| FirstCompletionProbabilityV3 {
            additional_recruitment_count: *count,
            probability: *samples as f64 / divisor,
        })
        .collect();
    let cumulative = cumulative_counts(&first_completion_counts, run_count)?;
    let first_all_target_completion_cdf = cumulative
        .into_iter()
        .map(|(count, samples)| FirstCompletionProbabilityV3 {
            additional_recruitment_count: count,
            probability: samples as f64 / divisor,
        })
        .collect();

    Ok(MonteCarloAnalysisResultV3 {
        engine_kind: "monte_carlo",
        provenance: AnalysisProvenanceV3::from_bundle(bundle),
        context: AnalysisContextV3::from_bundle(bundle),
        rng: rng_provenance(master_seed, run_count),
        all_target_success_probability: successes as f64 / divisor,
        terminal_owned_set_probabilities,
        terminal_reason_probabilities,
        per_target_acquisition_probabilities,
        ordered_prefix_completion_probabilities,
        expected_additional_primitive_recruitments: total_terminal_count as f64 / divisor,
        expected_additional_primitive_recruitments_given_success: (successes > 0)
            .then_some(successful_terminal_count as f64 / successes as f64),
        expected_first_all_target_completion_count_given_success: (successes > 0)
            .then_some(total_first_completion_count as f64 / successes as f64),
        expected_paid_pyroxene_spent: total_paid_spend as f64 / divisor,
        expected_ticket_funded_primitive_recruitments: total_ticket_draws as f64 / divisor,
        expected_residual_resources: residual_sums.expectation(run_count),
        expected_milestone_rewards_acquired: reward_sums.expectation(run_count),
        absolute_campaign_milestone_reach_probabilities,
        first_all_target_completion_pmf,
        first_all_target_completion_cdf,
        sample_counts: MonteCarloSampleCountsV3 {
            total_runs: run_count,
            successful_runs: successes,
        },
        estimation: MonteCarloEstimationMetadataV3 {
            all_target_success_probability_interval_95: wilson_interval(successes, run_count),
            per_target_probability_intervals_95,
            ordered_prefix_probability_intervals_95,
            terminal_owned_set_probability_intervals_95,
            expected_additional_primitive_recruitments: terminal_moments.diagnostics(),
            expected_residual_resources: residual_moments.diagnostics(),
            expected_milestone_rewards_acquired: reward_moments.diagnostics(),
        },
    })
}

pub fn simulate_trace_v3(
    bundle: &ValidatedScenarioBundleV3,
    master_seed: u64,
) -> Result<RunTraceResultV3, EngineError> {
    simulate_trace_v3_with_limits(bundle, master_seed, SimulationLimits::default())
}

pub fn simulate_trace_v3_with_limits(
    bundle: &ValidatedScenarioBundleV3,
    master_seed: u64,
    limits: SimulationLimits,
) -> Result<RunTraceResultV3, EngineError> {
    let limits = limits.validate()?;
    let mut rng = ChaCha8Rng::from_seed(derive_run_seed_v3(bundle, master_seed, 0));
    let run = execute_run_v3(
        bundle,
        true,
        limits.max_trace_primitive_transitions,
        "v3 trace",
        |distribution| sample_outcome_v3(distribution, &mut rng),
    )?;
    concrete_result_v3(bundle, run, Some(rng_provenance(master_seed, 1)))
}

pub fn replay_v3(
    bundle: &ValidatedScenarioBundleV3,
    outcomes: &[PrimitiveAcquisition],
) -> Result<RunTraceResultV3, EngineError> {
    replay_v3_with_limits(bundle, outcomes, SimulationLimits::default())
}

pub fn replay_v3_with_limits(
    bundle: &ValidatedScenarioBundleV3,
    outcomes: &[PrimitiveAcquisition],
    limits: SimulationLimits,
) -> Result<RunTraceResultV3, EngineError> {
    let limits = limits.validate()?;
    let mut cursor = 0_usize;
    let run =
        execute_run_v3(
            bundle,
            true,
            limits.max_trace_primitive_transitions,
            "v3 replay",
            |distribution| {
                let outcome = outcomes.get(cursor).copied().ok_or_else(|| {
                    EngineError::InvalidTransition {
                        message: format!(
                            "v3 replay outcome stream ended at primitive draw {cursor}"
                        ),
                    }
                })?;
                cursor = cursor
                    .checked_add(1)
                    .ok_or(EngineError::ArithmeticOverflow {
                        context: "advancing v3 replay outcome cursor",
                    })?;
                if distribution.contains(outcome) {
                    Ok(outcome)
                } else {
                    Err(EngineError::InvalidTransition {
                        message: format!(
                            "v3 replay outcome {outcome:?} is impossible at primitive draw {}",
                            cursor - 1
                        ),
                    })
                }
            },
        )?;
    if cursor != outcomes.len() {
        return Err(EngineError::InvalidTransition {
            message: format!(
                "v3 replay supplied {} unused outcomes after terminal state",
                outcomes.len() - cursor
            ),
        });
    }
    concrete_result_v3(bundle, run, None)
}

pub fn compare_v3(
    bundle: &ValidatedScenarioBundleV3,
    runs: NonZeroU64,
    master_seed: u64,
) -> Result<ComparisonResultV3, EngineError> {
    let exact = analyze_exact_v3(bundle, ExactSolverOptions::default())?;
    let monte_carlo = simulate_monte_carlo_v3(bundle, runs, master_seed)?;
    let success_interval = monte_carlo
        .estimation
        .all_target_success_probability_interval_95;
    let all_target_success = ProbabilityComparisonV3 {
        id: "all_targets".to_owned(),
        simulation_minus_exact: monte_carlo.all_target_success_probability
            - exact.all_target_success_probability,
        exact_within_monte_carlo_interval: (success_interval.lower..=success_interval.upper)
            .contains(&exact.all_target_success_probability),
    };
    let per_target = exact
        .per_target_acquisition_probabilities
        .iter()
        .zip(&monte_carlo.per_target_acquisition_probabilities)
        .zip(&monte_carlo.estimation.per_target_probability_intervals_95)
        .map(
            |((exact_metric, sampled), interval)| ProbabilityComparisonV3 {
                id: exact_metric.target_id.to_string(),
                simulation_minus_exact: sampled.probability - exact_metric.probability,
                exact_within_monte_carlo_interval: (interval.confidence_interval_95.lower
                    ..=interval.confidence_interval_95.upper)
                    .contains(&exact_metric.probability),
            },
        )
        .collect();
    let ordered_prefixes = exact
        .ordered_prefix_completion_probabilities
        .iter()
        .zip(&monte_carlo.ordered_prefix_completion_probabilities)
        .zip(
            &monte_carlo
                .estimation
                .ordered_prefix_probability_intervals_95,
        )
        .map(
            |((exact_metric, sampled), interval)| ProbabilityComparisonV3 {
                id: format!("prefix_{}", exact_metric.prefix_length),
                simulation_minus_exact: sampled.probability - exact_metric.probability,
                exact_within_monte_carlo_interval: (interval.confidence_interval_95.lower
                    ..=interval.confidence_interval_95.upper)
                    .contains(&exact_metric.probability),
            },
        )
        .collect();

    let sampled_by_set = monte_carlo
        .terminal_owned_set_probabilities
        .iter()
        .map(|metric| {
            (
                metric
                    .owned_targets
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>(),
                metric.probability,
            )
        })
        .collect::<BTreeMap<_, _>>();
    let intervals_by_set = monte_carlo
        .estimation
        .terminal_owned_set_probability_intervals_95
        .iter()
        .map(|metric| {
            (
                metric
                    .owned_targets
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>(),
                metric.confidence_interval_95,
            )
        })
        .collect::<BTreeMap<_, _>>();
    let exact_by_set = exact
        .terminal_owned_set_probabilities
        .iter()
        .map(|metric| {
            (
                metric
                    .owned_targets
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>(),
                metric.probability,
            )
        })
        .collect::<BTreeMap<_, _>>();
    let terminal_owned_sets = (0..=bundle.scenario().all_targets_mask())
        .map(|mask| {
            let owned_targets = owned_targets(bundle, mask);
            let key = owned_targets
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>();
            let exact_probability = exact_by_set.get(&key).copied().unwrap_or(0.0);
            let sampled = sampled_by_set.get(&key).copied().unwrap_or(0.0);
            let interval = intervals_by_set.get(&key).copied().ok_or(
                EngineError::InternalInvariantViolation {
                    message: "v3 terminal-set comparison interval is missing".to_owned(),
                },
            )?;
            Ok(TerminalSetComparisonV3 {
                owned_targets,
                simulation_minus_exact: sampled - exact_probability,
                exact_within_monte_carlo_interval: (interval.lower..=interval.upper)
                    .contains(&exact_probability),
            })
        })
        .collect::<Result<Vec<_>, EngineError>>()?;

    Ok(ComparisonResultV3 {
        engine_kind: "comparison",
        exact,
        monte_carlo,
        all_target_success,
        per_target,
        ordered_prefixes,
        terminal_owned_sets,
    })
}

fn execute_run_v3<F>(
    bundle: &ValidatedScenarioBundleV3,
    trace: bool,
    primitive_limit: u64,
    limit_scope: &'static str,
    mut choose_outcome: F,
) -> Result<ConcreteRunV3, EngineError>
where
    F: FnMut(&ba_core::CompiledOutcomeDistribution) -> Result<PrimitiveAcquisition, EngineError>,
{
    let mut world = initial_world_v3(bundle);
    let mut first_completion =
        (world.owned_target_mask == bundle.scenario().all_targets_mask()).then_some(0);
    let mut outcomes = Vec::new();
    let mut events = Vec::new();
    if first_completion == Some(0) && trace {
        events.push(RunTraceEventV3::FirstAllTargetsCompleted {
            additional_recruitment_count: 0,
            absolute_campaign_recruitment_count: bundle.scenario().initial_recruitment_count(),
        });
    }
    let terminal_reason = loop {
        match decide_v3(bundle, &world)? {
            StrategyDecision::Stop(reason) => break reason,
            StrategyDecision::Act(action) => {
                let (mut in_flight, started) = begin_action_v3(bundle, &world, &action)?;
                if trace {
                    events.push(RunTraceEventV3::ActionStarted(started));
                }
                while in_flight.remaining_primitive_draws > 0 {
                    let observed = in_flight
                        .world
                        .cumulative_primitive_recruitments
                        .checked_add(1)
                        .ok_or(EngineError::ArithmeticOverflow {
                            context: "counting concrete v3 primitive transitions",
                        })?;
                    if observed > primitive_limit {
                        return Err(EngineError::SimulationPrimitiveLimitExceeded {
                            scope: limit_scope,
                            observed,
                            maximum: primitive_limit,
                        });
                    }
                    let distribution = outcome_distribution_v3(bundle, &in_flight)?;
                    let outcome = choose_outcome(distribution)?;
                    if trace {
                        outcomes.push(outcome);
                    }
                    let transitioned = apply_primitive_transition_v3(bundle, &in_flight, outcome)?;
                    if trace {
                        events.push(RunTraceEventV3::PrimitiveTransition(
                            transitioned.event.clone(),
                        ));
                        if !transitioned.event.rewards.is_empty() {
                            events.push(RunTraceEventV3::RewardGranted {
                                absolute_campaign_recruitment_count: transitioned
                                    .event
                                    .absolute_campaign_recruitment_count,
                                rewards: transitioned.event.rewards.clone(),
                            });
                        }
                    }
                    if transitioned.event.first_all_targets_completed {
                        first_completion = Some(transitioned.event.additional_recruitment_count);
                        if trace {
                            events.push(RunTraceEventV3::FirstAllTargetsCompleted {
                                additional_recruitment_count: transitioned
                                    .event
                                    .additional_recruitment_count,
                                absolute_campaign_recruitment_count: transitioned
                                    .event
                                    .absolute_campaign_recruitment_count,
                            });
                        }
                    }
                    in_flight = transitioned.state;
                    if in_flight.world.cumulative_primitive_recruitments
                        > bundle.compiled_strategy().max_additional_recruitments.get()
                    {
                        return Err(EngineError::InternalInvariantViolation {
                            message: "concrete v3 run exceeded the validated additional horizon"
                                .to_owned(),
                        });
                    }
                }
                let (next_world, completed) = complete_action_v3(in_flight)?;
                world = next_world;
                if trace {
                    events.push(RunTraceEventV3::ActionCompleted(completed));
                }
            }
        }
    };
    if trace {
        events.push(RunTraceEventV3::Terminal { terminal_reason });
    }
    Ok(ConcreteRunV3 {
        terminal: world,
        first_completion,
        terminal_reason,
        outcomes,
        events,
    })
}

fn concrete_result_v3(
    bundle: &ValidatedScenarioBundleV3,
    run: ConcreteRunV3,
    rng: Option<RngProvenance>,
) -> Result<RunTraceResultV3, EngineError> {
    let funding = reconstruct_funding_v3(bundle, &run.terminal)?;
    let terminal_resources = terminal_resources_v3(bundle, &run.terminal)?;
    let milestone_rewards =
        milestone_rewards_acquired_v3(bundle, run.terminal.cumulative_primitive_recruitments)?;
    Ok(RunTraceResultV3 {
        engine_kind: "trace",
        provenance: AnalysisProvenanceV3::from_bundle(bundle),
        context: AnalysisContextV3::from_bundle(bundle),
        rng,
        terminal_additional_primitive_recruitments: run.terminal.cumulative_primitive_recruitments,
        terminal_absolute_campaign_recruitment_count: bundle
            .scenario()
            .absolute_campaign_count(run.terminal.cumulative_primitive_recruitments)?,
        first_all_target_completion_additional_count: run.first_completion,
        paid_pyroxene_spent: funding.paid_pyroxene_spent,
        ticket_funded_primitive_recruitments: funding.ticket_funded_primitive_recruitments,
        terminal_resources: ResourcesV3::from(terminal_resources),
        milestone_rewards_acquired: ResourcesV3::from(milestone_rewards),
        terminal_owned_targets: owned_targets(bundle, run.terminal.owned_target_mask),
        terminal_reason: run.terminal_reason,
        replay_outcomes: run.outcomes,
        events: run.events,
    })
}

fn sample_outcome_v3(
    distribution: &ba_core::CompiledOutcomeDistribution,
    rng: &mut impl RngCore,
) -> Result<PrimitiveAcquisition, EngineError> {
    match distribution.branches() {
        [only] => Ok(only.acquisition),
        branches => {
            let sampled = uniform_below(rng, distribution.denominator().get())?;
            branches
                .iter()
                .find(|branch| sampled < branch.upper_exclusive)
                .map(|branch| branch.acquisition)
                .ok_or(EngineError::InternalInvariantViolation {
                    message: "v3 categorical sample exceeded every half-open endpoint".to_owned(),
                })
        }
    }
}

fn wilson_interval(successes: u64, runs: u64) -> ConfidenceInterval {
    let n = runs as f64;
    let p = successes as f64 / n;
    let z = 1.96_f64;
    let z_squared = z * z;
    let denominator = 1.0 + z_squared / n;
    let center = (p + z_squared / (2.0 * n)) / denominator;
    let half_width = z * ((p * (1.0 - p) / n + z_squared / (4.0 * n * n)).sqrt()) / denominator;
    ConfidenceInterval {
        lower: (center - half_width).max(0.0),
        upper: (center + half_width).min(1.0),
    }
}

fn milestone_reach_counts_v3(
    bundle: &ValidatedScenarioBundleV3,
    terminal_count_counts: &BTreeMap<u64, u64>,
) -> Result<Vec<(u64, u64)>, EngineError> {
    let mut terminal_counts = terminal_count_counts.iter().rev().peekable();
    let mut running_count = 0_u64;
    let mut reversed = Vec::with_capacity(bundle.scenario().effective_milestones().len());
    for milestone in bundle.scenario().effective_milestones().iter().rev() {
        let additional = milestone
            .count
            .checked_sub(bundle.scenario().initial_recruitment_count())
            .ok_or(EngineError::InternalInvariantViolation {
                message: "effective v3 milestone precedes initial count".to_owned(),
            })?;
        while terminal_counts
            .peek()
            .is_some_and(|(count, _)| **count >= additional)
        {
            if let Some((_, count)) = terminal_counts.next() {
                running_count =
                    running_count
                        .checked_add(*count)
                        .ok_or(EngineError::ArithmeticOverflow {
                            context: "summing v3 milestone reach samples",
                        })?;
            }
        }
        reversed.push((milestone.count, running_count));
    }
    reversed.reverse();
    Ok(reversed)
}

fn cumulative_counts(
    counts: &BTreeMap<u64, u64>,
    run_count: u64,
) -> Result<Vec<(u64, u64)>, EngineError> {
    let mut running = 0_u64;
    let mut cumulative = Vec::with_capacity(counts.len());
    for (coordinate, count) in counts {
        running = running
            .checked_add(*count)
            .ok_or(EngineError::ArithmeticOverflow {
                context: "summing v3 first-completion CDF samples",
            })?;
        if running > run_count {
            return Err(EngineError::InternalInvariantViolation {
                message: "v3 first-completion samples exceed run count".to_owned(),
            });
        }
        cumulative.push((*coordinate, running));
    }
    Ok(cumulative)
}

fn rng_provenance(master_seed: u64, run_count: u64) -> RngProvenance {
    RngProvenance {
        rng_algorithm: RNG_ALGORITHM,
        master_seed,
        run_count,
        run_index_contract: "zero-based ascending indices 0..runs-1",
        stream_derivation_version: STREAM_DERIVATION_VERSION,
    }
}

fn increment_count<K: Ord + Copy>(
    counts: &mut BTreeMap<K, u64>,
    key: K,
    context: &'static str,
) -> Result<(), EngineError> {
    let entry = counts.entry(key).or_default();
    *entry = entry
        .checked_add(1)
        .ok_or(EngineError::ArithmeticOverflow { context })?;
    Ok(())
}

fn owned_targets(bundle: &ValidatedScenarioBundleV3, mask: u8) -> Vec<ba_core::StudentId> {
    bundle
        .scenario()
        .targets()
        .iter()
        .enumerate()
        .filter(|(index, _)| mask & (1_u8 << index) != 0)
        .map(|(_, target)| target.student_id.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;

    use ba_core::{
        CompiledOutcomeDistribution, PrimitiveAcquisition, ProbabilityRatio, TargetIndex,
    };
    use rand_core::RngCore;

    use super::sample_outcome_v3;

    struct CountingRng {
        values: VecDeque<u64>,
        calls: u64,
    }

    impl RngCore for CountingRng {
        fn next_u32(&mut self) -> u32 {
            self.next_u64() as u32
        }

        fn next_u64(&mut self) -> u64 {
            self.calls += 1;
            self.values.pop_front().unwrap_or(0)
        }

        fn fill_bytes(&mut self, destination: &mut [u8]) {
            for chunk in destination.chunks_mut(8) {
                let bytes = self.next_u64().to_le_bytes();
                chunk.copy_from_slice(&bytes[..chunk.len()]);
            }
        }
    }

    #[test]
    fn categorical_half_open_boundaries_are_exact() {
        let target = TargetIndex::new(1, 2).expect("target");
        let distribution = CompiledOutcomeDistribution::compile(
            ProbabilityRatio::new(2, 10).expect("featured"),
            10,
            &[(target, 3)],
        )
        .expect("distribution");
        for (sample, expected) in [
            (0, PrimitiveAcquisition::CurrentFeaturedTarget),
            (1, PrimitiveAcquisition::CurrentFeaturedTarget),
            (
                2,
                PrimitiveAcquisition::OtherConfiguredTarget {
                    target_index: target,
                },
            ),
            (
                4,
                PrimitiveAcquisition::OtherConfiguredTarget {
                    target_index: target,
                },
            ),
            (5, PrimitiveAcquisition::NoConfiguredTarget),
            (9, PrimitiveAcquisition::NoConfiguredTarget),
        ] {
            let mut rng = CountingRng {
                values: VecDeque::from([if sample < 6 { sample + 10 } else { sample }]),
                calls: 0,
            };
            assert_eq!(
                sample_outcome_v3(&distribution, &mut rng).expect("sample"),
                expected
            );
            assert_eq!(rng.calls, 1);
        }
    }

    #[test]
    fn deterministic_categorical_distribution_consumes_no_rng() {
        let distribution = CompiledOutcomeDistribution::compile(
            ProbabilityRatio::new(1, 1).expect("certain"),
            1,
            &[],
        )
        .expect("distribution");
        let mut rng = CountingRng {
            values: VecDeque::new(),
            calls: 0,
        };
        assert_eq!(
            sample_outcome_v3(&distribution, &mut rng).expect("sample"),
            PrimitiveAcquisition::CurrentFeaturedTarget
        );
        assert_eq!(rng.calls, 0);
    }

    #[test]
    fn equivalent_scales_select_identical_outcomes() {
        let target = TargetIndex::new(1, 2).expect("target");
        let left = CompiledOutcomeDistribution::compile(
            ProbabilityRatio::new(7, 1000).expect("featured"),
            1000,
            &[(target, 7)],
        )
        .expect("left");
        let right = CompiledOutcomeDistribution::compile(
            ProbabilityRatio::new(7, 1000).expect("featured"),
            10_000,
            &[(target, 70)],
        )
        .expect("right");
        for sample in [0, 6, 7, 13, 14, 999] {
            let mut left_rng = CountingRng {
                values: VecDeque::from([sample + 1000]),
                calls: 0,
            };
            let mut right_rng = CountingRng {
                values: VecDeque::from([sample + 1000]),
                calls: 0,
            };
            assert_eq!(
                sample_outcome_v3(&left, &mut left_rng).expect("left sample"),
                sample_outcome_v3(&right, &mut right_rng).expect("right sample")
            );
        }
    }
}
