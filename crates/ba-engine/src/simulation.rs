use std::collections::BTreeMap;
use std::num::NonZeroU64;

use ba_core::{
    RecruitOutcome, Resources, StrategyDecision, TerminalReason, ValidatedScenarioBundle,
    WorldStateKey, apply_primitive_transition, begin_action, complete_action, decide,
    initial_world, milestone_rewards_acquired, outcome_distribution, reconstruct_funding,
    terminal_resources,
};
use rand_chacha::ChaCha8Rng;
use rand_core::{RngCore, SeedableRng};
use sha2::{Digest, Sha256};

use crate::error::EngineError;
use crate::exact::analyze_exact;
use crate::options::{ExactSolverOptions, SimulationLimits};
use crate::result::{
    AnalysisContext, AnalysisProvenance, ComparisonResult, ConfidenceInterval, EstimateDiagnostics,
    ExpectedResources, FirstSuccessProbability, MilestoneReachProbability,
    MonteCarloAnalysisResult, MonteCarloEstimationMetadata, MonteCarloProbabilityIntervals,
    MonteCarloSampleCounts, OwnedTargetProbabilityInterval, OwnedTargetTerminalProbability,
    RNG_ALGORITHM, RecruitmentCountProbabilityInterval, ResourceEstimateDiagnostics, RngProvenance,
    RunTraceEvent, RunTraceResult, STREAM_DERIVATION_VERSION, TerminalReasonProbability,
    TerminalReasonProbabilityInterval,
};

const STREAM_DOMAIN: &[u8] = b"ba-strategy/mc-run-stream/v1\0";

#[derive(Debug, Clone)]
struct ConcreteRun {
    terminal: WorldStateKey,
    first_success: Option<u64>,
    terminal_reason: TerminalReason,
    outcomes: Vec<RecruitOutcome>,
    events: Vec<RunTraceEvent>,
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
                message: "non-finite Monte Carlo observation".to_owned(),
            });
        }
        self.count = self
            .count
            .checked_add(1)
            .ok_or(EngineError::ArithmeticOverflow {
                context: "counting Monte Carlo moments",
            })?;
        let delta = value - self.mean;
        self.mean += delta / self.count as f64;
        let second_delta = value - self.mean;
        self.m2 += delta * second_delta;
        Ok(())
    }

    fn standard_error(self) -> f64 {
        if self.count <= 1 {
            0.0
        } else {
            let variance = self.m2 / (self.count - 1) as f64;
            (variance / self.count as f64).sqrt()
        }
    }

    fn diagnostics(self) -> EstimateDiagnostics {
        let standard_error = self.standard_error();
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
struct ResourceSums {
    pyroxene: u128,
    tickets: u128,
    eligma: u128,
    advanced_bd: u128,
    advanced_tech: u128,
    superior_tech: u128,
    gift_boxes: u128,
}

impl ResourceSums {
    fn add(&mut self, value: Resources) -> Result<(), EngineError> {
        self.pyroxene = checked_u128_add(self.pyroxene, value.pyroxene)?;
        self.tickets = checked_u128_add(self.tickets, value.limited_ten_recruitment_tickets)?;
        self.eligma = checked_u128_add(self.eligma, value.eligma)?;
        self.advanced_bd = checked_u128_add(self.advanced_bd, value.advanced_bd_selectors)?;
        self.advanced_tech =
            checked_u128_add(self.advanced_tech, value.advanced_tech_note_selectors)?;
        self.superior_tech =
            checked_u128_add(self.superior_tech, value.superior_tech_note_selectors)?;
        self.gift_boxes = checked_u128_add(self.gift_boxes, value.gift_boxes)?;
        Ok(())
    }

    fn expectation(&self, runs: u64) -> ExpectedResources {
        let divisor = runs as f64;
        ExpectedResources {
            pyroxene: self.pyroxene as f64 / divisor,
            limited_ten_recruitment_tickets: self.tickets as f64 / divisor,
            eligma: self.eligma as f64 / divisor,
            advanced_bd_selectors: self.advanced_bd as f64 / divisor,
            advanced_tech_note_selectors: self.advanced_tech as f64 / divisor,
            superior_tech_note_selectors: self.superior_tech as f64 / divisor,
            gift_boxes: self.gift_boxes as f64 / divisor,
        }
    }
}

#[derive(Debug, Default)]
struct ResourceMoments {
    pyroxene: Moments,
    tickets: Moments,
    eligma: Moments,
    advanced_bd: Moments,
    advanced_tech: Moments,
    superior_tech: Moments,
    gift_boxes: Moments,
}

impl ResourceMoments {
    fn add(&mut self, value: Resources) -> Result<(), EngineError> {
        self.pyroxene.add(value.pyroxene as f64)?;
        self.tickets
            .add(value.limited_ten_recruitment_tickets as f64)?;
        self.eligma.add(value.eligma as f64)?;
        self.advanced_bd.add(value.advanced_bd_selectors as f64)?;
        self.advanced_tech
            .add(value.advanced_tech_note_selectors as f64)?;
        self.superior_tech
            .add(value.superior_tech_note_selectors as f64)?;
        self.gift_boxes.add(value.gift_boxes as f64)?;
        Ok(())
    }

    fn diagnostics(self) -> ResourceEstimateDiagnostics {
        ResourceEstimateDiagnostics {
            pyroxene: self.pyroxene.diagnostics(),
            limited_ten_recruitment_tickets: self.tickets.diagnostics(),
            eligma: self.eligma.diagnostics(),
            advanced_bd_selectors: self.advanced_bd.diagnostics(),
            advanced_tech_note_selectors: self.advanced_tech.diagnostics(),
            superior_tech_note_selectors: self.superior_tech.diagnostics(),
            gift_boxes: self.gift_boxes.diagnostics(),
        }
    }
}

pub fn derive_run_seed(
    bundle: &ValidatedScenarioBundle,
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

pub fn simulate_monte_carlo(
    bundle: &ValidatedScenarioBundle,
    runs: NonZeroU64,
    master_seed: u64,
) -> Result<MonteCarloAnalysisResult, EngineError> {
    simulate_monte_carlo_with_limits(bundle, runs, master_seed, SimulationLimits::default())
}

pub fn simulate_monte_carlo_with_limits(
    bundle: &ValidatedScenarioBundle,
    runs: NonZeroU64,
    master_seed: u64,
    limits: SimulationLimits,
) -> Result<MonteCarloAnalysisResult, EngineError> {
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
    let mut first_success_counts = BTreeMap::<u64, u64>::new();
    let mut milestone_counts = bundle
        .reward_schedule()
        .milestones()
        .iter()
        .map(|milestone| (milestone.count, 0_u64))
        .collect::<BTreeMap<_, _>>();
    let mut total_terminal_count = 0_u128;
    let mut successful_terminal_count = 0_u128;
    let mut total_first_success_count = 0_u128;
    let mut total_paid_spend = 0_u128;
    let mut total_ticket_draws = 0_u128;
    let mut residual_sums = ResourceSums::default();
    let mut reward_sums = ResourceSums::default();
    let mut residual_moments = ResourceMoments::default();
    let mut reward_moments = ResourceMoments::default();
    let mut terminal_moments = Moments::default();
    let mut successful_terminal_moments = Moments::default();
    let mut first_success_moments = Moments::default();
    let mut paid_moments = Moments::default();
    let mut ticket_moments = Moments::default();
    let mut total_primitives = 0_u64;

    for run_index in 0..run_count {
        let mut rng = ChaCha8Rng::from_seed(derive_run_seed(bundle, master_seed, run_index));
        let remaining_total = limits
            .max_total_primitive_transitions
            .checked_sub(total_primitives)
            .ok_or(EngineError::InternalInvariantViolation {
                message: "Monte Carlo primitive total exceeded its validated limit".to_owned(),
            })?;
        let run_limit = limits
            .max_primitive_transitions_per_run
            .min(remaining_total);
        let scope = if run_limit == remaining_total
            && remaining_total < limits.max_primitive_transitions_per_run
        {
            "Monte Carlo total"
        } else {
            "Monte Carlo run"
        };
        let run = execute_run(bundle, false, run_limit, scope, |branches| {
            sample_outcome(branches, &mut rng)
        })?;
        total_primitives = total_primitives
            .checked_add(run.terminal.cumulative_primitive_recruitments)
            .ok_or(EngineError::ArithmeticOverflow {
                context: "counting Monte Carlo primitive transitions",
            })?;
        let funding = reconstruct_funding(bundle, &run.terminal)?;
        let residual = terminal_resources(bundle, &run.terminal)?;
        let rewards =
            milestone_rewards_acquired(bundle, run.terminal.cumulative_primitive_recruitments)?;

        *terminal_reason_counts
            .entry(run.terminal_reason)
            .or_default() = terminal_reason_counts
            .get(&run.terminal_reason)
            .copied()
            .unwrap_or(0)
            .checked_add(1)
            .ok_or(EngineError::ArithmeticOverflow {
                context: "counting Monte Carlo terminal reasons",
            })?;
        *owned_counts
            .entry(run.terminal.owned_target_mask)
            .or_default() = owned_counts
            .get(&run.terminal.owned_target_mask)
            .copied()
            .unwrap_or(0)
            .checked_add(1)
            .ok_or(EngineError::ArithmeticOverflow {
                context: "counting Monte Carlo ownership masks",
            })?;
        if run.terminal_reason == TerminalReason::TargetsAcquired {
            let first =
                run.first_success
                    .ok_or_else(|| EngineError::InternalInvariantViolation {
                        message: "successful run has no first-success count".to_owned(),
                    })?;
            successes = successes
                .checked_add(1)
                .ok_or(EngineError::ArithmeticOverflow {
                    context: "counting successful Monte Carlo runs",
                })?;
            successful_terminal_count = successful_terminal_count
                .checked_add(u128::from(run.terminal.cumulative_primitive_recruitments))
                .ok_or(EngineError::ArithmeticOverflow {
                    context: "summing successful terminal counts",
                })?;
            successful_terminal_moments
                .add(run.terminal.cumulative_primitive_recruitments as f64)?;
            first_success_moments.add(first as f64)?;
        }
        if let Some(first) = run.first_success {
            let entry = first_success_counts.entry(first).or_default();
            *entry = entry
                .checked_add(1)
                .ok_or(EngineError::ArithmeticOverflow {
                    context: "counting first-success samples",
                })?;
            total_first_success_count = total_first_success_count
                .checked_add(u128::from(first))
                .ok_or(EngineError::ArithmeticOverflow {
                    context: "summing first-success counts",
                })?;
        }
        for (count, reached) in &mut milestone_counts {
            if *count <= run.terminal.cumulative_primitive_recruitments {
                *reached = reached
                    .checked_add(1)
                    .ok_or(EngineError::ArithmeticOverflow {
                        context: "counting milestone reach samples",
                    })?;
            }
        }
        total_terminal_count = total_terminal_count
            .checked_add(u128::from(run.terminal.cumulative_primitive_recruitments))
            .ok_or(EngineError::ArithmeticOverflow {
                context: "summing terminal recruitment samples",
            })?;
        total_paid_spend = total_paid_spend
            .checked_add(u128::from(funding.paid_pyroxene_spent))
            .ok_or(EngineError::ArithmeticOverflow {
                context: "summing paid-spend samples",
            })?;
        total_ticket_draws = total_ticket_draws
            .checked_add(u128::from(funding.ticket_funded_primitive_recruitments))
            .ok_or(EngineError::ArithmeticOverflow {
                context: "summing ticket-funded samples",
            })?;
        residual_sums.add(residual)?;
        reward_sums.add(rewards)?;
        residual_moments.add(residual)?;
        reward_moments.add(rewards)?;
        terminal_moments.add(run.terminal.cumulative_primitive_recruitments as f64)?;
        paid_moments.add(funding.paid_pyroxene_spent as f64)?;
        ticket_moments.add(funding.ticket_funded_primitive_recruitments as f64)?;
    }

    let divisor = run_count as f64;
    let success_probability = successes as f64 / divisor;
    let owned_target_terminal_probabilities = owned_counts
        .iter()
        .map(|(mask, count)| OwnedTargetTerminalProbability {
            owned_targets: owned_targets(bundle, *mask),
            probability: *count as f64 / divisor,
        })
        .collect::<Vec<_>>();
    let owned_probability_intervals = owned_counts
        .iter()
        .map(|(mask, count)| OwnedTargetProbabilityInterval {
            owned_targets: owned_targets(bundle, *mask),
            sample_count: *count,
            confidence_interval_95: wilson_interval(*count, run_count),
        })
        .collect::<Vec<_>>();
    let terminal_reason_probabilities = terminal_reason_counts
        .iter()
        .map(|(terminal_reason, count)| TerminalReasonProbability {
            terminal_reason: *terminal_reason,
            probability: *count as f64 / divisor,
        })
        .collect::<Vec<_>>();
    let terminal_reason_probability_intervals = terminal_reason_counts
        .iter()
        .map(
            |(terminal_reason, count)| TerminalReasonProbabilityInterval {
                terminal_reason: *terminal_reason,
                sample_count: *count,
                confidence_interval_95: wilson_interval(*count, run_count),
            },
        )
        .collect::<Vec<_>>();
    let milestone_reach_probabilities = milestone_counts
        .iter()
        .map(|(recruitment_count, count)| MilestoneReachProbability {
            recruitment_count: *recruitment_count,
            probability: *count as f64 / divisor,
        })
        .collect::<Vec<_>>();
    let milestone_probability_intervals = milestone_counts
        .iter()
        .map(
            |(recruitment_count, count)| RecruitmentCountProbabilityInterval {
                recruitment_count: *recruitment_count,
                sample_count: *count,
                confidence_interval_95: wilson_interval(*count, run_count),
            },
        )
        .collect::<Vec<_>>();
    let first_success_pmf = first_success_counts
        .iter()
        .map(|(count, samples)| FirstSuccessProbability {
            recruitment_count: *count,
            probability: *samples as f64 / divisor,
        })
        .collect::<Vec<_>>();
    let first_success_pmf_intervals = first_success_counts
        .iter()
        .map(
            |(recruitment_count, count)| RecruitmentCountProbabilityInterval {
                recruitment_count: *recruitment_count,
                sample_count: *count,
                confidence_interval_95: wilson_interval(*count, run_count),
            },
        )
        .collect::<Vec<_>>();
    let mut running = 0.0;
    let mut running_count = 0_u64;
    let first_success_cdf = first_success_counts
        .iter()
        .map(|(recruitment_count, count)| {
            running += *count as f64 / divisor;
            FirstSuccessProbability {
                recruitment_count: *recruitment_count,
                probability: running,
            }
        })
        .collect::<Vec<_>>();
    let first_success_cdf_intervals = first_success_counts
        .iter()
        .map(|(recruitment_count, count)| -> Result<_, EngineError> {
            running_count =
                running_count
                    .checked_add(*count)
                    .ok_or(EngineError::ArithmeticOverflow {
                        context: "summing first-success CDF samples",
                    })?;
            Ok(RecruitmentCountProbabilityInterval {
                recruitment_count: *recruitment_count,
                sample_count: running_count,
                confidence_interval_95: wilson_interval(running_count, run_count),
            })
        })
        .collect::<Result<Vec<_>, _>>()?;

    Ok(MonteCarloAnalysisResult {
        engine_kind: "monte_carlo",
        provenance: AnalysisProvenance::from_bundle(bundle),
        context: AnalysisContext::from_bundle(bundle),
        rng: rng_provenance(master_seed, run_count),
        success_probability,
        owned_target_terminal_probabilities,
        terminal_reason_probabilities,
        expected_terminal_primitive_recruitments: total_terminal_count as f64 / divisor,
        expected_terminal_primitive_recruitments_given_success: (successes > 0)
            .then_some(successful_terminal_count as f64 / successes as f64),
        expected_first_success_recruitment_count_given_success: (successes > 0)
            .then_some(total_first_success_count as f64 / successes as f64),
        expected_paid_pyroxene_spent: total_paid_spend as f64 / divisor,
        expected_ticket_funded_primitive_recruitments: total_ticket_draws as f64 / divisor,
        expected_residual_resources: residual_sums.expectation(run_count),
        expected_milestone_rewards_acquired: reward_sums.expectation(run_count),
        milestone_reach_probabilities,
        first_success_pmf,
        first_success_cdf,
        sample_counts: MonteCarloSampleCounts {
            total_runs: run_count,
            successful_runs: successes,
        },
        estimation: MonteCarloEstimationMetadata {
            success_probability_interval_95: wilson_interval(successes, run_count),
            expected_terminal_primitive_recruitments: terminal_moments.diagnostics(),
            expected_terminal_primitive_recruitments_given_success: (successes > 0)
                .then(|| successful_terminal_moments.diagnostics()),
            expected_first_success_recruitment_count_given_success: (successes > 0)
                .then(|| first_success_moments.diagnostics()),
            expected_paid_pyroxene_spent: paid_moments.diagnostics(),
            expected_ticket_funded_primitive_recruitments: ticket_moments.diagnostics(),
            expected_residual_resources: residual_moments.diagnostics(),
            expected_milestone_rewards_acquired: reward_moments.diagnostics(),
            probability_intervals_95: MonteCarloProbabilityIntervals {
                owned_target_terminal_probabilities: owned_probability_intervals,
                terminal_reason_probabilities: terminal_reason_probability_intervals,
                milestone_reach_probabilities: milestone_probability_intervals,
                first_success_pmf: first_success_pmf_intervals,
                first_success_cdf: first_success_cdf_intervals,
            },
        },
    })
}

pub fn simulate_trace(
    bundle: &ValidatedScenarioBundle,
    master_seed: u64,
) -> Result<RunTraceResult, EngineError> {
    simulate_trace_with_limits(bundle, master_seed, SimulationLimits::default())
}

pub fn simulate_trace_with_limits(
    bundle: &ValidatedScenarioBundle,
    master_seed: u64,
    limits: SimulationLimits,
) -> Result<RunTraceResult, EngineError> {
    let limits = limits.validate()?;
    let mut rng = ChaCha8Rng::from_seed(derive_run_seed(bundle, master_seed, 0));
    let run = execute_run(
        bundle,
        true,
        limits.max_trace_primitive_transitions,
        "trace",
        |branches| sample_outcome(branches, &mut rng),
    )?;
    concrete_result(bundle, run, Some(rng_provenance(master_seed, 1)))
}

pub fn replay(
    bundle: &ValidatedScenarioBundle,
    outcomes: &[RecruitOutcome],
) -> Result<RunTraceResult, EngineError> {
    replay_with_limits(bundle, outcomes, SimulationLimits::default())
}

pub fn replay_with_limits(
    bundle: &ValidatedScenarioBundle,
    outcomes: &[RecruitOutcome],
    limits: SimulationLimits,
) -> Result<RunTraceResult, EngineError> {
    let limits = limits.validate()?;
    let mut cursor = 0_usize;
    let run =
        execute_run(
            bundle,
            true,
            limits.max_trace_primitive_transitions,
            "replay",
            |branches| {
                let outcome = outcomes.get(cursor).copied().ok_or_else(|| {
                    EngineError::InvalidTransition {
                        message: format!("replay outcome stream ended at primitive draw {cursor}"),
                    }
                })?;
                cursor = cursor
                    .checked_add(1)
                    .ok_or(EngineError::ArithmeticOverflow {
                        context: "advancing replay outcome cursor",
                    })?;
                if branches.iter().any(|branch| branch.outcome == outcome) {
                    Ok(outcome)
                } else {
                    Err(EngineError::InvalidTransition {
                        message: format!(
                            "replay outcome {outcome:?} is impossible at primitive draw {}",
                            cursor - 1
                        ),
                    })
                }
            },
        )?;
    if cursor != outcomes.len() {
        return Err(EngineError::InvalidTransition {
            message: format!(
                "replay supplied {} unused outcomes after terminal state",
                outcomes.len() - cursor
            ),
        });
    }
    concrete_result(bundle, run, None)
}

pub fn compare(
    bundle: &ValidatedScenarioBundle,
    runs: NonZeroU64,
    master_seed: u64,
) -> Result<ComparisonResult, EngineError> {
    let exact = analyze_exact(bundle, ExactSolverOptions::default())?;
    let monte_carlo = simulate_monte_carlo(bundle, runs, master_seed)?;
    let interval = monte_carlo.estimation.success_probability_interval_95;
    let difference = monte_carlo.success_probability - exact.success_probability;
    Ok(ComparisonResult {
        engine_kind: "comparison",
        success_probability_within_monte_carlo_interval: (interval.lower..=interval.upper)
            .contains(&exact.success_probability),
        success_probability_difference: difference,
        exact,
        monte_carlo,
    })
}

fn execute_run<F>(
    bundle: &ValidatedScenarioBundle,
    trace: bool,
    primitive_limit: u64,
    limit_scope: &'static str,
    mut choose_outcome: F,
) -> Result<ConcreteRun, EngineError>
where
    F: FnMut(&[ba_core::OutcomeBranch]) -> Result<RecruitOutcome, EngineError>,
{
    let mut world = initial_world(bundle);
    let mut first_success =
        (world.owned_target_mask == bundle.scenario().all_targets_mask()).then_some(0);
    let mut outcomes = Vec::new();
    let mut events = Vec::new();
    if first_success == Some(0) && trace {
        events.push(RunTraceEvent::FirstSuccess {
            recruitment_count: 0,
        });
    }

    let terminal_reason = loop {
        match decide(bundle, &world)? {
            StrategyDecision::Stop(reason) => break reason,
            StrategyDecision::Act(action) => {
                let (mut in_flight, started) = begin_action(bundle, &world, &action)?;
                if trace {
                    events.push(RunTraceEvent::ActionStarted(started));
                }
                while in_flight.remaining_primitive_draws > 0 {
                    let observed = in_flight
                        .world
                        .cumulative_primitive_recruitments
                        .checked_add(1)
                        .ok_or(EngineError::ArithmeticOverflow {
                            context: "counting concrete primitive transitions",
                        })?;
                    if observed > primitive_limit {
                        return Err(EngineError::SimulationPrimitiveLimitExceeded {
                            scope: limit_scope,
                            observed,
                            maximum: primitive_limit,
                        });
                    }
                    let branches = outcome_distribution(bundle, &in_flight)?;
                    let outcome = choose_outcome(&branches)?;
                    if trace {
                        outcomes.push(outcome);
                    }
                    let transitioned = apply_primitive_transition(bundle, &in_flight, outcome)?;
                    if trace {
                        events.push(RunTraceEvent::PrimitiveTransition(
                            transitioned.event.clone(),
                        ));
                        if !transitioned.event.rewards.is_empty() {
                            events.push(RunTraceEvent::RewardGranted {
                                recruitment_count: transitioned.event.recruitment_count,
                                rewards: transitioned.event.rewards.clone(),
                            });
                        }
                    }
                    if transitioned.event.first_success {
                        first_success = Some(transitioned.event.recruitment_count);
                        if trace {
                            events.push(RunTraceEvent::FirstSuccess {
                                recruitment_count: transitioned.event.recruitment_count,
                            });
                        }
                    }
                    in_flight = transitioned.state;
                    if in_flight.world.cumulative_primitive_recruitments
                        > bundle.scenario().termination_bound()
                    {
                        return Err(EngineError::InternalInvariantViolation {
                            message: "concrete run exceeded the validated finite termination bound"
                                .to_owned(),
                        });
                    }
                }
                let (next_world, completed) = complete_action(in_flight)?;
                world = next_world;
                if trace {
                    events.push(RunTraceEvent::ActionCompleted(completed));
                }
            }
        }
    };
    if trace {
        events.push(RunTraceEvent::Terminal { terminal_reason });
    }
    Ok(ConcreteRun {
        terminal: world,
        first_success,
        terminal_reason,
        outcomes,
        events,
    })
}

fn concrete_result(
    bundle: &ValidatedScenarioBundle,
    run: ConcreteRun,
    rng: Option<RngProvenance>,
) -> Result<RunTraceResult, EngineError> {
    let funding = reconstruct_funding(bundle, &run.terminal)?;
    let terminal_resources = terminal_resources(bundle, &run.terminal)?;
    let milestone_rewards_acquired =
        milestone_rewards_acquired(bundle, run.terminal.cumulative_primitive_recruitments)?;
    let terminal_owned_targets = owned_targets(bundle, run.terminal.owned_target_mask);
    Ok(RunTraceResult {
        engine_kind: "trace",
        provenance: AnalysisProvenance::from_bundle(bundle),
        context: AnalysisContext::from_bundle(bundle),
        rng,
        terminal_primitive_recruitments: run.terminal.cumulative_primitive_recruitments,
        first_success_recruitment_count: run.first_success,
        paid_pyroxene_spent: funding.paid_pyroxene_spent,
        ticket_funded_primitive_recruitments: funding.ticket_funded_primitive_recruitments,
        terminal_resources,
        milestone_rewards_acquired,
        terminal_owned_targets,
        terminal_reason: run.terminal_reason,
        replay_outcomes: run.outcomes,
        events: run.events,
    })
}

fn sample_outcome(
    branches: &[ba_core::OutcomeBranch],
    rng: &mut impl RngCore,
) -> Result<RecruitOutcome, EngineError> {
    match branches {
        [only] => Ok(only.outcome),
        [_, _] => {
            let pickup = branches
                .iter()
                .find(|branch| branch.outcome == RecruitOutcome::Pickup)
                .ok_or_else(|| EngineError::InternalInvariantViolation {
                    message: "two-branch distribution has no pickup branch".to_owned(),
                })?
                .probability;
            let sampled = uniform_below(rng, pickup.denominator())?;
            Ok(if sampled < pickup.numerator() {
                RecruitOutcome::Pickup
            } else {
                RecruitOutcome::Miss
            })
        }
        _ => Err(EngineError::InternalInvariantViolation {
            message: "kernel returned an unsupported branch count".to_owned(),
        }),
    }
}

fn uniform_below(rng: &mut impl RngCore, bound: u64) -> Result<u64, EngineError> {
    if bound == 0 {
        return Err(EngineError::InternalInvariantViolation {
            message: "cannot sample with a zero bound".to_owned(),
        });
    }
    if bound == 1 {
        return Ok(0);
    }
    let threshold = bound.wrapping_neg() % bound;
    loop {
        let value = rng.next_u64();
        if value >= threshold {
            return Ok(value % bound);
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

fn rng_provenance(master_seed: u64, run_count: u64) -> RngProvenance {
    RngProvenance {
        rng_algorithm: RNG_ALGORITHM,
        master_seed,
        run_count,
        run_index_contract: "zero-based ascending indices 0..runs-1",
        stream_derivation_version: STREAM_DERIVATION_VERSION,
    }
}

fn owned_targets(bundle: &ValidatedScenarioBundle, mask: u8) -> Vec<ba_core::StudentId> {
    bundle
        .scenario()
        .targets()
        .iter()
        .enumerate()
        .filter(|(index, _)| {
            u32::try_from(*index)
                .ok()
                .and_then(|shift| 1_u8.checked_shl(shift))
                .is_some_and(|bit| mask & bit != 0)
        })
        .map(|(_, target)| target.student_id.clone())
        .collect()
}

fn checked_u128_add(current: u128, value: u64) -> Result<u128, EngineError> {
    current
        .checked_add(u128::from(value))
        .ok_or(EngineError::ArithmeticOverflow {
            context: "accumulating Monte Carlo integer resource totals",
        })
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;

    use ba_core::{OutcomeBranch, ProbabilityRatio, RecruitOutcome};
    use rand_core::RngCore;

    use super::{sample_outcome, uniform_below};

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
    fn deterministic_distributions_do_not_consume_rng() {
        let mut rng = CountingRng {
            values: VecDeque::new(),
            calls: 0,
        };
        let outcome = sample_outcome(
            &[OutcomeBranch {
                outcome: RecruitOutcome::Pickup,
                probability: ProbabilityRatio::new(1, 1).expect("certain"),
            }],
            &mut rng,
        )
        .expect("deterministic outcome");
        assert_eq!(outcome, RecruitOutcome::Pickup);
        assert_eq!(rng.calls, 0);
    }

    #[test]
    fn bounded_u64_sampling_rejects_the_biased_tail() {
        let mut rng = CountingRng {
            values: VecDeque::from([5, 16]),
            calls: 0,
        };
        assert_eq!(uniform_below(&mut rng, 10).expect("sample"), 6);
        assert_eq!(rng.calls, 2);
    }
}
