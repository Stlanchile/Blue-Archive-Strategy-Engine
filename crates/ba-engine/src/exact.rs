use std::collections::BTreeMap;

use ba_core::{
    Milestone, StrategyDecision, TerminalReason, ValidatedScenarioBundle, WorldStateKey,
    apply_primitive_transition, begin_action, complete_action, decide, initial_world,
    milestone_rewards_acquired, outcome_distribution, reconstruct_funding, terminal_resources,
};

use crate::error::{EngineError, ExactAnalysisFailure};
use crate::options::ExactSolverOptions;
use crate::result::{
    AnalysisContext, AnalysisProvenance, ExactAnalysisResult, ExpectedResources,
    FirstSuccessProbability, MilestoneReachProbability, OwnedTargetTerminalProbability,
    ProbabilityConservationDiagnostics, SolverDiagnostics, TerminalReasonProbability,
};

#[derive(Debug, Clone, Copy, Default)]
struct CompensatedMass {
    sum: f64,
    correction: f64,
}

impl CompensatedMass {
    fn add(&mut self, value: f64) -> Result<(), EngineError> {
        if !value.is_finite() || value < 0.0 {
            return Err(EngineError::ProbabilityInvariantViolation {
                message: format!("attempted to accumulate invalid mass {value}"),
            });
        }
        let combined = self.sum + value;
        if self.sum.abs() >= value.abs() {
            self.correction += (self.sum - combined) + value;
        } else {
            self.correction += (value - combined) + self.sum;
        }
        self.sum = combined;
        if !self.value().is_finite() || self.value() < 0.0 {
            return Err(EngineError::ProbabilityInvariantViolation {
                message: "compensated probability accumulation became invalid".to_owned(),
            });
        }
        Ok(())
    }

    fn value(self) -> f64 {
        self.sum + self.correction
    }
}

#[derive(Debug, Default)]
struct RuntimeDiagnostics {
    peak_boundary_frontier: usize,
    peak_in_flight_frontier: usize,
    processed_states: u64,
    transition_expansions: u64,
    maximum_deviation: f64,
}

pub fn analyze_exact(
    bundle: &ValidatedScenarioBundle,
    options: ExactSolverOptions,
) -> Result<ExactAnalysisResult, EngineError> {
    let options = options.validate()?;
    let initial = initial_world(bundle);
    let mut boundary = BTreeMap::new();
    add_mass(&mut boundary, initial.clone(), 1.0)?;
    let mut terminal: BTreeMap<(WorldStateKey, TerminalReason), CompensatedMass> = BTreeMap::new();
    let mut first_success: BTreeMap<u64, CompensatedMass> = BTreeMap::new();
    let mut diagnostics = RuntimeDiagnostics::default();

    if initial.owned_target_mask == bundle.scenario().all_targets_mask() {
        boundary.clear();
        add_mass(
            &mut terminal,
            (initial, TerminalReason::TargetsAcquired),
            1.0,
        )?;
        add_mass(&mut first_success, 0, 1.0)?;
    }

    while !boundary.is_empty() {
        diagnostics.peak_boundary_frontier = diagnostics.peak_boundary_frontier.max(boundary.len());
        check_active_limit(boundary.len(), &options)?;
        let mut in_flight = BTreeMap::new();
        for (state, mass) in std::mem::take(&mut boundary) {
            increment_processed(&mut diagnostics, &options)?;
            match decide(bundle, &state)? {
                StrategyDecision::Stop(reason) => {
                    add_mass(&mut terminal, (state, reason), mass.value())?;
                }
                StrategyDecision::Act(action) => {
                    let (started, _) = begin_action(bundle, &state, &action)?;
                    add_mass(&mut in_flight, started, mass.value())?;
                }
            }
        }
        observe_conservation(
            &terminal,
            &in_flight,
            &BTreeMap::<WorldStateKey, CompensatedMass>::new(),
            &mut diagnostics,
            &options,
        )?;

        let mut completed_boundary = BTreeMap::new();
        while !in_flight.is_empty() {
            diagnostics.peak_in_flight_frontier =
                diagnostics.peak_in_flight_frontier.max(in_flight.len());
            check_active_limit(in_flight.len(), &options)?;
            let mut next_in_flight = BTreeMap::new();
            for (state, mass) in std::mem::take(&mut in_flight) {
                increment_processed(&mut diagnostics, &options)?;
                let branches = outcome_distribution(bundle, &state)?;
                validate_local_distribution(&branches, options.conservation_tolerance)?;
                for branch in branches {
                    diagnostics.transition_expansions = diagnostics
                        .transition_expansions
                        .checked_add(1)
                        .ok_or(EngineError::ArithmeticOverflow {
                            context: "counting exact transition expansions",
                        })?;
                    if diagnostics.transition_expansions > options.max_transition_expansions {
                        return Err(EngineError::SolverTransitionLimitExceeded {
                            observed: diagnostics.transition_expansions,
                            maximum: options.max_transition_expansions,
                        });
                    }
                    let child_mass = mass.value() * branch.probability.as_f64();
                    if child_mass == 0.0 && mass.value() > 0.0 {
                        return Err(EngineError::ProbabilityInvariantViolation {
                            message: "a nonzero exact branch underflowed to zero".to_owned(),
                        });
                    }
                    let transitioned = apply_primitive_transition(bundle, &state, branch.outcome)?;
                    if transitioned.event.first_success {
                        add_mass(
                            &mut first_success,
                            transitioned.event.recruitment_count,
                            child_mass,
                        )?;
                    }
                    if transitioned.state.remaining_primitive_draws == 0 {
                        let (world, _) = complete_action(transitioned.state)?;
                        add_mass(&mut completed_boundary, world, child_mass)?;
                    } else {
                        add_mass(&mut next_in_flight, transitioned.state, child_mass)?;
                    }
                }
            }
            check_active_limit(next_in_flight.len(), &options)?;
            check_active_limit(completed_boundary.len(), &options)?;
            observe_conservation(
                &terminal,
                &next_in_flight,
                &completed_boundary,
                &mut diagnostics,
                &options,
            )?;
            in_flight = next_in_flight;
        }
        boundary = completed_boundary;
        observe_conservation(
            &terminal,
            &BTreeMap::<ba_core::InFlightStateKey, CompensatedMass>::new(),
            &boundary,
            &mut diagnostics,
            &options,
        )?;
    }

    let final_mass = sum_values(terminal.values().copied());
    validate_total(
        final_mass,
        &mut diagnostics,
        &options,
        "final terminal fold",
    )?;
    build_result(bundle, options, terminal, first_success, diagnostics)
}

pub fn analyze_exact_detailed(
    bundle: &ValidatedScenarioBundle,
    options: ExactSolverOptions,
) -> Result<ExactAnalysisResult, ExactAnalysisFailure> {
    analyze_exact(bundle, options).map_err(|error| ExactAnalysisFailure {
        error,
        effective_options: options,
        provenance: Box::new(AnalysisProvenance::from_bundle(bundle)),
    })
}

fn build_result(
    bundle: &ValidatedScenarioBundle,
    options: ExactSolverOptions,
    terminal: BTreeMap<(WorldStateKey, TerminalReason), CompensatedMass>,
    first_success: BTreeMap<u64, CompensatedMass>,
    diagnostics: RuntimeDiagnostics,
) -> Result<ExactAnalysisResult, EngineError> {
    let final_terminal_probability = sum_values(terminal.values().copied());
    let mut success_probability = CompensatedMass::default();
    let mut terminal_reason_masses = BTreeMap::<TerminalReason, CompensatedMass>::new();
    let mut owned_masses = BTreeMap::<u8, CompensatedMass>::new();
    let mut expected_terminal = 0.0;
    let mut expected_terminal_success = 0.0;
    let mut expected_paid_spend = 0.0;
    let mut expected_ticket_draws = 0.0;
    let mut expected_residual = ExpectedResources::default();
    let mut expected_rewards = ExpectedResources::default();
    let mut terminal_count_masses = BTreeMap::<u64, CompensatedMass>::new();

    for ((state, reason), mass) in &terminal {
        let weight = mass.value();
        add_mass(
            &mut terminal_count_masses,
            state.cumulative_primitive_recruitments,
            weight,
        )?;
        terminal_reason_masses
            .entry(*reason)
            .or_default()
            .add(weight)?;
        owned_masses
            .entry(state.owned_target_mask)
            .or_default()
            .add(weight)?;
        if *reason == TerminalReason::TargetsAcquired {
            success_probability.add(weight)?;
            expected_terminal_success += state.cumulative_primitive_recruitments as f64 * weight;
        }
        expected_terminal += state.cumulative_primitive_recruitments as f64 * weight;
        let funding = reconstruct_funding(bundle, state)?;
        expected_paid_spend += funding.paid_pyroxene_spent as f64 * weight;
        expected_ticket_draws += funding.ticket_funded_primitive_recruitments as f64 * weight;
        expected_residual.add_weighted(terminal_resources(bundle, state)?, weight);
        expected_rewards.add_weighted(
            milestone_rewards_acquired(bundle, state.cumulative_primitive_recruitments)?,
            weight,
        );
    }

    let success = success_probability.value();
    let mut pmf = Vec::with_capacity(first_success.len());
    let mut cdf = Vec::with_capacity(first_success.len());
    let mut running = CompensatedMass::default();
    let mut weighted_first_success = 0.0;
    for (count, mass) in first_success {
        let probability = mass.value();
        running.add(probability)?;
        weighted_first_success += count as f64 * probability;
        pmf.push(FirstSuccessProbability {
            recruitment_count: count,
            probability,
        });
        cdf.push(FirstSuccessProbability {
            recruitment_count: count,
            probability: running.value(),
        });
    }
    let pmf_total = running.value();
    let pmf_deviation = (pmf_total - success).abs();
    if pmf_deviation > options.conservation_tolerance {
        return Err(EngineError::ProbabilityInvariantViolation {
            message: format!(
                "first-success PMF total {pmf_total} differs from success mass {success}"
            ),
        });
    }

    let owned_target_terminal_probabilities = owned_masses
        .into_iter()
        .map(|(mask, mass)| OwnedTargetTerminalProbability {
            owned_targets: owned_targets(bundle, mask),
            probability: mass.value(),
        })
        .collect();
    let terminal_reason_probabilities = terminal_reason_masses
        .into_iter()
        .map(|(terminal_reason, mass)| TerminalReasonProbability {
            terminal_reason,
            probability: mass.value(),
        })
        .collect();
    let milestone_reach_probabilities = milestone_reach_probabilities(
        bundle.reward_schedule().milestones(),
        &terminal_count_masses,
    )?;

    Ok(ExactAnalysisResult {
        engine_kind: "exact",
        provenance: AnalysisProvenance::from_bundle(bundle),
        context: AnalysisContext::from_bundle(bundle),
        exact_options: options,
        success_probability: success,
        owned_target_terminal_probabilities,
        terminal_reason_probabilities,
        expected_terminal_primitive_recruitments: expected_terminal,
        expected_terminal_primitive_recruitments_given_success: (success > 0.0)
            .then_some(expected_terminal_success / success),
        expected_first_success_recruitment_count_given_success: (success > 0.0)
            .then_some(weighted_first_success / success),
        expected_paid_pyroxene_spent: expected_paid_spend,
        expected_ticket_funded_primitive_recruitments: expected_ticket_draws,
        expected_residual_resources: expected_residual,
        expected_milestone_rewards_acquired: expected_rewards,
        milestone_reach_probabilities,
        first_success_pmf: pmf,
        first_success_cdf: cdf,
        probability_conservation: ProbabilityConservationDiagnostics {
            maximum_observed_deviation: diagnostics.maximum_deviation,
            final_terminal_probability,
            first_success_probability: pmf_total,
            first_success_success_deviation: pmf_deviation,
        },
        solver_diagnostics: SolverDiagnostics {
            peak_boundary_frontier: diagnostics.peak_boundary_frontier,
            peak_in_flight_frontier: diagnostics.peak_in_flight_frontier,
            processed_states: diagnostics.processed_states,
            transition_expansions: diagnostics.transition_expansions,
        },
    })
}

fn milestone_reach_probabilities(
    milestones: &[Milestone],
    terminal_count_masses: &BTreeMap<u64, CompensatedMass>,
) -> Result<Vec<MilestoneReachProbability>, EngineError> {
    let mut terminal_counts = terminal_count_masses.iter().rev().peekable();
    let mut running_mass = CompensatedMass::default();
    let mut reversed = Vec::with_capacity(milestones.len());

    for milestone in milestones.iter().rev() {
        while terminal_counts
            .peek()
            .is_some_and(|(count, _)| **count >= milestone.count)
        {
            if let Some((_, mass)) = terminal_counts.next() {
                running_mass.add(mass.value())?;
            }
        }
        reversed.push(MilestoneReachProbability {
            recruitment_count: milestone.count,
            probability: running_mass.value(),
        });
    }
    reversed.reverse();
    Ok(reversed)
}

fn validate_local_distribution(
    branches: &[ba_core::OutcomeBranch],
    tolerance: f64,
) -> Result<(), EngineError> {
    let total = branches
        .iter()
        .map(|branch| branch.probability.as_f64())
        .sum::<f64>();
    if !total.is_finite() || (total - 1.0).abs() > tolerance {
        Err(EngineError::ProbabilityInvariantViolation {
            message: format!("local outcome probabilities sum to {total}"),
        })
    } else {
        Ok(())
    }
}

fn increment_processed(
    diagnostics: &mut RuntimeDiagnostics,
    options: &ExactSolverOptions,
) -> Result<(), EngineError> {
    diagnostics.processed_states =
        diagnostics
            .processed_states
            .checked_add(1)
            .ok_or(EngineError::ArithmeticOverflow {
                context: "counting processed exact states",
            })?;
    if diagnostics.processed_states > options.max_processed_states {
        return Err(EngineError::SolverProcessedStateLimitExceeded {
            observed: diagnostics.processed_states,
            maximum: options.max_processed_states,
        });
    }
    Ok(())
}

fn check_active_limit(observed: usize, options: &ExactSolverOptions) -> Result<(), EngineError> {
    if observed > options.max_active_states {
        Err(EngineError::SolverStateLimitExceeded {
            observed,
            maximum: options.max_active_states,
        })
    } else {
        Ok(())
    }
}

fn observe_conservation<I, K>(
    terminal: &BTreeMap<(WorldStateKey, TerminalReason), CompensatedMass>,
    in_flight: &BTreeMap<I, CompensatedMass>,
    boundary: &BTreeMap<K, CompensatedMass>,
    diagnostics: &mut RuntimeDiagnostics,
    options: &ExactSolverOptions,
) -> Result<(), EngineError> {
    let total = sum_values(
        terminal
            .values()
            .chain(in_flight.values())
            .chain(boundary.values())
            .copied(),
    );
    validate_total(total, diagnostics, options, "exact propagation layer")
}

fn validate_total(
    total: f64,
    diagnostics: &mut RuntimeDiagnostics,
    options: &ExactSolverOptions,
    context: &str,
) -> Result<(), EngineError> {
    if !total.is_finite() || total < 0.0 {
        return Err(EngineError::ProbabilityInvariantViolation {
            message: format!("{context} has invalid total mass {total}"),
        });
    }
    let deviation = (total - 1.0).abs();
    diagnostics.maximum_deviation = diagnostics.maximum_deviation.max(deviation);
    if deviation > options.conservation_tolerance {
        return Err(EngineError::ProbabilityInvariantViolation {
            message: format!("{context} total mass is {total}, deviation {deviation}"),
        });
    }
    Ok(())
}

fn sum_values(values: impl IntoIterator<Item = CompensatedMass>) -> f64 {
    let mut total = CompensatedMass::default();
    for value in values {
        if total.add(value.value()).is_err() {
            return f64::NAN;
        }
    }
    total.value()
}

fn add_mass<K: Ord>(
    map: &mut BTreeMap<K, CompensatedMass>,
    key: K,
    mass: f64,
) -> Result<(), EngineError> {
    map.entry(key).or_default().add(mass)
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
