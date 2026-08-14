use ba_core::{LedgerResourceKind, ResourceLedger, Resources, ResourcesV3, resource_kind_name_v3};

#[test]
fn v3_requires_all_eleven_explicit_resource_fields() {
    let complete = serde_json::json!({
        "pyroxene": 1,
        "limited_ten_recruitment_tickets": 2,
        "eligma": 3,
        "advanced_bd_selectors": 4,
        "advanced_tech_note_selectors": 5,
        "superior_tech_note_selectors": 6,
        "gift_boxes": 7,
        "keystone_fragments": 8,
        "secret_tech_notes": 9,
        "superior_bd_selectors": 10,
        "high_grade_gift_boxes": 11
    });
    let resources: ResourcesV3 = serde_json::from_value(complete.clone()).expect("complete");
    assert_eq!(resources.high_grade_gift_boxes, 11);
    let mut missing = complete;
    missing
        .as_object_mut()
        .expect("object")
        .remove("secret_tech_notes");
    assert!(serde_json::from_value::<ResourcesV3>(missing).is_err());
}

#[test]
fn v2_resource_dto_remains_exactly_seven_fields() {
    let resources = Resources::default();
    assert_eq!(
        serde_json::to_value(resources)
            .expect("JSON")
            .as_object()
            .expect("object")
            .len(),
        7
    );
    assert!(
        serde_json::from_value::<Resources>(serde_json::json!({
            "pyroxene": 0,
            "limited_ten_recruitment_tickets": 0,
            "eligma": 0,
            "advanced_bd_selectors": 0,
            "advanced_tech_note_selectors": 0,
            "superior_tech_note_selectors": 0,
            "gift_boxes": 0,
            "keystone_fragments": 0
        }))
        .is_err()
    );
}

#[test]
fn ledger_iteration_and_checked_differences_are_canonical() {
    let mut ledger = ResourceLedger::default();
    ledger
        .checked_add(LedgerResourceKind::HighGradeGiftBoxes, 3)
        .expect("add");
    ledger
        .checked_add(LedgerResourceKind::Pyroxene, 120)
        .expect("add");
    let names = ledger
        .iter_canonical()
        .map(|(kind, _)| resource_kind_name_v3(kind))
        .collect::<Vec<_>>();
    assert_eq!(
        names,
        vec![
            "pyroxene",
            "limited_ten_recruitment_tickets",
            "eligma",
            "advanced_bd_selectors",
            "advanced_tech_note_selectors",
            "superior_tech_note_selectors",
            "gift_boxes",
            "keystone_fragments",
            "secret_tech_notes",
            "superior_bd_selectors",
            "high_grade_gift_boxes",
        ]
    );
    let original = ledger;
    ledger.checked_sub_ledger(original).expect("subtract");
    assert_eq!(ledger, ResourceLedger::default());
    assert!(ledger.checked_sub_ledger(original).is_err());
}
