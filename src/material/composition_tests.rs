//! Tests for the sibling composition module; isolated so test-only edits do not invalidate production builds.

use super::*;

#[test]
fn normalizes_order_and_projects_constituent_mass_without_rounding_up() {
    let copper = MaterialId::new(3);
    let slag = MaterialId::new(4);
    let composition = MaterialComposition::new(vec![
        CompositionComponent::new(slag, 275_000),
        CompositionComponent::new(copper, 725_000),
    ])
    .unwrap_or_else(|error| panic!("composition unexpectedly failed: {error}"));

    assert_eq!(composition.components()[0].material(), copper);
    assert_eq!(composition.components()[1].material(), slag);
    assert_eq!(composition.parts_per_million(copper), 725_000);
    assert_eq!(
        composition.constituent_mass_floor(Mass::from_milligrams(3), copper),
        Mass::from_milligrams(2)
    );
    assert_eq!(
        composition.constituent_mass_floor(Mass::from_milligrams(3), slag),
        Mass::ZERO
    );
}

#[test]
fn rejects_non_normalized_fraction_total() {
    let result = MaterialComposition::new(vec![
        CompositionComponent::new(MaterialId::new(3), 500_000),
        CompositionComponent::new(MaterialId::new(4), 499_999),
    ]);

    assert_eq!(
        result,
        Err(CompositionError::FractionSumMismatch { found: 999_999 })
    );
}

#[test]
fn deserialization_rejects_noncanonical_order() {
    let encoded = br#"{
            "components": [
                {"material":4,"parts_per_million":500000},
                {"material":3,"parts_per_million":500000}
            ]
        }"#;
    let result: Result<MaterialComposition, _> = serde_json::from_slice(encoded);

    assert!(result.is_err());
}

#[test]
fn constraints_match_composition_ranges_inclusively() {
    let copper = MaterialId::new(3);
    let slag = MaterialId::new(4);
    let composition = MaterialComposition::new(vec![
        CompositionComponent::new(copper, 800_000),
        CompositionComponent::new(slag, 200_000),
    ])
    .unwrap_or_else(|error| panic!("composition unexpectedly failed: {error}"));
    let accepts = CompositionConstraint::new(copper, 800_000, 900_000)
        .unwrap_or_else(|error| panic!("constraint unexpectedly failed: {error}"));
    let rejects = CompositionConstraint::new(slag, 0, 199_999)
        .unwrap_or_else(|error| panic!("constraint unexpectedly failed: {error}"));

    assert!(accepts.is_satisfied_by(&composition));
    assert!(!rejects.is_satisfied_by(&composition));
}
