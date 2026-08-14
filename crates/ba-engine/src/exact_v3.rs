use std::collections::BTreeMap;

use ba_core::{
    InFlightStateKey, StrategyDecision, TerminalReason, ValidatedScenarioBundleV3, WorldStateKey,
    apply_primitive_transition_v3, begin_action_v3, complete_action_v3, decide_v3,
    initial_world_v3, milestone_rewards_acquired_v3, outcome_distribution_v3,
    reconstruct_funding_v3, terminal_resources_v3,
};

use crate::error::EngineError;
use crate::options::ExactSolverOptions;
use crate::result::SolverDiagnostics;
use crate::result_v3::{
    AbsoluteMilestoneReachProbabilityV3, AnalysisContextV3, AnalysisProvenanceV3,
    ExactAnalysisResultV3, FirstCompletionProbabilityV3, PrefixCompletionProbabilityV3,
    ProbabilityConservationDiagnosticsV3, TargetAcquisitionProbabilityV3,
    TerminalOwnedSetProbabilityV3, TerminalReasonProbabilityV3, expected_from_ledger_sums,
    ledger_values,
};

#[derive(Debug, Clone, Copy, Default)]
struct ScaledMass {
    sum: f64,
    correction: f64,
    exponent: i128,
}

impl ScaledMass {
    const fn one() -> Self {
        Self {
            sum: 0.5,
            correction: 0.0,
            exponent: 1,
        }
    }

    fn from_f64(value: f64) -> Result<Self, EngineError> {
        if !value.is_finite() || value < 0.0 {
            return Err(EngineError::ProbabilityInvariantViolation {
                message: format!("attempted to construct invalid v3 mass {value}"),
            });
        }
        if value == 0.0 {
            return Ok(Self::default());
        }
        const FRACTION_MASK: u64 = (1_u64 << 52) - 1;
        let bits = value.to_bits();
        let biased_exponent = (bits >> 52) & 0x7ff;
        let fraction = bits & FRACTION_MASK;
        let (sum, exponent) = if biased_exponent == 0 {
            let highest_bit = u64::BITS - 1 - fraction.leading_zeros();
            let denominator = 1_u64 << (highest_bit + 1);
            (
                fraction as f64 / denominator as f64,
                i128::from(highest_bit) + 1 - 1_074,
            )
        } else {
            let significand = (1_u64 << 52) | fraction;
            (
                significand as f64 / (1_u64 << 53) as f64,
                i128::from(biased_exponent) - 1_023 + 1,
            )
        };
        Ok(Self {
            sum,
            correction: 0.0,
            exponent,
        })
    }

    fn add(&mut self, other: Self) -> Result<(), EngineError> {
        if other.is_zero() {
            return Ok(());
        }
        if self.is_zero() {
            *self = other;
            return Ok(());
        }
        if other.exponent > self.exponent {
            let previous = *self;
            *self = other;
            return self.add(previous);
        }
        let exponent_delta =
            other
                .exponent
                .checked_sub(self.exponent)
                .ok_or(EngineError::ArithmeticOverflow {
                    context: "aligning v3 scaled probability exponents",
                })?;
        let scale = power_of_two(exponent_delta);
        if scale == 0.0 {
            return Ok(());
        }
        self.add_component(other.sum * scale)?;
        self.add_component(other.correction * scale)?;
        self.normalize()
    }

    fn multiplied_ratio(self, numerator: u64, denominator: u64) -> Result<Self, EngineError> {
        if numerator == 0 || denominator == 0 || numerator > denominator {
            return Err(EngineError::ProbabilityInvariantViolation {
                message: format!("invalid v3 branch ratio {numerator}/{denominator}"),
            });
        }
        let factor = numerator as f64 / denominator as f64;
        let factor = Self::from_f64(factor)?;
        let factor_significand = factor.sum + factor.correction;
        let exponent =
            self.exponent
                .checked_add(factor.exponent)
                .ok_or(EngineError::ArithmeticOverflow {
                    context: "multiplying v3 scaled probability exponents",
                })?;
        let mut product = Self {
            sum: self.sum * factor_significand,
            correction: self.correction * factor_significand,
            exponent,
        };
        product.normalize()?;
        Ok(product)
    }

    fn add_component(&mut self, value: f64) -> Result<(), EngineError> {
        if !value.is_finite() {
            return Err(EngineError::ProbabilityInvariantViolation {
                message: "v3 scaled probability accumulation became non-finite".to_owned(),
            });
        }
        let combined = self.sum + value;
        if self.sum.abs() >= value.abs() {
            self.correction += (self.sum - combined) + value;
        } else {
            self.correction += (value - combined) + self.sum;
        }
        self.sum = combined;
        Ok(())
    }

    fn normalize(&mut self) -> Result<(), EngineError> {
        let mut value = self.sum + self.correction;
        if !value.is_finite() || value <= 0.0 {
            return Err(EngineError::ProbabilityInvariantViolation {
                message: "v3 scaled probability normalization became invalid".to_owned(),
            });
        }
        while value >= 1.0 {
            self.sum *= 0.5;
            self.correction *= 0.5;
            self.exponent =
                self.exponent
                    .checked_add(1)
                    .ok_or(EngineError::ArithmeticOverflow {
                        context: "normalizing a v3 scaled probability exponent",
                    })?;
            value *= 0.5;
        }
        while value < 0.5 {
            self.sum *= 2.0;
            self.correction *= 2.0;
            self.exponent =
                self.exponent
                    .checked_sub(1)
                    .ok_or(EngineError::ArithmeticOverflow {
                        context: "normalizing a v3 scaled probability exponent",
                    })?;
            value *= 2.0;
        }
        Ok(())
    }

    fn to_f64(self) -> f64 {
        (self.sum + self.correction) * power_of_two(self.exponent)
    }

    const fn is_zero(self) -> bool {
        self.sum == 0.0
    }
}

fn power_of_two(exponent: i128) -> f64 {
    match i32::try_from(exponent) {
        Ok(exponent) => 2.0_f64.powi(exponent),
        Err(_) if exponent.is_negative() => 0.0,
        Err(_) => f64::INFINITY,
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

#[derive(Debug, Clone, Copy, Default)]
struct CompensatedSum {
    sum: f64,
    correction: f64,
}

impl CompensatedSum {
    fn add(&mut self, value: f64) {
        let combined = self.sum + value;
        if self.sum.abs() >= value.abs() {
            self.correction += (self.sum - combined) + value;
        } else {
            self.correction += (value - combined) + self.sum;
        }
        self.sum = combined;
    }

    fn value(self) -> f64 {
        self.sum + self.correction
    }
}

pub fn analyze_exact_v3(
    bundle: &ValidatedScenarioBundleV3,
    options: ExactSolverOptions,
) -> Result<ExactAnalysisResultV3, EngineError> {
    let options = options.validate()?;
    let initial = initial_world_v3(bundle);
    let mut boundary = BTreeMap::new();
    add_mass(&mut boundary, initial.clone(), ScaledMass::one())?;
    let mut terminal = BTreeMap::<(WorldStateKey, TerminalReason), ScaledMass>::new();
    let mut first_completion = BTreeMap::<u64, ScaledMass>::new();
    let mut diagnostics = RuntimeDiagnostics::default();

    if initial.owned_target_mask == bundle.scenario().all_targets_mask() {
        boundary.clear();
        add_mass(
            &mut terminal,
            (initial, TerminalReason::TargetsAcquired),
            ScaledMass::one(),
        )?;
        add_mass(&mut first_completion, 0, ScaledMass::one())?;
    }

    while !boundary.is_empty() {
        diagnostics.peak_boundary_frontier = diagnostics.peak_boundary_frontier.max(boundary.len());
        check_active_limit(boundary.len(), &options)?;
        let mut in_flight = BTreeMap::new();
        for (state, mass) in std::mem::take(&mut boundary) {
            increment_processed(&mut diagnostics, &options)?;
            match decide_v3(bundle, &state)? {
                StrategyDecision::Stop(reason) => {
                    add_mass(&mut terminal, (state, reason), mass)?;
                }
                StrategyDecision::Act(action) => {
                    let (started, _) = begin_action_v3(bundle, &state, &action)?;
                    add_mass(&mut in_flight, started, mass)?;
                }
            }
        }
        observe_conservation(
            &terminal,
            &in_flight,
            &BTreeMap::<WorldStateKey, ScaledMass>::new(),
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
                let distribution = outcome_distribution_v3(bundle, &state)?;
                validate_local_distribution(distribution)?;
                for branch in distribution.branches() {
                    diagnostics.transition_expansions = diagnostics
                        .transition_expansions
                        .checked_add(1)
                        .ok_or(EngineError::ArithmeticOverflow {
                            context: "counting v3 exact transition expansions",
                        })?;
                    if diagnostics.transition_expansions > options.max_transition_expansions {
                        return Err(EngineError::SolverTransitionLimitExceeded {
                            observed: diagnostics.transition_expansions,
                            maximum: options.max_transition_expansions,
                        });
                    }
                    let child_mass = mass.multiplied_ratio(
                        branch.canonical_weight,
                        distribution.denominator().get(),
                    )?;
                    let transitioned =
                        apply_primitive_transition_v3(bundle, &state, branch.acquisition)?;
                    if transitioned.event.first_all_targets_completed {
                        add_mass(
                            &mut first_completion,
                            transitioned.event.additional_recruitment_count,
                            child_mass,
                        )?;
                    }
                    if transitioned.state.remaining_primitive_draws == 0 {
                        let (world, _) = complete_action_v3(transitioned.state)?;
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
            &BTreeMap::<InFlightStateKey, ScaledMass>::new(),
            &boundary,
            &mut diagnostics,
            &options,
        )?;
    }

    let final_mass = sum_values(terminal.values().copied())?;
    validate_total(
        final_mass,
        &mut diagnostics,
        &options,
        "v3 final terminal fold",
    )?;
    build_result(bundle, options, terminal, first_completion, diagnostics)
}

fn build_result(
    bundle: &ValidatedScenarioBundleV3,
    options: ExactSolverOptions,
    terminal: BTreeMap<(WorldStateKey, TerminalReason), ScaledMass>,
    first_completion: BTreeMap<u64, ScaledMass>,
    diagnostics: RuntimeDiagnostics,
) -> Result<ExactAnalysisResultV3, EngineError> {
    let target_count = bundle.scenario().targets().len();
    let final_terminal_probability = sum_values(terminal.values().copied())?.to_f64();
    let mut success_mass = ScaledMass::default();
    let mut terminal_reason_masses = BTreeMap::<TerminalReason, ScaledMass>::new();
    let mut owned_masses = BTreeMap::<u8, ScaledMass>::new();
    let mut target_masses = vec![ScaledMass::default(); target_count];
    let mut prefix_masses = vec![ScaledMass::default(); target_count];
    let mut terminal_count_masses = BTreeMap::<u64, ScaledMass>::new();
    let mut expected_terminal = 0.0_f64;
    let mut expected_terminal_success = 0.0_f64;
    let mut expected_paid_spend = 0.0_f64;
    let mut expected_ticket_draws = 0.0_f64;
    let mut expected_residual = [CompensatedSum::default(); 11];
    let mut expected_rewards = [CompensatedSum::default(); 11];

    for ((state, reason), mass) in &terminal {
        let weight = mass.to_f64();
        add_mass(
            &mut terminal_count_masses,
            state.cumulative_primitive_recruitments,
            *mass,
        )?;
        terminal_reason_masses
            .entry(*reason)
            .or_default()
            .add(*mass)?;
        owned_masses
            .entry(state.owned_target_mask)
            .or_default()
            .add(*mass)?;
        for (index, target_mass) in target_masses.iter_mut().enumerate() {
            if state.owned_target_mask & (1_u8 << index) != 0 {
                target_mass.add(*mass)?;
            }
        }
        for (index, prefix_mass) in prefix_masses.iter_mut().enumerate() {
            let prefix_mask = (1_u8 << (index + 1)) - 1;
            if state.owned_target_mask & prefix_mask == prefix_mask {
                prefix_mass.add(*mass)?;
            }
        }
        if *reason == TerminalReason::TargetsAcquired {
            success_mass.add(*mass)?;
            expected_terminal_success += state.cumulative_primitive_recruitments as f64 * weight;
        }
        expected_terminal += state.cumulative_primitive_recruitments as f64 * weight;
        let funding = reconstruct_funding_v3(bundle, state)?;
        expected_paid_spend += funding.paid_pyroxene_spent as f64 * weight;
        expected_ticket_draws += funding.ticket_funded_primitive_recruitments as f64 * weight;
        for (index, value) in ledger_values(terminal_resources_v3(bundle, state)?)
            .into_iter()
            .enumerate()
        {
            expected_residual[index].add(value as f64 * weight);
        }
        for (index, value) in ledger_values(milestone_rewards_acquired_v3(
            bundle,
            state.cumulative_primitive_recruitments,
        )?)
        .into_iter()
        .enumerate()
        {
            expected_rewards[index].add(value as f64 * weight);
        }
    }

    let success = success_mass.to_f64();
    let mut pmf = Vec::with_capacity(first_completion.len());
    let mut cdf = Vec::with_capacity(first_completion.len());
    let mut running = ScaledMass::default();
    let mut weighted_first_completion = 0.0_f64;
    for (count, mass) in first_completion {
        let probability = mass.to_f64();
        running.add(mass)?;
        weighted_first_completion += count as f64 * probability;
        pmf.push(FirstCompletionProbabilityV3 {
            additional_recruitment_count: count,
            probability,
        });
        cdf.push(FirstCompletionProbabilityV3 {
            additional_recruitment_count: count,
            probability: running.to_f64(),
        });
    }
    let pmf_total = running.to_f64();
    let pmf_deviation = (pmf_total - success).abs();
    if pmf_deviation > options.conservation_tolerance {
        return Err(EngineError::ProbabilityInvariantViolation {
            message: format!(
                "v3 first-completion PMF total {pmf_total} differs from success mass {success}"
            ),
        });
    }

    let terminal_owned_set_probabilities = owned_masses
        .into_iter()
        .map(|(mask, mass)| TerminalOwnedSetProbabilityV3 {
            owned_targets: owned_targets(bundle, mask),
            probability: mass.to_f64(),
        })
        .collect();
    let terminal_reason_probabilities = terminal_reason_masses
        .into_iter()
        .map(|(terminal_reason, mass)| TerminalReasonProbabilityV3 {
            terminal_reason,
            probability: mass.to_f64(),
        })
        .collect();
    let per_target_acquisition_probabilities = bundle
        .scenario()
        .targets()
        .iter()
        .zip(target_masses)
        .map(|(target, mass)| TargetAcquisitionProbabilityV3 {
            target_id: target.student_id.clone(),
            probability: mass.to_f64(),
        })
        .collect::<Vec<_>>();
    let ordered_prefix_completion_probabilities = bundle
        .scenario()
        .targets()
        .iter()
        .enumerate()
        .zip(prefix_masses)
        .map(|((index, _), mass)| PrefixCompletionProbabilityV3 {
            prefix_length: index + 1,
            target_ids: bundle.scenario().targets()[..=index]
                .iter()
                .map(|target| target.student_id.clone())
                .collect(),
            probability: mass.to_f64(),
        })
        .collect::<Vec<_>>();
    validate_prefixes(
        &ordered_prefix_completion_probabilities,
        success,
        options.conservation_tolerance,
    )?;
    let absolute_campaign_milestone_reach_probabilities =
        milestone_reach_probabilities(bundle, &terminal_count_masses)?;
    let residual_values = expected_residual.map(CompensatedSum::value);
    let reward_values = expected_rewards.map(CompensatedSum::value);

    Ok(ExactAnalysisResultV3 {
        engine_kind: "exact",
        provenance: AnalysisProvenanceV3::from_bundle(bundle),
        context: AnalysisContextV3::from_bundle(bundle),
        exact_options: options,
        all_target_success_probability: success,
        terminal_owned_set_probabilities,
        terminal_reason_probabilities,
        per_target_acquisition_probabilities,
        ordered_prefix_completion_probabilities,
        expected_additional_primitive_recruitments: expected_terminal,
        expected_additional_primitive_recruitments_given_success: (success > 0.0)
            .then_some(expected_terminal_success / success),
        expected_first_all_target_completion_count_given_success: (success > 0.0)
            .then_some(weighted_first_completion / success),
        expected_paid_pyroxene_spent: expected_paid_spend,
        expected_ticket_funded_primitive_recruitments: expected_ticket_draws,
        expected_residual_resources: expected_from_ledger_sums(residual_values),
        expected_milestone_rewards_acquired: expected_from_ledger_sums(reward_values),
        absolute_campaign_milestone_reach_probabilities,
        first_all_target_completion_pmf: pmf,
        first_all_target_completion_cdf: cdf,
        probability_conservation: ProbabilityConservationDiagnosticsV3 {
            maximum_observed_deviation: diagnostics.maximum_deviation,
            final_terminal_probability,
            first_all_target_completion_probability: pmf_total,
            first_completion_success_deviation: pmf_deviation,
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
    bundle: &ValidatedScenarioBundleV3,
    terminal_count_masses: &BTreeMap<u64, ScaledMass>,
) -> Result<Vec<AbsoluteMilestoneReachProbabilityV3>, EngineError> {
    let mut terminal_counts = terminal_count_masses.iter().rev().peekable();
    let mut running_mass = ScaledMass::default();
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
            if let Some((_, mass)) = terminal_counts.next() {
                running_mass.add(*mass)?;
            }
        }
        reversed.push(AbsoluteMilestoneReachProbabilityV3 {
            absolute_campaign_recruitment_count: milestone.count,
            probability: running_mass.to_f64(),
        });
    }
    reversed.reverse();
    Ok(reversed)
}

fn validate_prefixes(
    prefixes: &[PrefixCompletionProbabilityV3],
    success: f64,
    tolerance: f64,
) -> Result<(), EngineError> {
    if prefixes
        .windows(2)
        .any(|window| window[1].probability - window[0].probability > tolerance)
    {
        return Err(EngineError::ProbabilityInvariantViolation {
            message: "v3 prefix completion probabilities are not monotone".to_owned(),
        });
    }
    if prefixes
        .last()
        .is_none_or(|last| (last.probability - success).abs() > tolerance)
    {
        return Err(EngineError::ProbabilityInvariantViolation {
            message: "final v3 prefix probability differs from all-target success".to_owned(),
        });
    }
    Ok(())
}

fn validate_local_distribution(
    distribution: &ba_core::CompiledOutcomeDistribution,
) -> Result<(), EngineError> {
    let total = distribution
        .branches()
        .iter()
        .try_fold(0_u64, |total, branch| {
            total.checked_add(branch.canonical_weight)
        })
        .ok_or(EngineError::ArithmeticOverflow {
            context: "summing canonical v3 branch weights",
        })?;
    if total == distribution.denominator().get()
        && distribution
            .branches()
            .last()
            .is_some_and(|branch| branch.upper_exclusive == total)
    {
        Ok(())
    } else {
        Err(EngineError::ProbabilityInvariantViolation {
            message: "canonical v3 outcome distribution does not conserve integer mass".to_owned(),
        })
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
                context: "counting processed v3 exact states",
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
    terminal: &BTreeMap<(WorldStateKey, TerminalReason), ScaledMass>,
    in_flight: &BTreeMap<I, ScaledMass>,
    boundary: &BTreeMap<K, ScaledMass>,
    diagnostics: &mut RuntimeDiagnostics,
    options: &ExactSolverOptions,
) -> Result<(), EngineError> {
    let total = sum_values(
        terminal
            .values()
            .chain(in_flight.values())
            .chain(boundary.values())
            .copied(),
    )?;
    validate_total(total, diagnostics, options, "v3 exact propagation layer")
}

fn validate_total(
    total: ScaledMass,
    diagnostics: &mut RuntimeDiagnostics,
    options: &ExactSolverOptions,
    context: &str,
) -> Result<(), EngineError> {
    let total = total.to_f64();
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

fn sum_values(values: impl IntoIterator<Item = ScaledMass>) -> Result<ScaledMass, EngineError> {
    let mut total = ScaledMass::default();
    for value in values {
        total.add(value)?;
    }
    Ok(total)
}

fn add_mass<K: Ord>(
    map: &mut BTreeMap<K, ScaledMass>,
    key: K,
    mass: ScaledMass,
) -> Result<(), EngineError> {
    map.entry(key).or_default().add(mass)
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
