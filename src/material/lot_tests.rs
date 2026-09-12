//! Contract tests for material-lot physical state.

use super::*;
use crate::material::{COMPOSITION_PARTS_PER_MILLION, CompositionComponent, FormId};

#[test]
fn pure_input_spec_accepts_only_exact_host_material() {
    let host = MaterialId::new(3);
    let other = MaterialId::new(4);
    let spec = MaterialInputSpec::pure(
        CommodityKey::new(host, FormId::new(1)),
        Mass::from_milligrams(10),
    );
    let mixed = MaterialComposition::new(vec![
        CompositionComponent::new(host, 900_000),
        CompositionComponent::new(other, 100_000),
    ])
    .unwrap_or_else(|error| panic!("mixed input fixture failed: {error}"));

    assert!(spec.requires_pure_material());
    assert!(spec.is_satisfied_by(&MaterialComposition::pure(host)));
    assert!(!spec.is_satisfied_by(&mixed));
}

#[test]
fn input_spec_rejects_duplicate_material_constraints() {
    let material = MaterialId::new(3);
    let constraint = CompositionConstraint::new(material, 100_000, 900_000)
        .unwrap_or_else(|error| panic!("constraint fixture failed: {error}"));

    assert_eq!(
        MaterialInputSpec::with_constraints(
            CommodityKey::new(material, FormId::new(1)),
            Mass::from_milligrams(10),
            vec![constraint, constraint],
        ),
        Err(MaterialInputSpecError::DuplicateConstraint { material })
    );
}

#[test]
fn lot_spec_deserialization_enforces_intrinsic_matter_invariants() {
    let host = MaterialId::new(3);
    let form = FormId::new(1);
    let valid = MaterialLotSpec::new(
        CommodityKey::new(host, form),
        Mass::from_milligrams(10),
        Temperature::from_millikelvin(300_000),
    );
    let encoded = serde_json::to_value(&valid)
        .unwrap_or_else(|error| panic!("lot specification serialization failed: {error}"));
    let decoded: MaterialLotSpec = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("lot specification deserialization failed: {error}"));
    assert_eq!(decoded, valid);

    let other = MaterialId::new(4);
    for invalid in [
        serde_json::json!({
            "commodity": CommodityKey::new(host, form).value(),
            "mass": 0,
            "temperature": 300_000,
            "composition": [{"material": host.value(), "parts_per_million": 1_000_000}],
            "particle_size": null,
        }),
        serde_json::json!({
            "commodity": CommodityKey::new(host, form).value(),
            "mass": 10,
            "temperature": 300_000,
            "composition": [{"material": other.value(), "parts_per_million": 1_000_000}],
            "particle_size": null,
        }),
    ] {
        assert!(serde_json::from_value::<MaterialLotSpec>(invalid).is_err());
    }
}

#[test]
fn input_spec_requires_room_for_its_commodity_host() {
    let host = MaterialId::new(3);
    let other = MaterialId::new(4);
    let all_other = CompositionConstraint::new(
        other,
        COMPOSITION_PARTS_PER_MILLION,
        COMPOSITION_PARTS_PER_MILLION,
    )
    .unwrap_or_else(|error| panic!("other-material constraint fixture failed: {error}"));
    assert_eq!(
        MaterialInputSpec::with_constraints(
            CommodityKey::new(host, FormId::new(1)),
            Mass::from_milligrams(10),
            vec![all_other],
        ),
        Err(MaterialInputSpecError::ImpossibleMinimumTotal {
            total_ppm: u64::from(COMPOSITION_PARTS_PER_MILLION) + 1,
        })
    );

    let excludes_host = CompositionConstraint::new(host, 0, 0)
        .unwrap_or_else(|error| panic!("host-exclusion constraint fixture failed: {error}"));
    assert_eq!(
        MaterialInputSpec::with_constraints(
            CommodityKey::new(host, FormId::new(1)),
            Mass::from_milligrams(10),
            vec![excludes_host],
        ),
        Err(MaterialInputSpecError::HostExcluded { host })
    );
}

#[test]
fn input_spec_rejects_physically_impossible_combined_minimums() {
    let host = MaterialId::new(3);
    let other = MaterialId::new(4);
    let host_constraint = CompositionConstraint::new(host, 600_000, 900_000)
        .unwrap_or_else(|error| panic!("host constraint fixture failed: {error}"));
    let other_constraint = CompositionConstraint::new(other, 500_000, 800_000)
        .unwrap_or_else(|error| panic!("other constraint fixture failed: {error}"));

    assert_eq!(
        MaterialInputSpec::with_constraints(
            CommodityKey::new(host, FormId::new(1)),
            Mass::from_milligrams(10),
            vec![host_constraint, other_constraint],
        ),
        Err(MaterialInputSpecError::ImpossibleMinimumTotal {
            total_ppm: 1_100_000,
        })
    );
}

#[test]
fn lot_spec_requires_composition_to_contain_host_material() {
    let host = MaterialId::new(3);
    let other = MaterialId::new(4);
    let composition = MaterialComposition::new(vec![CompositionComponent::new(
        other,
        COMPOSITION_PARTS_PER_MILLION,
    )])
    .unwrap_or_else(|error| panic!("composition fixture failed: {error}"));

    assert_eq!(
        MaterialLotSpec::with_composition(
            CommodityKey::new(host, FormId::new(1)),
            Mass::from_milligrams(10),
            Temperature::from_millikelvin(300_000),
            composition,
        ),
        Err(MaterialLotSpecError::MissingHostMaterial { host })
    );
}
