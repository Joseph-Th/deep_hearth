//! Tests for the sibling particle module; isolated so test-only edits do not invalidate production builds.

use super::*;

#[test]
fn distribution_canonicalizes_order_and_preserves_weighted_classes() {
    let fine = ParticleSizeClass::new(
        ParticleSizeRange::new(Length::from_micrometers(100), Length::from_micrometers(500))
            .unwrap_or_else(|error| panic!("fine particle range failed: {error}")),
        3,
    )
    .unwrap_or_else(|error| panic!("fine particle class failed: {error}"));
    let coarse = ParticleSizeClass::new(
        ParticleSizeRange::new(
            Length::from_micrometers(501),
            Length::from_micrometers(2_000),
        )
        .unwrap_or_else(|error| panic!("coarse particle range failed: {error}")),
        7,
    )
    .unwrap_or_else(|error| panic!("coarse particle class failed: {error}"));

    let fine = ParticleSizeClass::new(fine.range(), 6)
        .unwrap_or_else(|error| panic!("scaled fine particle class failed: {error}"));
    let coarse = ParticleSizeClass::new(coarse.range(), 14)
        .unwrap_or_else(|error| panic!("scaled coarse particle class failed: {error}"));
    let distribution = ParticleSizeDistribution::new(vec![coarse, fine])
        .unwrap_or_else(|error| panic!("particle distribution failed: {error}"));

    assert_eq!(distribution.classes()[0].weight(), 3);
    assert_eq!(distribution.classes()[1].weight(), 7);
    assert_eq!(distribution.total_weight(), 10);
    assert_eq!(
        distribution.envelope(),
        ParticleSizeRange::new(
            Length::from_micrometers(100),
            Length::from_micrometers(2_000),
        )
        .unwrap_or_else(|error| panic!("particle envelope failed: {error}"))
    );
}

#[test]
fn distribution_rejects_ambiguous_boundaries_and_noncanonical_save_order() {
    let first = ParticleSizeClass::new(
        ParticleSizeRange::new(Length::from_micrometers(100), Length::from_micrometers(500))
            .unwrap_or_else(|error| panic!("first particle range failed: {error}")),
        1,
    )
    .unwrap_or_else(|error| panic!("first particle class failed: {error}"));
    let touching = ParticleSizeClass::new(
        ParticleSizeRange::new(
            Length::from_micrometers(500),
            Length::from_micrometers(1_000),
        )
        .unwrap_or_else(|error| panic!("touching particle range failed: {error}")),
        1,
    )
    .unwrap_or_else(|error| panic!("touching particle class failed: {error}"));
    assert!(matches!(
        ParticleSizeDistribution::new(vec![first, touching]),
        Err(ParticleSizeDistributionError::OverlappingClasses {
            previous: _previous,
            current: _current,
        })
    ));

    let encoded = br#"{
            "classes": [
                {"range":{"minimum_diameter":501,"maximum_diameter":1000},"weight":1},
                {"range":{"minimum_diameter":100,"maximum_diameter":500},"weight":1}
            ]
        }"#;
    let decoded: Result<ParticleSizeDistribution, _> = serde_json::from_slice(encoded);
    assert!(decoded.is_err());

    let noncanonical_weights = br#"{
            "classes": [
                {"range":{"minimum_diameter":100,"maximum_diameter":500},"weight":4},
                {"range":{"minimum_diameter":501,"maximum_diameter":1000},"weight":2}
            ]
        }"#;
    let decoded: Result<ParticleSizeDistribution, _> = serde_json::from_slice(noncanonical_weights);
    assert!(decoded.is_err());
}
