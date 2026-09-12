//! Contract tests for material-lot provenance value semantics.

use super::*;

#[test]
fn provenance_deserialization_rejects_inverted_creation_range() {
    let valid = MaterialLotProvenance::single(SimulationTick::new(7));
    let encoded = serde_json::to_value(valid)
        .unwrap_or_else(|error| panic!("material provenance serialization failed: {error}"));
    let decoded: MaterialLotProvenance = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("material provenance deserialization failed: {error}"));
    assert_eq!(decoded, valid);

    assert!(
        serde_json::from_value::<MaterialLotProvenance>(serde_json::json!({
            "earliest_created_at": 8,
            "latest_created_at": 7,
        }))
        .is_err()
    );
}

#[test]
fn provenance_merge_preserves_full_creation_range() {
    let older = MaterialLotProvenance::single(SimulationTick::new(3));
    let newer = MaterialLotProvenance::single(SimulationTick::new(11));
    let merged = newer.merged(older);

    assert_eq!(merged.earliest_created_at(), SimulationTick::new(3));
    assert_eq!(merged.latest_created_at(), SimulationTick::new(11));
}
