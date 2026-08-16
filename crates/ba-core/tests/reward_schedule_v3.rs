use ba_core::schema::RawRewardScheduleV3;
use ba_core::{LedgerResourceKind, RewardScheduleV3};

fn schedule(value: serde_json::Value) -> RewardScheduleV3 {
    let raw: RawRewardScheduleV3 = serde_json::from_value(value).expect("raw schedule");
    RewardScheduleV3::from_raw(raw, None).expect("compiled schedule")
}

#[test]
fn repeat_coordinates_cover_offset_period_and_next_cycle() {
    let schedule = schedule(serde_json::json!({
        "schema_version": 3,
        "document_type": "reward_schedule",
        "reward_schedule_id": "repeat_test",
        "provenance": {
            "provenance_status": "provisional",
            "sources": [],
            "claim_bindings": []
        },
        "compatible_ruleset_ids": ["rules"],
        "initial_milestones": [{
            "count": 390,
            "rewards": [{"resource": "eligma", "quantity": 3}]
        }],
        "repeating_cycle": {
            "starts_after_count": 390,
            "period": 200,
            "milestones": [
                {
                    "offset": 20,
                    "rewards": [{"resource": "eligma", "quantity": 1}]
                },
                {
                    "offset": 200,
                    "rewards": [{"resource": "gift_boxes", "quantity": 2}]
                }
            ]
        }
    }));
    for absent in [389, 391, 409, 411, 589, 591, 609, 611] {
        assert!(schedule.milestone_at(absent).is_none(), "{absent}");
    }
    for present in [390, 410, 590, 610, 790] {
        assert_eq!(
            schedule
                .milestone_at(present)
                .map(|milestone| milestone.count),
            Some(present)
        );
    }
    assert_eq!(
        schedule.first_future_repeat_milestone(590).expect("next"),
        Some(610)
    );
    let interval = schedule
        .resources_earned_between(390, 610)
        .expect("interval");
    assert_eq!(interval.get(LedgerResourceKind::Eligma), 2);
    assert_eq!(interval.get(LedgerResourceKind::GiftBoxes), 2);
    assert_eq!(
        schedule
            .materialized_future_milestones(390, 220)
            .expect("materialized")
            .iter()
            .map(|milestone| milestone.count)
            .collect::<Vec<_>>(),
        vec![410, 590, 610]
    );
}

#[test]
fn direct_interval_does_not_require_historical_cumulative_ledger() {
    let schedule = schedule(serde_json::json!({
        "schema_version": 3,
        "document_type": "reward_schedule",
        "reward_schedule_id": "overflow_test",
        "provenance": {
            "provenance_status": "provisional",
            "sources": [],
            "claim_bindings": []
        },
        "compatible_ruleset_ids": ["rules"],
        "initial_milestones": [],
        "repeating_cycle": {
            "starts_after_count": 0,
            "period": 1,
            "milestones": [{
                "offset": 1,
                "rewards": [{
                    "resource": "eligma",
                    "quantity": 18446744073709551615u64
                }]
            }]
        }
    }));
    assert!(schedule.resources_earned_through(2).is_err());
    assert_eq!(
        schedule
            .resources_earned_between(1, 2)
            .expect("one future occurrence")
            .get(LedgerResourceKind::Eligma),
        u64::MAX
    );
}

#[test]
fn analytic_intervals_match_bounded_reference_enumeration() {
    let schedule = schedule(serde_json::json!({
        "schema_version": 3,
        "document_type": "reward_schedule",
        "reward_schedule_id": "differential_test",
        "provenance": {
            "provenance_status": "provisional",
            "sources": [],
            "claim_bindings": []
        },
        "compatible_ruleset_ids": ["rules"],
        "initial_milestones": [
            {
                "count": 10,
                "rewards": [{"resource": "eligma", "quantity": 3}]
            },
            {
                "count": 390,
                "rewards": [{"resource": "gift_boxes", "quantity": 2}]
            }
        ],
        "repeating_cycle": {
            "starts_after_count": 390,
            "period": 200,
            "milestones": [
                {
                    "offset": 20,
                    "rewards": [{"resource": "eligma", "quantity": 5}]
                },
                {
                    "offset": 200,
                    "rewards": [{"resource": "gift_boxes", "quantity": 7}]
                }
            ]
        }
    }));
    for start in (0..=800).step_by(17) {
        for width in [0, 1, 19, 20, 199, 200, 401] {
            let end = start + width;
            let actual = schedule
                .resources_earned_between(start, end)
                .expect("analytic");
            let mut expected = ba_core::ResourceLedger::default();
            for count in (start + 1)..=end {
                if let Some(milestone) = schedule.milestone_at(count) {
                    for reward in milestone.rewards {
                        expected
                            .checked_add(reward.resource, reward.quantity)
                            .expect("reference");
                    }
                }
            }
            assert_eq!(actual, expected, "interval ({start}, {end}]");
        }
    }
}

#[test]
fn endpoint_and_effective_materialization_guards_reject_without_truncation() {
    let guard_schedule = schedule(serde_json::json!({
        "schema_version": 3,
        "document_type": "reward_schedule",
        "reward_schedule_id": "guard_test",
        "provenance": {
            "provenance_status": "provisional",
            "sources": [],
            "claim_bindings": []
        },
        "compatible_ruleset_ids": ["rules"],
        "initial_milestones": [],
        "repeating_cycle": {
            "starts_after_count": 0,
            "period": 1,
            "milestones": [{
                "offset": 1,
                "rewards": [{"resource": "eligma", "quantity": 1}]
            }]
        }
    }));
    assert!(
        guard_schedule
            .materialized_future_milestones(0, 65_537)
            .is_err()
    );
    assert_eq!(
        guard_schedule
            .materialized_future_milestones(0, 65_536)
            .expect("at guard")
            .len(),
        65_536
    );
    assert!(
        guard_schedule
            .effective_milestone_count(u64::MAX, 1)
            .is_err()
    );

    let maximum_endpoint_schedule = schedule(serde_json::json!({
        "schema_version": 3,
        "document_type": "reward_schedule",
        "reward_schedule_id": "maximum_endpoint_test",
        "provenance": {
            "provenance_status": "provisional",
            "sources": [],
            "claim_bindings": []
        },
        "compatible_ruleset_ids": ["rules"],
        "initial_milestones": [],
        "repeating_cycle": {
            "starts_after_count": u64::MAX - 1,
            "period": 1,
            "milestones": [{
                "offset": 1,
                "rewards": [{"resource": "eligma", "quantity": 1}]
            }]
        }
    }));
    assert_eq!(
        maximum_endpoint_schedule
            .materialized_future_milestones(u64::MAX - 1, 1)
            .expect("u64::MAX endpoint")
            .iter()
            .map(|milestone| milestone.count)
            .collect::<Vec<_>>(),
        vec![u64::MAX]
    );
}

#[test]
fn unrepresentable_future_repeat_is_absent_instead_of_an_overflow_error() {
    let schedule = schedule(serde_json::json!({
        "schema_version": 3,
        "document_type": "reward_schedule",
        "reward_schedule_id": "unrepresentable_future_test",
        "provenance": {
            "provenance_status": "provisional",
            "sources": [],
            "claim_bindings": []
        },
        "compatible_ruleset_ids": ["rules"],
        "initial_milestones": [],
        "repeating_cycle": {
            "starts_after_count": 0,
            "period": 10,
            "milestones": [{
                "offset": 1,
                "rewards": [{"resource": "eligma", "quantity": 1}]
            }]
        }
    }));

    assert_eq!(
        schedule
            .first_future_repeat_milestone(u64::MAX - 4)
            .expect("absence is valid"),
        None
    );
    assert!(
        schedule
            .materialized_future_milestones(u64::MAX - 4, 1)
            .expect("bounded interval without a future occurrence")
            .is_empty()
    );
    assert_eq!(
        schedule
            .materialized_future_milestones(u64::MAX - 14, 14)
            .expect("last representable occurrence")
            .iter()
            .map(|milestone| milestone.count)
            .collect::<Vec<_>>(),
        vec![u64::MAX - 4]
    );
}
