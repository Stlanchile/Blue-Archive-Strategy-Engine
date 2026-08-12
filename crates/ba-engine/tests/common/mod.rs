#![allow(dead_code)]

use std::num::NonZeroU64;

use ba_core::schema::{
    NullablePositive, RawBanner, RawInitialCharge, RawMilestone, RawRewardScheduleV1,
    RawScenarioV1, RawStrategy, RawStrategyKind, RawStudent, RawTarget,
};
use ba_core::{
    CompiledRuleset, ResourceKind, Resources, RewardSchedule, RulesetId, RulesetMechanics,
    ValidatedScenarioBundle,
};

pub fn synthetic_bundle(
    name: &str,
    mechanics: RulesetMechanics,
    initial_resources: Resources,
    initial_charge: u64,
    horizon: Option<u64>,
    milestones: Vec<RawMilestone>,
) -> ValidatedScenarioBundle {
    let ruleset_id = RulesetId::new(format!("rules_{name}")).expect("test ruleset ID");
    let ruleset =
        CompiledRuleset::from_parts(ruleset_id.clone(), mechanics).expect("test mechanics");
    let reward_schedule = RewardSchedule::from_raw(
        RawRewardScheduleV1 {
            schema_version: 1,
            document_type: "reward_schedule".to_owned(),
            reward_schedule_id: format!("rewards_{name}"),
            compatible_ruleset_ids: vec![ruleset_id.to_string()],
            milestones,
        },
        None,
    )
    .expect("test rewards");
    let raw_scenario = RawScenarioV1 {
        schema_version: 1,
        document_type: "scenario".to_owned(),
        scenario_id: format!("scenario_{name}"),
        ruleset_id: ruleset_id.to_string(),
        reward_schedule_id: reward_schedule.id().to_string(),
        students: vec![RawStudent {
            student_id: "target".to_owned(),
        }],
        banners: vec![RawBanner {
            banner_id: "banner".to_owned(),
            featured_student_id: "target".to_owned(),
            charge_group_id: "group".to_owned(),
        }],
        initial_charges: vec![RawInitialCharge {
            charge_group_id: "group".to_owned(),
            pre_recruitment_charge: initial_charge,
        }],
        initial_resources,
        initial_owned_targets: Vec::new(),
        strategy: RawStrategy {
            strategy_id: "strategy".to_owned(),
            kind: RawStrategyKind::SequentialTargetsPreferTickets,
            max_total_recruitments: NullablePositive::Present(
                horizon.map(|value| NonZeroU64::new(value).expect("positive test horizon")),
            ),
        },
        targets: vec![RawTarget {
            student_id: "target".to_owned(),
            banner_id: "banner".to_owned(),
        }],
    };
    ValidatedScenarioBundle::from_programmatic(raw_scenario, ruleset, reward_schedule)
        .expect("test bundle")
}

pub fn half_probability_mechanics() -> RulesetMechanics {
    RulesetMechanics {
        paid_single_cost: 120,
        paid_single_action_size: 1,
        ticket_action_size: 10,
        ordinary_pickup_probability: ba_core::ProbabilityRatio::new(1, 2).expect("half ratio"),
        maximum_pre_recruitment_charge: 10,
        hit_reset_charge: 0,
        miss_increment: 1,
        threshold_overrides: vec![(
            10,
            ba_core::ProbabilityRatio::new(1, 1).expect("certain ratio"),
        )],
    }
}

pub fn ticket_reward(count: u64, quantity: u64) -> RawMilestone {
    RawMilestone {
        count,
        rewards: vec![ba_core::schema::RawReward {
            resource: ResourceKind::LimitedTenRecruitmentTickets,
            quantity,
        }],
    }
}
