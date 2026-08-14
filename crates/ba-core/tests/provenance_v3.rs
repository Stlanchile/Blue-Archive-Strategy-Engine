use ba_core::schema::RawProvenanceV3;
use ba_core::{ClaimGroupV3, ProvenanceSubjectV3, ProvenanceV3, RULESET_CLAIM_GROUPS_V3};

fn source() -> serde_json::Value {
    serde_json::json!({
        "source_id": "official_source",
        "source_category": "first_party_official",
        "label": "Official source",
        "reference": "https://example.invalid/inert",
        "published_on": "2026-07-28",
        "retrieved_on": "2026-08-13",
        "content_sha256": null
    })
}

#[test]
fn source_backed_requires_complete_first_party_claim_coverage() {
    let bindings = RULESET_CLAIM_GROUPS_V3
        .iter()
        .map(|group| {
            serde_json::json!({
                "claim_group": group.as_str(),
                "source_ids": ["official_source"]
            })
        })
        .collect::<Vec<_>>();
    let raw: RawProvenanceV3 = serde_json::from_value(serde_json::json!({
        "provenance_status": "source_backed",
        "sources": [source()],
        "claim_bindings": bindings
    }))
    .expect("raw provenance");
    let compiled =
        ProvenanceV3::from_raw(raw, ProvenanceSubjectV3::Ruleset, None).expect("coverage");
    assert_eq!(compiled.claim_bindings.len(), 7);
    assert_eq!(
        compiled.claim_bindings[0].claim_group,
        ClaimGroupV3::RecruitmentCost
    );
}

#[test]
fn missing_claim_and_verified_status_are_rejected() {
    let raw: RawProvenanceV3 = serde_json::from_value(serde_json::json!({
        "provenance_status": "source_backed",
        "sources": [source()],
        "claim_bindings": [{
            "claim_group": "recruitment_cost",
            "source_ids": ["official_source"]
        }]
    }))
    .expect("raw provenance");
    assert!(ProvenanceV3::from_raw(raw, ProvenanceSubjectV3::Ruleset, None).is_err());

    let verified = serde_json::from_value::<RawProvenanceV3>(serde_json::json!({
        "provenance_status": "verified",
        "sources": [],
        "claim_bindings": []
    }));
    assert!(verified.is_err());
}

#[test]
fn dates_hashes_and_binding_references_are_structural() {
    let malformed_date: RawProvenanceV3 = serde_json::from_value(serde_json::json!({
        "provenance_status": "provisional",
        "sources": [{
            "source_id": "source",
            "source_category": "first_party_official",
            "label": "Official",
            "reference": "../../never-opened",
            "published_on": "2025-02-29",
            "retrieved_on": "2026-08-13",
            "content_sha256": null
        }],
        "claim_bindings": []
    }))
    .expect("typed raw");
    assert!(
        ProvenanceV3::from_raw(malformed_date, ProvenanceSubjectV3::RewardSchedule, None).is_err()
    );
}
