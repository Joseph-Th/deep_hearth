//! Tests for the sibling mod module; isolated so test-only edits do not invalidate production builds.

use super::*;

const CHAMBER_TEMPERATURE: CapabilityId = CapabilityId::new(1);
const LOAD_CAPACITY: CapabilityId = CapabilityId::new(2);

fn make_registry() -> CapabilityRegistry {
    let mut registry = CapabilityRegistry::new();
    registry.register_capability(CapabilityDefinition::new(
        CHAMBER_TEMPERATURE,
        "test chamber temperature",
        CapabilityValueKind::Temperature,
    ));
    registry.register_capability(CapabilityDefinition::new(
        LOAD_CAPACITY,
        "test load capacity",
        CapabilityValueKind::Mass,
    ));
    registry
}

#[test]
fn typed_capabilities_enforce_at_least_and_at_most_without_generic_tiers() {
    let registry = make_registry();
    let profile = match CapabilityProfile::new([
        (
            CHAMBER_TEMPERATURE,
            CapabilityValue::Temperature(Temperature::from_millikelvin(1_500_000)),
        ),
        (
            LOAD_CAPACITY,
            CapabilityValue::Mass(Mass::from_milligrams(20_000)),
        ),
    ]) {
        Ok(profile) => profile,
        Err(error) => panic!("profile fixture failed: {error}"),
    };
    let requirements = [
        CapabilityRequirement::new(
            CHAMBER_TEMPERATURE,
            CapabilityComparison::AtLeast,
            CapabilityValue::Temperature(Temperature::from_millikelvin(1_200_000)),
        ),
        CapabilityRequirement::new(
            LOAD_CAPACITY,
            CapabilityComparison::AtMost,
            CapabilityValue::Mass(Mass::from_milligrams(25_000)),
        ),
    ];

    assert_eq!(
        evaluate_capabilities(&registry, &profile, &requirements),
        Ok(())
    );
}

#[test]
fn wrong_physical_dimension_is_rejected_before_threshold_comparison() {
    let registry = make_registry();
    let profile = match CapabilityProfile::new([(
        CHAMBER_TEMPERATURE,
        CapabilityValue::Temperature(Temperature::from_millikelvin(1_500_000)),
    )]) {
        Ok(profile) => profile,
        Err(error) => panic!("profile fixture failed: {error}"),
    };
    let requirement = CapabilityRequirement::new(
        CHAMBER_TEMPERATURE,
        CapabilityComparison::AtLeast,
        CapabilityValue::Power(Power::from_picowatts(1)),
    );

    assert_eq!(
        evaluate_capabilities(&registry, &profile, &[requirement]),
        Err(CapabilityEvaluationError::RequirementKindMismatch {
            capability: CHAMBER_TEMPERATURE,
            expected: CapabilityValueKind::Temperature,
            found: CapabilityValueKind::Power,
        })
    );
}

#[test]
fn insufficient_capability_reports_requirement_and_provided_values() {
    let registry = make_registry();
    let provided = CapabilityValue::Temperature(Temperature::from_millikelvin(900_000));
    let required = CapabilityValue::Temperature(Temperature::from_millikelvin(1_200_000));
    let profile = match CapabilityProfile::new([(CHAMBER_TEMPERATURE, provided)]) {
        Ok(profile) => profile,
        Err(error) => panic!("profile fixture failed: {error}"),
    };
    let requirement =
        CapabilityRequirement::new(CHAMBER_TEMPERATURE, CapabilityComparison::AtLeast, required);

    assert_eq!(
        evaluate_capabilities(&registry, &profile, &[requirement]),
        Err(CapabilityEvaluationError::ThresholdNotMet {
            capability: CHAMBER_TEMPERATURE,
            comparison: CapabilityComparison::AtLeast,
            required,
            provided,
        })
    );
}

#[test]
fn capability_interpolation_handles_full_width_and_decreasing_ranges() {
    assert_eq!(
        interpolate_capability_value(
            CapabilityValue::Power(Power::ZERO),
            CapabilityValue::Power(Power::from_picowatts(u128::MAX)),
            1,
            2,
        ),
        Some(CapabilityValue::Power(Power::from_picowatts(u128::MAX / 2)))
    );
    assert_eq!(
        interpolate_capability_value(
            CapabilityValue::Mass(Mass::from_milligrams(100)),
            CapabilityValue::Mass(Mass::ZERO),
            1,
            3,
        ),
        Some(CapabilityValue::Mass(Mass::from_milligrams(67)))
    );
    assert_eq!(
        interpolate_capability_value(
            CapabilityValue::Mass(Mass::from_milligrams(1)),
            CapabilityValue::Power(Power::from_picowatts(1)),
            1,
            2,
        ),
        None
    );
}

#[test]
fn material_throughput_is_a_distinct_typed_capability() {
    assert_eq!(
        CapabilityValue::MassFlow(MassFlow::from_milligrams_per_second(25)).kind(),
        CapabilityValueKind::MassFlow
    );
    assert_eq!(
        interpolate_capability_value(
            CapabilityValue::MassFlow(MassFlow::from_milligrams_per_second(10)),
            CapabilityValue::MassFlow(MassFlow::from_milligrams_per_second(30)),
            1,
            2,
        ),
        Some(CapabilityValue::MassFlow(
            MassFlow::from_milligrams_per_second(20)
        ))
    );
}
