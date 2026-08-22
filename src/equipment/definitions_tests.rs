//! Tests for the sibling definitions module; isolated so test-only edits do not invalidate production builds.

use super::*;

#[test]
fn continuous_condition_curve_rejects_presence_capability() {
    let capability = CapabilityId::new(810_001);
    let result = std::panic::catch_unwind(|| {
        CapabilityConditionCurve::new(
            capability,
            vec![CapabilityConditionPoint::new(
                Condition::FAILED,
                CapabilityValue::Present,
            )],
        )
    });

    assert!(result.is_err());
}

#[test]
fn condition_curve_rejects_nonmonotonic_recovery_toward_nominal_value() {
    let capability = CapabilityId::new(810_002);
    let nominal = CapabilityValue::Mass(Mass::from_milligrams(100));
    let profile = CapabilityProfile::new([(capability, nominal)])
        .unwrap_or_else(|error| panic!("capability profile fixture failed: {error}"));
    let thresholds = MaintenanceThresholds::new(
        Condition::new(600_000)
            .unwrap_or_else(|error| panic!("warning condition fixture failed: {error}")),
        Condition::new(250_000)
            .unwrap_or_else(|error| panic!("critical condition fixture failed: {error}")),
    )
    .unwrap_or_else(|error| panic!("maintenance threshold fixture failed: {error}"));
    let curve = CapabilityConditionCurve::new(
        capability,
        vec![
            CapabilityConditionPoint::new(Condition::FAILED, CapabilityValue::Mass(Mass::ZERO)),
            CapabilityConditionPoint::new(
                Condition::new(500_000)
                    .unwrap_or_else(|error| panic!("midpoint condition fixture failed: {error}")),
                CapabilityValue::Mass(Mass::from_milligrams(80)),
            ),
            CapabilityConditionPoint::new(
                Condition::new(750_000)
                    .unwrap_or_else(|error| panic!("late condition fixture failed: {error}")),
                CapabilityValue::Mass(Mass::from_milligrams(70)),
            ),
        ],
    );

    let result = std::panic::catch_unwind(|| {
        EquipmentDefinition::new_with_capability_condition_curves(
            EquipmentDefinitionId::new(810_002),
            "nonmonotonic condition fixture",
            Mass::from_milligrams(1),
            profile,
            thresholds,
            vec![curve],
        )
    });

    assert!(result.is_err());
}
